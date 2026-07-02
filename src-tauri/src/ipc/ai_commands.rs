// src-tauri/src/ipc/ai_commands.rs
//! IPC commands for AI inference engine management and semantic search.
//! AI 推理引擎管理和语义搜索的 IPC 命令。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;
use tracing::{info, warn};

use crate::ai::engine::AiEnginePool;
use crate::ai::pipeline::start_ai_pipeline;
use crate::ai::profile::{self, ModelProfile};
use crate::ai::remote_registry::{self, BatchKind};
use crate::ai::search::semantic_search;
use crate::db::models::AiStatusSummary;
use crate::db::queries::{
    count_embeddings_for_model, count_total_ai_items, get_config, reset_ai_embeddings, set_config,
    sync_ai_status_for_model,
};
use crate::error::{AppError, Result};
use crate::state::AppState;

// ── Helpers ───────────────────────────────────────────────────────────────────
// ── 辅助函数 ──────────────────────────────────────────────────────────────────

/// Get the models directory from app data.
/// 从应用数据获取模型目录。
pub(crate) fn models_dir(state: &AppState) -> PathBuf {
    // We derive models_dir from the log_dir parent (= app_data_dir)
    // 我们从 log_dir 的父目录（= app_data_dir）推导模型目录
    let app_data_dir = state.log_dir.parent().unwrap_or(&state.log_dir);
    app_data_dir.join("models")
}

/// tokio `spawn_blocking` 的 JoinError（后台阻塞任务 panic 或被取消）统一归为内部错误，
/// 使所有 AI 命令的「任务调度失败」走同一稳定 code（Internal），而非各自拼裸字符串丢给前端。
fn join_err(e: tokio::task::JoinError) -> AppError {
    AppError::Internal(format!("后台任务异常 | blocking task failed: {e}"))
}

/// Resolve the currently-active model profile from config (`ai_active_model`), falling back
/// to the default. The id is also the `ai_embeddings.model_name` key for this model's vectors.
/// 从配置（`ai_active_model`）解析当前激活的模型 profile，缺省回退默认。该 id 同时是本模型
/// 向量在 `ai_embeddings.model_name` 的键。
fn active_profile(state: &AppState) -> ModelProfile {
    // 现在「激活模型」由两段配置组成：`ai_active_model`=架构 id（= 向量空间主键），
    // `ai_active_image_file`=选中的图像 onnx 变体文件名（决定加载哪份图像塔，不改向量身份）。
    // image_file 缺省时由 resolve_profile 取该架构的 dyn/fp16 缺省变体。
    let (arch_id, image_file) = state
        .db_read_pool
        .get()
        .ok()
        .map(|conn| {
            (
                get_config(&conn, "ai_active_model").ok().flatten(),
                get_config(&conn, "ai_active_image_file").ok().flatten(),
            )
        })
        .unwrap_or((None, None));

    arch_id
        .as_deref()
        .and_then(|a| profile::resolve_profile(a, image_file.as_deref()))
        .unwrap_or_else(profile::default_profile)
}

/// Resolve the active face model profile from config (`face_model_active`), default fallback.
/// Returns `None` when face feature is disabled (`face_enabled=0`) → engine skips loading face
/// sessions entirely (saves load time/VRAM). The id is also the `faces.model_name` vector-space key.
/// 从配置（`face_model_active`）解析当前激活的人脸模型 profile，缺省回退默认。人脸功能关闭
/// （`face_enabled=0`）时返回 `None`，引擎完全跳过加载人脸 session（省加载时间/显存）。
/// 该 id 同时是人脸向量在 `faces.model_name` 的键。
fn active_face_profile(state: &AppState) -> Option<crate::ai::face_profile::FaceProfile> {
    use crate::ai::face_profile;
    let conn = state.db_read_pool.get().ok()?;
    if get_config(&conn, "face_enabled").ok().flatten().as_deref() == Some("0") {
        return None;
    }
    let id = get_config(&conn, "face_model_active")
        .ok()
        .flatten()
        .unwrap_or_else(|| face_profile::DEFAULT_FACE_PROFILE_ID.to_string());
    Some(face_profile::find_face_profile(&id).unwrap_or_else(face_profile::default_face_profile))
}

/// Whether a specific image-encoder variant is fully usable on disk: image onnx header + its
/// external-data weights, the shared text encoder + its weights, and the vocab. Existence-only — a
/// present-but-wrong-size file still counts (download re-fetches/repairs via size + sha256 checks).
/// 某个图像编码器变体是否已就位可用：图像 onnx 头 + 其外部权重、共享文本塔 + 其权重、词表。
/// 仅按存在判定 —— 存在但大小不符仍算已装（下载命令会按 大小+sha256 校验并按需重拉/修复）。
fn variant_installed(models_dir: &std::path::Path, image_file: &str, text_file: &str) -> bool {
    let needed = [
        image_file.to_string(),
        format!("{image_file}.extra_file"),
        text_file.to_string(),
        format!("{text_file}.extra_file"),
        "vocab.txt".to_string(),
    ];
    needed.iter().all(|f| models_dir.join(f).exists())
}

/// Map an image-encoder variant filename back to its architecture metadata. The prefix before
/// `.img.` plus the fp16/fp32 marker uniquely identifies the architecture (e.g. the two B/16
/// archs share prefix `vit-b-16` but differ by fp16 vs fp32).
/// 由图像变体文件名反查所属架构元数据。`.img.` 前缀 + fp16/fp32 标记唯一确定架构
/// （两个 B/16 同前缀 `vit-b-16`，靠 fp16/fp32 区分）。
fn arch_for_image_file(image_file: &str) -> Option<profile::ArchMeta> {
    let prefix = image_file.split(".img.").next();
    let fp16 = image_file.contains(".fp16.");
    profile::arch_metas()
        .into_iter()
        .find(|m| m.default_image_file.split(".img.").next() == prefix && m.fp16 == fp16)
}

/// The fixed batch size `k` (>1) of an image variant, or `None` for dynamic / single-batch — used
/// to enforce the "configured batch must be ≥ k" rule and to clamp auto batch.
/// 图像变体的固定 batch `k`（>1），动态/单批返回 `None` —— 用于「设置 batch 不得 < k」约束与自动 batch 兜底。
fn variant_fixed_batch(image_file: &str) -> Option<u32> {
    match remote_registry::parse_batch(image_file) {
        Some(BatchKind::Fixed(k)) if k > 1 => Some(k),
        _ => None,
    }
}

