// src-tauri/src/ipc/config_commands.rs
//! App configuration key-value commands (§ 6.1 — config).
//! 应用配置键值命令（§ 6.1 — config）。

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::db::queries::{get_config, set_config};
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Get a configuration value by key.
/// 根据键获取配置值。
///
/// Uses spawn_blocking because r2d2::Pool::get() and SQLite are synchronous.
/// Without this, the Tokio executor thread would be blocked during pool acquisition,
/// which degrades concurrency especially when multiple IPC calls arrive simultaneously.
///
/// 使用 spawn_blocking，因为 r2d2::Pool::get() 和 SQLite 是同步阻塞操作。
/// 若不包装，Tokio 执行器线程将在连接池获取期间被阻塞，
/// 特别是多个 IPC 调用并发时会严重降低并发性（例如启动时 App.vue 的 4 次并行调用）。
#[tauri::command]
pub async fn get_app_config(key: String, state: State<'_, Arc<AppState>>) -> Result<Option<String>> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        get_config(&pool, &key)
    })
    .await
    .map_err(|e| AppError::Db(format!("spawn_blocking join error: {e}")))?
}

/// All config values needed by the frontend on startup, fetched in a single SQLite round-trip.
/// Replaces 4 separate get_app_config IPC calls in App.vue onMounted, reducing IPC overhead
/// from N×(serialisation + Tokio schedule + pool acquire + SQLite + deserialise)
/// down to 1×(same overhead) + N×SQLite row reads (negligible, same connection).
///
/// 前端启动时所需的所有配置，通过单次 SQLite 往返批量获取。
/// 替代 App.vue onMounted 里 4 次独立 get_app_config IPC，将开销从
/// N×（序列化 + Tokio 调度 + 连接池获取 + SQLite + 反序列化）
/// 降低到 1×相同开销 + N×SQLite 行读取（可忽略，同一连接）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupConfig {
    pub language: Option<String>,
    pub timeline_scroll_width: Option<String>,
    pub ui_font_size: Option<String>,
    pub enable_thumb_hover_scale: Option<String>,
}

#[tauri::command]
pub async fn get_startup_config(state: State<'_, Arc<AppState>>) -> Result<StartupConfig> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        Ok(StartupConfig {
            language:               get_config(&pool, "language")?,
            timeline_scroll_width:  get_config(&pool, "timeline_scroll_width")?,
            ui_font_size:           get_config(&pool, "ui_font_size")?,
            enable_thumb_hover_scale: get_config(&pool, "enable_thumb_hover_scale")?,
        })
    })
    .await
    .map_err(|e| AppError::Db(format!("spawn_blocking join error: {e}")))?
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
    } else if key == "thumb_strategy" {
        let mut config = state.thumb_config.write().unwrap();
        config.strategy = value;
    } else if key == "gpu_engine" {
        let mut config = state.thumb_config.write().unwrap();
        config.gpu_engine = value;
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

/// Get the resolved absolute log directory.
/// 获取解析后的绝对路径日志目录。
#[tauri::command]
pub async fn get_log_dir(state: State<'_, Arc<AppState>>) -> Result<String> {
    let path = state.log_dir.clone();
    Ok(path.to_string_lossy().to_string())
}

