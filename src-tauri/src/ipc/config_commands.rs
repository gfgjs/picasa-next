// src-tauri/src/ipc/config_commands.rs
//! App configuration key-value commands (§ 6.1 — config).
//! 应用配置键值命令（§ 6.1 — config）。

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::db::queries::{get_config, set_config};
use crate::error::{AppError, Result};
use crate::state::AppState;
use std::sync::OnceLock;
use tracing_subscriber::{reload::Handle, EnvFilter, Registry};

pub static LOG_RELOAD: OnceLock<Handle<EnvFilter, Registry>> = OnceLock::new();

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
pub async fn get_app_config(
    key: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<String>> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        get_config(&pool, &key)
    })
    .await
    .map_err(|e| AppError::System(format!("spawn_blocking join error: {e}")))?
}

/// All config values needed by the frontend on startup, fetched in a single SQLite round-trip.
/// Replaces 4 separate get_app_config IPC calls in App.vue onMounted, reducing IPC overhead
/// from N×(serialisation + Tokio schedule + pool acquire + SQLite + deserialise)
/// down to 1×(same overhead) + N×SQLite row reads (negligible, same connection).
///
/// 前端启动时所需的所有配置，通过单次 SQLite 往返批量获取。
/// 替代 App.vue onMounted 里 4 次独立 get_app_config IPC，将开销从
/// N×（序列化 + Tokio 调度 + 连接池获取 + SQLite + 反序列化）
/// 降低到 1×相同开销 + N×SQLite 行读取(可忽略,同一连接)。
///
/// R2-4(2026-07-02):扩容为 14 键——uiStore 的 9 项模块初始化配置与 App.vue 的
/// first_launch 一并并入,整个启动阶段的配置 IPC 由 11 次归 1 次。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupConfig {
    pub language: Option<String>,
    pub timeline_scroll_width: Option<String>,
    pub ui_font_size: Option<String>,
    pub enable_thumb_hover_scale: Option<String>,
    // R2-4:uiStore 9 项模块初始化配置 + first_launch(App.vue 首启检测)。
    pub grid_row_height: Option<String>,
    pub group_by: Option<String>,
    pub sort_within_group: Option<String>,
    pub layout_mode: Option<String>,
    pub close_behavior: Option<String>,
    pub pinned_settings: Option<String>,
    pub show_thumb_info: Option<String>,
    pub thumb_info_elements: Option<String>,
    pub hover_autoplay: Option<String>,
    pub first_launch: Option<String>,
}

#[tauri::command]
pub async fn get_startup_config(state: State<'_, Arc<AppState>>) -> Result<StartupConfig> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        Ok(StartupConfig {
            language: get_config(&pool, "language")?,
            timeline_scroll_width: get_config(&pool, "timeline_scroll_width")?,
            ui_font_size: get_config(&pool, "ui_font_size")?,
            enable_thumb_hover_scale: get_config(&pool, "enable_thumb_hover_scale")?,
            grid_row_height: get_config(&pool, "grid_row_height")?,
            group_by: get_config(&pool, "group_by")?,
            sort_within_group: get_config(&pool, "sort_within_group")?,
            layout_mode: get_config(&pool, "layout_mode")?,
            close_behavior: get_config(&pool, "close_behavior")?,
            pinned_settings: get_config(&pool, "pinned_settings")?,
            show_thumb_info: get_config(&pool, "show_thumb_info")?,
            thumb_info_elements: get_config(&pool, "thumb_info_elements")?,
            hover_autoplay: get_config(&pool, "hover_autoplay")?,
            first_launch: get_config(&pool, "first_launch")?,
        })
    })
    .await
    .map_err(|e| AppError::System(format!("spawn_blocking join error: {e}")))?
}

