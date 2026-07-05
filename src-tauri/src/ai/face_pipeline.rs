// src-tauri/src/ai/face_pipeline.rs
//! Background face-recognition pipeline (F3) — worker 派发架构(T16 收束)。
//! 后台人脸识别流水线:推理恒经 ai-worker 子进程派发。
//!
//!   Producer → 攒批 → CPU permit → 三级定源(缩略图档位 → face 缓存 640 → 小原图直派)
//!   → 缺缓存现场预解码(T16-R2 方案 A,镜像 CLIP T18)→ GPU 令牌(D2 顺序)
//!   → FaceDetectEmbed → faces_to_records 映射 → Writer
//!
//! 1. Producer:批量查询 face_status=0 的 media_items,标记 Processing,发 FaceTask。
//! 2. 派发线程:**worker 端只解小图**——缩略图档位(预测短边 ≥ detect_size)或 host 预解码
//!    的 face 缓存(短边 640 WebP,WIC 优先产出,exotic 原图也在覆盖内);仅短边本就 ≤640
//!    的小原图直派 worker 解码(白名单格式)。预解码失败(双引擎都解不开)标 Error,与
//!    CLIP T18 现场派生失败同语义。几何按协议回报的**实际解码尺寸**归一化
//!    (FaceItemResult::Ok.width/height),不用 host 预测尺寸。
//! 3. Writer:批量收集 FaceResult,成功项先删后插(batch_replace_faces),小批置
//!    face_status(问题2 进度平滑)、大批跑增量聚类;零脸图也是成功(Done 非 Error)。
//!
//! 与 CLIP 分析共用 F5 GPU 分析槽(互斥);让步复用 ai_yield_blockers()。
//! 进程内推理路径(engine 快照/rayon 预处理/detect+embed 线程)已随 T16 删除,
//! 历史实现见 git。
//!
//! # 解码源:不能复用 CLIP 的 ai_cache
//! CLIP 的 ai_cache 固定短边 336px,对 YuNet 的 640px 输入太小,会悄悄损害小脸召回率;
//! face 自持一份 `face_thumbs/`(短边 640,FACE_CACHE_SHORT_EDGE)。
//! resolve_face_decode_source 先在「常规分档缩略图」与「原图」间选择,原图回退项再由
//! 派发批升级为 face 缓存(决策核见 face_cache_applies,装配见 dispatch_face_batch)。

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver, Sender};
use rayon::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::ai::clip::embedding_to_bytes;
use crate::ai::face::DetectedFace;
use crate::ai::face_profile::FaceProfile;
use crate::db::models::FaceStatus;
use crate::db::queries::{
    batch_replace_faces, batch_update_face_status, count_pending_face_items,
    get_pending_face_items, reset_processing_face_items, NewFace,
};
use crate::error::AppError;
use crate::state::AppState;
use crate::thumbnail::cache::{face_cache_path, FACE_CACHE_SHORT_EDGE};

/// Batch size for reading from DB and for the writer's clustering flush.
/// 从数据库读取、及写入器聚类刷新的批次大小。
const BATCH_SIZE: i64 = 512;

/// 写入器「脸行 + face_status」的高频小批阈值（问题2 进度平滑）。
/// 脸行落库便宜且必须先于聚类（聚类按 item_id 从库读回脸），故脸+状态一起小批落库——
/// 让前端 `processedItems`（数 `face_status≠0`）平滑增长，且崩溃安全（Done 永远蕴含脸已写）；
/// 真正昂贵的聚类仍按 `BATCH_SIZE` 大批跑（见 `flush_face_rows` / `flush_cluster`）。
const STATUS_FLUSH_EVERY: usize = 16;

/// Channel capacity between stages.
/// 各阶段之间的通道容量。
const CHANNEL_CAPACITY: usize = 1024;

/// Task item sent from producer to preprocessor.
/// 从生产者发送到预处理器的任务项。
struct FaceTask {
    item_id: i64,
    source_path: PathBuf,
    file_format: String,
    thumb_status: i64,
    thumb_path: Option<String>,
    width: i64,
    height: i64,
    /// 缩略图/派生缓存键(xxh3(路径+mtime),兼陈旧防护)——face 缓存(方案 A)按此寻址。
    cache_key: i64,
}

/// Outcome sent from detect+embed (or an early preprocess failure) to the writer.
/// 从检测+嵌入阶段（或预处理早期失败）发送到写入器的结果。
struct FaceResult {
    item_id: i64,
    /// `Some(rows)` on success — possibly empty (zero-face image, still Done).
    /// `None` on decode/detect/embed failure → Error.
    /// 成功时为 `Some(rows)`——可能为空（零脸图，仍算 Done）。解码/检测/嵌入失败为 `None` → Error。
    records: Option<Vec<NewFace>>,
}

