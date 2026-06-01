// src-tauri/src/ipc/thumbnail_commands.rs
//! Tauri IPC commands for thumbnail generation (§ 6.1 — thumbnail).

use rayon::prelude::*;
use tauri::State;
use tracing::debug;

use crate::db::models::ThumbResult;
use crate::db::queries::get_thumb_by_item_ids;
use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::thumbnail::generate_thumbnail;

/// Batch thumbnail request — the primary thumbnail interface.
///
/// Accepts 1-64 item IDs, generates any missing thumbnails in parallel (rayon),
/// returns results in the same order as the input.
#[tauri::command]
pub async fn batch_request_thumbnails(
    item_ids: Vec<i64>,
    size: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<ThumbResult>> {
    if item_ids.is_empty() {
        return Ok(vec![]);
    }

    // First: check which items already have thumbnails
    let existing: Vec<ThumbResult> = {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        get_thumb_by_item_ids(&pool, &item_ids)?
    };

    // Build a map of id → existing result
    let mut result_map: std::collections::HashMap<i64, ThumbResult> = existing
        .into_iter()
        .map(|r| (r.item_id, r))
        .collect();

    // Find items needing generation (thumb_status != 1 and != 3)
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
        let config = if let Some(sz) = size {
            let mut c = state.thumb_config.clone();
            c.size = sz;
            c
        } else {
            state.thumb_config.clone()
        };

        // Generate in parallel using rayon (via spawn_blocking)
        let writer_ptr = &state.db_writer as *const _ as usize; // raw ptr for Send
        let arena_ptr  = &state.engine_arena as *const _ as usize;

        let generated: Vec<(i64, Result<ThumbResult>)> = tokio::task::spawn_blocking(move || {
            needs_gen
                .par_iter()
                .map(|&id| {
                    // SAFETY: AppState outlives this closure; pointers are valid
                    let writer = unsafe { &*(writer_ptr as *const _) };
                    let arena  = unsafe { &*(arena_ptr  as *const _) };
                    (id, generate_thumbnail(writer, arena, id, &config))
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
    let results: Vec<ThumbResult> = item_ids
        .iter()
        .filter_map(|id| result_map.remove(id))
        .collect();

    Ok(results)
}

/// Single thumbnail request (supplementary).
#[tauri::command]
pub async fn request_thumbnail(
    item_id: i64,
    size: Option<u32>,
    state: State<'_, AppState>,
) -> Result<ThumbResult> {
    let results = batch_request_thumbnails(vec![item_id], size, state).await?;
    results.into_iter().next().ok_or(AppError::MediaNotFound(item_id))
}