/// Set a configuration value.
/// 设置配置值。
#[tauri::command]
pub async fn set_app_config(
    app: AppHandle,
    key: String,
    value: String,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    {
        // R1-3：落库离开 tokio worker（key/value 随后还要驱动内存配置分支，克隆入闭包）。
        let (k, v) = (key.clone(), value.clone());
        super::blocking::write_blocking(&state, move |c| set_config(c, &k, &v)).await?;
    }

    // Track whether a thumb-config key changed that requires re-evaluation
    // of direct-display items (thumb_status=3).
    // 跟踪是否有缩略图配置键的变更需要重新评估直接显示项（thumb_status=3）。
    let mut needs_thumb_reset = false;

    if key == "thumb_skip_max_kb" {
        if let Ok(val) = value.parse::<u64>() {
            let mut config = state.thumb_config.write().unwrap();
            config.skip_max_bytes = val * 1024;
            needs_thumb_reset = true;
        }
    } else if key == "thumb_size" {
        if let Ok(val) = value.parse::<u32>() {
            let mut config = state.thumb_config.write().unwrap();
            config.size = val;
        }
    } else if key == "thumb_cache_dir" {
        let new_cache_dir = std::path::PathBuf::from(&value);
        let mut config = state.thumb_config.write().unwrap();
        config.cache_dir = new_cache_dir.clone();
        std::fs::create_dir_all(&config.cache_dir).unwrap_or_default();
        if let Err(e) = app
            .asset_protocol_scope()
            .allow_directory(&new_cache_dir, true)
        {
            tracing::warn!(
                "Failed to allow updated cache_dir in asset scope | 更新后的缓存目录授权失败: {}",
                e
            );
        }
    } else if key == "thumb_strategy" {
        let mut config = state.thumb_config.write().unwrap();
        config.strategy = value;
        needs_thumb_reset = true;
    } else if key == "gpu_engine" {
        let mut config = state.thumb_config.write().unwrap();
        config.gpu_engine = value;
    } else if key == "ai_hq_cache_enabled" {
        // 让 AI 高清缓存开关运行时即时生效：缩略图流水线据此决定是否顺带产出 AI 缓存（无需重启）。
        let mut config = state.thumb_config.write().unwrap();
        config.ai_hq_cache = value == "true";
    } else if key == "log_level" {
        if let Some(handle) = LOG_RELOAD.get() {
            if let Ok(filter) = EnvFilter::try_new(&value) {
                if let Err(e) = handle.modify(|f| *f = filter) {
                    tracing::warn!("Failed to reload log level: {}", e);
                }
            }
        }
        tracing::info!("Log level dynamically updated to: {}", value);
    }

    // When skip threshold or strategy changes, items that were previously
    // marked as "direct display" (status=3) may now need real thumbnails.
    // Reset them to pending (status=0) so on-demand generation picks them up.
    // 当跳过阈值或策略变更时，之前标记为"直接显示"（status=3）的项
    // 可能现在需要真正的缩略图。将它们重置为待处理（status=0），
    // 以便按需生成机制重新处理它们。
    if needs_thumb_reset {
        let affected = super::blocking::write_blocking(&state, |c| {
            Ok(c.execute(
                "UPDATE media_items SET thumb_status = 0, thumb_path = NULL, thumbhash = NULL \
             WHERE thumb_status = 3 AND is_deleted = 0",
                [],
            )
            .unwrap_or(0))
        })
        .await?;
        tracing::info!(
            "[Config] thumb config changed (key={}), reset {} direct-display items to pending | 缩略图配置变更，重置 {} 个直接显示项为待处理",
            key, affected, affected
        );
        // Invalidate layout cache so compute_layout reads fresh status from DB
        // 清空布局缓存，使 compute_layout 从 DB 读取最新状态
        *state.layout_cache.write().unwrap() = None;
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

/// 缓存占用统计（各子目录字节 + 总量 + LRU 上限），供设置面板展示（Part3 §3.3.3 / Q8）。
/// 遍历缓存目录是阻塞 IO，走 spawn_blocking。
#[tauri::command]
pub async fn get_cache_stats(
    state: State<'_, Arc<AppState>>,
) -> Result<crate::thumbnail::cache::CacheStats> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let cache_dir = state.thumb_config.read().unwrap().cache_dir.clone();
        let limit_mb = {
            let pool = state.db_read_pool.get().map_err(AppError::from)?;
            get_config(&pool, "thumb_cache_max_mb")
                .ok()
                .flatten()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
        };
        Ok(crate::thumbnail::cache::compute_cache_stats(
            &cache_dir, limit_mb,
        ))
    })
    .await
    .map_err(|e| AppError::System(format!("spawn_blocking join error: {e}")))?
}

/// 手动清理缓存子集（`kind` ∈ thumbnails/ai/sprites/motion/all），返回释放字节数（§3.3.3 / Q8）。
/// 与 LRU 一致：只删磁盘、不改 DB——缺图由生成流水线按需重建。删除是阻塞 IO，走 spawn_blocking。
#[tauri::command]
pub async fn clear_cache(kind: String, state: State<'_, Arc<AppState>>) -> Result<u64> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let cache_dir = state.thumb_config.read().unwrap().cache_dir.clone();
        Ok(crate::thumbnail::cache::clear_cache_kind(&cache_dir, &kind))
    })
    .await
    .map_err(|e| AppError::System(format!("spawn_blocking join error: {e}")))?
}