/// Start the background face-recognition pipeline.
/// 启动后台人脸识别流水线。
///
/// Returns immediately; all work is done in background threads.
/// 立即返回；所有工作在后台线程中完成。
pub fn start_face_pipeline(state: Arc<AppState>, token: CancellationToken) {
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let start_time = std::time::Instant::now();
        // Keep a token handle so the completion handler can tell natural completion from a
        // pause/stop/restart cancellation (drives the GPU-slot release decision below).
        // 保留一个 token 句柄，使完成回调能区分自然完成与暂停/停止/重启取消（驱动下面 GPU 槽位释放决策）。
        let token_outer = token.clone();
        let result =
            tokio::task::spawn_blocking(move || run_face_pipeline_blocking(&state_clone, &token))
                .await;

        let elapsed_ms = start_time.elapsed().as_millis();
        match result {
            Ok(Ok(())) => info!(
                "Face pipeline completed: elapsed={}ms | 人脸流水线完成: 耗时={}ms",
                elapsed_ms, elapsed_ms
            ),
            Ok(Err(e)) => warn!("Face pipeline error | 人脸流水线错误: {}", e),
            Err(e) => warn!("Face pipeline task panicked | 人脸流水线任务崩溃: {}", e),
        }

        // 仅在自然完成时释放共享 GPU 分析槽（F5 互斥；同 pipeline.rs 的理由——取消由命令释放
        // 或 restart 须保持持有）。判定方式与 set_config 续传标志一致：未被取消即自然完成。
        if !token_outer.is_cancelled() {
            let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            let _ = crate::db::queries::set_config(&conn, "face_analysis_active", "0");
            drop(conn);
            state.release_gpu_analysis(crate::state::GPU_OWNER_FACE);
        }

        // 不卸载共享的 ai-worker 子进程（与 CLIP 分析共用,空闲 300s 自杀兜底）。仅清空本流水线自己的令牌。
        state.cancel_face_analysis();
    });
}

/// Blocking pipeline runner:T16 起恒走 worker 派发(进程内 ort 路径已删)。
fn run_face_pipeline_blocking(
    state: &Arc<AppState>,
    token: &CancellationToken,
) -> crate::error::Result<()> {
    crate::ipc::ai_commands::warn_legacy_ai_backend(state);
    run_face_pipeline_worker_blocking(state, token)
}

/// Resume support (问题7): release any items a previous run claimed but never finished
/// (face_status=Processing) back to Pending — mirrors `reset_processing_ai_items`.
/// 续传支持(问题7):把上次运行已领取但未完成的项(face_status=Processing)放回 Pending——
/// 镜像 `reset_processing_ai_items`;进程内与 worker 派发两路径共用。
fn recover_orphaned_face_items(state: &Arc<AppState>) {
    let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
    match reset_processing_face_items(&conn) {
        Ok(n) if n > 0 => info!(
            "Recovered {} orphaned face items (processing → pending) | 恢复 {} 个孤儿人脸项（处理中 → 待处理）",
            n, n
        ),
        Ok(_) => {}
        Err(e) => warn!(
            "Failed to recover orphaned face items | 恢复孤儿人脸项失败: {}",
            e
        ),
    }
}

/// Producer: batch-query pending items, push tasks to channel.
/// 生产者：批量查询待处理项，推送任务到通道。
fn produce_face_tasks(state: &Arc<AppState>, task_tx: Sender<FaceTask>, token: &CancellationToken) {
    loop {
        if token.is_cancelled() {
            info!("Face producer cancelled | 人脸生产者已取消");
            break;
        }

        // 复用既有让步层（scan/缩略图/派生/exotic/交互）——与 CLIP 生产者调用同一函数。
        // v3.1 R9：阻塞源已含 exotic（冷门格式子进程解码优先于 AI/face），单点改动两边自动生效。
        // 人脸↔CLIP 互斥不在此实现，见模块头。
        let blockers = state.ai_yield_blockers();
        if !blockers.is_empty() {
            debug!(
                blockers = %blockers.join(","),
                "Face producer yielding to higher priority task | 人脸生产者让步给高优先级任务"
            );
            std::thread::sleep(std::time::Duration::from_millis(500));
            continue;
        }

        let conn = match state.db_read_pool.get() {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "DB pool error in face producer | 人脸生产者 DB 池错误: {}",
                    e
                );
                break;
            }
        };

        let batch = match get_pending_face_items(&conn, BATCH_SIZE) {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    "Query pending face items failed | 查询待处理人脸项失败: {}",
                    e
                );
                break;
            }
        };

        if batch.is_empty() {
            info!("Face producer: no more pending items | 人脸生产者：没有更多待处理项");
            break;
        }

        // Mark items as "processing" to avoid re-queuing on restart.
        // 将项标记为"处理中"，避免重启时重新排队。
        let write_conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<i64> = batch.iter().map(|it| it.id).collect();
        if let Err(e) = batch_update_face_status(&write_conn, &ids, FaceStatus::Processing.as_i64())
        {
            warn!(
                "Failed to mark face items as processing | 标记人脸项为处理中失败: {}",
                e
            );
        }
        drop(write_conn);

        for item in batch {
            if token.is_cancelled() {
                break;
            }

            if task_tx
                .send(FaceTask {
                    item_id: item.id,
                    source_path: PathBuf::from(item.abs_path),
                    file_format: item.file_format,
                    thumb_status: item.thumb_status,
                    thumb_path: item.thumb_path,
                    width: item.width,
                    height: item.height,
                    cache_key: item.cache_key,
                })
                .is_err()
            {
                break;
            }
        }
    }

    info!("Face producer finished | 人脸生产者已完成");
}

/// A resolved decode source: which file to decode and as what format.
/// 已解析的解码源：解码哪个文件、按什么格式。
struct FaceDecodeSource {
    path: PathBuf,
    format: String,
    kind: FaceSourceKind,
}

