// src-tauri/src/ai/pipeline.rs
//! Background AI analysis pipeline — 控制面(Producer/Writer)与入口。
//! 后台 AI 分析流水线:控制面(生产者/写入器)与入口。
//!
//! T16 收束:推理恒经 ai-worker 子进程派发(worker_pipeline.rs),本模块只保留
//! 两条路径曾共用的控制面——Producer(领取/让步/续传)、Writer(落库/状态机)、
//! 孤儿恢复与 batch 解析。进程内 ort 推理中段(预处理线程池/推理线程/解码源决策)
//! 已随 T16 删除,历史实现见 git。
//!
//! 1. Producer: batch-query media_items WHERE ai_status=0 → AiTask 通道
//! 2. 中段: worker_pipeline dispatch(攒批 → CPU permit → GPU 令牌 → EmbedBatch)
//! 3. Writer: 批量收集结果,写 ai_embeddings + ai_status,失效嵌入缓存
//! 4. 每批检查 ai_yield_blockers() + CancellationToken

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::ai::profile::ModelProfile;
use crate::db::models::AiStatus;
use crate::db::queries::{
    batch_update_ai_status, batch_upsert_ai_embeddings, get_pending_ai_items,
};
use crate::state::AppState;

/// Batch size for reading from DB and writing embeddings.
/// 从数据库读取和写入嵌入向量的批次大小。
const BATCH_SIZE: i64 = 512;

/// Task item sent from producer to the worker dispatcher.
/// 从生产者发送到 worker 派发线程的任务项。
pub(crate) struct AiTask {
    pub(crate) item_id: i64,
    /// 原图绝对路径(缺 ai_cache 时现场派生的解码源,T18)。
    pub(crate) source_path: PathBuf,
    pub(crate) file_format: String,
    /// 经 `ai_cache_path(cache_dir, cache_key)` 定位 AI 缓存;worker 端解码的唯一源。
    pub(crate) cache_key: i64,
}

/// Embedding result sent from consumers to writer.
/// 从消费者发送到写入器的嵌入向量结果。
pub(crate) struct AiResult {
    pub(crate) item_id: i64,
    /// `Some(bytes)` on success, `None` on inference failure.
    /// 成功时为 `Some(bytes)`，推理失败时为 `None`。
    pub(crate) embedding: Option<Vec<u8>>,
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
    });
}

/// Blocking pipeline runner:T16 起恒走 worker 派发(进程内 ort 路径已删)。
fn run_pipeline_blocking(
    state: &Arc<AppState>,
    token: &CancellationToken,
) -> crate::error::Result<()> {
    crate::ipc::ai_commands::warn_legacy_ai_backend(state);
    crate::ai::worker_pipeline::run_pipeline_worker_blocking(state, token)
}

/// 孤儿恢复(问题7,进程内与 worker 两条路径共用):Processing → Pending。
pub(crate) fn recover_orphaned_ai_items(state: &Arc<AppState>) {
    let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
    match crate::db::queries::reset_processing_ai_items(&conn) {
        Ok(n) if n > 0 => info!("Recovered {} orphaned AI items (processing → pending) | 恢复 {} 个孤儿 AI 项（处理中 → 待处理）", n, n),
        Ok(_) => {}
        Err(e) => warn!("Failed to recover orphaned AI items | 恢复孤儿 AI 项失败: {}", e),
    }
}

/// Producer: batch-query pending items, push tasks to channel.
/// 生产者：批量查询待处理项，推送任务到通道。
pub(crate) fn produce_tasks(
    state: &Arc<AppState>,
    task_tx: Sender<AiTask>,
    token: &CancellationToken,
) {
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
                })
                .is_err()
            {
                break;
            }
        }
    }

    info!("AI producer finished | AI 生产者已完成");
}

/// 统一解析有效 batch(进程内推理与 worker 派发/SessionInit 快照共用,T17 提取):
/// 配置 `ai_batch_size`(0=按 VRAM 自动)→ 上限 256 防 OOM → 固定 batch 模型抬到 ≥k。
pub(crate) fn resolve_batch_size(state: &AppState, profile: &ModelProfile) -> usize {
    let batch_size_str = state.db_read_pool.get().ok().and_then(|conn| {
        crate::db::queries::get_config(&conn, "ai_batch_size").unwrap_or_default()
    });

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
}

/// Writer: batch-collect results and write to DB.
/// 写入器：批量收集结果并写入 DB。
pub(crate) fn write_results(
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
