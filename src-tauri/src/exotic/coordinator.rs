// src-tauri/src/exotic/coordinator.rs
//! 冷门格式插件 · Coordinator（v3 Part2 §4.1）。
//!
//! 单一调度器：接收扫描/安装/激活/配置/重试时钟等事件，**幂等**唤醒**唯一**一条 Pipeline。
//!
//! 设计：
//!   - 有界事件通道 + `dirty` 原子位：通道满时不静默丢失最后一次 wake（置 dirty，循环结束后补查）。
//!   - 串行循环：唯一 owner，commands 只发 wake → 天然「两个并发 start 只启动一条 Pipeline」。
//!   - 尾部竞态：每条 Pipeline 自然完成后再查 pending + 到期 retry，解决运行期间新增任务（§4.1）。
//!   - 重试时钟：独立 interval 周期发 `RetryDue`，使到期 retryable 任务被重新评估。
//!   - 门控（[`evaluate_run`]）：enabled/未暂停/可领取(授权+平台+能力)/有就绪任务/Worker 可用 → 才跑。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::db::queries as q;
use crate::exotic::catalog::Capability;
use crate::exotic::pipeline::{run_exotic_pipeline_blocking, PipelineDeps, SupervisorFactory};
use crate::exotic::worker::{WorkerConfig, WorkerSpec};
use crate::exotic::ExoticHost;
use crate::state::AppState;

/// 首发唯一插件 + 能力（Part2）。
pub const PSD_PLUGIN_ID: &str = "exotic-image-psd";
const PSD_WORKER_ID: &str = "psd-worker";
// T13 后调度路径全走注册表(descriptor),此常量仅测试作 shorthand。
#[cfg(test)]
const CAPABILITY: Capability = Capability::Thumbnail;

/// per-op 请求超时表(Part4 D3 §5/T13:常量集中一处,Supervisor 请求执行按 op 取值)。
/// 起步值,实测再调。SESSION_INIT/SESSION_CLOSE/EMBED_BATCH/ENCODE_TEXT 由
/// `ai::worker_client`(T17 派发)消费。
pub(crate) mod op_timeouts {
    use std::time::Duration;
    /// thumbnail 单请求(原 pipeline::TASK_TIMEOUT 同值收拢至此)。
    pub const THUMBNAIL: Duration = Duration::from_secs(30);
    /// SessionInit:冷加载 ViT-L fp32 + DirectML 编译内核的上界(D3 §2;进程握手
    /// 仍 5s 不动——模型加载不在握手)。
    pub const SESSION_INIT: Duration = Duration::from_secs(300);
    /// SessionClose:健康 worker 卸载即 drop(毫秒级);上界只兜「驱动释放 VRAM 慢」,
    /// 超时即 kill 回收,会话随进程消亡(T17)。
    pub const SESSION_CLOSE: Duration = Duration::from_secs(30);
    /// EmbedBatch 一批(D3 §5 起步值)。
    pub const EMBED_BATCH: Duration = Duration::from_secs(120);
    /// FaceDetectEmbed 基础超时(2026-07-03 GUI 实测修订:原与 EmbedBatch 同档固定
    /// 120s,64 张全尺寸原图批在 dev 构建下必然超时 → supervisor 误杀正常 worker,
    /// 重试同批再超时 → 硬止损终止整轮)。
    pub const FACE_DETECT_EMBED_BASE: Duration = Duration::from_secs(60);
    /// FaceDetectEmbed 单项增量:人脸源可为全尺寸原图(解码+letterbox 秒级,慢盘/dev
    /// 构建更甚),超时按批内项数线性放宽。这是「假死检测器」而非性能指标——用户取消
    /// 走 cancelled 回调即时生效,不受本值影响,宁可宽松。
    pub const FACE_DETECT_EMBED_PER_ITEM: Duration = Duration::from_secs(6);
    /// FaceDetectEmbed 一批的实际超时 = 基础 + 单项增量 × 项数。
    pub fn face_detect_embed(items: usize) -> Duration {
        FACE_DETECT_EMBED_BASE + FACE_DETECT_EMBED_PER_ITEM * (items as u32)
    }
    /// EncodeText 一批(文本塔恒 CPU、查询通常单条,轻;30s 已是慢盘冷启余量)。
    pub const ENCODE_TEXT: Duration = Duration::from_secs(30);
}