/// Ensure the AI engine is initialised (lazy init on first call). Loads CLIP AND face sessions
/// together (F1), so `face_commands` reuses this rather than duplicating engine bring-up.
/// 确保 AI 引擎已初始化（首次调用时懒加载初始化）。CLIP 与人脸 session 一并加载（F1），
/// 故 `face_commands` 复用此函数而非另写一套引擎启动。
pub(crate) fn ensure_engine_initialised(state: &AppState) -> Result<()> {
    // Fast path: already initialised
    // 快速路径：已初始化
    {
        let guard = state.ai_engine.read().unwrap();
        if guard.is_some() {
            return Ok(());
        }
    }

    // Slow path: initialise under write lock
    // 慢速路径：在写锁下初始化
    let mut guard = state.ai_engine.write().unwrap();
    if guard.is_some() {
        return Ok(()); // Race check | 竞争检查
    }

    // [方案 5 准备] 指定加载系统自带的 onnxruntime.dll，避免打包官方 DLL 导致体积膨胀。
    // 若要测试方案 5，请在 Cargo.toml 中切换 ort 依赖，并取消下面这行代码的注释：
    // std::env::set_var("ORT_DYLIB_PATH", "C:\\Windows\\System32\\onnxruntime.dll");

    // Initialise ORT runtime once, lazily (avoids blocking Tauri setup() and the
    // white-screen delay caused by loading the 160 MB onnxruntime.dll at startup).
    // 惰性初始化 ORT runtime（避免在 Tauri setup() 中阻塞并导致白屏）。
    let ort_init_res = ort::init().with_name("PicasaNext").commit();
    info!("ORT initialization result: {:?}", ort_init_res);

    let models = models_dir(state);
    std::fs::create_dir_all(&models).map_err(AppError::Io)?;

    info!("Initialising AI engine | 正在初始化 AI 引擎...");
    let conn = state.db_read_pool.get()?;
    // Active model is now profile-driven: file names, image_size, embed_dim, normalisation,
    // tokenizer and the embeddings model_name key all come from the ModelProfile (profile.rs),
    // so switching among cn-clip sizes (and later other families) is data, not code.
    // 当前模型改为 profile 驱动：文件名、image_size、embed_dim、归一化、分词器、嵌入向量
    // model_name 键全部来自 ModelProfile（profile.rs），使在 cn-clip 各尺寸（及将来其它家族）
    // 间切换变成换数据而非改代码。
    let prof = active_profile(state);
    let provider_override = get_config(&conn, "ai_provider_override")
        .unwrap_or(None)
        .unwrap_or_else(|| "auto".to_string());
    drop(conn);

    info!(
        "Active AI model | 当前 AI 模型: {} ({})",
        prof.id, prof.display_name
    );
    // 同时解析激活的人脸模型（F1：随 CLIP 引擎一并加载人脸 session 插槽；模型未下载则降级 None）。
    let face_prof = active_face_profile(state);
    let mut pool = AiEnginePool::init(&models, &prof, face_prof.as_ref(), &provider_override)?;

    // Load tokenizer eagerly and cache it in the pool (profile decides which vocab/spec).
    // Avoids reloading it on every semantic_search_cmd call.
    // 立即加载分词器并缓存到池中（由 profile 决定词表/规格），避免每次搜索都重载。
    match crate::ai::clip::ClipTokenizer::from_profile(&models, &prof) {
        Ok(tokenizer) => {
            pool.clip_tokenizer = Some(tokenizer);
            info!("CLIP tokenizer cached in engine pool | CLIP 分词器已缓存到引擎池");
        }
        Err(e) => warn!(
            "Failed to load tokenizer for {} | {} 分词器加载失败: {}",
            prof.id, prof.id, e
        ),
    }

    // Persist detected provider to app_config
    // 将探测到的提供者持久化到 app_config
    {
        let conn = state.db_writer.lock().unwrap();
        let _ = set_config(&conn, "ai_provider", pool.provider.as_str());
        let _ = set_config(&conn, "ai_gpu_name", &pool.gpu_name);
    }

    *guard = Some(pool);
    info!("AI engine initialised | AI 引擎初始化完成");
    Ok(())
}

// ── Commands ──────────────────────────────────────────────────────────────────
// ── 命令 ──────────────────────────────────────────────────────────────────────

/// Detect and initialise the best available AI provider.
/// 探测并初始化最优的 AI 提供者。
///
/// Returns: `{ provider: string, gpu_name: string }`
#[tauri::command]
pub async fn detect_ai_provider(state: State<'_, Arc<AppState>>) -> Result<serde_json::Value> {
    let state = Arc::clone(&state);

    tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
        ensure_engine_initialised(&state)?;

        let guard = state.ai_engine.read().unwrap();
        let pool = guard.as_ref().unwrap();

        Ok(serde_json::json!({
            "provider": pool.provider.as_str(),
            "gpuName":  pool.gpu_name.clone(),
            "clipLoaded": pool.clip_ready(),
        }))
    })
    .await
    .map_err(join_err)?
}

