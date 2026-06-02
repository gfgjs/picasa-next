// src-tauri/src/ipc/config_commands.rs
//! App configuration key-value commands (§ 6.1 — config).
//! 应用配置键值命令（§ 6.1 — config）。

use std::sync::Arc;

use tauri::State;

use crate::db::queries::{get_config, set_config};
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Get a configuration value by key.
/// 根据键获取配置值。
#[tauri::command]
pub async fn get_app_config(key: String, state: State<'_, Arc<AppState>>) -> Result<Option<String>> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    get_config(&pool, &key)
}

/// Set a configuration value.
/// 设置配置值。
#[tauri::command]
pub async fn set_app_config(key: String, value: String, state: State<'_, Arc<AppState>>) -> Result<()> {
    {
        let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        set_config(&conn, &key, &value)?;
    }

    if key == "thumb_skip_max_kb" {
        if let Ok(val) = value.parse::<u64>() {
            let mut config = state.thumb_config.write().unwrap();
            config.skip_max_bytes = val * 1024;
        }
    } else if key == "thumb_size" {
        if let Ok(val) = value.parse::<u32>() {
            let mut config = state.thumb_config.write().unwrap();
            config.size = val;
        }
    } else if key == "thumb_cache_dir" {
        let mut config = state.thumb_config.write().unwrap();
        config.cache_dir = std::path::PathBuf::from(&value);
        std::fs::create_dir_all(&config.cache_dir).unwrap_or_default();
    }

    Ok(())
}

/// Get the resolved absolute thumbnail cache directory.
/// 获取解析后的绝对路径缩略图缓存目录。
#[tauri::command]
pub async fn get_thumb_cache_dir(state: State<'_, Arc<AppState>>) -> Result<String> {
    let path = state.thumb_config.read().unwrap().cache_dir.clone();
    Ok(path.to_string_lossy().to_string())
}
