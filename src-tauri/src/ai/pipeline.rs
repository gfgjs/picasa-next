// src-tauri/src/ai/pipeline.rs
//! Background AI analysis pipeline.
//! 后台 AI 分析流水线。
//!
//! Architecture (matches thumbnail pipeline pattern):
//! 架构（与缩略图流水线模式匹配）：
//!
//!  Producer thread → crossbeam channel → Consumer pool (rayon) → Writer
//!  生产者线程 → crossbeam 通道 → 消费者池（rayon）→ 写入器
//!
//! 1. Producer: batch-query media_items WHERE ai_status=0, send (id, thumb_path) to channel
//!    生产者：批量查询 ai_status=0 的 media_items，发送 (id, thumb_path) 到通道
//! 2. Consumer (rayon): read thumbnail bytes → CLIP image inference → send embedding to result_tx
//!    消费者（rayon）：读取缩略图字节 → CLIP 图像推理 → 发送嵌入向量到 result_tx
//! 3. Writer: batch-collect results (128/tx), write to ai_embeddings, update ai_status=2
//!    写入器：批量收集结果（128/事务），写入 ai_embeddings，更新 ai_status=2
//! 4. Each batch checks should_yield() + CancellationToken
//!    每批检查 should_yield() + CancellationToken

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver, Sender};
use ndarray::Array4;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::ai::clip::embedding_to_bytes;
use crate::ai::profile::ModelProfile;
use crate::db::models::AiStatus;
use crate::db::queries::{
    batch_update_ai_status, batch_upsert_ai_embeddings, count_pending_ai_items,
    get_pending_ai_items,
};
use crate::engine::gpu::get_gpu_engine;
use crate::engine::traits::ResizeHint;
use crate::error::AppError;
use crate::state::AppState;

/// Batch size for reading from DB and writing embeddings.
/// 从数据库读取和写入嵌入向量的批次大小。
const BATCH_SIZE: i64 = 512;

/// Channel capacity between producer and consumers.
/// 生产者和消费者之间的通道容量。
const CHANNEL_CAPACITY: usize = 1024;

/// Task item sent from producer to consumers.
/// 从生产者发送到消费者的任务项。
///
/// Carries thumbnail/dimension hints so the preprocessor can pick the **cheapest sufficient**
/// decode source (a large-enough thumbnail instead of the full-resolution original) — see
/// `resolve_decode_source`.
/// 携带缩略图/尺寸提示，使预处理器可选择**最廉价且足够**的解码源（足够大的缩略图而非全分辨率原图）
/// —— 见 `resolve_decode_source`。
struct AiTask {
    item_id: i64,
    source_path: PathBuf,
    file_format: String,
    /// 用于按文件存在性定位 AI 缓存（`ai_cache_path(cache_dir, cache_key)`）—— 最高优先级解码源。
    cache_key: i64,
    thumb_status: i64,
    thumb_path: Option<String>,
    width: i64,
    height: i64,
}

/// Task sent from preprocessor to inferencer.
/// 从预处理器发送到推理器的任务。
struct InferenceTask {
    item_id: i64,
    tensor: Array4<f32>,
}

/// Embedding result sent from consumers to writer.
/// 从消费者发送到写入器的嵌入向量结果。
struct AiResult {
    item_id: i64,
    /// `Some(bytes)` on success, `None` on inference failure.
    /// 成功时为 `Some(bytes)`，推理失败时为 `None`。
    embedding: Option<Vec<u8>>,
}