/// Get comprehensive AI status for the UI status bar.
/// 获取 UI 状态栏所需的综合 AI 状态。
#[tauri::command]
pub async fn get_ai_status(state: State<'_, Arc<AppState>>) -> Result<AiStatusSummary> {
    let state = Arc::clone(&state);

    tokio::task::spawn_blocking(move || -> Result<AiStatusSummary> {
        let conn = state.db_read_pool.get()?;

        let provider = get_config(&conn, "ai_provider")
            .unwrap_or_default()
            .unwrap_or_default();
        let gpu_name = get_config(&conn, "ai_gpu_name")
            .unwrap_or_default()
            .unwrap_or_default();

        let active_prof = active_profile(&state);
        let active_model = active_prof.id.clone();
        // 当前图像变体若是固定 batch（k>1），向前端暴露 k 以驱动「设置 batch 不得 < k」约束。
        let active_fixed_batch = variant_fixed_batch(&active_prof.image_file);
        let total_items = count_total_ai_items(&conn).unwrap_or(0);
        // 搜索只依赖 ai_embeddings；Error 状态没有向量，不能算“可搜索的已分析”。
        let analyzed_items = count_embeddings_for_model(&conn, &active_model).unwrap_or(0);
        let pending_items = total_items.saturating_sub(analyzed_items);

        let clip_loaded = {
            let guard = state.ai_engine.read().unwrap();
            guard.as_ref().map(|e| e.clip_ready()).unwrap_or(false)
        };

        let is_analyzing = state.ai_analysis_token.lock().unwrap().is_some();

        // "Desired" flag persisted across runs/restarts: set on start/resume/pause,
        // cleared on stop or natural completion. Drives resume + auto-resume (问题7).
        // 跨运行/重启持久化的「期望运行」标志：开始/续传/暂停时置位，停止或自然完成时清除。
        // 驱动续传与自动续传（问题7）。
        let analysis_active = get_config(&conn, "ai_analysis_active")
            .unwrap_or_default()
            .map(|v| v == "1")
            .unwrap_or(false);

        let vram_bytes = crate::ai::provider::detect_vram_bytes();
        let vram_gb = vram_bytes.map(|b| (b / (1024 * 1024 * 1024)) as i64);

        let batch_size_str = get_config(&conn, "ai_batch_size").unwrap_or_default();
        let mut batch_size = if let Some(s) = batch_size_str {
            s.parse::<i64>().unwrap_or(8)
        } else {
            let default_batch = if let Some(gb) = vram_gb {
                if gb >= 8 {
                    64
                } else if gb >= 4 {
                    32
                } else if gb >= 2 {
                    16
                } else {
                    8
                }
            } else {
                8
            };
            let w_conn = state.db_writer.lock().unwrap();
            let _ = set_config(&w_conn, "ai_batch_size", &default_batch.to_string());
            default_batch
        };
        // 固定 batch 模型：有效 batch 不得小于 k（与 pipeline 内的 clamp 一致）。0=自动，保持原样。
        if let Some(k) = active_fixed_batch {
            if batch_size > 0 {
                batch_size = batch_size.max(k as i64);
            }
        }

        Ok(AiStatusSummary {
            provider,
            gpu_name,
            vram_gb,
            batch_size,
            active_fixed_batch: active_fixed_batch.map(|k| k as i64),
            clip_loaded,
            total_items,
            analyzed_items,
            pending_items,
            is_analyzing,
            analysis_active,
        })
    })
    .await
    .map_err(join_err)?
}

/// Perform semantic search using Chinese-CLIP text encoder.
/// 使用 Chinese-CLIP 文本编码器执行语义搜索。
#[tauri::command]
pub async fn semantic_search_cmd(
    query: String,
    limit: Option<usize>,
    state: State<'_, Arc<AppState>>,
) -> Result<usize> {
    let state = Arc::clone(&state);
    let top_k = limit.unwrap_or(50).min(1000);

    tokio::task::spawn_blocking(move || -> Result<usize> {
        // Ensure engine is ready
        // 确保引擎就绪
        ensure_engine_initialised(&state)?;

        let engine_guard = state.ai_engine.read().unwrap();
        let engine = engine_guard.as_ref().unwrap();

        let text_session = match engine.clip_text_session.as_ref() {
            Some(s) => s,
            None => {
                return Err(AppError::AiModelNotLoaded(
                    "文本编码器未加载 | text encoder not loaded".into(),
                ))
            }
        };

        // semantic_search manages its own connections: it loads the resident embedding
        // cache from the READ pool and only takes the write lock briefly to persist
        // results — so scoring no longer blocks all DB writes.
        // semantic_search 自行管理连接：从读连接池加载常驻嵌入缓存，仅在持久化结果时
        // 短暂持有写锁 —— 打分阶段不再阻塞所有数据库写入。
        // The profile the engine was built with drives query encoding + cache dim (must match
        // the model that produced the stored vectors).
        // 引擎所加载的 profile 驱动查询编码与缓存维度（须与生成已存向量的模型一致）。
        let prof = &engine.profile;
        if let Some(tokenizer) = engine.clip_tokenizer.as_ref() {
            semantic_search(&state, text_session, tokenizer, &query, top_k, prof)
        } else {
            // Fallback: load tokenizer from disk (happens if vocab wasn't present at init time)
            // 回退：从磁盘加载分词器（词表初始化时不存在的情况）
            let models = models_dir(&state);
            let tokenizer = crate::ai::clip::ClipTokenizer::from_profile(&models, prof)?;
            semantic_search(&state, text_session, &tokenizer, &query, top_k, prof)
        }
    })
    .await
    .map_err(join_err)?
}

/// Persist the "analysis desired" flag and launch the pipeline.
/// 持久化「期望运行」标志并启动流水线。
/// R1-3：标志位落库下沉 blocking（保留 into_inner 毒锁恢复）；`start_ai_pipeline` 内部要
/// `tokio::spawn`，须留在 async 上下文，故本函数整体改 async 而非塞进 spawn_blocking。
async fn launch_ai_pipeline(state: &Arc<AppState>) -> Result<()> {
    let s = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let _ = set_config(&conn, "ai_analysis_active", "1");
    })
    .await
    .map_err(join_err)?;
    let token = state.new_ai_analysis_token();
    start_ai_pipeline(Arc::clone(state), token);
    Ok(())
}

