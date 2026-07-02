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
const CAPABILITY: Capability = Capability::Thumbnail;
const CAPABILITY_STR: &str = "thumbnail";

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

/// 反复运行 Pipeline 直至无就绪任务（解决尾部竞态：运行期间新增任务在本轮被消化）。
async fn maybe_run_until_drained(
    app: &AppHandle,
    state: &Arc<AppState>,
    host: &Arc<ExoticHost>,
    bypass_auto: bool,
) {
    // Worker 定位 + 启动前完整性复核（Part3 §3.6）：dev env 优先（不验签），否则对已装插件
    // 重新验签 manifest + 全文件 hash，通过才返回路径。信任根解析失败 → 不运行（fail-closed）。
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let worker_path = crate::exotic::crypto::VerifyingKeyset::builtin()
        .ok()
        .and_then(|ks| {
            crate::exotic::installer::resolve_worker_path(
                &state.exotic_install_dir(),
                PSD_PLUGIN_ID,
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
                PSD_PLUGIN_ID,
                CAPABILITY,
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
        let token = state.new_exotic_analysis_token();
        let app_run = app.clone();
        let state_run = Arc::clone(state);
        let exe_path = exe_path.clone();
        // §5.3 派发前授权复核：把 host 移入阻塞任务，供 Claimer 每批领取前调 is_task_runnable。
        let host_run = Arc::clone(host);

        let result = tokio::task::spawn_blocking(move || {
            let factory = SupervisorFactory {
                spec: WorkerSpec {
                    exe_path,
                    expected_worker_id: PSD_WORKER_ID.to_string(),
                    required_capabilities: vec![CAPABILITY_STR.to_string()],
                },
                cfg: WorkerConfig {
                    handshake_timeout: HANDSHAKE_TIMEOUT,
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
                layout_cache: &state_run.layout_cache,
                cache_dir,
                requested_size,
                plugin_id: PSD_PLUGIN_ID.to_string(),
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
                is_runnable: Arc::new(move || host_run.is_task_runnable(PSD_PLUGIN_ID, CAPABILITY)),
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

    fn authorized_host() -> ExoticHost {
        let store = Arc::new(CatalogStore::from_builtin().unwrap());
        ExoticHost::with_authorized_fixture(store, PSD_PLUGIN_ID)
    }

    fn unauthorized_host() -> ExoticHost {
        let store = Arc::new(CatalogStore::from_builtin().unwrap());
        ExoticHost::new(store)
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