/// Start the background AI analysis pipeline.
/// 启动后台 AI 分析流水线。
///
/// Returns immediately; all work is done in background threads.
/// 立即返回；所有工作在后台线程中完成。
pub fn start_ai_pipeline(state: Arc<AppState>, token: CancellationToken) {
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let start_time = std::time::Instant::now();
        // Keep a token handle so that, after the blocking run returns, we can tell a
        // natural completion from a pause/stop cancellation (问题7).
        // 保留一个 token 句柄，使阻塞运行返回后能区分自然完成与暂停/停止取消（问题7）。
        let token_outer = token.clone();
        // Run blocking work in a spawn_blocking to avoid blocking the async runtime.
        // 在 spawn_blocking 中运行阻塞工作，避免阻塞异步运行时。
        let result =
            tokio::task::spawn_blocking(move || run_pipeline_blocking(&state_clone, &token)).await;

        let elapsed_ms = start_time.elapsed().as_millis();
        match result {
            Ok(Ok(())) => info!(
                "AI analysis pipeline completed: elapsed={}ms | AI 分析流水线完成: 耗时={}ms",
                elapsed_ms, elapsed_ms
            ),
            Ok(Err(e)) => warn!("AI analysis pipeline error | AI 分析流水线错误: {}", e),
            Err(e) => warn!("AI analysis task panicked | AI 分析任务崩溃: {}", e),
        }

        // If the run finished naturally (not cancelled by pause/stop), clear the
        // auto-resume flag — there's nothing left to resume. A cancellation leaves the
        // flag as the pause/stop command set it. (问题7)
        // 若自然完成（未被暂停/停止取消），清除自动续传标志——无可续传。被取消则保留标志为
        // 暂停/停止命令设定的值。（问题7）
        if !token_outer.is_cancelled() {
            let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            let _ = crate::db::queries::set_config(&conn, "ai_analysis_active", "0");
            drop(conn);
            // Release the shared GPU-analysis slot ONLY on natural completion (F5 mutual
            // exclusion). A cancellation is either pause/stop (which release the slot in the
            // command) or restart (cancel-then-relaunch the SAME pipeline, which must KEEP the
            // slot) — releasing here on cancel would let the just-relaunched run lose its slot.
            // 仅在自然完成时释放共享 GPU 分析槽（F5 互斥）。被取消的情形要么是暂停/停止（由命令
            // 释放槽），要么是 restart（取消后重启同一流水线，须保持持有）——此处若在取消时释放，
            // 会让刚重启的运行丢掉槽位。
            state.release_gpu_analysis(crate::state::GPU_OWNER_AI);
        }

        // Clear the token from state after completion.
        // 完成后从状态中清除令牌。
        state.cancel_ai_analysis();

        // Release AI engine to free up VRAM after analysis ends. Time the unload so the
        // VRAM-release lag after a manual stop is observable (问题8). NOTE: dropping the
        // ORT/DirectML session does not always reclaim VRAM instantly — that's a driver /
        // runtime behaviour — but we ensure the session is dropped here without waiting on
        // any further pipeline work.
        // 结束后卸载 AI 引擎以释放显存。对卸载计时，使手动停止后的显存释放延迟可观测（问题8）。
        // 注意：丢弃 ORT/DirectML 会话不一定立即回收显存（驱动/运行时行为），但我们在此确保
        // 会话被立即丢弃、不再等待任何后续流水线工作。
        let unload_start = std::time::Instant::now();
        if let Ok(mut engine) = state.ai_engine.write() {
            *engine = None;
            info!(
                "AI engine unloaded to release VRAM (drop took {}ms) | AI 引擎已卸载以释放显存（drop 耗时 {}ms）",
                unload_start.elapsed().as_millis(), unload_start.elapsed().as_millis()
            );
        }
    });
}