/// 解码源三级构成(T16-R2 方案 A):诊断计数随批输出;原图直派常态应只剩短边 ≤640 的小图,
/// 若原图源占比高且单项耗时大,优先怀疑防呆回退或预解码异常。
#[derive(Clone, Copy, PartialEq)]
enum FaceSourceKind {
    /// 常规分档缩略图(预测短边 ≥ detect_size)。
    Thumb,
    /// host 预解码的 face 缓存(短边 640 WebP)。
    FaceCache,
    /// 原图直派 worker 解码(短边本就 ≤640 的小图,或 detect_size 超缓存尺寸的防呆回退)。
    Original,
}

/// Pick the cheapest decode source whose short edge still satisfies `detect_size` (640).
///
/// Deliberately NOT shared with CLIP's `resolve_decode_source` in `pipeline.rs` (see module
/// header for why: that function's priority-1 AI-cache shortcut is hard-baked at a 336px short
/// edge, which is too small for YuNet's 640px input). This version only ever considers the
/// regular tiered thumbnail or the original — no AI cache.
///
/// 选择短边仍满足 `detect_size`（640）的最廉价解码源。
///
/// 故意不与 `pipeline.rs` 的 `resolve_decode_source` 共用（原因见模块头：那个函数优先级 1 的
/// AI 缓存捷径固定短边 336px，对 YuNet 的 640px 输入而言太小）。此版本只在「常规分档缩略图」
/// 与「原图」之间选择——不涉及 AI 缓存。
fn resolve_face_decode_source(
    task: &FaceTask,
    state: &AppState,
    detect_size: u32,
) -> FaceDecodeSource {
    let original = FaceDecodeSource {
        path: task.source_path.clone(),
        format: task.file_format.clone(),
        kind: FaceSourceKind::Original,
    };

    if task.thumb_status != 1 || task.width <= 0 || task.height <= 0 {
        return original;
    }
    let Some(rel) = task.thumb_path.as_deref() else {
        return original;
    };

    // Parse the tier (long edge) from the rel path "{tier}/{prefix}/{hex}.webp".
    // 从相对路径 "{档位}/{前缀}/{hex}.webp" 解析档位（长边）。
    let Some(tier) = rel.split('/').next().and_then(|s| s.parse::<u32>().ok()) else {
        return original;
    };

    // Thumbnail is LongEdge(tier) but never upscaled → long edge = min(tier, max(W,H)).
    // Predict short edge WITHOUT touching disk; use the thumbnail only if it's ≥ detect_size.
    // 缩略图按长边=tier 缩放但绝不放大 → 长边 = min(tier, max(W,H))；不读盘预测短边，
    // 仅当 ≥ detect_size 时采用。
    let (w, h) = (task.width as u32, task.height as u32);
    let (long, short) = (w.max(h), w.min(h));
    let thumb_short = if long <= tier {
        short
    } else {
        (short as f32 * tier as f32 / long as f32).round() as u32
    };
    if thumb_short < detect_size {
        return original;
    }

    let cache_dir = state.thumb_config.read().unwrap().cache_dir.clone();
    let thumb_full = cache_dir.join("thumbnails").join(rel);
    if thumb_full.exists() {
        // Thumbnail is already EXIF-upright (baked in at generation) — "webp" keeps WIC's
        // rotation branch (jpg/heic only) off so orientation isn't re-applied.
        // 缩略图生成时已转正——用"webp"格式使 WIC 的旋转分支（仅 jpg/heic）不触发，避免二次转向。
        FaceDecodeSource {
            path: thumb_full,
            format: "webp".to_string(),
            kind: FaceSourceKind::Thumb,
        }
    } else {
        original
    }
}

/// 把「检测几何 + 逐脸嵌入」映射为 DB 就绪行:bbox/关键点按解码图 `(img_w, img_h)` 归一化
/// 为 `[0,1]`,quality 同源派生。进程内(detect_and_embed_one)与 worker 派发(几何经协议
/// FaceDet 原样搬回 DetectedFace + 协议回报的实际解码尺寸)两路径共用,保证落库语义逐位一致。
fn faces_to_records(
    item_id: i64,
    faces: &[DetectedFace],
    embeddings: &[Vec<f32>],
    img_w: u32,
    img_h: u32,
) -> Vec<NewFace> {
    let (w, h) = (img_w.max(1) as f32, img_h.max(1) as f32);
    faces
        .iter()
        .zip(embeddings)
        .map(|(face, emb)| {
            let quality = face.quality(img_w, img_h);
            let mut lm_flat = [0f32; 10];
            for i in 0..5 {
                lm_flat[i * 2] = (face.landmarks[i][0] / w).clamp(0.0, 1.0);
                lm_flat[i * 2 + 1] = (face.landmarks[i][1] / h).clamp(0.0, 1.0);
            }
            NewFace {
                item_id,
                bbox_x: (face.bbox[0] / w).clamp(0.0, 1.0),
                bbox_y: (face.bbox[1] / h).clamp(0.0, 1.0),
                bbox_w: (face.bbox[2] / w).clamp(0.0, 1.0),
                bbox_h: (face.bbox[3] / h).clamp(0.0, 1.0),
                landmarks: embedding_to_bytes(&lm_flat),
                det_score: face.score,
                quality,
                embedding: embedding_to_bytes(emb),
            }
        })
        .collect()
}

