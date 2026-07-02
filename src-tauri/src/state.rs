// src-tauri/src/state.rs
// src-tauri/src/state.rs
//! Application state shared across all Tauri commands.
//! 在所有 Tauri 命令之间共享的应用程序状态。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use crate::ai::engine::AiEnginePool;
use crate::ai::search::EmbeddingCache;
use crate::db::{DbPool, DbWriter};
use crate::engine::EngineArena;
use crate::exotic::CatalogStore;
use crate::layout::cache::new_layout_cache;
use crate::layout::LayoutCache;
use crate::thumbnail::generator::ThumbConfig;

/// Global application state.
/// 全局应用程序状态。
pub struct AppState {
    /// Write connection — serialised via Mutex.
    /// 写入连接 — 通过 Mutex 序列化。
    pub db_writer: DbWriter,

    /// Read connection pool (WAL concurrent reads).
    /// 读取连接池（WAL 并发读取）。
    pub db_read_pool: DbPool,

    /// Per-root cancellation tokens for scan operations.
    /// 用于扫描操作的每个根目录的取消令牌。
    pub scan_tokens: Mutex<HashMap<i64, CancellationToken>>,

    /// In-memory Justified Layout cache.
    /// 内存中的两端对齐布局缓存。
    pub layout_cache: LayoutCache,

    /// Image engine arena (format → engine dispatch).
    /// 图像引擎容器（格式 → 引擎分发）。
    pub engine_arena: EngineArena,

    /// 冷门格式能力目录（内置 + 远程合并的只读快照）。扫描分类与缩略图路由的「能力真相」。
    /// 持 `Arc<CatalogStore>`，热路径经 `.snapshot()` 取 `Arc<CatalogSnapshot>`（一次读锁）。
    pub exotic_catalog: std::sync::Arc<CatalogStore>,

    /// Thumbnail configuration (cache dir, size, skip threshold).
    /// 缩略图配置（缓存目录、大小、跳过阈值）。
    pub thumb_config: RwLock<ThumbConfig>,

    /// Cancellation token for full thumbnail generation task.
    /// 全量缩略图生成任务的取消令牌。
    pub thumb_gen_token: Mutex<Option<CancellationToken>>,

    /// Cancelled thumbnail item IDs for viewport scrolling aborts.
    pub cancelled_thumb_ids: Mutex<HashSet<i64>>,

    /// Resolved log directory path.
    pub log_dir: PathBuf,

    /// 冷门格式插件数据根（`<app_data>/exotic`）。子目录：plugins(已装)/staging(解包)/registry(签名缓存)。
    /// 安装/卸载/修复/list_registry 命令据此定位（Part3 §6.4）。
    pub exotic_dir: PathBuf,

    /// 安装/卸载/回滚命令互斥锁（安全评审 medium）：这些命令做 quiesce + 目录原子切换，并发执行会
    /// 在 backup 清理/rename 之间产生破损窗口(无 current)。串行化保证同一时刻只一个目录变更操作。
    pub exotic_install_lock: tokio::sync::Mutex<()>,

    /// AI inference engine pool (lazily initialised, None until first use).
    /// AI 推理引擎池（懒加载，首次使用前为 None）。
    pub ai_engine: RwLock<Option<AiEnginePool>>,

    /// Resident half-precision embedding cache for semantic search (C1).
    /// Loaded once from SQLite, reused across queries, invalidated on embedding writes.
    /// 语义搜索的常驻半精度嵌入缓存（C1）。一次性从 SQLite 加载，跨查询复用，写入时失效。
    pub ai_embedding_cache: RwLock<Option<EmbeddingCache>>,

    /// Cancellation token for the background AI analysis pipeline.
    /// 后台 AI 分析流水线的取消令牌。
    pub ai_analysis_token: Mutex<Option<CancellationToken>>,

    /// Cancellation token for the background face-recognition pipeline (F3). Mirrors
    /// `ai_analysis_token` 1:1; presence = running.
    /// 后台人脸识别流水线（F3）的取消令牌。与 `ai_analysis_token` 一一对应；存在即代表正在运行。
    pub face_analysis_token: Mutex<Option<CancellationToken>>,

