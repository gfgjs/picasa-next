// src-tauri/src/ipc/face_commands.rs
//! IPC commands for face-recognition pipeline management (F5).
//! 人脸识别流水线管理的 IPC 命令（F5）。
//!
//! Mirrors `ai_commands` (start/pause/stop/restart/status) but for the face pipeline. Engine
//! bring-up is shared: `ensure_engine_initialised` loads CLIP AND face sessions together (F1),
//! so there's no separate face-engine init here. CLIP and face share ONE GPU-analysis slot
//! (`AppState::gpu_analysis_owner`) and are mutually exclusive — these commands claim/release it
//! the same way the CLIP commands do.
//! 仿 `ai_commands`（开始/暂停/停止/重启/状态），但面向人脸流水线。引擎启动共享：
//! `ensure_engine_initialised` 一并加载 CLIP 与人脸 session（F1），故此处无独立人脸引擎初始化。
//! CLIP 与人脸共用唯一 GPU 分析槽（`AppState::gpu_analysis_owner`）且互斥——这些命令以与 CLIP
//! 命令相同的方式占用/释放它。

use std::sync::Arc;

use tauri::State;
use tracing::info;

use std::path::Path;

use crate::ai::face_pipeline::start_face_pipeline;
use crate::ai::face_profile::{face_profiles, find_face_profile, DEFAULT_FACE_PROFILE_ID};
use crate::db::models::{
    FaceBox, FaceModelInfo, FaceStatusSummary, LikelyMatchGroup, PersonSummary,
};
use crate::db::queries::{
    confirm_face_assignment, count_faces_for_model, count_pending_face_items, count_persons,
    count_processed_face_items, count_total_ai_items, create_person_from_faces, get_config,
    get_faces_for_item, list_likely_matches, list_persons, merge_persons, reassign_face_to_person,
    reject_face_candidate, rename_person, reset_face_data, set_config, set_person_hidden,
    unassign_face,
};
use crate::ipc::ai_commands::{
    download_assets, ensure_engine_initialised, models_dir, DownloadProgress,
};
use crate::state::{AppState, GPU_OWNER_FACE};

/// Both onnx files of a face track present on disk (single-file models, no shared vocab/extra
/// unlike CLIP variants).
/// 一条人脸轨的两个 onnx 文件均在磁盘上（单文件模型，不像 CLIP 变体有共享 vocab/extra）。
fn face_variant_installed(models_dir: &Path, detect_file: &str, embed_file: &str) -> bool {
    models_dir.join(detect_file).exists() && models_dir.join(embed_file).exists()
}

/// Resolve the active face model id from config (`face_model_active`), default fallback. This is
/// the `faces.model_name` vector-space key for counting this model's faces.
/// 从配置（`face_model_active`）解析当前激活的人脸模型 id，缺省回退默认。这是统计该模型人脸时用的
/// `faces.model_name` 向量空间键。
fn active_face_model_id(state: &AppState) -> String {
    state
        .db_read_pool
        .get()
        .ok()
        .and_then(|conn| get_config(&conn, "face_model_active").ok().flatten())
        .unwrap_or_else(|| DEFAULT_FACE_PROFILE_ID.to_string())
}

/// Whether both face sessions (detector + embedder) are currently loaded.
/// 人脸双 session（检测器 + 嵌入器）当前是否均已加载。
fn face_loaded(state: &AppState) -> bool {
    state
        .ai_engine
        .read()
        .unwrap()
        .as_ref()
        .map(|e| e.face_ready())
        .unwrap_or(false)
}