/// 单插件运行描述(Part6 §3.3 C1/T13):调度循环按注册表逐项评估与运行,不再写死 PSD。
pub(crate) struct PluginDescriptor {
    pub plugin_id: String,
    /// 握手校验的 ReadyBody.worker_id 期望值。
    pub worker_id: String,
    /// 逐能力评估/运行(exotic_tasks 队列按 capability 列分流)。
    pub capabilities: Vec<Capability>,
    /// 进程握手超时(per-plugin;模型加载不在握手,ai/face 也保持 5s,D3)。
    pub handshake_timeout: Duration,
    /// 是否占 GPU 令牌(AppState.gpu_token):psd=false;ai/face descriptor 随 T15
    /// 加入时=true(发批前先 CPU permit 后 GPU 令牌,D2 顺序天条)。
    pub uses_gpu: bool,
}

/// 运行注册表(Part6 §3.3:Catalog + 运行时支持信息构建)。capabilities 取自
/// Catalog(权威);worker_id/uses_gpu 是运行时支持信息,Catalog 与插件 manifest
/// 尚无此数据(Part8 扩展 manifest 字段后改为全数据驱动)。ai/face worker 化
/// descriptor 随 T15 加入。新插件加入 = 注册表添一项,调度代码零改动。
fn plugin_descriptors(snap: &crate::exotic::catalog::CatalogSnapshot) -> Vec<PluginDescriptor> {
    let psd_caps = snap
        .resolve_format("psd")
        .map(|o| o.capabilities.clone())
        .unwrap_or_else(|| vec![Capability::Thumbnail]);
    vec![PluginDescriptor {
        plugin_id: PSD_PLUGIN_ID.to_string(),
        worker_id: PSD_WORKER_ID.to_string(),
        capabilities: psd_caps,
        handshake_timeout: HANDSHAKE_TIMEOUT,
        uses_gpu: false,
    }]
}

/// 事件通道容量（可合并；满时置 dirty 不丢 wake）。
const WAKE_CHANNEL_CAP: usize = 32;
/// 重试时钟周期。
const RETRY_TICK: Duration = Duration::from_secs(30);
/// 握手超时。
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// 唤醒原因。多数仅 informational（调度按统一 `wake` 处理）；但 [`WakeReason::UserRequested`]
/// 额外携带「用户显式请求」语义——`exotic_auto_process=false` 时只有它能触发运行（绕过 auto 门控）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeReason {
    Startup,
    ScanCommitted,
    CatalogBackfill,
    PluginInstalled,
    LicenseActivated,
    ConfigChanged,
    RetryDue,
    /// 用户显式要求处理（start_exotic_processing / retry 命令）。绕过 `exotic_auto_process` 门控。
    UserRequested,
}

/// Coordinator 句柄。`wake()` 只通知；实际调度在后台循环串行进行。
pub struct ExoticCoordinator {
    tx: mpsc::Sender<WakeReason>,
    dirty: Arc<AtomicBool>,
}

impl ExoticCoordinator {
    /// 启动后台调度循环 + 重试时钟，返回句柄。需在 tokio 运行时内调用（Tauri setup）。
    pub fn start(app: AppHandle, state: Arc<AppState>, host: Arc<ExoticHost>) -> Arc<Self> {
        let (tx, rx) = mpsc::channel::<WakeReason>(WAKE_CHANNEL_CAP);
        let dirty = Arc::new(AtomicBool::new(false));

        // 调度循环。
        {
            let app = app.clone();
            let state = Arc::clone(&state);
            let host = Arc::clone(&host);
            let dirty = Arc::clone(&dirty);
            tauri::async_runtime::spawn(async move {
                run_loop(app, state, host, rx, dirty).await;
            });
        }

        let handle = Arc::new(ExoticCoordinator {
            tx: tx.clone(),
            dirty,
        });

        // 重试时钟：周期发 RetryDue（到期 retryable 任务重新评估）。
        {
            let tx = tx.clone();
            tauri::async_runtime::spawn(async move {
                let mut ticker = tokio::time::interval(RETRY_TICK);
                ticker.tick().await; // 跳过立即触发
                loop {
                    ticker.tick().await;
                    if tx.send(WakeReason::RetryDue).await.is_err() {
                        break; // 接收端已关闭
                    }
                }
            });
        }

        handle
    }

