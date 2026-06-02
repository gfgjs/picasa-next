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
    size: Option<u32>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ThumbResult>> {
    if item_ids.is_empty() {
        return Ok(vec![]);
    }

    // First: check which items already have thumbnails
    // 首先：检查哪些项目已经有缩略图
    let existing: Vec<ThumbResult> = {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        get_thumb_by_item_ids(&pool, &item_ids)?
    };

    // Build a map of id → existing result
    // 构建 id → 现有结果的映射
    let mut result_map: std::collections::HashMap<i64, ThumbResult> = existing
        .into_iter()
        .map(|r| (r.item_id, r))
        .collect();

    // Find items needing generation (thumb_status != 1 and != 3)
    // 查找需要生成的项目（thumb_status != 1 且 != 3）
    let needs_gen: Vec<i64> = item_ids
        .iter()
        .filter(|&&id| {
            result_map
                .get(&id)
                .map(|r| r.thumb_status != 1 && r.thumb_status != 3)
                .unwrap_or(true)
        })
        .copied()
        .collect();

    debug!("batch_request_thumbnails: total={} needs_gen={}", item_ids.len(), needs_gen.len());

    if !needs_gen.is_empty() {
        // Override size if provided
        // 如果提供则覆盖大小
        let config = if let Some(sz) = size {
            let mut c = state.thumb_config.read().unwrap().clone();
            c.size = sz;
            c
        } else {
            state.thumb_config.read().unwrap().clone()
        };

        // Clone Arc so the blocking closure owns it (no unsafe raw pointers)
        // 克隆 Arc，以便阻塞闭包拥有它（无不安全的原始指针）
        let state_arc = Arc::clone(&*state);

        let generated: Vec<(i64, Result<ThumbResult>)> = tokio::task::spawn_blocking(move || {
            needs_gen
                .par_iter()
                .map(|&id| {
                    (id, generate_thumbnail(&state_arc.db_writer, &state_arc.engine_arena, id, &config))
                })
                .collect()
        })
        .await
        .map_err(|e| AppError::Io(e.to_string()))?;

        for (id, result) in generated {
            match result {
                Ok(r) => { result_map.insert(id, r); }
                Err(e) => {
                    // Insert a failure record so the frontend doesn't spin
                    // 插入失败记录，这样前端就不会一直等待
                    result_map.insert(id, ThumbResult {
                        item_id:      id,
                        thumb_status: 2,
                        thumb_path:   None,
                        thumbhash:    None,
                    });
                    tracing::warn!("Thumbnail gen failed for id={id}: {e}");
                }
            }
        }
    }

    // Return in original order
    // 按原始顺序返回
    let results: Vec<ThumbResult> = item_ids
        .iter()
        .filter_map(|id| result_map.remove(id))
        .collect();

    // Update the in-memory layout cache so subsequent get_layout_rows calls are not stale
    // 更新内存中的布局缓存，这样后续的 get_layout_rows 调用就不会过时
    {
        let mut cache_guard = state.layout_cache.write().unwrap();
        if let Some(layout) = cache_guard.as_mut() {
            for row in layout.rows.iter_mut() {
                if let crate::layout::justified::LayoutRow::Normal { items, .. } = row {
                    for item in items.iter_mut() {
                        if let Some(res) = results.iter().find(|r| r.item_id == item.id) {
                            item.thumb_status = res.thumb_status;
                            item.thumb_path = res.thumb_path.clone();
                            item.thumbhash = res.thumbhash.clone();
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Single thumbnail request (supplementary).
/// 单个缩略图请求（补充）。
#[tauri::command]
pub async fn request_thumbnail(
    item_id: i64,
    size: Option<u32>,
    state: State<'_, Arc<AppState>>,
) -> Result<ThumbResult> {
    let results = batch_request_thumbnails(vec![item_id], size, state).await?;
    results.into_iter().next().ok_or(AppError::MediaNotFound(item_id))
}
