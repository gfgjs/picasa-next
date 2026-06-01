// src-tauri/src/ipc/media_commands.rs
//! Tauri IPC commands for media item operations (§ 6.1 — media queries).

use std::sync::Arc;

use tauri::State;

use crate::db::models::{AppStats, DirNode, MediaDetail, MediaItem};
use crate::db::queries as q;
use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::utils::path::resolve_media_path;

/// Get full detail for a single media item.
#[tauri::command]
pub async fn get_media_detail(id: i64, state: State<'_, Arc<AppState>>) -> Result<MediaDetail> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::get_media_detail(&pool, id)
}

/// Get the playable video URL for a Live Photo companion.
/// Returns the absolute file path (caller wraps with convertFileSrc).
#[tauri::command]
pub async fn get_companion_video_url(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<String> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;

    // Check if the item has a companion MOV (Apple Live Photo)
    let companion_id = q::get_companion_item_id(&pool, item_id);

    if let Ok(Some(comp_id)) = companion_id {
        let (root, rel, name) = q::get_item_path_info(&pool, comp_id)?;
        return Ok(resolve_media_path(&root, &rel, &name));
    }

    // Check for embedded video (Google/Samsung Motion Photo)
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    let (has_embedded, cache_key): (bool, i64) = conn.query_row(
        "SELECT has_embedded_video, cache_key FROM media_items WHERE id=?1",
        rusqlite::params![item_id],
        |row| Ok((row.get::<_, i64>(0)? != 0, row.get(1)?)),
    )?;
    drop(conn);

    if has_embedded {
        let (root, rel, name) = q::get_item_path_info(&pool, item_id)?;
        let abs_path = resolve_media_path(&root, &rel, &name);

        // Check motion video cache
        let cache_path = crate::thumbnail::cache::motion_video_cache_path(
            &state.thumb_config.cache_dir,
            cache_key,
        );

        if cache_path.exists() {
            return Ok(cache_path.to_string_lossy().replace('\\', "/"));
        }

        // Extract from JPEG (read trailing bytes)
        let video_bytes = extract_embedded_mp4(&abs_path)?;
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent).map_err(AppError::from)?;
        }
        std::fs::write(&cache_path, &video_bytes).map_err(AppError::from)?;
        return Ok(cache_path.to_string_lossy().replace('\\', "/"));
    }

    Err(AppError::MediaNotFound(item_id))
}

/// Toggle the favorite status of a media item.
#[tauri::command]
pub async fn toggle_favorite(item_id: i64, state: State<'_, Arc<AppState>>) -> Result<bool> {
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::toggle_favorite(&conn, item_id)
}

/// Set the rating for a media item (0-5).
#[tauri::command]
pub async fn set_rating(item_id: i64, rating: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::set_rating(&conn, item_id, rating.clamp(0, 5))
}

/// Soft-delete media items (mark is_deleted=1).
#[tauri::command]
pub async fn soft_delete_items(item_ids: Vec<i64>, state: State<'_, Arc<AppState>>) -> Result<()> {
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::soft_delete_items(&conn, &item_ids)
}

/// Restore soft-deleted items.
#[tauri::command]
pub async fn restore_items(item_ids: Vec<i64>, state: State<'_, Arc<AppState>>) -> Result<()> {
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::restore_items(&conn, &item_ids)
}

/// Get items in the trash (paginated).
#[tauri::command]
pub async fn get_trash(
    offset: i64,
    limit: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<MediaItem>> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::get_trash(&pool, offset, limit.min(200))
}

/// Get overall app statistics.
#[tauri::command]
pub async fn get_stats(state: State<'_, Arc<AppState>>) -> Result<AppStats> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::get_app_stats(&pool)
}

/// Get the full directory tree for a scan root.
#[tauri::command]
pub async fn get_directory_tree(root_id: i64, state: State<'_, Arc<AppState>>) -> Result<Vec<DirNode>> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::get_directory_tree(&pool, root_id)
}

/// Get direct children of a directory node (lazy loading).
#[tauri::command]
pub async fn get_directory_children(
    parent_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<DirNode>> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::get_directory_children(&pool, parent_id)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract embedded MP4 from a Google/Samsung Motion Photo JPEG.
fn extract_embedded_mp4(abs_path: &str) -> Result<Vec<u8>> {
    let data = std::fs::read(abs_path).map_err(AppError::from)?;
    let ftyp_marker = b"ftyp";
    for i in (4..data.len().saturating_sub(4)).rev() {
        if &data[i..i + 4] == ftyp_marker {
            let mp4_start = i - 4;
            if mp4_start + 8 < data.len() {
                return Ok(data[mp4_start..].to_vec());
            }
        }
    }
    Err(AppError::Engine("No embedded MP4 found in Motion Photo".into()))
}
