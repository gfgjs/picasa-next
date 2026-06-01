// src-tauri/src/ipc/config_commands.rs
//! App configuration key-value commands (§ 6.1 — config).

use tauri::State;

use crate::db::queries::{get_config, set_config};
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Get a configuration value by key.
#[tauri::command]
pub async fn get_app_config(key: String, state: State<'_, AppState>) -> Result<Option<String>> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    get_config(&pool, &key)
}

/// Set a configuration value.
#[tauri::command]
pub async fn set_app_config(key: String, value: String, state: State<'_, AppState>) -> Result<()> {
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    set_config(&conn, &key, &value)
}
