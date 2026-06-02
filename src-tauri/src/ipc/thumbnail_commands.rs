// src-tauri/src/ipc/thumbnail_commands.rs
//! Tauri IPC commands for thumbnail generation (§ 6.1 — thumbnail).
//! 用于缩略图生成的 Tauri IPC 命令（§ 6.1 — 缩略图）。

use std::sync::Arc;

use rayon::prelude::*;
use tauri::State;
use tracing::debug;

use crate::db::models::ThumbResult;
use crate::db::queries::get_thumb_by_item_ids;
use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::thumbnail::generate_thumbnail;

/// Batch thumbnail request — the primary thumbnail interface.
/// 批量缩略图请求 — 主要的缩略图接口。
///
/// Accepts 1-64 item IDs, generates any missing thumbnails in parallel (rayon),
/// 接收 1-64 个项目 ID，并行生成任何丢失的缩略图 (rayon)，
/// returns results in the same order as the input.
/// 按照与输入相同的顺序返回结果。
#[tauri::command]
pub async fn batch_request_thumbnails(
    item_ids: Vec<i64>,
    on_result: tauri::ipc::Channel<ThumbResult>,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    let state_arc = state.inner().clone();
    let config = { state_arc.thumb_config.read().unwrap().clone() };

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

    tracing::info!("batch_request_thumbnails: total={} needs_gen={} | 批量请求缩略图: 总计={} 需要生成={}", item_ids.len(), needs_gen.len(), item_ids.len(), needs_gen.len());

    if !needs_gen.is_empty() {
        let config = config.clone();

        tokio::task::spawn_blocking(move || {
            let results: Vec<ThumbResult> = needs_gen
                .par_iter()
                .filter_map(|&id| {
                    if state_arc.cancelled_thumb_ids.lock().unwrap().remove(&id) {
                        return None;
                    }

                    let pool = match state_arc.db_read_pool.get() {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::error!("Failed to get read pool: {e}");
                            return None;
                        }
                    };

                    let item = match crate::db::queries::get_media_item(&pool, id) {
                        Ok(i) => i,
                        Err(e) => {
                            tracing::error!("Failed to get item {id}: {e}");
                            return None;
                        }
                    };

                    let (root_path, rel_path, file_name) = match crate::db::queries::get_item_path_info(&pool, id) {
                        Ok(paths) => paths,
                        Err(e) => {
                            tracing::error!("Failed to get path info for {id}: {e}");
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
                            tracing::error!("Thumbnail gen failed for id={id}: {e}");
                            Some(r)
                        }
                    }
                })
                .collect();

            if !results.is_empty() {
                if let Ok(mut conn) = state_arc.db_writer.lock() {
                    if let Ok(tx) = conn.transaction() {
                        for res in results {
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
            }
            tracing::info!("batch_request_thumbnails: finished parallel block | 批量生成缩略图并行块完成");
        })
        .await
        .map_err(|e| AppError::Io(e.to_string()))?;
    }

    Ok(())
}

// request_thumbnail removed.

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
    // Force reset all thumbnail statuses to 0 to always regenerate everything
    // 强制将所有缩略图状态重置为 0，以便始终重新生成所有内容
    {
        let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        conn.execute("UPDATE media_items SET thumb_status = 0 WHERE is_deleted = 0", [])
            .map_err(|e| AppError::Db(e.to_string()))?;
    }

    // Check total needed
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

    // Cancel any existing run
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

        loop {
            if cancel_token.is_cancelled() {
                let _ = on_progress.send(FullThumbProgressPayload {
                    generated: generated_count.load(std::sync::atomic::Ordering::Relaxed),
                    total: total as u64,
                    status: "cancelled".to_string(),
                    current_item: None,
                });
                return Ok(());
            }

            let batch_ids = {
                let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
                let items = crate::db::queries::get_pending_thumb_items(&pool, 50)?;
                if items.is_empty() {
                    break; // done
                }
                items.into_iter().map(|(id, _)| id).collect::<Vec<_>>()
            };

            let config = state_arc.thumb_config.read().unwrap().clone();
            
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

                    tracing::info!("Full gen: processing id={} ({}) | 全量生成: 处理中 id={} ({})", id, file_name, id, file_name);

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
            
            // Batch update database
            if let Ok(mut conn) = state_arc.db_writer.lock() {
                if let Ok(tx) = conn.transaction() {
                    for (id, res) in &batch_results {
                        match res {
                            Ok(r) => {
                                successful_results.push(r.clone());
                                let _ = crate::db::queries::update_thumb_result(&tx, r.item_id, r.thumb_status, r.thumb_path.as_deref(), r.thumbhash.as_deref());
                            }
                            Err(e) => {
                                tracing::error!("Full gen failed for id={}: {}", id, e);
                                let _ = crate::db::queries::update_thumb_result(&tx, *id, 2, None, None);
                            }
                        }
                    }
                    let _ = tx.commit();
                }
            }
            
            tracing::info!("Full gen: batch completed, generated_count={} | 全量生成: 批次完成，已生成={}", generated_count.load(std::sync::atomic::Ordering::Relaxed), generated_count.load(std::sync::atomic::Ordering::Relaxed));

            // Update in-memory layout cache
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

            // No need to add here, atomic handles it
            // generated_count += batch_ids.len() as u64;

            let current_gen = generated_count.load(std::sync::atomic::Ordering::Relaxed);
            let _ = on_progress.send(FullThumbProgressPayload {
                generated: current_gen,
                total: total as u64,
                status: "running".to_string(),
                current_item: None,
            });
        }

        let final_gen = generated_count.load(std::sync::atomic::Ordering::Relaxed);
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
        
        Ok(())
    });

    Ok(())
}

#[tauri::command]
pub fn stop_full_thumbnail_generation(state: State<'_, Arc<AppState>>) -> Result<()> {
    tracing::info!("User action: Stopping full thumbnail generation | 用户操作：停止全量缩略图生成");
    state.cancel_thumb_gen();
    Ok(())
}

#[tauri::command]
pub async fn cancel_thumbnail_request(id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    state.cancelled_thumb_ids.lock().unwrap().insert(id);
    Ok(())
}