    /// 幂等唤醒：通知调度循环重新评估。通道满时置 dirty（不丢失最后一次 wake）。
    pub fn wake(&self, reason: WakeReason) {
        match self.tx.try_send(reason) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                // 通道满：循环很快会处理；置 dirty 保证结束后补查一次。
                self.dirty.store(true, Ordering::SeqCst);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                debug!("exotic Coordinator 通道已关闭，wake 丢弃");
            }
        }
    }
}

/// 调度主循环：等 wake → 合并 → 运行直至无就绪 → 处理 dirty 补查。
async fn run_loop(
    app: AppHandle,
    state: Arc<AppState>,
    host: Arc<ExoticHost>,
    mut rx: mpsc::Receiver<WakeReason>,
    dirty: Arc<AtomicBool>,
) {
    info!("exotic Coordinator 启动");
    while let Some(reason) = rx.recv().await {
        debug!("exotic wake: {reason:?}");
        // 合并通道内其余 wake；本批只要有一个 UserRequested 即绕过 auto 门控（auto=false 时也运行）。
        let mut bypass_auto = reason == WakeReason::UserRequested;
        while let Ok(r) = rx.try_recv() {
            bypass_auto |= r == WakeReason::UserRequested;
        }
        dirty.store(false, Ordering::SeqCst);

        maybe_run_until_drained(&app, &state, &host, bypass_auto).await;

        // 运行期间被丢弃的 wake（通道满）→ 再补查一轮（沿用本批 bypass：通道满时无法重建其 reason）。
        if dirty.swap(false, Ordering::SeqCst) {
            maybe_run_until_drained(&app, &state, &host, bypass_auto).await;
        }
    }
    info!("exotic Coordinator 退出");
}

/// 反复运行 Pipeline 直至无就绪任务(解决尾部竞态:运行期间新增任务在本轮被消化)。
/// T13 通用化:按注册表逐 (插件, 能力) 调度,循环体不再写死 PSD;单调度循环串行,
/// 插件间天然互不并发。
async fn maybe_run_until_drained(
    app: &AppHandle,
    state: &Arc<AppState>,
    host: &Arc<ExoticHost>,
    bypass_auto: bool,
) {
    for desc in plugin_descriptors(&state.exotic_catalog.snapshot()) {
        for &capability in &desc.capabilities {
            run_capability_until_drained(app, state, host, &desc, capability, bypass_auto).await;
        }
    }
}

