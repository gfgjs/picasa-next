// src-tauri/src/ipc/thumbnail_commands.rs
//! Tauri IPC commands for thumbnail generation (§ 6.1 — thumbnail).
//! 用于缩略图生成的 Tauri IPC 命令（§ 6.1 — 缩略图）。

use std::sync::Arc;

use tauri::State;
use tracing::{error, info};
use crossbeam_channel::bounded;
use rayon::prelude::*;

use crate::db::models::ThumbResult;
use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::thumbnail::{decode_media_step, encode_media_step, generate_thumbnail, DecodeResult};

/// 【一键切换架构开关】
/// true: 使用方案二（多阶段流水线解耦），适合未来进行深度的并发与 IO 性能调优。
/// false: 使用方案一（直接通过 Rayon 进行无锁并发迭代），由于 Rayon 的工作窃取调度极其成熟，目前在多核 CPU 上性能最好。
const USE_PIPELINE: bool = true;

#[tauri::command]
pub async fn batch_request_thumbnails(
    item_ids: Vec<i64>,
    target_size: Option<u32>,
    on_result: tauri::ipc::Channel<ThumbResult>,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    let state_arc = state.inner().clone();
    let mut config = { state_arc.thumb_config.read().unwrap().clone() };
    if let Some(size) = target_size {
        config.size = size;
    }

    let mut fast_results = std::collections::HashMap::new();
    let mut needs_gen = Vec::new();

    {
        // Check cache in batch
        let conn = state.db_read_pool.get().map_err(|e| AppError::Db(e.to_string()))?;
        let in_clause = item_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        if !in_clause.is_empty() {
            let sql = format!(
                "SELECT id, thumb_status, thumb_path, thumbhash FROM media_items WHERE id IN ({})",
                in_clause
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| AppError::Db(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(ThumbResult {
                        item_id:      row.get(0)?,
                        thumb_status: row.get(1)?,
                        thumb_path:   row.get(2)?,
                        thumbhash:    row.get(3)?,
                    })
                })
                .map_err(|e| AppError::Db(e.to_string()))?;

            for r in rows.flatten() {
                if r.thumb_status == 1 || r.thumb_status == 3 || r.thumb_status == 2 {
                    fast_results.insert(r.item_id, r);
                }
            }
        }
    }

    for &id in &item_ids {
        if let Some(r) = fast_results.get(&id) {
            let _ = on_result.send(r.clone());
        } else {
            needs_gen.push(id);
        }
    }

    // Sync fast-path results into layout_cache so fetchRowsByY returns
    // up-to-date thumb_status (avoids stale status=0 after prior generation).
    // 将快速路径结果同步到 layout_cache，使 fetchRowsByY 返回
    // 最新的 thumb_status（避免先前生成后仍返回陈旧的 status=0）。
    if !fast_results.is_empty() {
        let mut cache_guard = state.layout_cache.write().unwrap();
        if let Some(layout) = cache_guard.as_mut() {
            for row in layout.rows.iter_mut() {
                if let crate::layout::justified::LayoutRow::Normal { items, .. } = row {
                    for item in items.iter_mut() {
                        if let Some(r) = fast_results.get(&item.id) {
                            item.thumb_status = r.thumb_status;
                            item.thumb_path = r.thumb_path.clone();
                            item.thumbhash = r.thumbhash.clone();
                        }
                    }
                }
            }
        }
    }

    info!("batch_request_thumbnails: total={} needs_gen={} | 批量请求缩略图: 总计={} 需要生成={}", item_ids.len(), needs_gen.len(), item_ids.len(), needs_gen.len());

    if !needs_gen.is_empty() {
        let config = config.clone();

        tokio::task::spawn_blocking(move || {
            if !USE_PIPELINE {
                // 方案一：Rayon 直线并发 (Scheme 1)
                let results: Vec<ThumbResult> = needs_gen
                    .par_iter()
                    .filter_map(|&id| {
                        if state_arc.cancelled_thumb_ids.lock().unwrap().remove(&id) {
                            return None;
                        }

                        let pool = match state_arc.db_read_pool.get() {
                            Ok(p) => p,
                            Err(e) => {
                                error!("Failed to get read pool: {e}");
                                return None;
                            }
                        };

                        let item = match crate::db::queries::get_media_item(&pool, id) {
                            Ok(i) => i,
                            Err(e) => {
                                error!("Failed to get item {id}: {e}");
                                return None;
                            }
                        };

                        let (root_path, rel_path, file_name) = match crate::db::queries::get_item_path_info(&pool, id) {
                            Ok(paths) => paths,
                            Err(e) => {
                                error!("Failed to get path info for {id}: {e}");
                                return None;
                            }
                        };
                        
                        let abs_path_str = crate::utils::path::resolve_media_path(&root_path, &rel_path, &file_name);
                        let abs_path = std::path::Path::new(&abs_path_str);

                        let res = generate_thumbnail(&item, abs_path, &state_arc.engine_arena, &config);
                        match res {
                            Ok(r) => { 
                                let _ = on_result.send(r.clone()); 
                                Some(r)
                            },
                            Err(e) => {
                                let r = ThumbResult {
                                    item_id:      id,
                                    thumb_status: 2,
                                    thumb_path:   None,
                                    thumbhash:    None,
                                };
                                let _ = on_result.send(r.clone());
                                error!("Thumbnail gen failed for id={id}: {e}");
                                Some(r)
                            }
                        }
                    })
                    .collect();

                if !results.is_empty() {
                    if let Ok(mut conn) = state_arc.db_writer.lock() {
                        if let Ok(tx) = conn.transaction() {
                            for res in &results {
                                let _ = crate::db::queries::update_thumb_result(
                                    &tx, 
                                    res.item_id, 
                                    res.thumb_status, 
                                    res.thumb_path.as_deref(), 
                                    res.thumbhash.as_deref()
                                );
                            }
                            let _ = tx.commit();
                        }
                    }
                    // Sync results into layout_cache so fetchRowsByY returns fresh data
                    // 同步结果到 layout_cache，使 fetchRowsByY 返回最新数据
                    let mut cache_guard = state_arc.layout_cache.write().unwrap();
                    if let Some(layout) = cache_guard.as_mut() {
                        for row in layout.rows.iter_mut() {
                            if let crate::layout::justified::LayoutRow::Normal { items, .. } = row {
                                for item in items.iter_mut() {
                                    if let Some(r) = results.iter().find(|res| res.item_id == item.id) {
                                        item.thumb_status = r.thumb_status;
                                        item.thumb_path = r.thumb_path.clone();
                                        item.thumbhash = r.thumbhash.clone();
                                    }
                                }
                            }
                        }
                    }
                }
                info!("batch_request_thumbnails: finished parallel block | 批量请求生成完成 (Rayon Scheme 1)");
                return;
            }

            // 方案二：多阶段流水线解耦 (Scheme 2)
            let (decode_tx, decode_rx) = bounded(1024);
            let (encode_tx, encode_rx) = bounded(1024);
            let (result_tx, result_rx) = bounded(needs_gen.len().max(1024));

            let needs_gen_clone = needs_gen.clone();
            let state_dispatcher = state_arc.clone();
            std::thread::spawn(move || {
                for id in needs_gen_clone {
                    if state_dispatcher.cancelled_thumb_ids.lock().unwrap().contains(&id) {
                        continue;
                    }
                    if let Ok(pool) = state_dispatcher.db_read_pool.get() {
                        if let Ok(item) = crate::db::queries::get_media_item(&pool, id) {
                            if let Ok((root_path, rel_path, file_name)) = crate::db::queries::get_item_path_info(&pool, id) {
                                let abs_path_str = crate::utils::path::resolve_media_path(&root_path, &rel_path, &file_name);
                                let abs_path = std::path::PathBuf::from(abs_path_str);
                                let _ = decode_tx.send((item, abs_path));
                            }
                        }
                    }
                }
            });

            let config_decode = config.clone();
            let cpu_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
            let decode_threads = (cpu_cores * 2).max(4);
            for _ in 0..decode_threads {
                let rx = decode_rx.clone();
                let tx = encode_tx.clone();
                let res_tx = result_tx.clone();
                let cfg = config_decode.clone();
                let state_worker = state_arc.clone();
                std::thread::spawn(move || {
                    while let Ok((item, abs_path)) = rx.recv() {
                        match decode_media_step(&item, &abs_path, &state_worker.engine_arena, &cfg) {
                            Ok(DecodeResult::Ready(res)) => {
                                let _ = res_tx.send(res);
                            }
                            Ok(DecodeResult::ToEncode { item_id, cache_key, decoded }) => {
                                let _ = tx.send((item_id, cache_key, decoded));
                            }
                            Err(e) => {
                                error!("Decode failed for id={}: {}", item.id, e);
                                let _ = res_tx.send(ThumbResult { item_id: item.id, thumb_status: 2, thumb_path: None, thumbhash: None });
                            }
                        }
                    }
                });
            }
            drop(encode_tx);

            let config_encode = config.clone();
            for _ in 0..cpu_cores {
                let rx = encode_rx.clone();
                let tx = result_tx.clone();
                let cfg = config_encode.clone();
                std::thread::spawn(move || {
                    while let Ok((item_id, cache_key, decoded)) = rx.recv() {
                        match encode_media_step(item_id, cache_key, decoded, &cfg) {
                            Ok(res) => { let _ = tx.send(res); }
                            Err(e) => {
                                error!("Encode failed for id={}: {}", item_id, e);
                                let _ = tx.send(ThumbResult { item_id, thumb_status: 2, thumb_path: None, thumbhash: None });
                            }
                        }
                    }
                });
            }
            drop(result_tx);

            let mut results = Vec::new();
            while let Ok(res) = result_rx.recv() {
                let _ = on_result.send(res.clone());
                results.push(res);
            }

            if !results.is_empty() {
                if let Ok(mut conn) = state_arc.db_writer.lock() {
                    if let Ok(tx) = conn.transaction() {
                        for res in &results {
                            let _ = crate::db::queries::update_thumb_result(
                                &tx, 
                                res.item_id, 
                                res.thumb_status, 
                                res.thumb_path.as_deref(), 
                                res.thumbhash.as_deref()
                            );
                        }
                        let _ = tx.commit();
                    }
                }
                // Sync results into layout_cache so fetchRowsByY returns fresh data
                // 同步结果到 layout_cache，使 fetchRowsByY 返回最新数据
                let mut cache_guard = state_arc.layout_cache.write().unwrap();
                if let Some(layout) = cache_guard.as_mut() {
                    for row in layout.rows.iter_mut() {
                        if let crate::layout::justified::LayoutRow::Normal { items, .. } = row {
                            for item in items.iter_mut() {
                                if let Some(r) = results.iter().find(|res| res.item_id == item.id) {
                                    item.thumb_status = r.thumb_status;
                                    item.thumb_path = r.thumb_path.clone();
                                    item.thumbhash = r.thumbhash.clone();
                                }
                            }
                        }
                    }
                }
            }

            info!("batch_request_thumbnails: finished pipeline | 批量请求生成完成 (Pipeline Scheme 2)");
        })
        .await
        .map_err(|e| AppError::Io(e.to_string()))?;
    }

    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullThumbProgressPayload {
    pub generated: u64,
    pub total: u64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_item: Option<String>,
}

#[tauri::command]
pub async fn start_full_thumbnail_generation(
    on_progress: tauri::ipc::Channel<FullThumbProgressPayload>,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    {
        let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        conn.execute("UPDATE media_items SET thumb_status = 0, thumb_path = NULL, thumbhash = NULL WHERE is_deleted = 0", [])
            .map_err(|e| AppError::Db(e.to_string()))?;
    }

    let total = {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        crate::db::queries::count_pending_thumb_items(&pool)?
    };

    if total == 0 {
        let _ = on_progress.send(FullThumbProgressPayload {
            generated: 0,
            total: 0,
            status: "completed".to_string(),
            current_item: None,
        });
        return Ok(());
    }

    state.cancel_thumb_gen();
    let cancel_token = state.new_thumb_gen_token();

    let state_arc = Arc::clone(&*state);
    let generated_count = Arc::new(std::sync::atomic::AtomicU64::new(0));

    tokio::task::spawn_blocking(move || -> Result<()> {
        let _ = on_progress.send(FullThumbProgressPayload {
            generated: 0,
            total: total as u64,
            status: "running".to_string(),
            current_item: None,
        });

        let config = state_arc.thumb_config.read().unwrap().clone();
        info!(
            "[FullThumbGen] START: total={} strategy={} gpu_engine={} size={} skip_max_bytes={} cache_dir={:?} pipeline={} | 全量缩略图生成开始",
            total, config.strategy, config.gpu_engine, config.size, config.skip_max_bytes, config.cache_dir, USE_PIPELINE
        );
        
        if !USE_PIPELINE {
            // 方案一：Rayon 直线并发 (Scheme 1)
            let all_ids = {
                let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
                crate::db::queries::get_all_pending_thumb_ids(&pool).unwrap_or_default()
            };

            for batch_ids in all_ids.chunks(50) {
                if cancel_token.is_cancelled() {
                    break;
                }

                let batch_results: Vec<(i64, Result<ThumbResult>)> = batch_ids
                    .par_iter()
                    .filter_map(|&id| {
                        if cancel_token.is_cancelled() {
                            return None;
                        }
                        
                        let pool = match state_arc.db_read_pool.get() {
                            Ok(p) => p,
                            Err(_) => return None,
                        };
                        
                        let item = match crate::db::queries::get_media_item(&pool, id) {
                            Ok(i) => i,
                            Err(_) => return None,
                        };
                        
                        let (root_path, rel_path, file_name) = match crate::db::queries::get_item_path_info(&pool, id) {
                            Ok(p) => p,
                            Err(_) => return None,
                        };

                        let abs_path_str = crate::utils::path::resolve_media_path(&root_path, &rel_path, &file_name);
                        let abs_path = std::path::Path::new(&abs_path_str);

                        let current = generated_count.load(std::sync::atomic::Ordering::Relaxed);
                        let _ = on_progress.send(FullThumbProgressPayload {
                            generated: current,
                            total: total as u64,
                            status: "running".to_string(),
                            current_item: Some(file_name),
                        });
                        
                        let res = generate_thumbnail(&item, abs_path, &state_arc.engine_arena, &config);
                        
                        generated_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        
                        Some((id, res))
                    })
                    .collect();
                    
                let mut successful_results = Vec::new();
                
                if let Ok(mut conn) = state_arc.db_writer.lock() {
                    if let Ok(tx) = conn.transaction() {
                        for (id, res) in &batch_results {
                            match res {
                                Ok(r) => {
                                    successful_results.push(r.clone());
                                    let _ = crate::db::queries::update_thumb_result(&tx, r.item_id, r.thumb_status, r.thumb_path.as_deref(), r.thumbhash.as_deref());
                                }
                                Err(e) => {
                                    error!("Full gen failed for id={}: {}", id, e);
                                    let _ = crate::db::queries::update_thumb_result(&tx, *id, 2, None, None);
                                }
                            }
                        }
                        let _ = tx.commit();
                    }
                }

                if !successful_results.is_empty() {
                    let mut cache_guard = state_arc.layout_cache.write().unwrap();
                    if let Some(layout) = cache_guard.as_mut() {
                        for row in layout.rows.iter_mut() {
                            if let crate::layout::justified::LayoutRow::Normal { items, .. } = row {
                                for item in items.iter_mut() {
                                    if let Some(res) = successful_results.iter().find(|r| r.item_id == item.id) {
                                        item.thumb_status = res.thumb_status;
                                        item.thumb_path = res.thumb_path.clone();
                                        item.thumbhash = res.thumbhash.clone();
                                    }
                                }
                            }
                        }
                    }
                }

                let current_gen = generated_count.load(std::sync::atomic::Ordering::Relaxed);
                let _ = on_progress.send(FullThumbProgressPayload {
                    generated: current_gen,
                    total: total as u64,
                    status: "running".to_string(),
                    current_item: None,
                });
            }
        } else {
            // 方案二：多阶段流水线解耦 (Scheme 2)
            let (decode_tx, decode_rx) = bounded(1024);
            let (encode_tx, encode_rx) = bounded(1024);
            let (result_tx, result_rx) = bounded(1024);

            let state_dispatcher = state_arc.clone();
            let cancel_dispatcher = cancel_token.clone();
            std::thread::spawn(move || {
                let all_ids = {
                    let pool = match state_dispatcher.db_read_pool.get() { Ok(p) => p, Err(_) => return };
                    crate::db::queries::get_all_pending_thumb_ids(&pool).unwrap_or_default()
                };
                info!("[FullThumbGen] Dispatcher: {} pending IDs fetched | 调度器: 获取到 {} 个待处理 ID", all_ids.len(), all_ids.len());

                for chunk in all_ids.chunks(50) {
                    if cancel_dispatcher.is_cancelled() { break; }
                    let pool = match state_dispatcher.db_read_pool.get() { Ok(p) => p, Err(_) => break };
                    
                    for &id in chunk {
                        if cancel_dispatcher.is_cancelled() { break; }
                        if let Ok(item) = crate::db::queries::get_media_item(&pool, id) {
                            if let Ok((root_path, rel_path, file_name)) = crate::db::queries::get_item_path_info(&pool, id) {
                                let abs_path_str = crate::utils::path::resolve_media_path(&root_path, &rel_path, &file_name);
                                let abs_path = std::path::PathBuf::from(abs_path_str);
                                if decode_tx.send((item, abs_path)).is_err() {
                                    return;
                                }
                            } else {
                                error!("[FullThumbGen] path_info failed for id={}", id);
                            }
                        } else {
                            error!("[FullThumbGen] get_media_item failed for id={}", id);
                        }
                    }
                }
                info!("[FullThumbGen] Dispatcher: done sending items | 调度器: 发送完毕");
            });

            let config_decode = config.clone();
            let cancel_decode = cancel_token.clone();
            let cpu_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
            let decode_threads = (cpu_cores * 2).max(4);
            for _ in 0..decode_threads {
                let rx = decode_rx.clone();
                let tx = encode_tx.clone();
                let res_tx = result_tx.clone();
                let state_worker = state_arc.clone();
                let cfg = config_decode.clone();
                let cancel = cancel_decode.clone();
                std::thread::spawn(move || {
                    while let Ok((item, abs_path)) = rx.recv() {
                        if cancel.is_cancelled() { break; }
                        match decode_media_step(&item, &abs_path, &state_worker.engine_arena, &cfg) {
                            Ok(DecodeResult::Ready(res)) => { let _ = res_tx.send(res); }
                            Ok(DecodeResult::ToEncode { item_id, cache_key, decoded }) => {
                                if tx.send((item_id, cache_key, decoded)).is_err() { break; }
                            }
                            Err(e) => {
                                error!("Full gen decode failed for id={}: {}", item.id, e);
                                let _ = res_tx.send(ThumbResult { item_id: item.id, thumb_status: 2, thumb_path: None, thumbhash: None });
                            }
                        }
                    }
                });
            }
            drop(encode_tx);

            let config_encode = config.clone();
            let cancel_encode = cancel_token.clone();
            for _ in 0..cpu_cores {
                let rx = encode_rx.clone();
                let tx = result_tx.clone();
                let cfg = config_encode.clone();
                let cancel = cancel_encode.clone();
                std::thread::spawn(move || {
                    while let Ok((item_id, cache_key, decoded)) = rx.recv() {
                        if cancel.is_cancelled() { break; }
                        match encode_media_step(item_id, cache_key, decoded, &cfg) {
                            Ok(res) => { let _ = tx.send(res); }
                            Err(e) => {
                                error!("Full gen encode failed for id={}: {}", item_id, e);
                                let _ = tx.send(ThumbResult { item_id, thumb_status: 2, thumb_path: None, thumbhash: None });
                            }
                        }
                    }
                });
            }
            drop(result_tx);

            let mut successful_results = Vec::new();
            while let Ok(res) = result_rx.recv() {
                if cancel_token.is_cancelled() { break; }
                successful_results.push(res.clone());
                generated_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                
                let current = generated_count.load(std::sync::atomic::Ordering::Relaxed);
                
                // 每次都向前端发送事件，保证进度条平滑 (+1 变化)
                let _ = on_progress.send(FullThumbProgressPayload {
                    generated: current,
                    total: total as u64,
                    status: "running".to_string(),
                    current_item: None,
                });

                if successful_results.len() >= 50 {
                    // Log batch status breakdown
                    // 记录批次状态分布
                    let n_encoded = successful_results.iter().filter(|r| r.thumb_status == 1).count();
                    let n_direct  = successful_results.iter().filter(|r| r.thumb_status == 3).count();
                    let n_failed  = successful_results.iter().filter(|r| r.thumb_status == 2).count();
                    info!(
                        "[FullThumbGen] Batch flush: {} results (encoded={}, direct={}, failed={}) | 批次写入",
                        successful_results.len(), n_encoded, n_direct, n_failed
                    );

                    if let Ok(mut conn) = state_arc.db_writer.lock() {
                        if let Ok(tx) = conn.transaction() {
                            for r in &successful_results {
                                let _ = crate::db::queries::update_thumb_result(&tx, r.item_id, r.thumb_status, r.thumb_path.as_deref(), r.thumbhash.as_deref());
                            }
                            let _ = tx.commit();
                        }
                    }

                    let mut cache_guard = state_arc.layout_cache.write().unwrap();
                    if let Some(layout) = cache_guard.as_mut() {
                        for row in layout.rows.iter_mut() {
                            if let crate::layout::justified::LayoutRow::Normal { items, .. } = row {
                                for item in items.iter_mut() {
                                    if let Some(r) = successful_results.iter().find(|res| res.item_id == item.id) {
                                        item.thumb_status = r.thumb_status;
                                        item.thumb_path = r.thumb_path.clone();
                                        item.thumbhash = r.thumbhash.clone();
                                    }
                                }
                            }
                        }
                    }
                    successful_results.clear();
                }
            }

            // Flush remaining
            if !successful_results.is_empty() {
                if let Ok(mut conn) = state_arc.db_writer.lock() {
                    if let Ok(tx) = conn.transaction() {
                        for r in &successful_results {
                            let _ = crate::db::queries::update_thumb_result(&tx, r.item_id, r.thumb_status, r.thumb_path.as_deref(), r.thumbhash.as_deref());
                        }
                        let _ = tx.commit();
                    }
                }
                let mut cache_guard = state_arc.layout_cache.write().unwrap();
                if let Some(layout) = cache_guard.as_mut() {
                    for row in layout.rows.iter_mut() {
                        if let crate::layout::justified::LayoutRow::Normal { items, .. } = row {
                            for item in items.iter_mut() {
                                if let Some(r) = successful_results.iter().find(|res| res.item_id == item.id) {
                                    item.thumb_status = r.thumb_status;
                                    item.thumb_path = r.thumb_path.clone();
                                    item.thumbhash = r.thumbhash.clone();
                                }
                            }
                        }
                    }
                }
            }
        }

        let final_gen = generated_count.load(std::sync::atomic::Ordering::Relaxed);
        info!(
            "[FullThumbGen] FINISHED: generated={} total={} cancelled={} | 全量缩略图生成完成",
            final_gen, total, cancel_token.is_cancelled()
        );
        if cancel_token.is_cancelled() {
            let _ = on_progress.send(FullThumbProgressPayload {
                generated: final_gen,
                total: total as u64,
                status: "cancelled".to_string(),
                current_item: None,
            });
        } else {
            let _ = on_progress.send(FullThumbProgressPayload {
                generated: final_gen,
                total: total as u64,
                status: "completed".to_string(),
                current_item: None,
            });
        }

        // Clear the token after completion/cancellation so AI pipeline won't yield forever.
        // This mirrors how the AI pipeline itself clears ai_analysis_token on completion.
        // 清除 token，防止 AI pipeline 永远让步（与 AI pipeline 的 cancel_ai_analysis() 模式一致）。
        *state_arc.thumb_gen_token.lock().unwrap() = None;
        tracing::info!("Thumbnail gen token cleared after completion | 全量缩略图 token 已清除");

        // Invalidate layout cache so the next compute_layout reads fresh
        // thumb_status / thumb_path from DB instead of serving stale data.
        // 清空布局缓存，使下一次 compute_layout 从数据库读取最新的
        // thumb_status / thumb_path，而非提供陈旧数据。
        *state_arc.layout_cache.write().unwrap() = None;
        tracing::info!("Layout cache invalidated after full thumb gen | 全量缩略图后已清空布局缓存");

        Ok(())
    });

    Ok(())
}

#[tauri::command]
pub fn stop_full_thumbnail_generation(state: State<'_, Arc<AppState>>) -> Result<()> {
    info!("User action: Stopping full thumbnail generation | 用户操作：停止全量缩略图生成");
    state.cancel_thumb_gen();
    Ok(())
}

#[tauri::command]
pub async fn cancel_thumbnail_request(id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    state.cancelled_thumb_ids.lock().unwrap().insert(id);
    Ok(())
}