/// Blocking pipeline runner — runs inside spawn_blocking + rayon.
/// 阻塞式流水线运行器 — 在 spawn_blocking + rayon 中运行。
fn run_pipeline_blocking(
    state: &Arc<AppState>,
    token: &CancellationToken,
) -> crate::error::Result<()> {
    // Get AI engine — bail if not ready
    // 获取 AI 引擎 — 未就绪则退出
    let clip_session = state
        .ai_engine
        .read()
        .unwrap()
        .as_ref()
        .and_then(|p| p.clip_image_session.clone())
        .ok_or_else(|| AppError::Internal("CLIP engine not initialized".into()))?;

    // Snapshot the active model profile once; share it (read-only) across all stage threads.
    // It drives image_size (decode short-edge + tensor shape), embed_dim, normalisation and the
    // `model_name` DB key — so the whole pipeline stays consistent for whichever model is loaded.
    // 一次性快照当前模型契约，只读共享给各阶段线程。它驱动 image_size（解码短边 + 张量形状）、
    // embed_dim、归一化与 `model_name` 主键 —— 使整条流水线对所加载模型保持一致。
    let profile = Arc::new(
        state
            .ai_engine
            .read()
            .unwrap()
            .as_ref()
            .map(|p| p.profile.clone())
            .ok_or_else(|| AppError::Internal("CLIP engine not initialized".into()))?,
    );

    // Resume support (问题7): release any items a previous run claimed but never finished
    // (ai_status=Processing — left behind by a crash / forced exit / pause / stop) back to
    // Pending so THIS run picks them up. Without this the producer (which only queries
    // status=0) would strand them forever.
    // 续传支持（问题7）：把上次运行已领取但未完成的项（ai_status=Processing——崩溃/强退/
    // 暂停/停止遗留）放回 Pending，使本次运行能接续。否则生产者（只查 status=0）会永久搁置它们。
    {
        let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        match crate::db::queries::reset_processing_ai_items(&conn) {
            Ok(n) if n > 0 => info!("Recovered {} orphaned AI items (processing → pending) | 恢复 {} 个孤儿 AI 项（处理中 → 待处理）", n, n),
            Ok(_) => {}
            Err(e) => warn!("Failed to recover orphaned AI items | 恢复孤儿 AI 项失败: {}", e),
        }
    }

    // Count how many items need processing
    // 统计需要处理的项数
    let read_conn = state.db_read_pool.get()?;
    let total = count_pending_ai_items(&read_conn)?;
    info!(
        "AI pipeline starting: {} images to analyse | AI 流水线启动：待分析 {} 张图像",
        total, total
    );
    drop(read_conn);

    // ── Channel setup ─────────────────────────────────────────────────────────
    // ── 通道设置 ─────────────────────────────────────────────────────────────
    let (task_tx, task_rx) = bounded::<AiTask>(CHANNEL_CAPACITY);
    let (inference_tx, inference_rx) = bounded::<InferenceTask>(CHANNEL_CAPACITY);
    let (result_tx, result_rx) = bounded::<AiResult>(CHANNEL_CAPACITY);

    let token_prod = token.clone();
    let token_writer = token.clone();
    let state_prod = Arc::clone(state);
    let state_writer = Arc::clone(state);

    rayon::scope(|s| {
        // ── Producer thread ───────────────────────────────────────────────────
        // ── 生产者线程 ───────────────────────────────────────────────────────
        s.spawn(|_| {
            produce_tasks(&state_prod, task_tx, &token_prod);
        });

        // ── Preprocessor threads (rayon thread pool) ──────────────────────────
        // ── 预处理线程（rayon 线程池）────────────────────────────────────────
        let state_consumer = Arc::clone(state);
        let token_consumer = token.clone();
        let result_tx_preprocess = result_tx.clone();
        let profile_pp = Arc::clone(&profile);
        s.spawn(move |_| {
            preprocess_tasks(
                task_rx,
                inference_tx,
                result_tx_preprocess,
                &state_consumer,
                &token_consumer,
                &profile_pp,
            );
        });

        // ── Inferencer thread ─────────────────────────────────────────────────
        // ── 推理线程 ──────────────────────────────────────────────────────────
        let session_clone = clip_session.clone();
        let token_inferencer = token.clone();
        let result_tx_inferencer = result_tx.clone();
        let state_inferencer = Arc::clone(state);
        let profile_inf = Arc::clone(&profile);
        s.spawn(move |_| {
            run_inference_tasks(
                inference_rx,
                result_tx_inferencer,
                session_clone,
                &state_inferencer,
                &token_inferencer,
                &profile_inf,
            );
        });

        // ── Writer thread ─────────────────────────────────────────────────────
        // ── 写入器线程 ────────────────────────────────────────────────────────
        let profile_writer = Arc::clone(&profile);
        s.spawn(move |_| {
            write_results(&state_writer, result_rx, &token_writer, &profile_writer);
        });

        drop(result_tx); // Close so writer can detect completion | 关闭以便写入器可以检测完成
    });

    Ok(())
}