    /// Single-owner gate for the one GPU-analysis slot shared by CLIP and face (F5). CLIP and
    /// face both saturate GPU/VRAM, so only ONE may run at a time. `None` = idle; `Some("ai")` /
    /// `Some("face")` = that pipeline holds the slot. Check-and-claim happens atomically under
    /// this single lock (`try_acquire_gpu_analysis`), which is why a plain two-token cross-check
    /// would NOT suffice — two starts on two separate token mutexes can both observe `None` and
    /// both launch (TOCTOU); the App.vue startup that fires `ai`+`face` auto-resume without an
    /// await between them is exactly that race. The per-pipeline cancellation tokens above are
    /// kept for stop/pause; this gate is a separate concern (mutual exclusion).
    /// CLIP 与人脸共用的唯一 GPU 分析槽的单一持有者门闩（F5）。两者都吃满 GPU/显存，同一时刻
    /// 只能跑一条。`None`=空闲；`Some("ai")`/`Some("face")`=该流水线持有槽位。check-and-claim 在
    /// 这一把锁下原子完成（`try_acquire_gpu_analysis`）——这正是「两个独立 token 交叉检查」不够
    /// 的原因：两次启动各查各的 token mutex，可能都见 `None` 都启动（TOCTOU）；App.vue 启动时
    /// 不 await 地连发 `ai`+`face` 两个自动续传就是这个 race。上面的 per-pipeline 取消令牌用于
    /// 停止/暂停；本门闩是另一回事（互斥）。
    pub gpu_analysis_owner: Mutex<Option<&'static str>>,

    /// Cancellation token for the background derivation pipeline (video cover / keyframes /
    /// doc thumbnail / audio cover & meta). Presence = running, mirroring the AI token.
    /// 后台派生流水线（视频封面/关键帧、文档缩略图、音频封面与元数据）的取消令牌。
    /// 存在即代表正在运行，与 AI 令牌同构。
    pub derivation_token: Mutex<Option<CancellationToken>>,

    /// Cancellation token for the background exotic (冷门格式插件) processing pipeline (R1).
    /// Presence = running. AI/face yield to it via `ai_yield_blockers`; exotic itself yields to
    /// scan/thumbnail/interaction via `should_yield_exotic` (NOT to derivation — they are peers
    /// sharing a fair background-heavy pool, R4).
    /// 后台冷门格式处理流水线（Part2）的取消令牌（R1）。存在即运行。AI/人脸经 `ai_yield_blockers`
    /// 让步给它；exotic 自身经 `should_yield_exotic` 让步给扫描/缩略图/交互（**不**让步 derivation
    /// ——二者同级、共享公平后台重活池，R4）。
    /// 访问器一律 `into_inner` 毒锁恢复(R2-6,与 exotic 子系统契约一致,见 ai/pipeline.rs
    /// 问题6):coordinator 循环须在 Pipeline panic 后存活,运行态判定不得因毒锁级联 panic。
    pub exotic_analysis_token: Mutex<Option<CancellationToken>>,

    /// Fair background-heavy concurrency pool shared by derivation and exotic (R4). Both subsystems
    /// acquire a permit from THIS limiter before a heavy task, so持续派生不会饿死 exotic（FIFO 公平，
    /// 等待有上界）。预算 = `available_parallelism()`：exotic 空闲时 derivation 不受影响；二者并发时
    /// 共享同一全局预算、按到达顺序公平交错。
    /// 由 derivation 与 exotic 两条流水线共享的公平后台重活池（R4）。
    pub background_heavy_limiter: std::sync::Arc<crate::exotic::limiter::BackgroundHeavyLimiter>,

    /// exotic Coordinator 句柄（setup 内创建后写入；扫描/命令经此 wake 调度）。用 `OnceLock` 因为
    /// Coordinator 需 `AppHandle`（setup 才有），晚于 AppState 构造；一次写入、多处只读。
    /// exotic 调度器句柄（晚绑定）。
    pub exotic_coordinator:
        std::sync::OnceLock<std::sync::Arc<crate::exotic::coordinator::ExoticCoordinator>>,

