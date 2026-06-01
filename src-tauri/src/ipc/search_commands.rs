// src-tauri/src/ipc/search_commands.rs
//! Phase 1 file-name LIKE search (§ 6.1 — search).

use tauri::State;

use crate::db::models::{MediaFilter, SearchResult};
use crate::db::queries as q;
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Search media items by file name (LIKE query).
/// Phase 3 will migrate to FTS5.
///
/// Frontend must debounce calls by 150ms (in AppToolbar.vue).
#[tauri::command]
pub async fn search_media(
    query: String,
    directory_id: Option<i64>,
    filters: Option<MediaFilter>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let mut filter = filters.unwrap_or_default();
    if let Some(dir_id) = directory_id {
        filter.directory_id = Some(dir_id);
    }

    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::search_media(&pool, &query, &filter, limit.unwrap_or(100))
}