/// 单 (插件, 能力) 的 drained 循环(原 maybe_run_until_drained 主体参数化,T13)。
async fn run_capability_until_drained(
    app: &AppHandle,
    state: &Arc<AppState>,
    host: &Arc<ExoticHost>,
    desc: &PluginDescriptor,
    capability: Capability,
    bypass_auto: bool,
) {
    // T15 接缝:目前仅 thumbnail 能力有 pipeline 实装;embedding/face_detect_embed 的
    // 批派发随推理核心迁移(T15)落地——届时在此按能力分派 EmbedWorker 管线,并按
    // desc.uses_gpu 走「先 CPU permit 后 GPU 令牌」双取(D2)。
    if capability != Capability::Thumbnail {
        debug!(
            "{} 能力 {} 的 pipeline 未实装(T15),跳过",
            desc.plugin_id,
            capability.as_str()
        );
        return;
    }

    // Worker 定位 + 启动前完整性复核(Part3 §3.6):dev env 优先(不验签),否则对已装插件
    // 重新验签 manifest + 全文件 hash + 协议版本前置比对(P0-3,旧协议插件拒绝拉起、
    // 免付 spawn 代价),通过才返回路径。信任根解析失败 → 不运行(fail-closed)。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let worker_path = crate::exotic::trusted_keyset().ok().and_then(|ks| {
        crate::exotic::installer::resolve_worker_path(
            &state.exotic_install_dir(),
            &desc.plugin_id,
            &ks,
            now,
        )
    });
    let worker_available = worker_path.is_some();

    loop {
        // 门控判定（读配置 + 授权 + 就绪任务）。
        let should = {
            let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            evaluate_run(
                &conn,
                host,
                &desc.plugin_id,
                capability,
                worker_available,
                bypass_auto,
            )
        };
        if !should {
            break;
        }
        if state.is_exotic_running() {
            break; // 兜底：不应发生（单循环）
        }

        let Some(ref exe_path) = worker_path else {
            break;
        };

        // 启动唯一 Pipeline（token 入 AppState；stop 命令可取消）。
        info!(
            "启动 exotic Pipeline:{} cap={} uses_gpu={}",
            desc.plugin_id,
            capability.as_str(),
            desc.uses_gpu
        );
        let token = state.new_exotic_analysis_token();
        let app_run = app.clone();
        let state_run = Arc::clone(state);
        let exe_path = exe_path.clone();
        // §5.3 派发前授权复核：把 host 移入阻塞任务，供 Claimer 每批领取前调 is_task_runnable。
        let host_run = Arc::clone(host);
        let plugin_id = desc.plugin_id.clone();
        let worker_id = desc.worker_id.clone();
        let handshake_timeout = desc.handshake_timeout;

        let result = tokio::task::spawn_blocking(move || {
            let factory = SupervisorFactory {
                spec: WorkerSpec {
                    exe_path,
                    expected_worker_id: worker_id,
                    required_capabilities: vec![capability.as_str().to_string()],
                },
                cfg: WorkerConfig {
                    handshake_timeout,
                    host_version: env!("CARGO_PKG_VERSION").to_string(),
                    max_blob_len: exotic_protocol::MAX_BLOB_LEN,
                },
            };
            let (cache_dir, requested_size) = {
                let cfg = state_run.thumb_config.read().unwrap();
                (cfg.cache_dir.clone(), cfg.size)
            };
            let app_evt = app_run.clone();
            let state_yield = Arc::clone(&state_run);
            let deps = PipelineDeps {
                writer: &state_run.db_writer,
                limiter: &state_run.background_heavy_limiter,
                token: &token,
                items_cache: &state_run.layout_items_cache,
                cache_dir,
                requested_size,
                plugin_id: plugin_id.clone(),
                on_progress: Arc::new(move || {
                    // 合并发：画廊刷新（复用 enrichment 事件）+ 状态变化。
                    let _ = app_evt.emit(
                        "db:media_enriched",
                        crate::scanner::enricher::MediaEnrichedPayload {
                            root_id: 0,
                            enriched_count: 0,
                            total: 0,
                        },
                    );
                    let _ = app_evt.emit("exotic:status-changed", ());
                }),
                should_yield: Arc::new(move || state_yield.should_yield_exotic()),
                is_runnable: Arc::new(move || host_run.is_task_runnable(&plugin_id, capability)),
            };
            run_exotic_pipeline_blocking(&deps, &factory)
        })
        .await;

        // 清理 token 槽（run 已结束）。
        state.cancel_exotic_analysis();

        match result {
            Ok(stats) => {
                info!(
                    "exotic Pipeline 完成：done={} retried={} terminal={} lease_lost={} circuit={}",
                    stats.done,
                    stats.retried,
                    stats.terminal,
                    stats.lease_lost,
                    stats.circuit_opened
                );
                let _ = app.emit("exotic:status-changed", ());
                if stats.circuit_opened {
                    break; // 熔断 → 本轮停止，等用户修复/升级
                }
                // 尾部竞态：循环再查 pending（含运行期间新增）。无就绪即 evaluate_run 返回 false → break。
            }
            Err(e) => {
                warn!("exotic Pipeline 任务 panic：{e}");
                break;
            }
        }
    }
}