/// Producer: batch-query pending items, push tasks to channel.
/// 生产者：批量查询待处理项，推送任务到通道。
fn produce_tasks(state: &Arc<AppState>, task_tx: Sender<AiTask>, token: &CancellationToken) {
    loop {
        if token.is_cancelled() {
            info!("AI producer cancelled | AI 生产者已取消");
            break;
        }

        // Yield to higher-priority tasks and log the concrete blocker for diagnostics.
        // 让步给更高优先级的任务，并记录具体阻塞源便于排查。
        let blockers = state.ai_yield_blockers();
        if !blockers.is_empty() {
            debug!(
                blockers = %blockers.join(","),
                "AI producer yielding to higher priority task | AI 生产者让步给高优先级任务"
            );
            std::thread::sleep(std::time::Duration::from_millis(500));
            continue;
        }

        let conn = match state.db_read_pool.get() {
            Ok(c) => c,
            Err(e) => {
                warn!("DB pool error in AI producer | AI 生产者 DB 池错误: {}", e);
                break;
            }
        };

        let batch = match get_pending_ai_items(&conn, BATCH_SIZE) {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    "Query pending AI items failed | 查询待处理 AI 项失败: {}",
                    e
                );
                break;
            }
        };

        if batch.is_empty() {
            info!("AI producer: no more pending items | AI 生产者：没有更多待处理项");
            break;
        }

        // Mark items as "processing" to avoid re-queuing on restart
        // 将项标记为"处理中"，避免重启时重新排队
        // Recover from a poisoned lock instead of panicking — a panic here would poison
        // the shared writer permanently and cascade into hangs/failures elsewhere (问题6).
        // 从中毒锁恢复而非 panic —— 此处 panic 会永久毒化共享写连接，并级联成别处的卡死/失败（问题6）。
        let write_conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<i64> = batch.iter().map(|it| it.id).collect();
        if let Err(e) = batch_update_ai_status(&write_conn, &ids, AiStatus::Processing.as_i64()) {
            warn!(
                "Failed to mark items as processing | 标记项为处理中失败: {}",
                e
            );
        }
        drop(write_conn);

        for item in batch {
            if token.is_cancelled() {
                break;
            }

            if task_tx
                .send(AiTask {
                    item_id: item.id,
                    source_path: PathBuf::from(item.abs_path),
                    file_format: item.file_format,
                    cache_key: item.cache_key,
                    thumb_status: item.thumb_status,
                    thumb_path: item.thumb_path,
                    width: item.width,
                    height: item.height,
                })
                .is_err()
            {
                break;
            }
        }
    }

    info!("AI producer finished | AI 生产者已完成");
}

/// Preprocessor: receive tasks, decode image, run CLIP preprocessing, send to inferencer.
/// 预处理器：接收任务，解码图像，运行 CLIP 预处理，发送到推理器。
fn preprocess_tasks(
    task_rx: Receiver<AiTask>,
    inference_tx: Sender<InferenceTask>,
    result_tx: Sender<AiResult>,
    state: &Arc<AppState>,
    token: &CancellationToken,
    profile: &Arc<ModelProfile>,
) {
    rayon::scope(|s| {
        for task in task_rx {
            if token.is_cancelled() {
                break;
            }

            let inference_tx = inference_tx.clone();
            let result_tx = result_tx.clone();
            let state = Arc::clone(state);
            let token_clone = token.clone();
            let profile = Arc::clone(profile);

            s.spawn(move |_| {
                if token_clone.is_cancelled() {
                    return;
                }

                match process_preprocess_task(&task, &state, &profile) {
                    Ok(tensor) => {
                        let _ = inference_tx.send(InferenceTask {
                            item_id: task.item_id,
                            tensor,
                        });
                    }
                    Err(e) => {
                        debug!(
                            "Preprocess failed for item {} | 项 {} 预处理失败: {}",
                            task.item_id, task.item_id, e
                        );
                        let _ = result_tx.send(AiResult {
                            item_id: task.item_id,
                            embedding: None,
                        });
                    }
                }
            });
        }
    });

    info!("AI preprocessors finished | AI 预处理器已完成");
}

