// src-tauri/src/ipc/system_commands.rs
//! System-level commands (§ 6.1 — system).

use std::sync::Arc;

use tauri::State;
use tracing::info;

use crate::db::queries::get_item_path_info;
use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::utils::path::resolve_media_path;

/// Reveal a media item in the OS file explorer.
#[tauri::command]
pub async fn show_in_explorer(item_id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    let (root, rel, name) = get_item_path_info(&pool, item_id)?;
    let abs_path = resolve_media_path(&root, &rel, &name);
    info!("show_in_explorer: {abs_path}");

    // Platform-specific file reveal
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .args(["/select,", &abs_path.replace('/', "\\")])
            .spawn()
            .map_err(AppError::from)?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &abs_path])
            .spawn()
            .map_err(AppError::from)?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(std::path::Path::new(&abs_path).parent().unwrap_or(std::path::Path::new("/")))
            .spawn()
            .map_err(AppError::from)?;
    }

    Ok(())
}

/// Move items to the system trash (Phase 2 — stub for now).
#[tauri::command]
pub async fn move_to_trash(item_ids: Vec<i64>, state: State<'_, Arc<AppState>>) -> Result<()> {
    // Phase 2: integrate `trash` crate
    // For now, fall back to soft delete
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    crate::db::queries::soft_delete_items(&conn, &item_ids)
}