/// Writer: batch-collect results, delete+insert `faces` rows, update `face_status`.
/// 写入器：批量收集结果，先删后插 `faces` 行，更新 `face_status`。
fn write_face_results(
    state: &Arc<AppState>,
    result_rx: Receiver<FaceResult>,
    token: &CancellationToken,
    profile: &FaceProfile,
) {
    // 问题2 双节奏：
    // - 小批（STATUS_FLUSH_EVERY=16）：写脸行 + 置 face_status=Done，并把成功项累入 `cluster_pending`。
    //   `done_ids` 须含本批**所有**成功项（含零脸图），它驱动 `batch_replace_faces` 的删除阶段。
    // - 大批（BATCH_SIZE=512）：对累积的 `cluster_pending` 跑一次昂贵的增量聚类。
    // 解耦后 `processedItems`（数 face_status≠0）平滑增长，而聚类仍低频跑。
    let mut done_ids: Vec<i64> = Vec::with_capacity(STATUS_FLUSH_EVERY);
    let mut rows: Vec<NewFace> = Vec::new();
    let mut failed_ids: Vec<i64> = Vec::new();
    // 脸已落库、status 已 Done、但尚未聚类的 item_id。聚类按 item_id 从库读回脸，故此处累积安全。
    let mut cluster_pending: Vec<i64> = Vec::with_capacity(BATCH_SIZE as usize);
    let mut total_written: u64 = 0;

    for result in result_rx {
        if token.is_cancelled() {
            break;
        }

        match result.records {
            Some(mut recs) => {
                done_ids.push(result.item_id);
                rows.append(&mut recs);
            }
            None => {
                failed_ids.push(result.item_id);
            }
        }

        if done_ids.len() >= STATUS_FLUSH_EVERY {
            flush_face_rows(
                state,
                &mut done_ids,
                &mut rows,
                profile,
                &mut cluster_pending,
                &mut total_written,
            );
        }
        if failed_ids.len() >= STATUS_FLUSH_EVERY {
            flush_face_failed(state, &mut failed_ids);
        }
        if cluster_pending.len() >= BATCH_SIZE as usize {
            flush_cluster(state, &mut cluster_pending, profile);
        }
    }

    if !done_ids.is_empty() {
        flush_face_rows(
            state,
            &mut done_ids,
            &mut rows,
            profile,
            &mut cluster_pending,
            &mut total_written,
        );
    }
    if !failed_ids.is_empty() {
        flush_face_failed(state, &mut failed_ids);
    }
    // 收尾：把剩余未达大批阈值的项聚类掉。
    if !cluster_pending.is_empty() {
        flush_cluster(state, &mut cluster_pending, profile);
    }

    info!(
        "Face writer finished: {} images written | 人脸写入器已完成：写入 {} 张图像",
        total_written, total_written
    );
}

/// Flush a small batch of successfully-processed items: delete+insert their `faces` rows, mark
/// `face_status=Done`, then queue them for the (deferred, large-batch) clustering pass. On DB
/// failure, mark the batch `Error` instead (mirrors CLIP's `flush_batch`).
///
/// 刷新一小批处理成功的项：先删后插其 `faces` 行，标记 `face_status=Done`，再排入（延后的、
/// 大批的）聚类队列。DB 失败则改标 `Error`（镜像 CLIP 的 `flush_batch`）。脸与状态在此一起原子
/// 落库 → `Done` 永远蕴含脸已写，崩溃/取消时不会出现「Done 但零脸」的漏脸。
fn flush_face_rows(
    state: &Arc<AppState>,
    done_ids: &mut Vec<i64>,
    rows: &mut Vec<NewFace>,
    profile: &FaceProfile,
    cluster_pending: &mut Vec<i64>,
    total_written: &mut u64,
) {
    let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
    match batch_replace_faces(&conn, done_ids, &profile.id, rows) {
        Ok(()) => {
            if let Err(e) = batch_update_face_status(&conn, done_ids, FaceStatus::Done.as_i64()) {
                warn!(
                    "Failed to mark face items as done | 标记人脸项为完成失败: {}",
                    e
                );
            }
            *total_written += done_ids.len() as u64;
            drop(conn);
            // 脸已落库 → 排入聚类队列（聚类延后到大批，见 `flush_cluster`）。
            cluster_pending.extend(done_ids.iter().copied());
        }
        Err(e) => {
            warn!("Batch face write failed | 批量写入人脸失败: {}", e);
            let _ = batch_update_face_status(&conn, done_ids, FaceStatus::Error.as_i64());
            drop(conn);
        }
    }
    done_ids.clear();
    rows.clear();
}

/// Run the (expensive) incremental clustering on a large batch of already-persisted items, then
/// clear the queue. Kept separate from `flush_face_rows` so status/progress can advance at a fine
/// granularity while clustering stays coarse.
///
/// 对一大批**已落库**的项跑（昂贵的）增量聚类，然后清空队列。与 `flush_face_rows` 分离，使
/// 状态/进度可细粒度推进、聚类保持粗粒度。`cluster_new_faces` 自做短读+短写，不持外层写锁。
fn flush_cluster(state: &Arc<AppState>, cluster_pending: &mut Vec<i64>, profile: &FaceProfile) {
    // 阈值取「运行期 override 或 profile 默认」——可不重编译调参做实测比较（无 override 时同改动前）。
    let (threshold, min_quality) = crate::ai::face_cluster::effective_thresholds(state, profile);
    crate::ai::face_cluster::cluster_new_faces(
        state,
        cluster_pending,
        &profile.id,
        threshold,
        min_quality,
    );
    cluster_pending.clear();
}