/// A resolved decode source for CLIP preprocessing: which file to decode and as what format.
/// 已解析的解码源：解码哪个文件、按什么格式。
struct DecodeSource {
    path: PathBuf,
    format: String,
}

/// Pick the **cheapest decode source whose short edge still satisfies `image_size`** so the
/// preprocessor avoids decoding the full-resolution original whenever a large-enough thumbnail
/// already exists. This is the core CPU-saving lever: a 24MP JPEG must be fully entropy-decoded,
/// while a ~480px WebP thumbnail is two orders of magnitude cheaper — and the GPU stays fed
/// instead of starving on CPU decode.
///
/// Strategy (strict, no upscaling → zero extra quality loss): only use a generated tiered
/// thumbnail (`thumb_status==1`) whose **predicted short edge** (computed from the original
/// W×H + the tier long edge, WITHOUT touching disk) is `≥ image_size`. `thumb_status==3` stores
/// the original path itself (a small file) → just decode the original. Unknown dims, missing
/// path, or a thumbnail that's too small → fall back to the original.
///
/// 选择**短边仍满足 `image_size` 的最廉价解码源**，使预处理器在已有足够大缩略图时免去解码全分辨率原图。
/// 这是降 CPU 的核心杠杆：24MP JPEG 必须完整熵解码，而 ~480px 的 WebP 缩略图便宜两个数量级 ——
/// 从而让 GPU 持续有数据可吃，而非饿死在 CPU 解码上。
///
/// 策略（严格不上采样 → 零额外精度损失）：仅当「已生成的分档缩略图」(`thumb_status==1`) 的
/// **预测短边**（由原图 W×H + 档位长边算出，**不读盘**）`≥ image_size` 时才采用。
/// `thumb_status==3` 的 `thumb_path` 即原图（小文件）→ 直接解原图。尺寸未知 / 无路径 /
/// 缩略图太小 → 回退原图。
fn resolve_decode_source(task: &AiTask, state: &AppState, image_size: u32) -> DecodeSource {
    let original = DecodeSource {
        path: task.source_path.clone(),
        format: task.file_format.clone(),
    };

    // ── Priority 1: an AI cache file (short-edge≥336) — always satisfies any model ──
    // Discovery is by file existence keyed on cache_key, so caches built either by the `ai_thumb`
    // derivation OR as a byproduct of thumbnail generation are both picked up.
    // ── 优先级 1：AI 缓存文件（短边≥336）—— 永远满足任意模型 ──
    // 按 cache_key 的文件存在性发现，故 `ai_thumb` 派生或缩略图生成顺带产出的缓存都能命中。
    {
        let cache_dir = state.thumb_config.read().unwrap().cache_dir.clone();
        let ai_cache = crate::thumbnail::cache::ai_cache_path(&cache_dir, task.cache_key);
        if ai_cache.exists() {
            return DecodeSource {
                path: ai_cache,
                format: "webp".to_string(),
            };
        }
    }

    // ── Priority 2: a regular thumbnail whose short edge is already ≥ image_size ──
    // ── 优先级 2：短边已 ≥ image_size 的常规缩略图 ──
    if task.thumb_status != 1 || task.width <= 0 || task.height <= 0 {
        return original;
    }
    let Some(rel) = task.thumb_path.as_deref() else {
        return original;
    };

    // Parse the tier (long edge) from the rel path "{tier}/{prefix}/{hex}.webp".
    // status=3's thumb_path is an absolute original path → first segment won't parse →原图。
    // 从相对路径 "{档位}/{前缀}/{hex}.webp" 解析档位（长边）。status=3 的 thumb_path 是绝对原图路径
    // → 第一段无法解析为整数 → 回退原图。
    let Some(tier) = rel.split('/').next().and_then(|s| s.parse::<u32>().ok()) else {
        return original;
    };

    // Thumbnail is LongEdge(tier) but never upscaled → long edge = min(tier, max(W,H)).
    // Predict short edge; use the thumbnail only if it's ≥ image_size.
    // 缩略图按长边=tier 缩放但绝不放大 → 长边 = min(tier, max(W,H))；预测短边，仅当 ≥ image_size 时采用。
    let (w, h) = (task.width as u32, task.height as u32);
    let (long, short) = (w.max(h), w.min(h));
    let thumb_short = if long <= tier {
        short
    } else {
        (short as f32 * tier as f32 / long as f32).round() as u32
    };
    if thumb_short < image_size {
        return original;
    }

    let cache_dir = state.thumb_config.read().unwrap().cache_dir.clone();
    let thumb_full = cache_dir.join("thumbnails").join(rel);
    if thumb_full.exists() {
        // The thumbnail is already EXIF-upright (baked in at generation), so decoding it must NOT
        // re-apply orientation — the "webp" format keeps WIC's rotation branch (jpg/heic only) off.
        // 缩略图生成时已转正，故解码时绝不能再套方向 ——「webp」格式使 WIC 的旋转分支（仅 jpg/heic）不触发。
        DecodeSource {
            path: thumb_full,
            format: "webp".to_string(),
        }
    } else {
        original
    }
}