    /// Instant captured right after AppState::new() returns.
    /// Used to measure time-to-first-frame (AppState init → main window visible).
    ///
    /// AppState::new() 返回后立即记录的时间点。
    /// 用于测量从初始化完成到主界面弹出的耗时。
    pub startup_instant: Instant,

    /// Unix-millis deadline until which the user is treated as "actively interacting"
    /// (relayout / scroll). Background derivation & AI throttle while this is in the future, so a
    /// foreground `compute_layout` isn't starved of CPU by heavy video decode (布局被视频任务阻塞).
    /// Updated by the layout IPCs; read by the yield checks. 0 = never interacted.
    /// 「用户正在主动交互」（重排/滚动）的截止时刻（unix 毫秒）。在其到期前，后台派生与 AI 节流，
    /// 使前台 `compute_layout` 不被重型视频解码饿死。由布局 IPC 更新、让步检查读取。0 = 从未交互。
    pub interactive_until_ms: AtomicI64,
}

/// How long after an interactive layout op to keep throttling background decode (ms). Continuous
/// scrolling keeps refreshing this window; when the user stops, background work resumes full speed.
/// 一次交互布局操作后继续节流后台解码的时长（毫秒）。持续滚动会不断刷新该窗口；用户停手后后台恢复全速。
const INTERACTIVE_WINDOW_MS: i64 = 1500;