/// Mark a batch of failed items as `face_status=Error` (mirrors CLIP's `flush_failed`).
/// 把一批失败项标记为 `face_status=Error`（镜像 CLIP 的 `flush_failed`）。
fn flush_face_failed(state: &Arc<AppState>, failed_ids: &mut Vec<i64>) {
    let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = batch_update_face_status(&conn, failed_ids, FaceStatus::Error.as_i64()) {
        warn!(
            "Failed to mark face items as error | 标记人脸项为错误失败: {}",
            e
        );
    }
    failed_ids.clear();
}

// ── worker 派发路径(face 接线波;T16 起为唯一路径,进程内 ort 已删)──────────────────

/// worker 端可解码的源格式白名单(ai-worker 用纯 `image` crate 解码,无 WIC/exotic 引擎;
/// 与 ai-worker Cargo.toml 的 image features 对齐)。缩略图档位与 face 缓存源恒为 webp(可解);
/// 白名单外(heic/raw/psd 等)的原图回退项恒走 host 预解码的 face 缓存(方案 A,WIC 引擎可解
/// heic 等)——原「exotic 原图跳过」的过渡缺口就此收敛;仅 detect_size 超缓存尺寸的未来模型
/// 防呆分支仍会跳过(见 dispatch_face_batch 阶段1)。
const WORKER_DECODABLE_FORMATS: &[&str] =
    &["jpg", "jpeg", "png", "webp", "bmp", "gif", "tif", "tiff"];

/// 攒批的空闲刷新周期(与 CLIP worker 派发同值)。
const WORKER_FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

/// face 单批派发上限(2026-07-03 GUI 实测拍板):worker 人脸推理逐图进行(YuNet 固定
/// 640 输入),协议批只摊薄毫秒级的 IPC 往返,吞吐与批大小无关;批越大,单请求超时
/// 敞口与取消/落库粒度越差。CLIP 的 VRAM 自适应 batch(可达 64)对 face 无意义,
/// 派发时收窄到本值(方案 A 后源恒为 640 级小图,16 已兼顾 IPC 摊薄与取消/落库粒度)。
const FACE_DISPATCH_BATCH: usize = 16;

/// face 有效派发批 = min(会话声明 batch, FACE_DISPATCH_BATCH),至少 1。
fn face_dispatch_cap(session_batch: u32) -> usize {
    (session_batch as usize).clamp(1, FACE_DISPATCH_BATCH)
}

/// worker 派发路径主入口(由 run_face_pipeline_blocking 无条件调用;T16 后为唯一路径)。
/// Producer/Writer 与进程内共用;中段 = 单派发线程攒批 → FaceDetectEmbed(解码在 worker)。
fn run_face_pipeline_worker_blocking(
    state: &Arc<AppState>,
    token: &CancellationToken,
) -> crate::error::Result<()> {
    // profile 纯由配置解析(零进程内引擎):enabled 门与激活轨语义与引擎加载完全同源。
    let Some(face_profile) = crate::ipc::ai_commands::active_face_profile(state) else {
        return Err(AppError::Internal(
            "人脸功能未启用(face_enabled=0)或无激活轨".into(),
        ));
    };
    let clip_profile = crate::ipc::ai_commands::active_profile(state);
    let spec = crate::ai::worker_client::build_session_spec(
        state,
        clip_profile,
        Some(face_profile.clone()),
    );
    let profile = Arc::new(face_profile);

    recover_orphaned_face_items(state);

    let read_conn = state.db_read_pool.get()?;
    let total = count_pending_face_items(&read_conn)?;
    drop(read_conn);
    info!(
        "Face worker 流水线启动:待分析 {total} 张(backend=worker, face={}, batch={})",
        profile.id,
        face_dispatch_cap(spec.batch_size)
    );

    let (task_tx, task_rx) = bounded::<FaceTask>(CHANNEL_CAPACITY);
    let (result_tx, result_rx) = bounded::<FaceResult>(CHANNEL_CAPACITY);

    // 派发线程的批级致命错误经此带出 scope(同 CLIP worker 派发的手法)。
    let fatal: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

    let token_prod = token.clone();
    let state_prod = Arc::clone(state);
    let token_writer = token.clone();
    let state_writer = Arc::clone(state);
    let profile_writer = Arc::clone(&profile);

    rayon::scope(|s| {
        s.spawn(|_| {
            produce_face_tasks(&state_prod, task_tx, &token_prod);
        });

        let state_disp = Arc::clone(state);
        let token_disp = token.clone();
        let fatal_ref = &fatal;
        let spec_ref = &spec;
        let profile_disp = Arc::clone(&profile);
        s.spawn(move |_| {
            if let Err(e) = face_dispatch_loop(
                &state_disp,
                spec_ref,
                &profile_disp,
                task_rx,
                result_tx,
                &token_disp,
            ) {
                *fatal_ref.lock().unwrap_or_else(|p| p.into_inner()) = Some(e);
            }
        });

        s.spawn(move |_| {
            write_face_results(&state_writer, result_rx, &token_writer, &profile_writer);
        });
    });

    // provider 回声落库(T16)须在 close_session 之前——快照随 close 清空。
    crate::ipc::ai_commands::persist_provider_echo(state);
    // 结束即卸会话(自然完成/取消皆是;对齐 CLIP worker 派发「运行结束即 close_session」)。
    state
        .ai_worker
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .close_session();

    match fatal.into_inner().unwrap_or_else(|p| p.into_inner()) {
        Some(e) => Err(AppError::System(format!("Face worker 派发终止:{e}"))),
        None => Ok(()),
    }
}