/// Process a single AI preprocess task: decode the cheapest sufficient source via ImageEngine
/// (GPU-accelerated WIC), then run CLIP preprocessing.
/// 处理单个 AI 预处理任务：通过 ImageEngine（GPU 加速 WIC）解码最廉价且足够的源，然后运行 CLIP 预处理。
fn process_preprocess_task(
    task: &AiTask,
    state: &AppState,
    profile: &ModelProfile,
) -> crate::error::Result<Array4<f32>> {
    let gpu_engine_name = state.thumb_config.read().unwrap().gpu_engine.clone();
    // Decode short-edge follows the model's input size (224 for B/16·L/14, 336 for L/14@336).
    // 解码短边跟随模型输入尺寸（B/16·L/14 为 224，L/14@336 为 336）。
    let resize_hint = Some(ResizeHint::ShortEdge(profile.image_size));

    // Prefer a sufficiently large thumbnail (or AI cache) over the full-resolution original.
    // 优先用足够大的缩略图（或 AI 缓存）而非全分辨率原图。
    let src = resolve_decode_source(task, state, profile.image_size);

    let decoded = match get_gpu_engine(&gpu_engine_name) {
        Some(gpu) if gpu.can_handle(&src.format) => match gpu.decode(&src.path, resize_hint) {
            Ok(d) => d,
            Err(e) => {
                debug!(
                    "GPU decode failed for item {}, falling back to CPU | 项 {} GPU 解码失败，回退 CPU: {}",
                    task.item_id, task.item_id, e
                );
                state
                    .engine_arena
                    .engine_for(&src.format)
                    .ok_or_else(|| crate::error::AppError::UnsupportedFormat(src.format.clone()))?
                    .decode(&src.path, resize_hint)?
            }
        },
        _ => state
            .engine_arena
            .engine_for(&src.format)
            .ok_or_else(|| crate::error::AppError::UnsupportedFormat(src.format.clone()))?
            .decode(&src.path, resize_hint)?,
    };

    Ok(crate::ai::clip::preprocess_decoded(&decoded, profile))
}