/// 门控判定（可单测）：是否应启动 Pipeline。
///
/// 顺序：Worker 可用 → 子系统启用 → 未暂停 → auto 门控 → 插件可领取(授权+平台+能力) → 有就绪任务。
/// `bypass_auto`：本批含用户显式请求（start/retry）时为 true，绕过 `exotic_auto_process` 门控；
/// 自动 wake（扫描/重试时钟/启动）为 false，`exotic_auto_process=false` 时不运行。
pub(crate) fn evaluate_run(
    conn: &Connection,
    host: &ExoticHost,
    plugin_id: &str,
    capability: Capability,
    worker_available: bool,
    bypass_auto: bool,
) -> bool {
    if !worker_available {
        return false;
    }
    let enabled = q::get_config(conn, "exotic_enabled")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);
    let paused = q::get_config(conn, "exotic_paused")
        .ok()
        .flatten()
        .map(|v| v == "true")
        .unwrap_or(false);
    if !enabled || paused {
        return false;
    }
    // auto 门控：关闭自动处理后，仅用户显式请求（start/retry）可运行；自动 wake 一律不跑（P2）。
    let auto = q::get_config(conn, "exotic_auto_process")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true);
    if !auto && !bypass_auto {
        return false;
    }
    if !host.is_task_runnable(plugin_id, capability) {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // 🔴 第 8 轮核验 P1-4：用传入的 `capability` 而非硬编码 CAPABILITY_STR("thumbnail")——
    // 否则 AI/face 等多能力插件会查错任务类型（has_ready_exotic_task 按 capability 列硬过滤）。
    // 现行调用方均传 Capability::Thumbnail，故 PSD 路径行为不变（capability.as_str()=="thumbnail"）。
    q::has_ready_exotic_task(conn, plugin_id, capability.as_str(), now).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exotic::catalog::CatalogStore;

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c
    }

    /// 平台无关 Catalog 夹具(2026-07-05 Linux CI 面):本组测试验证 coordinator 的
    /// 运行条件逻辑(授权/任务/暂停/自动开关),**不验证平台门控**——内置 Catalog 的
    /// PSD 平台清单不含 Linux,在 ubuntu CI 上「应当运行」断言会因 UnsupportedPlatform
    /// 假红,负向断言则因错误的理由通过(测不到本要测的条件)。注入「支持当前平台」的
    /// 最小 Catalog 解耦;平台门控维度由 availability/catalog 自有测试覆盖。
    fn test_catalog() -> Arc<CatalogStore> {
        let json = format!(
            r#"{{"schema":1,"sequence":1,"offerings":[{{
                "plugin_id":"exotic-image-psd","name":"PSD 图像引擎","media_kind":"image",
                "formats":["psd"],"capabilities":["thumbnail"],"license_tier":"paid",
                "sku":"psd-engine-2026","platforms":["{}"],"min_host_version":"0.1.0",
                "override_common":false,"store_url":"https://example.invalid/plugins/psd"}}]}}"#,
            crate::exotic::current_target_triple()
        );
        Arc::new(CatalogStore::with_snapshot(
            crate::exotic::catalog::CatalogSnapshot::parse(&json).unwrap(),
        ))
    }

    fn authorized_host() -> ExoticHost {
        ExoticHost::with_authorized_fixture(test_catalog(), PSD_PLUGIN_ID)
    }

    fn unauthorized_host() -> ExoticHost {
        ExoticHost::new(test_catalog())
    }

    #[test]
    fn plugin_registry_from_builtin_catalog() {
        // 注册表(T13):capabilities 来自内置 Catalog;PSD 不占 GPU、握手 5s。
        let store = CatalogStore::from_builtin().unwrap();
        let descs = plugin_descriptors(&store.snapshot());
        assert_eq!(descs.len(), 1);
        let d = &descs[0];
        assert_eq!(d.plugin_id, PSD_PLUGIN_ID);
        assert_eq!(d.worker_id, "psd-worker");
        assert_eq!(d.capabilities, vec![Capability::Thumbnail]);
        assert!(!d.uses_gpu);
        assert_eq!(d.handshake_timeout, HANDSHAKE_TIMEOUT);
    }

    #[test]
    fn op_timeout_table_invariants() {
        // 表内不变量(D3):进程握手(5s,快失败)≪ thumbnail ≪ 批 ≤ SessionInit(冷加载上界)。
        assert!(HANDSHAKE_TIMEOUT < op_timeouts::THUMBNAIL);
        assert!(op_timeouts::THUMBNAIL < op_timeouts::EMBED_BATCH);
        assert!(op_timeouts::EMBED_BATCH <= op_timeouts::SESSION_INIT);
        // face 批超时按项数缩放(2026-07-03):单项不低于 thumbnail 档;
        // 派发上限 16 项(ai::face_pipeline::FACE_DISPATCH_BATCH)时不超 SessionInit。
        assert!(op_timeouts::face_detect_embed(1) >= op_timeouts::THUMBNAIL);
        assert!(op_timeouts::face_detect_embed(16) <= op_timeouts::SESSION_INIT);
        // T17 新档:文本编码(CPU 轻)不重于图像批;SessionClose 远小于 SessionInit。
        assert!(op_timeouts::ENCODE_TEXT <= op_timeouts::EMBED_BATCH);
        assert!(op_timeouts::SESSION_CLOSE < op_timeouts::SESSION_INIT);
    }

    #[test]
    fn no_run_without_worker() {
        let c = mem_db();
        q::seed_exotic_tasks_for_item(&c, 1, PSD_PLUGIN_ID, &["thumbnail".into()]).unwrap();
        // worker 不可用 → 不跑，即使其他条件满足。
        assert!(!evaluate_run(
            &c,
            &authorized_host(),
            PSD_PLUGIN_ID,
            CAPABILITY,
            false,
            false
        ));
    }

    #[test]
    fn no_run_when_unauthorized() {
        let c = mem_db();
        q::seed_exotic_tasks_for_item(&c, 1, PSD_PLUGIN_ID, &["thumbnail".into()]).unwrap();
        // 未授权（无 fixture）→ 不跑（Part2：需 License/Part3）。
        assert!(!evaluate_run(
            &c,
            &unauthorized_host(),
            PSD_PLUGIN_ID,
            CAPABILITY,
            true,
            false
        ));
    }

    #[test]
    fn no_run_when_paused() {
        let c = mem_db();
        q::seed_exotic_tasks_for_item(&c, 1, PSD_PLUGIN_ID, &["thumbnail".into()]).unwrap();
        q::set_config(&c, "exotic_paused", "true").unwrap();
        assert!(!evaluate_run(
            &c,
            &authorized_host(),
            PSD_PLUGIN_ID,
            CAPABILITY,
            true,
            false
        ));
    }

    #[test]
    fn no_run_when_no_ready_task() {
        let c = mem_db();
        // 无任务 → 不跑（避免空转）。
        assert!(!evaluate_run(
            &c,
            &authorized_host(),
            PSD_PLUGIN_ID,
            CAPABILITY,
            true,
            false
        ));
    }

    #[test]
    fn runs_when_all_conditions_met() {
        let c = mem_db();
        q::seed_exotic_tasks_for_item(&c, 1, PSD_PLUGIN_ID, &["thumbnail".into()]).unwrap();
        assert!(evaluate_run(
            &c,
            &authorized_host(),
            PSD_PLUGIN_ID,
            CAPABILITY,
            true,
            false
        ));
    }

    #[test]
    fn disabled_subsystem_blocks_run() {
        let c = mem_db();
        q::seed_exotic_tasks_for_item(&c, 1, PSD_PLUGIN_ID, &["thumbnail".into()]).unwrap();
        q::set_config(&c, "exotic_enabled", "false").unwrap();
        assert!(!evaluate_run(
            &c,
            &authorized_host(),
            PSD_PLUGIN_ID,
            CAPABILITY,
            true,
            false
        ));
    }

    #[test]
    fn auto_disabled_blocks_automatic_wake() {
        let c = mem_db();
        q::seed_exotic_tasks_for_item(&c, 1, PSD_PLUGIN_ID, &["thumbnail".into()]).unwrap();
        q::set_config(&c, "exotic_auto_process", "false").unwrap();
        // 自动 wake（bypass_auto=false）→ 不跑（P2）。
        assert!(!evaluate_run(
            &c,
            &authorized_host(),
            PSD_PLUGIN_ID,
            CAPABILITY,
            true,
            false
        ));
    }

    #[test]
    fn auto_disabled_allows_user_request() {
        let c = mem_db();
        q::seed_exotic_tasks_for_item(&c, 1, PSD_PLUGIN_ID, &["thumbnail".into()]).unwrap();
        q::set_config(&c, "exotic_auto_process", "false").unwrap();
        // 用户显式请求（start/retry → bypass_auto=true）→ 仍运行。
        assert!(evaluate_run(
            &c,
            &authorized_host(),
            PSD_PLUGIN_ID,
            CAPABILITY,
            true,
            true
        ));
    }
}
