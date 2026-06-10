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

use ndarray::Array4;
use crossbeam_channel::{bounded, Receiver, Sender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::ai::clip::{embedding_to_bytes, MODEL_NAME};
use crate::db::queries::{
    batch_upsert_ai_embeddings, batch_update_ai_status, count_pending_ai_items,
    get_pending_ai_items,
};
use crate::error::AppError;
use crate::db::models::AiStatus;
use crate::engine::gpu::get_gpu_engine;
use crate::engine::traits::ResizeHint;
use crate::state::AppState;

/// Batch size for reading from DB and writing embeddings.
/// 从数据库读取和写入嵌入向量的批次大小。
const BATCH_SIZE: i64 = 512;

/// Channel capacity between producer and consumers.
/// 生产者和消费者之间的通道容量。
const CHANNEL_CAPACITY: usize = 1024;

/// Task item sent from producer to consumers.
/// 从生产者发送到消费者的任务项。
struct AiTask {
    item_id:     i64,
    source_path: PathBuf,
    file_format: String,
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
    item_id:   i64,
    /// `Some(bytes)` on success, `None` on inference failure.
    /// 成功时为 `Some(bytes)`，推理失败时为 `None`。
    embedding: Option<Vec<u8>>,
}

/// Start the background AI analysis pipeline.
/// 启动后台 AI 分析流水线。
///
/// Returns immediately; all work is done in background threads.
/// 立即返回；所有工作在后台线程中完成。
pub fn start_ai_pipeline(
    state: Arc<AppState>,
    token: CancellationToken,
) {
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let start_time = std::time::Instant::now();
        // Run blocking work in a spawn_blocking to avoid blocking the async runtime.
        // 在 spawn_blocking 中运行阻塞工作，避免阻塞异步运行时。
        let result = tokio::task::spawn_blocking(move || {
            run_pipeline_blocking(&state_clone, &token)
        })
        .await;

        let elapsed_ms = start_time.elapsed().as_millis();
        match result {
            Ok(Ok(())) => info!("AI analysis pipeline completed: elapsed={}ms | AI 分析流水线完成: 耗时={}ms", elapsed_ms, elapsed_ms),
            Ok(Err(e)) => warn!("AI analysis pipeline error | AI 分析流水线错误: {}", e),
            Err(e) => warn!("AI analysis task panicked | AI 分析任务崩溃: {}", e),
        }

        // Clear the token from state after completion.
        // 完成后从状态中清除令牌。
        state.cancel_ai_analysis();

        // Release AI engine to free up VRAM after analysis ends
        // 结束分析后卸载 AI 引擎以释放显存
        if let Ok(mut engine) = state.ai_engine.write() {
            *engine = None;
            info!("AI engine unloaded to release VRAM | AI 引擎已卸载以释放显存");
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
    let clip_session = state.ai_engine.read().unwrap().as_ref()
        .and_then(|p| p.clip_image_session.clone())
        .ok_or_else(|| AppError::Ai("CLIP engine not initialized | CLIP 引擎未初始化".to_string()))?;

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

    let token_prod   = token.clone();
    let token_writer = token.clone();
    let state_prod   = Arc::clone(state);
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
        s.spawn(move |_| {
            preprocess_tasks(task_rx, inference_tx, result_tx_preprocess, &state_consumer, &token_consumer);
        });

        // ── Inferencer thread ─────────────────────────────────────────────────
        // ── 推理线程 ──────────────────────────────────────────────────────────
        let session_clone = clip_session.clone();
        let token_inferencer = token.clone();
        let result_tx_inferencer = result_tx.clone();
        let state_inferencer = Arc::clone(state);
        s.spawn(move |_| {
            run_inference_tasks(inference_rx, result_tx_inferencer, session_clone, &state_inferencer, &token_inferencer);
        });

        // ── Writer thread ─────────────────────────────────────────────────────
        // ── 写入器线程 ────────────────────────────────────────────────────────
        s.spawn(move |_| {
            write_results(&state_writer, result_rx, &token_writer);
        });

        drop(result_tx);  // Close so writer can detect completion | 关闭以便写入器可以检测完成
    });

    Ok(())
}