/// GPU-analysis gate owner tags (see `gpu_analysis_owner`). Constants, not literals, so a typo
/// can't silently break the CLIP↔face mutual exclusion.
/// GPU 分析门闩持有者标签（见 `gpu_analysis_owner`）。用常量而非字面量，避免拼写错误悄悄破坏
/// CLIP↔人脸互斥。
pub const GPU_OWNER_AI: &str = "ai";
pub const GPU_OWNER_FACE: &str = "face";

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl AppState {
    // 应用全局状态聚合构造，各依赖独立必需、无合理分组，沿用本仓库既有约定标注。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db_writer: DbWriter,
        db_read_pool: DbPool,
        cache_dir: PathBuf,
        log_dir: PathBuf,
        exotic_dir: PathBuf,
        thumb_size: u32,
        thumb_skip_max_kb: u64,
        thumb_strategy: String,
        gpu_engine: String,
        ai_hq_cache: bool,
        exotic_catalog: std::sync::Arc<CatalogStore>,
    ) -> Self {
        Self {
            db_writer,
            db_read_pool,
            scan_tokens: Mutex::new(HashMap::new()),
            layout_cache: new_layout_cache(),
            engine_arena: EngineArena::phase1(),
            exotic_catalog,
            thumb_config: RwLock::new(ThumbConfig {
                cache_dir,
                size: thumb_size,
                skip_max_bytes: thumb_skip_max_kb * 1024,
                strategy: thumb_strategy,
                gpu_engine,
                ai_hq_cache,
            }),
            thumb_gen_token: Mutex::new(None),
            cancelled_thumb_ids: Mutex::new(HashSet::new()),
            log_dir,
            exotic_dir,
            exotic_install_lock: tokio::sync::Mutex::new(()),
            ai_engine: RwLock::new(None),
            ai_embedding_cache: RwLock::new(None),
            ai_analysis_token: Mutex::new(None),
            face_analysis_token: Mutex::new(None),
            gpu_analysis_owner: Mutex::new(None),
            derivation_token: Mutex::new(None),
            exotic_analysis_token: Mutex::new(None),
            // 后台重活并发预算 = 可用并行度，**下限 2**（Part3 §3.5.2 / T12）。
            // 取 max(2)：单核/受限容器（available_parallelism()==1）下，若预算=1 则派生 dispatch 线程
            // 取走唯一额度后阻塞、rayon worker 空等且 exotic 完全饿死；保底 2 让派生与 exotic 至少能交错。
            // derivation 与 exotic 共享此池（R4）。
            background_heavy_limiter: crate::exotic::limiter::BackgroundHeavyLimiter::new(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
                    .max(2),
            ),
            exotic_coordinator: std::sync::OnceLock::new(),
            startup_instant: Instant::now(),
            interactive_until_ms: AtomicI64::new(0),
        }
    }

    /// Mark that the user just performed an interactive layout op (relayout / scroll), so
    /// background derivation/AI back off for a short window. Cheap (one relaxed atomic store).
    /// 标记用户刚进行了一次交互布局操作（重排/滚动），使后台派生/AI 在短窗口内退让。极廉价（一次原子写）。
    pub fn note_interaction(&self) {
        self.interactive_until_ms
            .store(now_millis() + INTERACTIVE_WINDOW_MS, Ordering::Relaxed);
    }

    /// Whether the user is actively interacting (within the throttle window).
    /// 用户是否正在主动交互（处于节流窗口内）。
    pub fn is_interactive(&self) -> bool {
        now_millis() < self.interactive_until_ms.load(Ordering::Relaxed)
    }

    /// Drop the resident embedding cache so the next semantic search reloads it.
    /// Called whenever embeddings are written or reset.
    /// 丢弃常驻嵌入缓存，使下次语义搜索重新加载。在嵌入向量写入或重置时调用。
    pub fn invalidate_embedding_cache(&self) {
        *self.ai_embedding_cache.write().unwrap() = None;
    }

    /// Create a new cancellation token for the AI analysis pipeline.
    /// 为 AI 分析流水线创建新的取消令牌。
    pub fn new_ai_analysis_token(&self) -> CancellationToken {
        let token = CancellationToken::new();
        *self.ai_analysis_token.lock().unwrap() = Some(token.clone());
        token
    }

    /// Cancel the AI analysis pipeline if running.
    /// 如果正在运行，取消 AI 分析流水线。
    pub fn cancel_ai_analysis(&self) {
        if let Some(token) = self.ai_analysis_token.lock().unwrap().take() {
            token.cancel();
        }
    }

    /// Create a new cancellation token for the face-recognition pipeline.
    /// 为人脸识别流水线创建新的取消令牌。
    pub fn new_face_analysis_token(&self) -> CancellationToken {
        let token = CancellationToken::new();
        *self.face_analysis_token.lock().unwrap() = Some(token.clone());
        token
    }

    /// Cancel the face-recognition pipeline if running.
    /// 如果正在运行，取消人脸识别流水线。
    pub fn cancel_face_analysis(&self) {
        if let Some(token) = self.face_analysis_token.lock().unwrap().take() {
            token.cancel();
        }
    }

    /// Atomically claim the single GPU-analysis slot for `owner` (`GPU_OWNER_AI` / `_FACE`).
    /// Returns `true` if the slot was idle (now claimed) or already held by `owner` (re-entrant —
    /// e.g. restart cancels then relaunches the same pipeline); `false` if the OTHER pipeline
    /// holds it. The check-and-set is one locked region, closing the TOCTOU window two separate
    /// token checks would leave open (see `gpu_analysis_owner`).
    /// 原子地为 `owner`（`GPU_OWNER_AI`/`_FACE`）占用唯一 GPU 分析槽。空闲（占用成功）或已由
    /// `owner` 持有（可重入——如 restart 先取消再重启同一流水线）返回 `true`；被**对方**持有返回
    /// `false`。check-and-set 在单一锁区内完成，堵住两个独立 token 检查会留下的 TOCTOU 窗口
    /// （见 `gpu_analysis_owner`）。
    pub fn try_acquire_gpu_analysis(&self, owner: &'static str) -> bool {
        let mut slot = self.gpu_analysis_owner.lock().unwrap();
        match *slot {
            None => {
                *slot = Some(owner);
                true
            }
            Some(cur) => cur == owner,
        }
    }

    /// Release the GPU-analysis slot, but only if `owner` still holds it (a no-op otherwise, so a
    /// stale completion handler can't release a slot a newer run re-claimed).
    /// 释放 GPU 分析槽，但仅当 `owner` 仍持有时（否则为空操作，使过期的完成回调不会释放更新一次
    /// 运行已重新占用的槽）。
    pub fn release_gpu_analysis(&self, owner: &'static str) {
        let mut slot = self.gpu_analysis_owner.lock().unwrap();
        if *slot == Some(owner) {
            *slot = None;
        }
    }

    /// Tiered priority ladder (high → low): scan > thumbnail > derivation > AI.
    /// 分级优先级阶梯（高 → 低）：扫描 > 缩略图 > 派生 > AI。
    ///
    /// Each lower tier yields (sleeps) while ANY higher tier is active, so the
    /// foreground-critical work (scan/thumb) is never starved by background derivation/AI.
    /// 每个低优先级层在任一高优先级层活动时让步（sleep），使前台关键工作（扫描/缩略图）
    /// 不会被后台派生/AI 抢占。
    ///
    /// True if scan or thumbnail generation is running.
    /// 扫描或缩略图生成是否正在运行。
    fn is_scan_or_thumb_running(&self) -> bool {
        !self.scan_tokens.lock().unwrap().is_empty()
            || self.thumb_gen_token.lock().unwrap().is_some()
    }

    /// Whether the derivation pipeline is currently running (token present).
    /// 派生流水线当前是否正在运行（令牌存在）。
    pub fn is_derivation_running(&self) -> bool {
        self.derivation_token.lock().unwrap().is_some()
    }

    /// Should the **AI** pipeline yield? AI is the lowest tier — it yields to scan, thumbnail,
    /// derivation AND active user interaction (so browsing stays responsive).
    /// **AI** 流水线是否应让步？AI 是最低层 —— 让步给扫描、缩略图、派生与用户主动交互（保持浏览流畅）。
    pub fn ai_yield_blockers(&self) -> Vec<&'static str> {
        let mut blockers = Vec::new();

        // 这里返回具体阻塞源，避免 AI 只能反复打印“高优先级任务”却看不出是谁。
        if !self.scan_tokens.lock().unwrap().is_empty() {
            blockers.push("scan");
        }
        if self.thumb_gen_token.lock().unwrap().is_some() {
            blockers.push("thumbnail");
        }
        if self.is_derivation_running() {
            blockers.push("derivation");
        }
        // R1：exotic 活动时 AI/人脸让步给它（exotic 优先级高于 AI/face）。逐把锁 → 读 → drop，
        // 不与上面的 token 锁同时持有，避免锁顺序反转。
        if self
            .exotic_analysis_token
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
        {
            blockers.push("exotic");
        }
        if self.is_interactive() {
            blockers.push("interaction");
        }

        blockers
    }

    pub fn should_yield_to_higher_priority(&self) -> bool {
        !self.ai_yield_blockers().is_empty()
    }

    /// Should the **derivation** pipeline yield? Derivation sits above AI but below scan/thumbnail
    /// — it yields to scan, thumbnail generation, AND active user interaction. The last one is key:
    /// heavy video cover/keyframe decode otherwise starves the foreground `compute_layout` of CPU
    /// (布局被视频派生阻塞). Continuous browsing keeps the throttle window alive; it lifts on idle.
    /// **派生** 流水线是否应让步？派生位于 AI 之上、扫描/缩略图之下 —— 让步给扫描、缩略图生成与用户
    /// 主动交互。最后一项是关键：否则重型视频封面/关键帧解码会把前台 `compute_layout` 的 CPU 饿死
    /// （布局被视频派生阻塞）。持续浏览保持节流窗口；空闲即解除。
    pub fn should_yield_derivation(&self) -> bool {
        self.is_scan_or_thumb_running() || self.is_interactive()
    }

    /// Create a new cancellation token for the derivation pipeline.
    /// 为派生流水线创建一个新的取消令牌。
    pub fn new_derivation_token(&self) -> CancellationToken {
        let token = CancellationToken::new();
        *self.derivation_token.lock().unwrap() = Some(token.clone());
        token
    }

    /// Cancel the derivation pipeline if running.
    /// 如果正在运行，取消派生流水线。
    pub fn cancel_derivation(&self) {
        if let Some(token) = self.derivation_token.lock().unwrap().take() {
            token.cancel();
        }
    }

    /// Should the **exotic** pipeline yield before dispatching a NEW task? (R1/R4)
    ///
    /// exotic 解码发生在 Worker **子进程**——主进程线程 sleep 无法令子进程让出 CPU，故「让步」
    /// 不是 sleep 抢占，而是：① Claimer/Dispatcher 派发**新任务前**用本判断暂缓领取；
    /// ② Worker 子进程以低优先级创建（OS 软让步，见 Part2 §3.6）。已派发的在途解码不可中断让步，
    /// 只能自然完成或超时 kill。
    ///
    /// 让步集 = scan / thumbnail / interaction。**不含** derivation：exotic 与 derivation 同级、
    /// 共享公平后台重活池（R4）；若硬让步 derivation，大视频库下 derivation 长期运行会饿死 exotic。
    /// 同一 item 不会被两者同时处理（exotic 认领的格式不进主派生），故同级不产生同 item 互等。
    pub fn should_yield_exotic(&self) -> bool {
        self.is_scan_or_thumb_running() || self.is_interactive()
    }

    /// Whether the exotic pipeline is currently running (token present).
    /// 冷门格式流水线当前是否正在运行（令牌存在）。
    pub fn is_exotic_running(&self) -> bool {
        self.exotic_analysis_token
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// 绑定 exotic Coordinator（setup 内一次性写入）。
    pub fn set_exotic_coordinator(
        &self,
        c: std::sync::Arc<crate::exotic::coordinator::ExoticCoordinator>,
    ) {
        let _ = self.exotic_coordinator.set(c);
    }

    /// 构造运行期 [`crate::exotic::ExoticHost`]：组合 catalog（能力真相）+ 只读连接池（安装真相）+ keyring（授权真相）。
    /// 廉价（Arc/Pool clone），命令与调度按需新建、不缓存——确保读到最新
    /// 安装/授权状态（安装、激活后立即生效）。
    pub fn exotic_host(&self) -> crate::exotic::ExoticHost {
        // 授权 provider 经组合根单点装配（Part6 §3.9.1a ①）：开源＝keyring 直销；商业构建把
        // default_entitlement_provider 换成 DirectEntitlement（③b）。for_runtime 本身对渠道无知。
        crate::exotic::ExoticHost::for_runtime(
            self.exotic_catalog.clone(),
            self.db_read_pool.clone(),
            crate::exotic::default_entitlement_provider(),
        )
    }

    /// 命令层取授权 provider 的统一入口（审查 R1-1）：激活 / 撤销与 evaluate 全走
    /// [`crate::exotic::default_entitlement_provider`] 同一 swap 点装配，消除「evaluate 走注入
    /// provider、activate 直构 KeyringLicenseStore」的信任根分裂——③b 商业构建换 swap 点函数体，
    /// 全部授权路径即一并切换。与 [`Self::exotic_host`] 同理按需新建、不缓存（廉价，读到最新态）。
    pub fn entitlement_provider(&self) -> std::sync::Arc<dyn crate::exotic::EntitlementProvider> {
        crate::exotic::default_entitlement_provider()
    }

    /// 已装插件根目录（`<exotic_dir>/plugins`；各插件装到其下 `<plugin_id>`）。
    pub fn exotic_install_dir(&self) -> std::path::PathBuf {
        self.exotic_dir.join("plugins")
    }
    /// 解包暂存根（`<exotic_dir>/staging`）。
    pub fn exotic_staging_dir(&self) -> std::path::PathBuf {
        self.exotic_dir.join("staging")
    }
    /// 签名 Registry 本地缓存目录（`<exotic_dir>/registry`）。
    pub fn exotic_registry_dir(&self) -> std::path::PathBuf {
        self.exotic_dir.join("registry")
    }

    /// 静默 exotic 子系统以便替换/移除安装目录（§6.4 第 9 步前置）：置 paused 阻止新一轮启动、
    /// 取消在途 Pipeline（Supervisor kill→wait Worker，释放 exe 句柄），轮询直至不再运行或超时。
    /// 返回 `(prev_paused, quiesced)`：`prev_paused`=操作前 paused 原值（据此 [`Self::resume_after_quiesce`]
    /// 恢复）；`quiesced`=是否在超时内真正停住（false=Pipeline 仍在跑）。**调用方必须检查 `quiesced`**——
    /// 为 false 时**不得**执行目录 rename/删除（Windows 下被占用的 worker.exe 无法改名，强行操作会留破损态，
    /// 安全评审 medium），应 resume 后向前端报错。
    /// **必须**在原子切换/删目录前调用。
    pub async fn quiesce_exotic(&self, timeout: std::time::Duration) -> (bool, bool) {
        let prev_paused = {
            let conn = self.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            let p = crate::db::queries::get_config(&conn, "exotic_paused")
                .ok()
                .flatten()
                .map(|v| v == "true")
                .unwrap_or(false);
            // 先置 paused：evaluate_run 见 paused 即不再启动新一轮（堵住取消后立即重启的竞态）。
            let _ = crate::db::queries::set_config(&conn, "exotic_paused", "true");
            p
        };
        self.cancel_exotic_analysis(); // 取消在途 → Supervisor kill→wait
        let start = Instant::now();
        while self.is_exotic_running() && start.elapsed() < timeout {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let quiesced = !self.is_exotic_running();
        if !quiesced {
            tracing::warn!("quiesce_exotic 超时：Pipeline 仍在运行，拒绝执行目录操作");
        }
        (prev_paused, quiesced)
    }

    /// 恢复 quiesce 前的 paused 状态并唤醒（安装/卸载完成后）。
    pub fn resume_after_quiesce(&self, prev_paused: bool) {
        {
            let conn = self.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            let _ = crate::db::queries::set_config(
                &conn,
                "exotic_paused",
                if prev_paused { "true" } else { "false" },
            );
        }
        self.wake_exotic(crate::exotic::coordinator::WakeReason::ConfigChanged);
    }

    /// 幂等唤醒 exotic 调度（Coordinator 未绑定则静默忽略）。扫描提交/命令/配置变更后调用。
    pub fn wake_exotic(&self, reason: crate::exotic::coordinator::WakeReason) {
        if let Some(c) = self.exotic_coordinator.get() {
            c.wake(reason);
        }
    }

    /// Create a new cancellation token for the exotic pipeline.
    /// 为冷门格式流水线创建一个新的取消令牌。
    pub fn new_exotic_analysis_token(&self) -> CancellationToken {
        let token = CancellationToken::new();
        *self
            .exotic_analysis_token
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
        token
    }

    /// Cancel the exotic pipeline if running.
    /// 如果正在运行，取消冷门格式流水线。
    pub fn cancel_exotic_analysis(&self) {
        if let Some(token) = self
            .exotic_analysis_token
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            token.cancel();
        }
    }

    /// Create a new cancellation token for a scan root, replacing any existing one.
    /// 为扫描根目录创建一个新的取消令牌，替换任何现有的令牌。
    pub fn new_scan_token(&self, root_id: i64) -> CancellationToken {
        let token = CancellationToken::new();
        self.scan_tokens
            .lock()
            .unwrap()
            .insert(root_id, token.clone());
        token
    }

    /// Cancel the scan token for a root, if it exists.
    /// 取消根目录的扫描令牌（如果存在）。
    pub fn cancel_scan(&self, root_id: i64) {
        if let Some(token) = self.scan_tokens.lock().unwrap().remove(&root_id) {
            token.cancel();
        }
    }

    /// Cancel all running scans.
    /// 取消所有正在运行的扫描。
    pub fn cancel_all_scans(&self) {
        let mut map = self.scan_tokens.lock().unwrap();
        for token in map.values() {
            token.cancel();
        }
        map.clear();
    }

    /// Create a new cancellation token for full thumbnail generation.
    /// 为全量缩略图生成创建一个新的取消令牌。
    pub fn new_thumb_gen_token(&self) -> CancellationToken {
        let token = CancellationToken::new();
        *self.thumb_gen_token.lock().unwrap() = Some(token.clone());
        token
    }

    /// Cancel the full thumbnail generation task if running.
    /// 如果正在运行，取消全量缩略图生成任务。
    pub fn cancel_thumb_gen(&self) {
        if let Some(token) = self.thumb_gen_token.lock().unwrap().take() {
            token.cancel();
        }
    }
}