/// Start (or RESUME) the background AI analysis pipeline WITHOUT resetting existing
/// embeddings — already-analysed images are skipped; only pending / interrupted items are
/// processed. This is the "开始 / 继续" action and also what auto-resume calls (问题7).
///
/// 启动（或续传）后台 AI 分析流水线，且不重置已有嵌入向量——已分析的图片会跳过，只处理
/// 待处理 / 被中断的项。这是「开始 / 继续」动作，也是自动续传调用的入口（问题7）。
#[tauri::command]
pub async fn start_ai_analysis(state: State<'_, Arc<AppState>>) -> Result<()> {
    let state_arc = Arc::clone(&state);

    // Initialise engine first (idempotent)
    // 首先初始化引擎（幂等）
    tokio::task::spawn_blocking({
        let s = Arc::clone(&state_arc);
        move || ensure_engine_initialised(&s)
    })
    .await
    .map_err(join_err)??;

    // R1-3：active_profile（读池 SQL）+ sync_ai_status（写锁 SQL）一并下沉 blocking；
    // 保留 into_inner 毒锁恢复（AI 命令族契约：控制类写不因毒锁失效）。
    tokio::task::spawn_blocking({
        let s = Arc::clone(&state_arc);
        move || {
            // ai_status 是全局列，历史失败（例如旧输入名导致的批量 Error）可能没有当前模型向量。
            // 启动前按真实向量覆盖重同步，确保“开始/继续”会补跑缺失项，而不是被 Error 永久跳过。
            let model_id = active_profile(&s).id;
            sync_ai_status_for_model(&s.db_writer, &model_id)
        }
    })
    .await
    .map_err(join_err)??;

    // F5 mutual exclusion: claim the shared GPU-analysis slot. Fails fast if face analysis holds
    // it (CLIP & face can't run together — VRAM contention). Re-entrant when CLIP already owns it
    // (resume/start while running). Placed AFTER the idempotent sync above but BEFORE cancel, so a
    // rejection leaves no half-applied state.
    // F5 互斥：占用共享 GPU 分析槽。若人脸分析持有则快速失败（CLIP 与人脸不能同跑——显存竞争）。
    // CLIP 已持有时可重入（运行中续传/开始）。放在上面幂等 sync 之后、cancel 之前，使被拒时不留半应用状态。
    if !state_arc.try_acquire_gpu_analysis(crate::state::GPU_OWNER_AI) {
        // GPU 分析槽被人脸分析占用。用 System(消息直透) 保留这条可操作中文文案给 UI；
        // 若未来前端要按类型分流「GPU 忙」，再升一个稳定 code 变体（见 no-contract-freeze）。
        return Err(AppError::System(
            "人脸分析正在进行，请先暂停后再开始语义分析".into(),
        ));
    }

    // Cancel any existing run, then resume (orphan recovery happens inside the pipeline).
    // 取消任何现有运行，然后续传（孤儿恢复在流水线内部完成）。
    state_arc.cancel_ai_analysis();
    info!("Starting/resuming AI analysis pipeline (no reset) | 启动/续传 AI 分析流水线（不重置）");
    // launch 失败（仅 blocking join panic 路径）须释放 GPU 槽，避免泄漏（与 restart 的 reset 失败同规）。
    if let Err(e) = launch_ai_pipeline(&state_arc).await {
        state_arc.release_gpu_analysis(crate::state::GPU_OWNER_AI);
        return Err(e);
    }

    Ok(())
}

/// Restart analysis from scratch: clear ALL embeddings (ai_status → 0) then run. This is
/// the "重新开始" action (问题7).
/// 从零重新开始：清除所有嵌入向量（ai_status → 0）后运行。这是「重新开始」动作（问题7）。
#[tauri::command]
pub async fn restart_ai_analysis(state: State<'_, Arc<AppState>>) -> Result<()> {
    let state_arc = Arc::clone(&state);

    tokio::task::spawn_blocking({
        let s = Arc::clone(&state_arc);
        move || ensure_engine_initialised(&s)
    })
    .await
    .map_err(join_err)??;

    // F5 mutual exclusion: claim the slot BEFORE the destructive reset below (so a rejection
    // doesn't wipe embeddings). Re-entrant when CLIP already owns it. If the reset then fails,
    // release the slot to avoid leaking it.
    // F5 互斥：在下面破坏性 reset 之前占用槽（使被拒时不会清空向量）。CLIP 已持有时可重入。
    // 若随后 reset 失败，释放槽以免泄漏。
    if !state_arc.try_acquire_gpu_analysis(crate::state::GPU_OWNER_AI) {
        return Err(AppError::System(
            "人脸分析正在进行，请先暂停后再重新开始语义分析".into(),
        ));
    }

    state_arc.cancel_ai_analysis();

    // Clear previous embeddings for a complete fresh run.
    // 清除之前的嵌入向量，保证全量重新分析。
    let reset_res = tokio::task::spawn_blocking({
        let s = Arc::clone(&state_arc);
        move || {
            // Reset embeddings for the ACTIVE model only (vectors of other models are kept).
            // 仅重置当前激活模型的嵌入向量（其它模型的向量保留）。
            let model_id = active_profile(&s).id;
            reset_ai_embeddings(&s.db_writer, &model_id)
        }
    })
    .await
    .map_err(join_err)
    .and_then(|inner| inner);
    if let Err(e) = reset_res {
        state_arc.release_gpu_analysis(crate::state::GPU_OWNER_AI);
        return Err(e);
    }

    state_arc.invalidate_embedding_cache();
    info!("Restarting AI analysis pipeline (full reset) | 重新开始 AI 分析流水线（全量重置）");
    // 同 start：launch 失败须释放 GPU 槽。
    if let Err(e) = launch_ai_pipeline(&state_arc).await {
        state_arc.release_gpu_analysis(crate::state::GPU_OWNER_AI);
        return Err(e);
    }

    Ok(())
}

/// Pause the running analysis: cancel the pipeline but KEEP the active flag so it can be
/// resumed later (incl. auto-resume on next launch). In-flight "processing" items are
/// recovered to pending on the next run (问题7).
/// 暂停运行中的分析：取消流水线但保留 active 标志，以便之后续传（含下次启动自动续传）。
/// 在途的「处理中」项会在下次运行时恢复为待处理（问题7）。
#[tauri::command]
pub async fn pause_ai_analysis(state: State<'_, Arc<AppState>>) -> Result<()> {
    info!("Pausing AI analysis (keeps resume flag) | 暂停 AI 分析（保留续传标志）");
    state.cancel_ai_analysis();
    // Release the shared GPU-analysis slot so face analysis can start while CLIP is paused
    // (F5 mutual exclusion). The completion handler only releases on natural completion, so
    // a pause/stop must release here.
    // 释放共享 GPU 分析槽，使 CLIP 暂停期间人脸分析可启动（F5 互斥）。完成回调仅在自然完成时
    // 释放，故暂停/停止须在此释放。
    state.release_gpu_analysis(crate::state::GPU_OWNER_AI);
    // R1-3：标志位落库下沉 blocking；保留 into_inner 毒锁恢复（暂停不能因毒锁失效）。
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let _ = set_config(&conn, "ai_analysis_active", "1");
    })
    .await
    .map_err(join_err)?;
    Ok(())
}