/// Producer: batch-query pending items, push tasks to channel.
/// 生产者：批量查询待处理项，推送任务到通道。
fn produce_tasks(
    state: &Arc<AppState>,
    task_tx: Sender<AiTask>,
    token: &CancellationToken,
) {
    loop {
        if token.is_cancelled() {
            info!("AI producer cancelled | AI 生产者已取消");
            break;
        }

        // Yield to higher-priority tasks (scan / thumb gen)
        // 让步给更高优先级的任务（扫描 / 缩略图生成）
        if state.should_yield_to_higher_priority() {
            debug!("AI producer yielding to higher priority task | AI 生产者让步给高优先级任务");
            std::thread::sleep(std::time::Duration::from_millis(500));
            continue;
        }

        let conn = match state.db_read_pool.get() {
            Ok(c) => c,
            Err(e) => { warn!("DB pool error in AI producer | AI 生产者 DB 池错误: {}", e); break; }
        };

        let batch = match get_pending_ai_items(&conn, BATCH_SIZE) {
            Ok(b) => b,
            Err(e) => { warn!("Query pending AI items failed | 查询待处理 AI 项失败: {}", e); break; }
        };

        if batch.is_empty() {
            info!("AI producer: no more pending items | AI 生产者：没有更多待处理项");
            break;
        }

        // Mark items as "processing" to avoid re-queuing on restart
        // 将项标记为"处理中"，避免重启时重新排队
        let write_conn = state.db_writer.lock().unwrap();
        let ids: Vec<i64> = batch.iter().map(|(id, _, _)| *id).collect();
        if let Err(e) = batch_update_ai_status(&write_conn, &ids, AiStatus::Processing.as_i64()) {
            warn!("Failed to mark items as processing | 标记项为处理中失败: {}", e);
        }
        drop(write_conn);

        for (item_id, abs_path, file_format) in batch {
            if token.is_cancelled() { break; }

            let source_path = PathBuf::from(abs_path);
            if task_tx.send(AiTask { item_id, source_path, file_format }).is_err() {
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
) {
    rayon::scope(|s| {
        for task in task_rx {
            if token.is_cancelled() { break; }

            let inference_tx = inference_tx.clone();
            let result_tx = result_tx.clone();
            let state = Arc::clone(state);
            let token_clone = token.clone();

            s.spawn(move |_| {
                if token_clone.is_cancelled() {
                    return;
                }
                
                match process_preprocess_task(&task, &state) {
                    Ok(tensor) => {
                        let _ = inference_tx.send(InferenceTask { item_id: task.item_id, tensor });
                    }
                    Err(e) => {
                        debug!(
                            "Preprocess failed for item {} | 项 {} 预处理失败: {}",
                            task.item_id, task.item_id, e
                        );
                        let _ = result_tx.send(AiResult { item_id: task.item_id, embedding: None });
                    }
                }
            });
        }
    });

    info!("AI preprocessors finished | AI 预处理器已完成");
}

/// Process a single AI preprocess task: decode source image via ImageEngine (GPU-accelerated),
/// then run CLIP preprocessing.
/// 处理单个 AI 预处理任务：通过 ImageEngine（GPU 加速）解码源图像，然后运行 CLIP 预处理。
fn process_preprocess_task(
    task: &AiTask,
    state: &AppState,
) -> crate::error::Result<Array4<f32>> {
    let gpu_engine_name = state.thumb_config.read().unwrap().gpu_engine.clone();
    let resize_hint = Some(ResizeHint::ShortEdge(224));

    let decoded = match get_gpu_engine(&gpu_engine_name) {
        Some(gpu) if gpu.can_handle(&task.file_format) => {
            match gpu.decode(&task.source_path, resize_hint) {
                Ok(d) => d,
                Err(e) => {
                    debug!(
                        "GPU decode failed for item {}, falling back to CPU | 项 {} GPU 解码失败，回退 CPU: {}",
                        task.item_id, task.item_id, e
                    );
                    state.engine_arena
                        .engine_for(&task.file_format)
                        .ok_or_else(|| crate::error::AppError::UnsupportedFormat(task.file_format.clone()))?
                        .decode(&task.source_path, resize_hint)?
                }
            }
        }
        _ => {
            state.engine_arena
                .engine_for(&task.file_format)
                .ok_or_else(|| crate::error::AppError::UnsupportedFormat(task.file_format.clone()))?
                .decode(&task.source_path, resize_hint)?
        }
    };

    Ok(crate::ai::clip::preprocess_decoded(&decoded))
}

/// Inferencer: receive preprocessed tensors, dynamically batch them, and run CLIP inference.
/// 推理器：接收预处理后的张量，动态批处理它们，并运行 CLIP 推理。
fn run_inference_tasks(
    inference_rx: Receiver<InferenceTask>,
    result_tx: Sender<AiResult>,
    session_pool: crate::ai::engine::SessionPool,
    state: &Arc<AppState>,
    token: &CancellationToken,
) {
    let conn = state.db_read_pool.get().unwrap();
    let batch_size_str = crate::db::queries::get_config(&conn, "ai_batch_size").unwrap_or_default();
    
    let batch_size = if let Some(s) = batch_size_str {
        s.parse::<usize>().unwrap_or(8)
    } else {
        let vram_bytes = crate::ai::provider::detect_vram_bytes();
        let gb = vram_bytes.map(|b| b / (1024 * 1024 * 1024)).unwrap_or(0);
        if gb >= 8 { 64 } else if gb >= 4 { 32 } else if gb >= 2 { 16 } else { 8 }
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
                    flush_inference_batch(&mut batch, &session_pool, &result_tx);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if !batch.is_empty() {
                    flush_inference_batch(&mut batch, &session_pool, &result_tx);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                if !batch.is_empty() {
                    flush_inference_batch(&mut batch, &session_pool, &result_tx);
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
                let _ = result_tx.send(AiResult { item_id: task.item_id, embedding: None });
            }
            return;
        }
    };

    match crate::ai::clip::encode_image_batch(session_pool, batch_tensor) {
        Ok(embeddings) => {
            for (task, embedding) in batch.drain(..).zip(embeddings) {
                let bytes = embedding_to_bytes(&embedding);
                let _ = result_tx.send(AiResult { item_id: task.item_id, embedding: Some(bytes) });
            }
        }
        Err(e) => {
            warn!("Batch inference failed | 批量推理失败: {}", e);
            for task in batch.drain(..) {
                let _ = result_tx.send(AiResult { item_id: task.item_id, embedding: None });
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
                batch.push((result.item_id, MODEL_NAME.to_string(), emb, 1));
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
    let conn = state.db_writer.lock().unwrap();
    let ids: Vec<i64> = batch.iter().map(|(id, _, _, _)| *id).collect();

    match batch_upsert_ai_embeddings(&conn, batch) {
        Ok(()) => {
            // Update status to Done for successfully processed items
            // 将成功处理的项状态更新为已完成
            if let Err(e) = batch_update_ai_status(&conn, &ids, AiStatus::Done.as_i64()) {
                warn!("Failed to update ai_status to done | 更新 ai_status 为已完成失败: {}", e);
            }
            *total_written += ids.len() as u64;
            debug!("Flushed {} embeddings to DB | 已将 {} 个嵌入向量刷新到 DB", ids.len(), ids.len());
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
fn flush_failed(
    state: &Arc<AppState>,
    failed_ids: &mut Vec<i64>,
) {
    let conn = state.db_writer.lock().unwrap();
    if let Err(e) = batch_update_ai_status(&conn, failed_ids, AiStatus::Error.as_i64()) {
        warn!("Failed to mark items as error | 标记项为错误失败: {}", e);
    } else {
        debug!(
            "Marked {} items as ai_status=Error | 已将 {} 个项标记为 ai_status=Error",
            failed_ids.len(), failed_ids.len()
        );
    }
    failed_ids.clear();
}