/// 派发批次累计诊断(2026-07-03 性能取证):随批输出墙钟与解码源构成,回答「慢在哪」。
/// 原图源占比高且单项耗时大 ⇒ worker 端全尺寸解码主导;worker 侧分段耗时(解码/检测/
/// 嵌入)见其 stderr 的「FaceDetectEmbed 批诊断」行,两侧日志对照即可定位瓶颈段。
#[derive(Default)]
struct FaceDispatchStats {
    /// 已派发并返回的项数(不含格式跳过项)。
    items: u64,
    /// face_detect_embed 往返墙钟累计(ms,含 IPC 与重试)。
    wall_ms: u128,
    /// 解码源构成:分档缩略图 / face 缓存(方案 A) / 原图直派。
    thumb: u64,
    cache: u64,
    orig: u64,
    /// 瞬态失败/防呆回退不可派项的跳过数(保持 Processing,下次运行恢复)。
    skipped: u64,
}

/// 单项派发计划(方案 A 三级定源的产物,与批内 tasks 同序)。
enum FacePlan {
    /// 源已可派(缩略图档位,或小原图直派)。
    Ready(FaceDecodeSource),
    /// 走 face 缓存;`predecode`=缓存缺失,须本批现场预解码。
    UseCache { predecode: bool },
    /// 不可派(detect_size 超缓存尺寸的防呆 + worker 不可解格式),保持 Processing。
    Skip,
}

/// 原图回退项是否应升级走 face 缓存(方案 A 决策核,纯函数可单测)。
/// - 防呆:未来 detect_size > 缓存短边(640)的模型不得吃偏小缓存(镜像 ai_cache 的
///   「336 绑定模型集」警示,但这里是运行期防护而非注释约定);
/// - 走缓存的条件:降采样有收益(原图短边 > 缓存短边,worker 解码量级级下降),或 worker
///   压根不可解(exotic 原图,host WIC 预解码是唯一通路);短边本就 ≤640 的小图直派更省
///   (预解码不缩尺寸,徒增一次编解码与盘占)。
fn face_cache_applies(width: i64, height: i64, decodable: bool, detect_size: u32) -> bool {
    if detect_size > FACE_CACHE_SHORT_EDGE {
        return false;
    }
    let downscale_wins =
        width > 0 && height > 0 && (width.min(height) as u32) > FACE_CACHE_SHORT_EDGE;
    downscale_wins || !decodable
}