/// Inferencer: receive preprocessed tensors, dynamically batch them, and run CLIP inference.
/// 推理器：接收预处理后的张量，动态批处理它们，并运行 CLIP 推理。
fn run_inference_tasks(
    inference_rx: Receiver<InferenceTask>,
    result_tx: Sender<AiResult>,
    session_pool: crate::ai::engine::SessionPool,
    state: &Arc<AppState>,
    token: &CancellationToken,
    profile: &Arc<ModelProfile>,
) {
    let conn = state.db_read_pool.get().unwrap();
    let batch_size_str = crate::db::queries::get_config(&conn, "ai_batch_size").unwrap_or_default();

    let batch_size = {
        let mut val = batch_size_str
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        if val == 0 {
            // Auto detection based on VRAM
            let vram_bytes = crate::ai::provider::detect_vram_bytes();
            let gb = vram_bytes.map(|b| b / (1024 * 1024 * 1024)).unwrap_or(0);
            val = if gb >= 12 {
                256
            } else if gb >= 8 {
                128
            } else if gb >= 4 {
                64
            } else if gb >= 2 {
                32
            } else {
                16
            };
            tracing::info!(
                "AI Batch Size auto-configured to {} based on {}GB VRAM",
                val,
                gb
            );
        } else {
            // Hard limit to prevent naive OOMs
            if val > 256 {
                tracing::warn!(
                    "User requested batch size {} exceeds safe limit, clamping to 256",
                    val
                );
                val = 256;
            }
        }
        // 固定 batch 模型（图像塔 bN 导出）要求每次喂入 ≥ k 行才高效（不足 k 的块会被补齐浪费）；
        // 这里把有效 batch 抬到 ≥ k，与设置页的最小限制一致。动态 batch / 单批模型不受影响。
        if let Some(crate::ai::remote_registry::BatchKind::Fixed(k)) =
            crate::ai::remote_registry::parse_batch(&profile.image_file)
        {
            let k = k as usize;
            if k > 1 && val < k {
                tracing::info!(
                    "Active model is fixed-batch k={}, raising batch size {} → {}",
                    k,
                    val,
                    k
                );
                val = k;
            }
        }
        val
    };
    drop(conn);

    let mut batch = Vec::with_capacity(batch_size);
    let timeout = std::time::Duration::from_millis(50);

    loop {
        if token.is_cancelled() {
            info!("AI inferencer cancelled | AI 推理器已取消");
            break;
        }

        match inference_rx.recv_timeout(timeout) {
            Ok(task) => {
                batch.push(task);
                if batch.len() >= batch_size {
                    // Don't START a new (heavy, uninterruptible) GPU inference once a stop
                    // was requested — discard the accumulated batch and exit so the ORT
                    // session (and its VRAM) is dropped promptly instead of after one more
                    // full batch runs (问题8). The discarded items stay Processing and are
                    // recovered to Pending on the next run (问题7).
                    // 一旦收到停止，就不再「启动」新的（重型且不可中断的）GPU 推理 —— 丢弃已累积
                    // 的批次并退出，使 ORT 会话（及其显存）尽快释放，而非再跑完一整批（问题8）。
                    // 被丢弃的项保持 Processing，下次运行时恢复为 Pending（问题7）。
                    if token.is_cancelled() {
                        info!("AI inferencer cancelled before flush; discarding {} queued tensors | 推理器在 flush 前被取消，丢弃 {} 个排队张量", batch.len(), batch.len());
                        batch.clear();
                        break;
                    }
                    flush_inference_batch(&mut batch, &session_pool, &result_tx, profile);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if !batch.is_empty() {
                    flush_inference_batch(&mut batch, &session_pool, &result_tx, profile);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                if !batch.is_empty() {
                    flush_inference_batch(&mut batch, &session_pool, &result_tx, profile);
                }
                break;
            }
        }
    }

    info!("AI inferencer finished | AI 推理器已完成");
}

/// Flush a batch of tensors to the CLIP inference engine.
/// 将一批张量刷新到 CLIP 推理引擎。
fn flush_inference_batch(
    batch: &mut Vec<InferenceTask>,
    session_pool: &crate::ai::engine::SessionPool,
    result_tx: &Sender<AiResult>,
    profile: &ModelProfile,
) {
    if batch.is_empty() {
        return;
    }

    let views: Vec<_> = batch.iter().map(|t| t.tensor.view()).collect();
    let batch_tensor = match ndarray::concatenate(ndarray::Axis(0), &views) {
        Ok(tensor) => tensor,
        Err(e) => {
            warn!("Failed to concatenate tensors | 拼接张量失败: {}", e);
            for task in batch.drain(..) {
                let _ = result_tx.send(AiResult {
                    item_id: task.item_id,
                    embedding: None,
                });
            }
            return;
        }
    };

    match crate::ai::clip::encode_image_batch(session_pool, batch_tensor, profile) {
        Ok(embeddings) => {
            for (task, embedding) in batch.drain(..).zip(embeddings) {
                let bytes = embedding_to_bytes(&embedding);
                let _ = result_tx.send(AiResult {
                    item_id: task.item_id,
                    embedding: Some(bytes),
                });
            }
        }
        Err(e) => {
            warn!("Batch inference failed | 批量推理失败: {}", e);
            for task in batch.drain(..) {
                let _ = result_tx.send(AiResult {
                    item_id: task.item_id,
                    embedding: None,
                });
            }
        }
    }
}

/// Writer: batch-collect results and write to DB.
/// 写入器：批量收集结果并写入 DB。
fn write_results(
    state: &Arc<AppState>,
    result_rx: Receiver<AiResult>,
    token: &CancellationToken,
    profile: &ModelProfile,
) {
    let mut batch: Vec<(i64, String, Vec<u8>, i64)> = Vec::with_capacity(BATCH_SIZE as usize);
    let mut total_written = 0u64;

    let mut failed_ids: Vec<i64> = Vec::new();

    for result in result_rx {
        if token.is_cancelled() {
            info!("AI writer cancelled | AI 写入器已取消");
            break;
        }

        match result.embedding {
            Some(emb) => {
                batch.push((result.item_id, profile.id.clone(), emb, 1));
            }
            None => {
                // Inference failed — collect for bulk status update
                // 推理失败 — 收集起来批量更新状态
                failed_ids.push(result.item_id);
            }
        }

        if batch.len() >= BATCH_SIZE as usize {
            flush_batch(state, &mut batch, &mut total_written);
        }

        if failed_ids.len() >= BATCH_SIZE as usize {
            flush_failed(state, &mut failed_ids);
        }
    }

    // Flush remaining items
    // 刷新剩余项
    if !batch.is_empty() {
        flush_batch(state, &mut batch, &mut total_written);
    }
    if !failed_ids.is_empty() {
        flush_failed(state, &mut failed_ids);
    }

    info!(
        "AI writer finished, total embeddings written: {} | AI 写入器完成，总共写入嵌入向量: {}",
        total_written, total_written
    );
}

/// Flush a batch of embeddings to the database.
/// 将一批嵌入向量刷新到数据库。
fn flush_batch(
    state: &Arc<AppState>,
    batch: &mut Vec<(i64, String, Vec<u8>, i64)>,
    total_written: &mut u64,
) {
    let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
    let ids: Vec<i64> = batch.iter().map(|(id, _, _, _)| *id).collect();

    match batch_upsert_ai_embeddings(&conn, batch) {
        Ok(()) => {
            // Update status to Done for successfully processed items
            // 将成功处理的项状态更新为已完成
            if let Err(e) = batch_update_ai_status(&conn, &ids, AiStatus::Done.as_i64()) {
                warn!(
                    "Failed to update ai_status to done | 更新 ai_status 为已完成失败: {}",
                    e
                );
            }
            *total_written += ids.len() as u64;
            debug!(
                "Flushed {} embeddings to DB | 已将 {} 个嵌入向量刷新到 DB",
                ids.len(),
                ids.len()
            );
            // New embeddings landed — invalidate the resident cache so the next search reloads.
            // 新嵌入向量已写入 —— 使常驻缓存失效，下次搜索将重新加载。
            drop(conn);
            state.invalidate_embedding_cache();
        }
        Err(e) => {
            warn!("Batch embedding write failed | 批量嵌入向量写入失败: {}", e);
            // Mark failed items as error so they are not re-processed indefinitely
            // 将失败的项标记为错误，避免无限重新处理
            let _ = batch_update_ai_status(&conn, &ids, AiStatus::Error.as_i64());
        }
    }

    batch.clear();
}

/// Mark a batch of failed items as `ai_status=3` (Error) in the database.
/// 将一批失败的项在数据库中标记为 `ai_status=3`（错误）。
fn flush_failed(state: &Arc<AppState>, failed_ids: &mut Vec<i64>) {
    let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = batch_update_ai_status(&conn, failed_ids, AiStatus::Error.as_i64()) {
        warn!("Failed to mark items as error | 标记项为错误失败: {}", e);
    } else {
        debug!(
            "Marked {} items as ai_status=Error | 已将 {} 个项标记为 ai_status=Error",
            failed_ids.len(),
            failed_ids.len()
        );
    }
    failed_ids.clear();
}