/// Get the comprehensive face-recognition status (counts, persons, running state).
/// 获取人脸识别综合状态（计数、人物、运行态）。
#[tauri::command]
pub async fn get_face_status(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<FaceStatusSummary, String> {
    let state = Arc::clone(&state);

    tokio::task::spawn_blocking(move || -> std::result::Result<FaceStatusSummary, String> {
        let conn = state.db_read_pool.get().map_err(|e| e.to_string())?;

        // Provider/GPU come from the shared engine (same AiEnginePool as CLIP) — reuse the keys
        // CLIP persisted on init.
        // provider/GPU 来自共享引擎（与 CLIP 同一 AiEnginePool）——复用 CLIP 初始化时持久化的键。
        let provider = get_config(&conn, "ai_provider")
            .unwrap_or_default()
            .unwrap_or_default();
        let gpu_name = get_config(&conn, "ai_gpu_name")
            .unwrap_or_default()
            .unwrap_or_default();

        let model_id = active_face_model_id(&state);
        let total_items = count_total_ai_items(&conn).unwrap_or(0);
        let processed_items = count_processed_face_items(&conn).unwrap_or(0);
        let pending_items = count_pending_face_items(&conn).unwrap_or(0);
        let person_count = count_persons(&conn).unwrap_or(0);
        let face_count = count_faces_for_model(&conn, &model_id).unwrap_or(0);

        let face_loaded = face_loaded(&state);
        let is_analyzing = state.face_analysis_token.lock().unwrap().is_some();
        let analysis_active = get_config(&conn, "face_analysis_active")
            .unwrap_or_default()
            .map(|v| v == "1")
            .unwrap_or(false);

        Ok(FaceStatusSummary {
            provider,
            gpu_name,
            face_loaded,
            total_items,
            processed_items,
            pending_items,
            person_count,
            face_count,
            is_analyzing,
            analysis_active,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// R1-3：本文件写命令统一下沉 blocking（String 错误契约 + 既有毒锁恢复语义保持）。
/// 原「同步持锁、不跨 `.await`」写法已被 CLAUDE.md rusqlite 硬化条款取代——同步 SQL 跑在
/// tokio worker 上仍会阻塞并发 IPC，与是否跨 await 无关。
async fn write_blocking_str<T, F>(
    state: &State<'_, Arc<AppState>>,
    f: F,
) -> std::result::Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> std::result::Result<T, String> + Send + 'static,
{
    let s = Arc::clone(state);
    tokio::task::spawn_blocking(move || {
        let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        f(&conn)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Persist the "face analysis desired" flag and launch the pipeline.
/// 持久化「人脸分析期望运行」标志并启动流水线。
fn launch_face_pipeline(state: &Arc<AppState>) {
    {
        let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let _ = set_config(&conn, "face_analysis_active", "1");
    }
    let token = state.new_face_analysis_token();
    start_face_pipeline(Arc::clone(state), token);
}

/// Start (or RESUME) the face pipeline without resetting existing faces — already-processed
/// images are skipped (face_status≠0); only pending / interrupted items are processed. This is
/// the "开始 / 继续" action and what auto-resume calls.
/// 启动（或续传）人脸流水线，不重置已有人脸——已处理图像跳过（face_status≠0），只处理待处理/
/// 中断项。这是「开始/继续」动作，也是自动续传的入口。
#[tauri::command]
pub async fn start_face_analysis(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    let state_arc = Arc::clone(&state);

    tokio::task::spawn_blocking({
        let s = Arc::clone(&state_arc);
        move || ensure_engine_initialised(&s)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // Face models may be disabled (face_enabled=0) or not downloaded → engine skips loading them.
    // 人脸模型可能被禁用（face_enabled=0）或未下载 → 引擎跳过加载。
    if !face_loaded(&state_arc) {
        return Err("人脸模型未启用或未下载".to_string());
    }

    // F5 mutual exclusion: claim the shared GPU-analysis slot (fails fast if CLIP holds it).
    // F5 互斥：占用共享 GPU 分析槽（若 CLIP 持有则快速失败）。
    if !state_arc.try_acquire_gpu_analysis(GPU_OWNER_FACE) {
        return Err("语义分析正在进行，请先暂停后再开始人脸分析".to_string());
    }

    state_arc.cancel_face_analysis();
    info!(
        "Starting/resuming face analysis pipeline (no reset) | 启动/续传人脸分析流水线（不重置）"
    );
    // launch 内含 set_config 落库（R1-3）。
    let s = Arc::clone(&state_arc);
    tokio::task::spawn_blocking(move || launch_face_pipeline(&s))
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Restart face analysis from scratch: wipe ALL faces + persons (face_status → 0) then run.
/// WARNING destroys user labor (named persons, confirmed assignments) — the frontend confirm
/// dialog must say so.
/// 从零重新开始人脸分析：清空所有人脸 + 人物（face_status → 0）后运行。警告会销毁用户劳动
/// （已命名人物、确认指派）——前端确认框须明示。
#[tauri::command]
pub async fn restart_face_analysis(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    let state_arc = Arc::clone(&state);

    tokio::task::spawn_blocking({
        let s = Arc::clone(&state_arc);
        move || ensure_engine_initialised(&s)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    if !face_loaded(&state_arc) {
        return Err("人脸模型未启用或未下载".to_string());
    }

    // Claim BEFORE the destructive reset (so a rejection doesn't wipe faces). Release on reset
    // failure to avoid leaking the slot.
    // 在破坏性 reset 之前占用（使被拒时不会清空人脸）。reset 失败时释放槽以免泄漏。
    if !state_arc.try_acquire_gpu_analysis(GPU_OWNER_FACE) {
        return Err("语义分析正在进行，请先暂停后再重新开始人脸分析".to_string());
    }

    state_arc.cancel_face_analysis();

    let reset_res = tokio::task::spawn_blocking({
        let s = Arc::clone(&state_arc);
        move || {
            let model_id = active_face_model_id(&s);
            reset_face_data(&s.db_writer, &model_id).map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|inner| inner);
    if let Err(e) = reset_res {
        state_arc.release_gpu_analysis(GPU_OWNER_FACE);
        return Err(e);
    }

    info!("Restarting face analysis pipeline (full reset) | 重新开始人脸分析流水线（全量重置）");
    // launch 内含 set_config 落库（R1-3）。
    let s = Arc::clone(&state_arc);
    tokio::task::spawn_blocking(move || launch_face_pipeline(&s))
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Pause the running face analysis: cancel the pipeline but KEEP the active flag for resume.
/// Releases the shared GPU slot so CLIP can run while face is paused.
/// 暂停运行中的人脸分析：取消流水线但保留 active 标志以便续传。释放共享 GPU 槽，使 CLIP 可在
/// 人脸暂停期间运行。
#[tauri::command]
pub async fn pause_face_analysis(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    info!("Pausing face analysis (keeps resume flag) | 暂停人脸分析（保留续传标志）");
    state.cancel_face_analysis();
    state.release_gpu_analysis(GPU_OWNER_FACE);
    write_blocking_str(&state, |c| {
        let _ = set_config(c, "face_analysis_active", "1");
        Ok(())
    })
    .await
}

/// Stop the running face analysis AND clear the resume flag (no auto-resume). Faces/persons are
/// preserved — only the auto-continue intent is dropped.
/// 停止运行中的人脸分析并清除续传标志（不再自动续传）。人脸/人物保留，仅放弃「自动继续」意图。
#[tauri::command]
pub async fn stop_face_analysis(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    info!(
        "Stopping face analysis pipeline (clears resume flag) | 停止人脸分析流水线（清除续传标志）"
    );
    state.cancel_face_analysis();
    state.release_gpu_analysis(GPU_OWNER_FACE);
    write_blocking_str(&state, |c| {
        let _ = set_config(c, "face_analysis_active", "0");
        Ok(())
    })
    .await
}

// ── People wall / detail overlay (F6) ───────────────────────────────────────
// ── 人物墙 / 详情画框（F6）────────────────────────────────────────────────────

/// List person clusters for the people wall.
/// 列出人物墙的人物簇。
#[tauri::command]
pub async fn list_face_persons(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<PersonSummary>, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> std::result::Result<Vec<PersonSummary>, String> {
        let conn = state.db_read_pool.get().map_err(|e| e.to_string())?;
        list_persons(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get all detected faces for one image (detail-viewer overlay).
/// 获取一张图的所有检测人脸（详情查看器叠加框）。
#[tauri::command]
pub async fn get_item_faces(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<FaceBox>, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> std::result::Result<Vec<FaceBox>, String> {
        let conn = state.db_read_pool.get().map_err(|e| e.to_string())?;
        get_faces_for_item(&conn, item_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Rename a person (empty name → unnamed).
/// 给人物命名（空名 → 未命名）。
#[tauri::command]
pub async fn rename_face_person(
    person_id: i64,
    name: String,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking_str(&state, move |c| {
        rename_person(c, person_id, &name).map_err(|e| e.to_string())
    })
    .await
}

/// Show/hide a person on the wall.
/// 在人物墙上显示/隐藏某人物。
#[tauri::command]
pub async fn set_face_person_hidden(
    person_id: i64,
    hidden: bool,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking_str(&state, move |c| {
        set_person_hidden(c, person_id, hidden).map_err(|e| e.to_string())
    })
    .await
}

/// Merge `srcIds` person clusters into `dstId` (reassign faces, recompute centroid, drop empties).
/// 将 `srcIds` 人物簇并入 `dstId`（改派人脸、重算质心、删空簇）。
#[tauri::command]
pub async fn merge_face_persons(
    src_ids: Vec<i64>,
    dst_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking_str(&state, move |c| {
        merge_persons(c, &src_ids, dst_id).map_err(|e| e.to_string())
    })
    .await
}

/// Full re-clustering: rebuild person clusters from scratch to fix incremental fragmentation
/// (same person split across several unnamed clusters), while PINNING user labor — confirmed
/// faces and named/ignored persons are never broken (see `ai::face_cluster::recluster_all`).
/// Refuses to run while the pipeline is analyzing (would race the face writer). Pure CPU cosine
/// math — does NOT touch the GPU-analysis slot.
/// 全量重新聚类：从零重建人物簇以修增量碎片化（同一人散成多个未命名簇），同时锁定用户劳动——
/// 已确认脸与已命名/忽略人物绝不被打散（见 `ai::face_cluster::recluster_all`）。分析运行中拒绝执行
///（会与人脸写入竞争）。纯 CPU 余弦计算——不碰 GPU 分析槽。
#[tauri::command]
pub async fn recluster_faces(state: State<'_, Arc<AppState>>) -> std::result::Result<(), String> {
    // Guard: don't rebuild while the pipeline is mid-write.
    // 守卫：流水线写入中不重建。
    if state.face_analysis_token.lock().unwrap().is_some() {
        return Err("人脸分析正在进行，请先暂停后再重新聚类".to_string());
    }

    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> std::result::Result<(), String> {
        let model_id = active_face_model_id(&state);
        // Threshold/min_quality come from the active face profile (same knobs the pipeline uses).
        // 阈值/最低质量取自当前人脸 profile（与流水线同一组旋钮）。
        let prof =
            find_face_profile(&model_id).ok_or_else(|| format!("未知人脸模型 {model_id}"))?;
        // 与增量聚类同源:阈值取「运行期 override 或 profile 默认」(同一组 config 键,保持一致)。
        let (threshold, min_quality) = crate::ai::face_cluster::effective_thresholds(&state, &prof);
        crate::ai::face_cluster::recluster_all(&state, &model_id, threshold, min_quality);
        info!(
            "Face re-clustering done | 人脸重新聚类完成 (model={})",
            model_id
        );
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Batch approval (Part4 T3 / §3.5.1) ──────────────────────────────────────
// ── 人脸批量审批（Part4 T3 / §3.5.1）───────────────────────────────────────────
//
// 写命令统一经 `write_blocking_str` 下沉 blocking 线程（R1-3：原「同步持锁不跨 .await」写法
// 已被 CLAUDE.md rusqlite 硬化条款取代）；DAO 内自带事务，归属变更连带重算受影响 person。
// `list_likely_matches` 为只读 + spawn_blocking（解码全部未确认脸嵌入算余弦，较重）。

/// Confirm (pin) the current assignment of `faceIds` so re-clustering won't move them.
/// 确认（锁定）`faceIds` 的当前归属，使重聚类不再移动它们。
#[tauri::command]
pub async fn confirm_faces(
    face_ids: Vec<i64>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking_str(&state, move |c| {
        confirm_face_assignment(c, &face_ids).map_err(|e| e.to_string())
    })
    .await
}

/// Reassign `faceIds` to `personId` and pin them (user correcting a clustering mistake). Rejects
/// cross-model reassignment; recomputes both source and target persons.
/// 把 `faceIds` 改派给 `personId` 并锁定（用户纠正聚类错误）。拒绝跨模型改派；重算源与目标 person。
#[tauri::command]
pub async fn reassign_faces(
    face_ids: Vec<i64>,
    person_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking_str(&state, move |c| {
        reassign_face_to_person(c, &face_ids, person_id).map_err(|e| e.to_string())
    })
    .await
}

/// Unassign `faceIds` (误检/归错): clears person_id AND is_confirmed; recomputes source persons.
/// 移出 `faceIds`（误检/归错）：清 person_id 与 is_confirmed；重算源 person。
#[tauri::command]
pub async fn unassign_faces(
    face_ids: Vec<i64>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking_str(&state, move |c| {
        unassign_face(c, &face_ids).map_err(|e| e.to_string())
    })
    .await
}

/// Reject `faceIds` as NOT `personId`: records negative samples + removes them from that person
/// now. A later full re-cluster consults the rejections to avoid re-attracting them.
/// 拒绝 `faceIds`「不是 `personId`」：记负样本 + 立即移出。后续全量重聚类查阅负样本以避免重新吸附。
#[tauri::command]
pub async fn reject_faces(
    face_ids: Vec<i64>,
    person_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking_str(&state, move |c| {
        reject_face_candidate(c, &face_ids, person_id).map_err(|e| e.to_string())
    })
    .await
}

/// Create a new person from `faceIds` (one-tap "make a person"), optional `name`. Returns the new
/// person id. Rejects faces spanning multiple models; recomputes source persons.
/// 从 `faceIds` 新建 person（一键「建人」），可选 `name`。返回新 person id。拒绝跨模型；重算源 person。
#[tauri::command]
pub async fn create_person(
    face_ids: Vec<i64>,
    name: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<i64, String> {
    write_blocking_str(&state, move |c| {
        create_person_from_faces(c, &face_ids, name.as_deref()).map_err(|e| e.to_string())
    })
    .await
}

/// List likely-match groups for the batch-approval UI: unconfirmed faces grouped by candidate
/// person, each with a face thumbnail + match similarity. Optional `personId` / `limit` filters.
/// 列出批量审批 UI 的 likely-match 组：未确认脸按候选 person 分组，各带人脸缩略图 + 匹配相似度。
/// 可选 `personId` / `limit` 过滤。
#[tauri::command]
pub async fn list_likely_face_matches(
    person_id: Option<i64>,
    limit: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<LikelyMatchGroup>, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(
        move || -> std::result::Result<Vec<LikelyMatchGroup>, String> {
            let conn = state.db_read_pool.get().map_err(|e| e.to_string())?;
            list_likely_matches(&conn, person_id, limit).map_err(|e| e.to_string())
        },
    )
    .await
    .map_err(|e| e.to_string())?
}

// ── Face model registry (F7, read-only) ─────────────────────────────────────
// ── 人脸模型库（F7，只读）──────────────────────────────────────────────────────

/// List the built-in face-model tracks with on-disk install status (F7, READ-ONLY).
///
/// Display-only: there is NO download command and NO active-track switch yet. Downloading needs
/// verified URLs+sha256+sizes and human license confirmation (the SCRFD/ArcFace track is
/// non-commercial — `commercialOk=false`); switching tracks changes embed_dim (128↔512) which
/// invalidates all stored face vectors and `persons` has no per-model column — both deferred.
///
/// IMPORTANT — the optional track has NO activation path yet. `face_model_active` is seeded to
/// `yunet-sface` and never written (switch command deferred), so the engine only ever loads the
/// default track and `detect_scrfd` is unreachable. Placing SCRFD/ArcFace onnx files makes
/// `installed=true` here but does NOT enable them — activation awaits a gated
/// `set_active_face_model` (+ InsightFace cross-check). `installed` is honest disk status, not
/// "ready to use".
///
/// 列出内置人脸模型轨 + 磁盘安装状态（F7，**只读**）。仅供展示：尚无下载命令、无激活轨切换。
/// 下载需已校验 URL+sha256+大小 + 人工确认许可（SCRFD/ArcFace 轨非商用，`commercialOk=false`）；
/// 切换轨会改 embed_dim（128↔512）使所有已存人脸向量失效，且 `persons` 无按模型列——均推迟。
/// **要害**：可选轨当前**无激活路径**。`face_model_active` 播种为 `yunet-sface` 且从不被写入
/// （切换命令推迟），故引擎永远只加载默认轨、`detect_scrfd` 不可达。放置 SCRFD/ArcFace onnx 会让
/// 此处 `installed=true`，但**不会**启用它——激活待 gated `set_active_face_model`（+ InsightFace
/// 对拍）。`installed` 是诚实的磁盘状态，不代表"可用"。
#[tauri::command]
pub async fn list_face_model_registry(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<FaceModelInfo>, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> std::result::Result<Vec<FaceModelInfo>, String> {
        let models = models_dir(&state);
        let active_id = active_face_model_id(&state);
        let infos = face_profiles()
            .into_iter()
            .map(|p| {
                let installed = face_variant_installed(&models, &p.detect_file, &p.embed_file);
                let active = p.id == active_id;
                FaceModelInfo {
                    active,
                    installed,
                    // 有已校验下载清单才可一键下载（默认轨）；SCRFD 轨清单为空 → 仅手动导入。
                    downloadable: !p.assets.is_empty(),
                    detector: format!("{:?}", p.detector),
                    embedder: format!("{:?}", p.embedder),
                    embed_dim: p.embed_dim as i64,
                    size_mb: p.size_mb as i64,
                    id: p.id,
                    display_name: p.display_name,
                    description: p.description,
                    commercial_ok: p.commercial_ok,
                    license: p.license,
                }
            })
            .collect();
        Ok(infos)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Download a face-model track's onnx files into the models dir (verified size+sha256, per-file
/// resume, progress over `on_progress`). Reuses the CLIP download machinery (`download_assets`).
/// Only tracks with a non-empty `assets` manifest are downloadable (the default YuNet+SFace
/// track); the SCRFD/ArcFace track has no verified manifest (non-commercial) → Err, manual import
/// only. NOTE downloading the optional track's files does NOT activate it — activation awaits the
/// (deferred) `set_active_face_model` + InsightFace cross-check (see `list_face_model_registry`).
/// 把某条人脸模型轨的 onnx 下载到 models 目录（size+sha256 校验、逐文件续传、`on_progress` 进度）。
/// 复用 CLIP 下载机制（`download_assets`）。仅有已校验清单的轨可下载（默认 YuNet+SFace）；SCRFD/
/// ArcFace 轨无已校验清单（非商用）→ 报错，仅手动导入。注意：下载可选轨文件**不会**启用它——启用
/// 待（推迟的）`set_active_face_model` + InsightFace 对拍（见 `list_face_model_registry`）。
#[tauri::command]
pub async fn download_face_model(
    profile_id: String,
    on_progress: tauri::ipc::Channel<DownloadProgress>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    let prof =
        find_face_profile(&profile_id).ok_or_else(|| format!("未知人脸模型 {profile_id}"))?;
    if prof.assets.is_empty() {
        return Err(format!(
            "「{}」无可下载清单，仅支持手动导入",
            prof.display_name
        ));
    }

    let models = models_dir(&state);
    std::fs::create_dir_all(&models).map_err(|e| e.to_string())?;

    // 首选下载源：mirror=国内镜像优先；其它=官方优先。两模式失败均自动回退另一源（复用 CLIP 约定）。
    // Preferred source mirrors CLIP's convention; either way the other source is the fallback.
    // R1-3：读池 SQL 走 read_blocking（断言测试补抓的漏网点）；本命令错误契约是 String，转拍。
    let mirror_first = super::blocking::read_blocking(&state, |conn| {
        Ok(get_config(conn, "ai_download_source")
            .ok()
            .flatten()
            .as_deref()
            == Some("mirror"))
    })
    .await
    .map_err(|e| e.to_string())?;

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    // 进度事件以 profile id 作 download_id（前端按轨键管理下载进度）。
    download_assets(
        &client,
        &models,
        &prof.assets,
        mirror_first,
        &on_progress,
        &prof.id,
    )
    .await
}