/// Stop the running AI analysis pipeline AND clear the resume flag (no auto-resume).
/// Progress is preserved (embeddings kept) — only the auto-continue intent is dropped.
/// 停止运行中的 AI 分析流水线并清除续传标志（不再自动续传）。进度保留（嵌入向量不删），
/// 仅放弃「自动继续」的意图。
#[tauri::command]
pub async fn stop_ai_analysis(state: State<'_, Arc<AppState>>) -> Result<()> {
    info!(
        "Stopping AI analysis pipeline (clears resume flag) | 停止 AI 分析流水线（清除续传标志）"
    );
    state.cancel_ai_analysis();
    // Release the shared GPU-analysis slot (F5 mutual exclusion) — see pause for why.
    // 释放共享 GPU 分析槽（F5 互斥）——理由见暂停。
    state.release_gpu_analysis(crate::state::GPU_OWNER_AI);
    // R1-3：同暂停——下沉 blocking + into_inner 毒锁恢复。
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let _ = set_config(&conn, "ai_analysis_active", "0");
    })
    .await
    .map_err(join_err)?;
    Ok(())
}

/// List all AI models in the models directory.
#[tauri::command]
pub async fn list_ai_models(state: State<'_, Arc<AppState>>) -> Result<Vec<String>> {
    let models = models_dir(&state);
    // R1-3：目录遍历是阻塞 IO，下沉 blocking。
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        if models.exists() && models.is_dir() {
            if let Ok(entries) = std::fs::read_dir(models) {
                for entry in entries.flatten() {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_file() {
                            if let Some(ext) = entry.path().extension() {
                                if ext == "onnx" {
                                    if let Some(name) = entry.file_name().to_str() {
                                        files.push(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        files.sort();
        Ok(files)
    })
    .await
    .map_err(join_err)?
}

/// Import an AI model into the models directory.
#[tauri::command]
pub async fn import_ai_model(source_path: String, state: State<'_, Arc<AppState>>) -> Result<()> {
    let models = models_dir(&state);
    // R1-3：模型文件可达 GB 级，fs::copy 整体下沉 blocking（不能占用 tokio worker 数秒）。
    tokio::task::spawn_blocking(move || {
        if !models.exists() {
            std::fs::create_dir_all(&models)?;
        }

        let source = std::path::Path::new(&source_path);
        let file_name = source
            .file_name()
            .ok_or_else(|| AppError::PathResolution("无效的文件名 | invalid file name".into()))?;
        let dest = models.join(file_name);

        // io::Error 经 `?` 归 AppError::Io（泛化 code，不向 UI 泄露底层路径细节）。
        std::fs::copy(source, &dest)?;
        Ok(())
    })
    .await
    .map_err(join_err)?
}

/// Reload the AI engine with new models
#[tauri::command]
pub async fn reload_ai_engine(state: State<'_, Arc<AppState>>) -> Result<()> {
    info!("Reloading AI engine | 重新加载 AI 引擎");
    // R1-3：引擎重建 = DB 读 + onnx session 加载（秒级重活），整体下沉 blocking。
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        s.cancel_ai_analysis();

        {
            let mut guard = s.ai_engine.write().unwrap();
            *guard = None; // Drop the current engine
        }

        ensure_engine_initialised(&s)
    })
    .await
    .map_err(join_err)?
}

/// Reset all embeddings and re-queue all images for analysis.
/// 重置所有嵌入向量，将所有图像重新排入分析队列。
#[tauri::command]
pub async fn rebuild_embeddings(state: State<'_, Arc<AppState>>) -> Result<()> {
    let state_arc = Arc::clone(&state);

    // Stop any running pipeline first
    // 首先停止任何正在运行的流水线
    state_arc.cancel_ai_analysis();

    tokio::task::spawn_blocking(move || -> Result<()> {
        let model_id = active_profile(&state_arc).id;
        reset_ai_embeddings(&state_arc.db_writer, &model_id)?;
        state_arc.invalidate_embedding_cache();
        Ok(())
    })
    .await
    .map_err(join_err)??;

    info!("Embeddings reset, re-queuing all images | 嵌入向量已重置，重新排队所有图像");
    Ok(())
}

/// Scan the models dir for installed image-encoder variants of a given architecture (offline
/// fallback when discovery fails — the user can still switch among already-downloaded variants).
/// Matches `<prefix>.img.*.<fp16|fp32>.onnx` (excluding `.extra_file`); the fp16/fp32 marker keeps
/// the two B/16 architectures (same `vit-b-16` prefix) apart.
/// 扫描 models 目录里某架构已安装的图像变体（发现失败时的离线回退——用户仍可在已下载变体间切换）。
/// 匹配 `<prefix>.img.*.<fp16|fp32>.onnx`（排除 `.extra_file`）；fp16/fp32 标记区分同前缀的两个 B/16。
fn scan_installed_image_files(models: &std::path::Path, meta: &profile::ArchMeta) -> Vec<String> {
    let Some(prefix) = meta.default_image_file.split(".img.").next() else {
        return Vec::new();
    };
    let marker = format!("{prefix}.img.");
    let fp = if meta.fp16 { ".fp16." } else { ".fp32." };
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(models) {
        for e in rd.flatten() {
            if let Some(name) = e.file_name().to_str() {
                if name.starts_with(&marker)
                    && name.contains(fp)
                    && name.ends_with(".onnx")
                    && !name.ends_with(".extra_file")
                {
                    out.push(name.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// List the model library grouped by architecture → batch variants, with per-variant install /
/// active status. The static fp16 B/16 is always present; the rest are discovered live from
/// `gficcg/clip_cn_vit-onnx`. On discovery failure it falls back to static + installed-on-disk
/// variants (so switching still works offline) and reports `online: false`.
/// 按「架构 → batch 变体」分组列出模型库，含每个变体的安装/激活状态。静态 fp16 B/16 恒在；其余
/// 从 `gficcg/clip_cn_vit-onnx` 在线发现。发现失败时回退为「静态 + 磁盘已安装变体」（离线仍可切换）
/// 并返回 `online: false`。
#[tauri::command]
pub async fn list_model_registry(state: State<'_, Arc<AppState>>) -> Result<serde_json::Value> {
    // 配置：激活架构 id、选中变体、镜像偏好。R1-3：读池 SQL 走 read_blocking。
    let (active_arch, active_image_cfg, mirror_first) =
        super::blocking::read_blocking(&state, |conn| {
            let active_arch = get_config(conn, "ai_active_model")
                .ok()
                .flatten()
                .unwrap_or_else(|| profile::DEFAULT_PROFILE_ID.to_string());
            let active_image_cfg = get_config(conn, "ai_active_image_file").ok().flatten();
            let mirror_first = get_config(conn, "ai_download_source")
                .ok()
                .flatten()
                .as_deref()
                == Some("mirror");
            Ok((active_arch, active_image_cfg, mirror_first))
        })
        .await?;

    // 动态发现（async，置于 spawn_blocking 之外）；失败则离线回退。
    let discovered = remote_registry::discover(mirror_first).await;
    let online = discovered.is_ok();
    let discovered = discovered.unwrap_or_default();

    let models = models_dir(&state);
    // R1-3：变体安装判定 = 每架构十余次 exists/read_dir（阻塞 IO），整段组装下沉 blocking。
    tokio::task::spawn_blocking(move || -> Result<serde_json::Value> {
        // 当前激活变体文件名：配置缺省时取激活架构的缺省图像文件。
        let active_image = active_image_cfg.unwrap_or_else(|| {
            profile::arch_by_id(&active_arch)
                .map(|m| m.default_image_file.to_string())
                .unwrap_or_default()
        });

        let mut archs_json: Vec<serde_json::Value> = Vec::new();
        for meta in profile::arch_metas() {
            let text_file = meta.text_file.to_string();
            let arch_active = meta.id == active_arch;
            let mut variants_json: Vec<serde_json::Value> = Vec::new();

            // 追加一个变体条目（含安装/激活判定与 batch 分类）。
            let mut push_variant = |image_file: &str, batch: Option<BatchKind>, size_bytes: u64| {
                let installed = variant_installed(&models, image_file, &text_file);
                let active = arch_active && image_file == active_image;
                let (batch_kind, fixed_batch) = match batch {
                    None => ("single", serde_json::Value::Null),
                    Some(BatchKind::Dynamic) => ("dynamic", serde_json::Value::Null),
                    Some(BatchKind::Fixed(k)) => ("fixed", serde_json::json!(k)),
                };
                variants_json.push(serde_json::json!({
                    "imageFile": image_file,
                    "batchKind": batch_kind,
                    "fixedBatch": fixed_batch,
                    "sizeBytes": size_bytes,
                    "installed": installed,
                    "active": active,
                }));
            };

            match meta.folder {
                // 静态 fp16 B/16：单变体（eisneim），体积取已校验清单合计。
                None => {
                    let size: u64 = profile::static_fp16_b16_assets()
                        .iter()
                        .map(|a| a.size_bytes)
                        .sum();
                    push_variant(
                        meta.default_image_file,
                        remote_registry::parse_batch(meta.default_image_file),
                        size,
                    );
                }
                // 动态架构：优先用发现结果；发现不到则扫描磁盘已安装变体（离线回退）。
                Some(folder) => {
                    if let Some(arch) = discovered.iter().find(|a| a.folder == folder) {
                        let text_sz = arch.text_onnx.as_ref().map(|f| f.size_bytes).unwrap_or(0);
                        let text_extra_sz =
                            arch.text_extra.as_ref().map(|f| f.size_bytes).unwrap_or(0);
                        for v in &arch.variants {
                            let extra = v.extra.as_ref().map(|f| f.size_bytes).unwrap_or(0);
                            let size = v.onnx.size_bytes + extra + text_sz + text_extra_sz;
                            push_variant(&v.onnx.file, Some(v.batch), size);
                        }
                    } else {
                        for image_file in scan_installed_image_files(&models, &meta) {
                            let batch = remote_registry::parse_batch(&image_file);
                            push_variant(&image_file, batch, 0);
                        }
                    }
                }
            }

            // 无任何变体（如尚未导出且磁盘也没有的 h-14）→ 不展示该架构。
            if variants_json.is_empty() {
                continue;
            }
            archs_json.push(serde_json::json!({
                "id": meta.id,
                "displayName": meta.display_name,
                "description": meta.description,
                "imageSize": meta.image_size,
                "embedDim": meta.embed_dim,
                "sizeMb": meta.size_mb,
                "fp16": meta.fp16,
                "active": arch_active,
                "variants": variants_json,
            }));
        }

        Ok(serde_json::json!({
            "archs": archs_json,
            "activeArchId": active_arch,
            "activeImageFile": active_image,
            "online": online,
        }))
    })
    .await
    .map_err(join_err)?
}

/// Switch the active model: validate it is installed, persist the choice, reload the engine,
/// re-sync `ai_status` to the new model's embedding coverage, and invalidate the resident
/// cache. Afterwards the user runs analysis to (re)embed items missing under the new model —
/// already-embedded items are skipped, and switching BACK to a previously-used model is free.
/// 切换激活模型：校验已安装 → 持久化选择 → 重载引擎 → 按新模型向量覆盖重同步 `ai_status` →
/// 失效常驻缓存。之后用户运行分析以（重新）嵌入新模型下缺失的项 —— 已嵌入项跳过，切回曾用
/// 模型零成本。
#[tauri::command]
pub async fn set_active_model(image_file: String, state: State<'_, Arc<AppState>>) -> Result<()> {
    // 由变体文件名反查架构，合成该变体的 profile（id=架构 = 向量主键，不随变体变化）。
    let meta = arch_for_image_file(&image_file).ok_or_else(|| {
        AppError::UnsupportedFormat(format!(
            "无法识别的模型文件 | unknown model file: {image_file}"
        ))
    })?;
    let prof = profile::resolve_profile(meta.id, Some(&image_file)).ok_or_else(|| {
        AppError::UnsupportedFormat(format!("无法解析架构 | cannot resolve arch: {}", meta.id))
    })?;
    let arch_id = prof.id.clone();
    let text_file = prof.text_file.clone();

    let state_arc = Arc::clone(&state);

    tokio::task::spawn_blocking(move || -> Result<()> {
        // Refuse to switch to a variant whose files aren't present yet (download first).
        // 拒绝切换到文件尚未就位的变体（请先下载）。
        let models = models_dir(&state_arc);
        if !variant_installed(&models, &image_file, &text_file) {
            // 「未安装」语义上等同模型未就位 → AiModelNotLoaded（消息直透，保留可操作的中文提示）。
            return Err(AppError::AiModelNotLoaded(format!(
                "模型「{}」尚未安装，请先下载其模型文件 | variant not installed: {}",
                prof.display_name, image_file
            )));
        }

        state_arc.cancel_ai_analysis();

        {
            let conn = state_arc
                .db_writer
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            // 架构 id = 向量主键；变体文件名仅决定加载哪份图像塔。两者一起持久化。
            set_config(&conn, "ai_active_model", &arch_id)?;
            set_config(&conn, "ai_active_image_file", &image_file)?;
        }
        // ai_status is global (not per-model) → re-point it at the new arch's coverage.
        // ai_status 是全局列(非按模型)→ 重新指向新架构的向量覆盖(分批,批间自行取锁,R2-6)。
        sync_ai_status_for_model(&state_arc.db_writer, &arch_id)?;
        state_arc.invalidate_embedding_cache();

        // Drop the current engine so the next ensure_engine_initialised loads the new variant.
        // 丢弃当前引擎，使下次 ensure_engine_initialised 加载新变体。
        {
            let mut guard = state_arc.ai_engine.write().unwrap();
            *guard = None;
        }
        ensure_engine_initialised(&state_arc)?;

        info!(
            "Active AI model switched to {} (variant {}) | 已切换 AI 模型: {}（变体 {}）",
            arch_id, image_file, arch_id, image_file
        );
        Ok(())
    })
    .await
    .map_err(join_err)?
}

// ── Model download (Layer B) ────────────────────────────────────────────────────
// ── 模型下载（Layer B）────────────────────────────────────────────────────────────

/// Progress event streamed to the frontend over a `Channel` during a model download.
/// 下载期间经 `Channel` 流式推给前端的进度事件。
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model_id: String,
    /// File currently being fetched (empty on the final `done` event).
    /// 当前正在下载的文件（最终 `done` 事件时为空）。
    pub current_file: String,
    /// 1-based index of the current file.
    pub file_index: usize,
    pub file_count: usize,
    /// Bytes received across ALL assets so far.
    /// 迄今所有资产累计已接收字节数。
    pub received: u64,
    /// Total bytes across all assets.
    pub total: u64,
    pub done: bool,
    pub error: Option<String>,
}

/// Download all assets for a model into the models dir, with per-file resume (HTTP Range),
/// size + sha256 verification, mirror fallback, and progress streamed over `on_progress`.
/// Files are written to a `.part` sidecar then atomically renamed, so an interrupted download
/// never leaves a truncated model in place. Already-correct files (size + sha256) are skipped,
/// making this also a "repair" action.
/// 下载某模型的全部资产到 models 目录：逐文件断点续传（HTTP Range）、大小 + sha256 校验、镜像回退、
/// 进度经 `on_progress` 流式推送。先写 `.part` 旁路文件再原子改名，使中断不会留下截断的模型；已正确
/// 文件（大小 + sha256）跳过，故此命令同时是「修复」动作。
#[tauri::command]
pub async fn download_model(
    image_file: String,
    on_progress: tauri::ipc::Channel<DownloadProgress>,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    // 由变体文件名反查架构（决定走静态清单还是在线发现）。
    let meta = arch_for_image_file(&image_file).ok_or_else(|| {
        AppError::UnsupportedFormat(format!(
            "无法识别的模型文件 | unknown model file: {image_file}"
        ))
    })?;
    let display_name = meta.display_name.to_string();
    // 进度事件用变体文件名作 id（前端按变体键管理下载进度）。
    let download_id = image_file.clone();

    let models = models_dir(&state);
    std::fs::create_dir_all(&models)?;

    // 用户选择的首选下载源：`mirror`=国内镜像(hf-mirror.com)优先，其它(含默认/`official`)=官方
    // (HuggingFace)优先。两种模式都会在首选源失败时自动回退到另一源，保证健壮性。
    // Preferred download source: `mirror` puts the China mirror first; anything else (incl.
    // default / `official`) puts the official source first. Either way we fall back to the other.
    // R1-3：读池 SQL 走 read_blocking。
    let mirror_first = super::blocking::read_blocking(&state, |conn| {
        Ok(get_config(conn, "ai_download_source")
            .ok()
            .flatten()
            .as_deref()
            == Some("mirror"))
    })
    .await?;

    // 构造下载清单：静态 fp16 = 已校验固定清单；动态架构 = 由在线发现拼出
    // 图像 onnx + 其 extra + 共享文本塔 onnx + 其 extra + vocab（均带 size/sha256 校验）。
    let assets: Vec<profile::ModelAsset> = match meta.folder {
        None => profile::static_fp16_b16_assets(),
        Some(folder) => {
            // 在线发现失败=网络/远端问题（可操作）→ System 直透详情；arch/variant 缺失=配置指向不存在=格式问题。
            let archs = remote_registry::discover(mirror_first).await.map_err(|e| {
                AppError::System(format!("获取在线模型列表失败 | discovery failed: {e}"))
            })?;
            let arch = archs.iter().find(|a| a.folder == folder).ok_or_else(|| {
                AppError::UnsupportedFormat(format!(
                    "仓库中找不到架构 | arch not in repo: {folder}"
                ))
            })?;
            let variant = arch
                .variants
                .iter()
                .find(|v| v.onnx.file == image_file)
                .ok_or_else(|| {
                    AppError::UnsupportedFormat(format!(
                        "仓库中找不到该变体 | variant not in repo: {image_file}"
                    ))
                })?;

            let mut a = vec![remote_registry::remote_asset(&variant.onnx)];
            if let Some(extra) = &variant.extra {
                a.push(remote_registry::remote_asset(extra));
            }
            if let Some(t) = &arch.text_onnx {
                a.push(remote_registry::remote_asset(t));
            }
            if let Some(te) = &arch.text_extra {
                a.push(remote_registry::remote_asset(te));
            }
            a.push(profile::vocab_asset());
            a
        }
    };

    if assets.is_empty() {
        return Err(AppError::AiModelNotLoaded(format!(
            "模型「{display_name}」暂无可下载清单 | no download manifest"
        )));
    }

    // R10：用通用引擎的安全 client（HTTPS 强制 + 重定向加固 + 大文件不设整体超时，避免 ~GB 模型
    // 被整体超时误杀）。HF `resolve/` 会 302 跳到 HTTPS CDN —— 安全策略只拒「降级到非 HTTPS」的跳转，故兼容。
    let client = crate::download::secure_client(crate::download::TimeoutPolicy::LargeFile)
        .map_err(|e| AppError::System(format!("HTTP 客户端构建失败 | client build failed: {e}")))?;

    // 逐资产下载循环已抽为共用函数 `download_assets`（人脸下载命令复用）。它仍返回 String
    // 以喂给 DownloadProgress.error（流式展示通道，非 IPC 契约）；命令边界把最终失败串包成
    // AppError::System（消息直透，保留「下载失败 <文件>: <原因>」这条可操作详情）。
    // The per-asset loop is extracted into `download_assets` (reused by the face download command).
    download_assets(
        &client,
        &models,
        &assets,
        mirror_first,
        &on_progress,
        &download_id,
    )
    .await
    .map_err(AppError::System)
}

/// Download a fixed list of assets into `models`: per-file resume (HTTP Range), mirror fallback,
/// size + sha256 verification, `.part` → atomic rename, and throttled progress over `on_progress`.
/// Shared by CLIP `download_model` and face `download_face_model` — the only upstream difference is
/// how the asset list is built (online discovery vs static profile assets).
/// 把一组固定资产下载到 `models`：逐文件断点续传（HTTP Range）、镜像回退、size+sha256 校验、
/// `.part`→原子改名、节流进度经 `on_progress`。CLIP 的 download_model 与人脸的 download_face_model
/// 共用——上游差异仅在如何构建资产清单（在线发现 vs profile 静态资产）。
pub(crate) async fn download_assets(
    client: &reqwest::Client,
    models: &std::path::Path,
    assets: &[profile::ModelAsset],
    mirror_first: bool,
    on_progress: &tauri::ipc::Channel<DownloadProgress>,
    download_id: &str,
) -> std::result::Result<(), String> {
    let total: u64 = assets.iter().map(|a| a.size_bytes).sum();
    let file_count = assets.len();

    let send = |current_file: &str,
                file_index: usize,
                received: u64,
                done: bool,
                error: Option<String>| {
        let _ = on_progress.send(DownloadProgress {
            model_id: download_id.to_string(),
            current_file: current_file.to_string(),
            file_index,
            file_count,
            received,
            total,
            done,
            error,
        });
    };

    let mut base_received: u64 = 0; // bytes contributed by already-completed files | 已完成文件累计

    for (i, asset) in assets.iter().enumerate() {
        let idx = i + 1;
        let dest = models.join(&asset.dest);

        // Skip files that are already present, correct size, and (if known) matching sha256.
        // 跳过已存在、大小正确、且（若已知）sha256 匹配的文件。
        // R1-3：sha256 校验要整读文件（模型可达 GB 级），下沉 blocking，别拖垮 tokio worker。
        let already_ok = {
            let dest = dest.clone();
            let expect_size = asset.size_bytes;
            let sha = asset.sha256.clone();
            tokio::task::spawn_blocking(move || {
                std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) == expect_size
                    && crate::download::sha256_matches(&dest, sha.as_deref())
            })
            .await
            .map_err(|e| format!("后台任务异常 | blocking task failed: {e}"))?
        };
        if already_ok {
            base_received += asset.size_bytes;
            send(&asset.dest, idx, base_received, false, None);
            continue;
        }

        let part = models.join(format!("{}.part", asset.dest));
        let mut resume_from = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        // A stale .part bigger than the target → start over.
        // 残留 .part 超过目标大小 → 重新开始。
        if resume_from > asset.size_bytes {
            let _ = std::fs::remove_file(&part);
            resume_from = 0;
        }

        // 按用户偏好排序候选源：首选源在前，另一源作为失败回退在后。
        // Order candidates by the user's preference; the other source stays as fallback.
        let mut urls: Vec<&str> = Vec::with_capacity(2);
        let mirror = asset.mirror_url.as_deref();
        if mirror_first {
            if let Some(m) = mirror {
                urls.push(m);
            }
            urls.push(asset.url.as_str());
        } else {
            urls.push(asset.url.as_str());
            if let Some(m) = mirror {
                urls.push(m);
            }
        }

        // R10：单文件流式下载（Range 续传）+ 镜像回退下沉通用引擎；进度回调把「本文件已收字节」
        // 聚合到全局 received（base_received = 已完成文件累计）。
        let on_bytes = |file_received: u64| {
            send(&asset.dest, idx, base_received + file_received, false, None);
        };
        if let Err(e) = crate::download::download_with_fallback(
            client,
            &urls,
            &part,
            resume_from,
            asset.size_bytes,
            &on_bytes,
        )
        .await
        {
            let msg = format!("下载失败 {} | download failed: {}", asset.dest, e);
            send(&asset.dest, idx, base_received, false, Some(msg.clone()));
            return Err(msg);
        }

        // Verify size, then sha256 (if known).
        // 校验大小，再校验 sha256（若已知）。
        let got = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if got != asset.size_bytes {
            let _ = std::fs::remove_file(&part);
            let msg = format!(
                "{} 大小校验失败：期望 {} 实得 {} | size mismatch",
                asset.dest, asset.size_bytes, got
            );
            send(&asset.dest, idx, base_received, false, Some(msg.clone()));
            return Err(msg);
        }
        // R1-3：同上——下载后整文件 sha256 下沉 blocking。
        let sha_ok = if asset.sha256.is_some() {
            let part_c = part.clone();
            let sha = asset.sha256.clone();
            tokio::task::spawn_blocking(move || {
                crate::download::sha256_matches(&part_c, sha.as_deref())
            })
            .await
            .map_err(|e| format!("后台任务异常 | blocking task failed: {e}"))?
        } else {
            true
        };
        if !sha_ok {
            let _ = std::fs::remove_file(&part);
            let msg = format!(
                "{} sha256 校验失败（文件损坏或被篡改）| checksum mismatch",
                asset.dest
            );
            send(&asset.dest, idx, base_received, false, Some(msg.clone()));
            return Err(msg);
        }

        // Atomic-ish swap into place.
        // 原子式就位。
        let _ = std::fs::remove_file(&dest);
        std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
        base_received += asset.size_bytes;
        send(&asset.dest, idx, base_received, false, None);
    }

    send("", file_count, total, true, None);
    info!(
        "Model downloaded: {} ({} files) | 模型下载完成: {}（{} 个文件）",
        download_id, file_count, download_id, file_count
    );
    Ok(())
}

// `download_file`（单文件流式下载 + Range 续传）与 `sha256_matches` 已下沉 `crate::download` 通用
// 引擎（R10，Part6 §3.1.2），与 exotic 共用；此处不再重复实现。
