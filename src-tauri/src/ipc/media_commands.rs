// src-tauri/src/ipc/media_commands.rs
//! Tauri IPC commands for media item operations (§ 6.1 — media queries).
//! 用于媒体项操作的 Tauri IPC 命令（§ 6.1 — 媒体查询）。

use std::sync::Arc;

use tauri::State;

use crate::db::models::{AppStats, DirNode, MediaDetail, MediaItem};
use crate::db::queries as q;
use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::utils::path::resolve_media_path;

/// Get full detail for a single media item.
/// 获取单个媒体项的完整详细信息。
#[tauri::command]
pub async fn get_media_detail(id: i64, state: State<'_, Arc<AppState>>) -> Result<MediaDetail> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::get_media_detail(&pool, id)
}

/// Get the adjacent media item detail.
/// 获取相邻的媒体项详细信息。
#[tauri::command]
pub async fn get_adjacent_media(
    current_id: i64,
    offset: isize,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<MediaDetail>> {
    let adj_id = crate::layout::cache::get_adjacent_item(&state.layout_cache, current_id, offset);
    if let Some(id) = adj_id {
        let detail = get_media_detail(id, state).await?;
        Ok(Some(detail))
    } else {
        Ok(None)
    }
}

/// Get the playable video URL for a Live Photo companion.
/// 获取实况照片（Live Photo）关联文件的可播放视频 URL。
/// Returns the absolute file path (caller wraps with convertFileSrc).
/// 返回绝对文件路径（调用者使用 convertFileSrc 进行包装）。
#[tauri::command]
pub async fn get_companion_video_url(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<String> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;

    // Check if the item has a companion MOV (Apple Live Photo)
    // 检查项目是否有配套的 MOV 文件（Apple 实况照片）
    let companion_id = q::get_companion_item_id(&pool, item_id);

    if let Ok(Some(comp_id)) = companion_id {
        let (root, rel, name) = q::get_item_path_info(&pool, comp_id)?;
        return Ok(resolve_media_path(&root, &rel, &name));
    }

    // Check for embedded video (Google/Samsung Motion Photo)
    // 检查是否有嵌入式视频（Google/Samsung 动态照片）
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
        // 检查动态视频缓存
        let cache_path = {
            let config = state.thumb_config.read().unwrap();
            crate::thumbnail::cache::motion_video_cache_path(
                &config.cache_dir,
                cache_key,
            )
        };

        if cache_path.exists() {
            return Ok(cache_path.to_string_lossy().replace('\\', "/"));
        }

        // Extract from JPEG (read trailing bytes)
        // 从 JPEG 中提取（读取尾部字节）
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
/// 切换媒体项的收藏状态。
#[tauri::command]
pub async fn toggle_favorite(item_id: i64, state: State<'_, Arc<AppState>>) -> Result<bool> {
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::toggle_favorite(&conn, item_id)
}

/// Batch set favorite status for multiple items.
/// 批量设置多个项目的收藏状态。
#[tauri::command]
pub async fn batch_toggle_favorite(
    state: State<'_, Arc<AppState>>,
    item_ids: Vec<i64>,
    value: bool,
) -> Result<u64> {
    if item_ids.is_empty() {
        return Ok(0);
    }

    let writer = state.db_writer.lock()
        .map_err(|e| AppError::Db(format!("Lock error: {} | 锁错误: {}", e, e)))?;

    // Use a single UPDATE with IN clause for efficiency
    // 使用单个 UPDATE + IN 子句以提高效率
    let placeholders: String = item_ids.iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 2))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "UPDATE media_items SET is_favorited = ?1 WHERE id IN ({}) AND is_deleted = 0",
        placeholders
    );

    let mut params: Vec<rusqlite::types::Value> = vec![
        rusqlite::types::Value::Integer(if value { 1 } else { 0 }),
    ];
    for id in &item_ids {
        params.push(rusqlite::types::Value::Integer(*id));
    }

    let affected = writer.execute(
        &sql,
        rusqlite::params_from_iter(params.iter()),
    )? as u64;

    tracing::info!(
        "Batch favorite: set {}/{} items to {} | 批量收藏：设置 {}/{} 项为 {}",
        affected, item_ids.len(), value, affected, item_ids.len(), value
    );

    Ok(affected)
}

/// Set the rating for a media item (0-5).
/// 设置媒体项的评分（0-5）。
#[tauri::command]
pub async fn set_rating(item_id: i64, rating: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::set_rating(&conn, item_id, rating.clamp(0, 5))
}

/// Soft-delete media items (mark is_deleted=1).
/// 软删除媒体项（标记 is_deleted=1）。
#[tauri::command]
pub async fn soft_delete_items(item_ids: Vec<i64>, state: State<'_, Arc<AppState>>) -> Result<()> {
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::soft_delete_items(&conn, &item_ids)
}

/// Restore soft-deleted items.
/// 恢复软删除的项目。
#[tauri::command]
pub async fn restore_items(item_ids: Vec<i64>, state: State<'_, Arc<AppState>>) -> Result<()> {
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::restore_items(&conn, &item_ids)
}

/// Get items in the trash (paginated).
/// 获取垃圾桶中的项目（分页）。
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
/// 获取整体应用统计信息。
#[tauri::command]
pub async fn get_stats(state: State<'_, Arc<AppState>>) -> Result<AppStats> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::get_app_stats(&pool)
}

/// Get the full directory tree for a scan root.
/// 获取扫描根目录的完整目录树。
#[tauri::command]
pub async fn get_directory_tree(root_id: i64, state: State<'_, Arc<AppState>>) -> Result<Vec<DirNode>> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::get_directory_tree(&pool, root_id)
}

/// Get direct children of a directory node (lazy loading).
/// 获取目录节点的直接子节点（延迟加载）。
#[tauri::command]
pub async fn get_directory_children(
    parent_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<DirNode>> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::get_directory_children(&pool, parent_id)
}

// ── Helpers ───────────────────────────────────────────────────────────────────
// ── 助手函数 ───────────────────────────────────────────────────────────────────

/// Extract embedded MP4 from a Google/Samsung Motion Photo JPEG.
/// 从 Google/Samsung 动态照片 JPEG 中提取嵌入的 MP4。
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