/// face 派发线程主循环:攒批 → 派发;通道关闭(生产者收尾)时刷余批后退出。
fn face_dispatch_loop(
    state: &Arc<AppState>,
    spec: &crate::ai::worker_client::SessionSpec,
    profile: &FaceProfile,
    task_rx: Receiver<FaceTask>,
    result_tx: Sender<FaceResult>,
    token: &CancellationToken,
) -> std::result::Result<(), String> {
    let batch_cap = face_dispatch_cap(spec.batch_size);
    let mut buf: Vec<FaceTask> = Vec::with_capacity(batch_cap);
    // 性能取证(2026-07-03):批墙钟/解码源构成/跳过数累计,循环尾输出总结。
    let mut stats = FaceDispatchStats::default();

    loop {
        if token.is_cancelled() {
            info!("Face worker 派发已取消 | face dispatcher cancelled");
            break;
        }
        match task_rx.recv_timeout(WORKER_FLUSH_TIMEOUT) {
            Ok(task) => {
                buf.push(task);
                if buf.len() >= batch_cap {
                    dispatch_face_batch(
                        state, spec, profile, &mut buf, &result_tx, token, &mut stats,
                    )?;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if !buf.is_empty() {
                    dispatch_face_batch(
                        state, spec, profile, &mut buf, &result_tx, token, &mut stats,
                    )?;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                if !buf.is_empty() {
                    dispatch_face_batch(
                        state, spec, profile, &mut buf, &result_tx, token, &mut stats,
                    )?;
                }
                break;
            }
        }
    }

    if stats.skipped > 0 {
        // 不静默截断(工作约定):跳过项保持 Processing,下次运行回 Pending。
        warn!(
            "Face worker 派发跳过 {} 项(瞬态失败,或 detect_size 超 face 缓存尺寸的防呆 \
             回退且源格式 worker 不可解;保持 Processing,下次运行恢复)",
            stats.skipped
        );
    }
    if stats.items > 0 {
        info!(
            "Face 派发总结:{} 项,批往返累计 {}ms,均 {}ms/项;解码源 缩略图 {} / face缓存 {} / 原图 {}",
            stats.items,
            stats.wall_ms,
            stats.wall_ms / stats.items as u128,
            stats.thumb,
            stats.cache,
            stats.orig
        );
    }
    info!("Face worker 派发完成 | face dispatcher finished");
    Ok(())
}

/// 派发一批:CPU permit → 三级定源(缩略图 → face 缓存 → 小原图)→ 缺缓存现场预解码
/// (方案 A,rayon 并行,镜像 CLIP 的 T18/T18.5)→ GPU 令牌 → FaceDetectEmbed →
/// 逐项映射落结果。返回 Err = 批级致命(终止本轮);取消返回 Ok 且清空 buf(在途项
/// 保持 Processing)。
fn dispatch_face_batch(
    state: &Arc<AppState>,
    spec: &crate::ai::worker_client::SessionSpec,
    profile: &FaceProfile,
    buf: &mut Vec<FaceTask>,
    result_tx: &Sender<FaceResult>,
    token: &CancellationToken,
    stats: &mut FaceDispatchStats,
) -> std::result::Result<(), String> {
    let tasks: Vec<FaceTask> = std::mem::take(buf);
    let thresh = profile.det_score_thresh;

    // D2 顺序天条:先 CPU permit(公平后台池)后 GPU 令牌;None = 已取消,直接收手
    // (本批项保持 Processing,下次运行恢复)。两者随作用域 Drop 释放。
    // permit 提前到定源/预解码之前(镜像 CLIP dispatch_batch):现场预解码是重 CPU 解码,
    // 必须在后台池配额内。
    let Some(_cpu_permit) = state.background_heavy_limiter.acquire(token) else {
        return Ok(());
    };

    let cache_dir = state.thumb_config.read().unwrap().cache_dir.clone();

    // 阶段1:三级定源(纯内存决策 + face 缓存存在性 stat),与 tasks 同序。
    let plans: Vec<FacePlan> = tasks
        .iter()
        .map(|t| {
            // 与进程内同一决策:thumbnails 档位预测短边 ≥ detect_size 才用缩略图。
            let src = resolve_face_decode_source(t, state, profile.detect_size);
            if src.kind == FaceSourceKind::Thumb {
                return FacePlan::Ready(src);
            }
            let decodable =
                WORKER_DECODABLE_FORMATS.contains(&src.format.to_ascii_lowercase().as_str());
            if face_cache_applies(t.width, t.height, decodable, profile.detect_size) {
                FacePlan::UseCache {
                    predecode: !face_cache_path(&cache_dir, t.cache_key).exists(),
                }
            } else if decodable {
                // 小原图直派(短边 ≤ 缓存尺寸,预解码无收益),或未来 detect_size 超
                // 缓存尺寸的防呆回退(全尺寸解码,慢但正确)。
                FacePlan::Ready(src)
            } else {
                // 仅防呆分支会走到:detect_size 超缓存尺寸 + worker 不可解格式。
                stats.skipped += 1;
                FacePlan::Skip
            }
        })
        .collect();

    // 阶段2:缺缓存现场预解码(方案 A,镜像 CLIP 的 T18.5):rayon 全局池并行,WIC 优先/
    // image crate 回退,tmp→rename 原子落盘;CPU permit 保持「1 批=1 槽」记账。取消检查
    // 在每项开工前:已落盘项幂等可复用,未开工项随本批放弃(保持 Processing)。
    let predecode_failed: std::collections::HashSet<i64> = tasks
        .par_iter()
        .zip(plans.par_iter())
        .filter(|(_, p)| matches!(p, FacePlan::UseCache { predecode: true }))
        .filter_map(|(t, _)| {
            if token.is_cancelled() {
                return None;
            }
            crate::derive::image::generate_face_cache(
                &cache_dir,
                t.cache_key,
                &t.file_format,
                &t.source_path,
            )
            .err()
            .map(|e| {
                // 双引擎(WIC+image crate)都解不开 → 标 Error(镜像 CLIP T18 派生失败
                // 语义;worker 端同为 image crate,回退直派几无胜算),不连坐整批。
                warn!("item {} face 缓存现场预解码失败:{e}(标 Error)", t.item_id);
                t.item_id
            })
        })
        .collect();
    if token.is_cancelled() {
        return Ok(()); // 预解码中途取消:本批项保持 Processing,下次运行恢复。
    }

    // 阶段3:装配协议项。
    let mut items: Vec<exotic_protocol::FaceItem> = Vec::with_capacity(tasks.len());
    let mut item_ids: Vec<i64> = Vec::with_capacity(tasks.len());
    // 本批解码源构成(缩略图 / face 缓存 / 原图):慢批定位的第一信号,与 worker 侧分段耗时对照。
    let (mut n_thumb, mut n_cache, mut n_orig) = (0u64, 0u64, 0u64);
    for (t, plan) in tasks.iter().zip(plans) {
        let src = match plan {
            FacePlan::Skip => continue,
            FacePlan::UseCache { .. } => {
                if predecode_failed.contains(&t.item_id) {
                    let _ = result_tx.send(FaceResult {
                        item_id: t.item_id,
                        records: None,
                    });
                    continue;
                }
                FaceDecodeSource {
                    path: face_cache_path(&cache_dir, t.cache_key),
                    format: "webp".to_string(),
                    kind: FaceSourceKind::FaceCache,
                }
            }
            FacePlan::Ready(src) => src,
        };
        match src.kind {
            FaceSourceKind::Thumb => n_thumb += 1,
            FaceSourceKind::FaceCache => n_cache += 1,
            FaceSourceKind::Original => n_orig += 1,
        }
        items.push(exotic_protocol::FaceItem {
            item_id: t.item_id,
            cache_key: None,
            // 信任语义同 Thumbnail.source_path:host 给绝对路径(缩略图档位/face 缓存/原图)。
            source_path: Some(src.path.to_string_lossy().into_owned()),
            // 回声核对指纹:检测阈值是行为参数(同图不同阈值不同结果),纳入其中。
            fingerprint: format!("{}:{:.4}", t.item_id, thresh),
        });
        item_ids.push(t.item_id);
    }
    if items.is_empty() {
        return Ok(());
    }

    let Some(_gpu_permit) = state.gpu_token.acquire(token) else {
        return Ok(());
    };

    let t0 = std::time::Instant::now();
    let outcomes = {
        let mut client = state.ai_worker.lock().unwrap_or_else(|p| p.into_inner());
        client.face_detect_embed(spec, &items, thresh, &|| token.is_cancelled())
    };
    let outcomes = match outcomes {
        Ok(o) => o,
        Err(e) => {
            // client 已做硬止损(重建重发一次);到这里即系统性失败,终止本轮。
            return Err(e.to_string());
        }
    };
    // 性能取证(2026-07-03):批墙钟含 IPC 往返与 worker 全链(解码/检测/嵌入);
    // 原图源=worker 端全尺寸解码,分段耗时见 worker stderr 的「FaceDetectEmbed 批诊断」。
    let wall_ms = t0.elapsed().as_millis();
    stats.items += items.len() as u64;
    stats.wall_ms += wall_ms;
    stats.thumb += n_thumb;
    stats.cache += n_cache;
    stats.orig += n_orig;
    info!(
        "Face 批:{} 项(缩略图源 {} / face缓存 {} / 原图源 {}) {}ms,均 {}ms/项;累计 {} 项,{:.2} 项/s",
        items.len(),
        n_thumb,
        n_cache,
        n_orig,
        wall_ms,
        wall_ms / items.len() as u128,
        stats.items,
        stats.items as f64 / (stats.wall_ms.max(1) as f64 / 1000.0)
    );

    for (item_id, outcome) in item_ids.into_iter().zip(outcomes) {
        match outcome {
            crate::exotic::worker::FaceItemOutcome::Ok {
                faces,
                embeddings,
                width,
                height,
            } => {
                // 协议 FaceDet 与 DetectedFace 字段同构,搬回后与进程内共用同一映射
                // (归一化按协议回报的实际解码尺寸,零脸也是 Ok → Done)。
                let det: Vec<DetectedFace> = faces
                    .iter()
                    .map(|f| DetectedFace {
                        bbox: f.bbox,
                        landmarks: f.landmarks,
                        score: f.score,
                    })
                    .collect();
                let records = faces_to_records(item_id, &det, &embeddings, width, height);
                let _ = result_tx.send(FaceResult {
                    item_id,
                    records: Some(records),
                });
            }
            crate::exotic::worker::FaceItemOutcome::Err(code) if code.default_retryable() => {
                // 瞬态(IoError 等):跳过,保持 Processing。
                stats.skipped += 1;
            }
            crate::exotic::worker::FaceItemOutcome::Err(code) => {
                // terminal(MalformedInput 等):标 Error,不再无限重查。
                warn!(
                    "item {item_id} 人脸检测/嵌入失败[{}](terminal)",
                    code.as_str()
                );
                let _ = result_tx.send(FaceResult {
                    item_id,
                    records: None,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 方案 A 决策核矩阵:防呆(detect_size 超缓存短边)恒 false;大图/不可解格式走缓存;
    /// 小图或尺寸未知的可解格式直派。
    #[test]
    fn face_cache_applies_matrix() {
        // 大图(短边 > 640):无论 worker 是否可解,都走缓存(降采样收益量级级)。
        assert!(face_cache_applies(4000, 3000, true, 640));
        assert!(face_cache_applies(4000, 3000, false, 640));
        // 小图(短边 ≤ 640):可解格式直派;不可解格式仍须缓存(host 预解码是唯一通路)。
        assert!(!face_cache_applies(800, 600, true, 640));
        assert!(face_cache_applies(800, 600, false, 640));
        // 宽幅全景:短边 600 ≤ 640 → 可解直派(长边虽大,沿用既有直派语义)。
        assert!(!face_cache_applies(6000, 600, true, 640));
        // 尺寸未知(0):可解直派(维持既有语义),不可解走缓存兜底。
        assert!(!face_cache_applies(0, 0, true, 640));
        assert!(face_cache_applies(0, 0, false, 640));
        // 防呆:detect_size 超缓存短边 → 一律不吃偏小缓存(可解回退全尺寸,不可解跳过)。
        assert!(!face_cache_applies(4000, 3000, true, 1024));
        assert!(!face_cache_applies(4000, 3000, false, 1024));
    }

    /// 边界:短边恰等于 640 → 缓存无降采样收益,可解格式直派;641 起走缓存。
    #[test]
    fn face_cache_applies_boundary_equal() {
        assert!(!face_cache_applies(640, 960, true, 640));
        assert!(face_cache_applies(641, 960, true, 640));
    }
}
