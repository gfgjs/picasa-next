// src-tauri/src/lib.rs
// src-tauri/src/lib.rs
//! Library entry point — module declarations and Tauri app builder.
//! 库入口点 — 模块声明和 Tauri 应用程序构建器。

pub mod db;
pub mod engine;
pub mod error;
pub mod ipc;
pub mod layout;
pub mod scanner;
pub mod state;
pub mod thumbnail;
pub mod utils;

use std::sync::Arc;

use tauri::Manager;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use crate::db::{create_read_pool, create_write_connection};
use crate::db::migration::run_migrations;
use crate::db::queries::get_config;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // ── Plugins ───────────────────────────────────────────────────────
        // ── 插件 ───────────────────────────────────────────────────────
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        // ── App setup ─────────────────────────────────────────────────────
        // ── 应用程序设置 ─────────────────────────────────────────────────────
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data dir");

            let db_path = app_data_dir.join("picasa_next.db");

            // ── Write connection + migrations ─────────────────────────────
            // ── 写入连接 + 迁移 ─────────────────────────────
            let db_writer = create_write_connection(&db_path)
                .expect("Failed to open write connection");

            {
                let conn = db_writer.lock().unwrap();
                run_migrations(&conn).expect("Migration failed");
            }

            // ── Read pool (4 connections for desktop) ─────────────────────
            // ── 读取池（桌面端 4 个连接） ─────────────────────
            let db_read_pool = create_read_pool(&db_path, 4)
                .expect("Failed to create read pool");

            // ── Read persisted config ─────────────────────────────────────
            // ── 读取持久化配置 ─────────────────────────────────────
            let (thumb_size, thumb_skip_max_kb, custom_cache_dir, log_level, custom_log_dir) = {
                let pool = db_read_pool.get().expect("Pool error");
                let size: u32 = get_config(&pool, "thumb_size")
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(300);
                let skip: u64 = get_config(&pool, "thumb_skip_max_kb")
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(200);
                let cache_dir: Option<String> = get_config(&pool, "thumb_cache_dir")
                    .ok()
                    .flatten();
                let lvl: String = get_config(&pool, "log_level")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "debug".to_string());
                let l_dir: Option<String> = get_config(&pool, "log_dir")
                    .ok()
                    .flatten();
                (size, skip, cache_dir, lvl, l_dir)
            };

            let cache_dir = custom_cache_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| app_data_dir.join("cache"));
            std::fs::create_dir_all(&cache_dir).unwrap_or_default();

            // ── Logging ───────────────────────────────────────────────────────────
            // ── 日志记录 ───────────────────────────────────────────────────────────
            let log_dir = custom_log_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| app_data_dir.join("logs"));
            std::fs::create_dir_all(&log_dir).unwrap_or_default();

            // Custom writer to ensure real-time NTFS metadata updates on Windows | 自定义写入器以确保在 Windows 上实时更新 NTFS 元数据
            #[derive(Clone)]
            struct RealTimeDailyAppender {
                log_dir: std::path::PathBuf,
            }
            impl std::io::Write for RealTimeDailyAppender {
                fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let path = self.log_dir.join(format!("picasa-next.{}.log", today));
                    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                        let res = file.write(buf);
                        let _ = file.sync_data(); // Force OS to flush metadata (size) to disk | 强制操作系统将元数据（大小）刷新到磁盘
                        res
                    } else {
                        Ok(buf.len()) // Silently drop if we can't write to avoid crashing | 如果无法写入则静默丢弃，避免崩溃
                    }
                }
                fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
            }

            // Use non_blocking to offload writes and flushes to a background thread | 使用 non_blocking 将写入和刷新转移到后台线程
            let appender = RealTimeDailyAppender { log_dir: log_dir.clone() };
            let (non_blocking, guard) = tracing_appender::non_blocking(appender);
            Box::leak(Box::new(guard));

            let env_filter_term = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level));
            let env_filter_file = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level));

            // Use Local time for log statements | 使用本地时间格式化日志
            let timer = tracing_subscriber::fmt::time::ChronoLocal::rfc_3339();

            tracing_subscriber::registry()
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_timer(timer.clone())
                        .with_ansi(true)
                        .with_filter(env_filter_term)
                )
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_timer(timer)
                        .with_writer(non_blocking)
                        .with_ansi(false)
                        .with_filter(env_filter_file)
                )
                .init();

            info!("Picasa Next starting up, database path: {:?} | Picasa Next 正在启动，数据库路径: {:?}", db_path, db_path);
            info!("Log level set to: {} | 日志级别已设置为: {}", log_level, log_level);

            // ── Build AppState ─────────────────────────────────────────────
            // ── 构建 AppState ─────────────────────────────────────────────
            let app_state = AppState::new(
                db_writer,
                db_read_pool,
                cache_dir,
                log_dir,
                thumb_size,
                thumb_skip_max_kb,
            );

            app.manage(Arc::new(app_state));
            info!("AppState initialised | 应用状态 (AppState) 初始化完成");
            Ok(())
        })
        // ── IPC command handlers ───────────────────────────────────────────
        // ── IPC 命令处理器 ───────────────────────────────────────────
        .invoke_handler(tauri::generate_handler![
            // scan
            // scan
            ipc::scan_commands::add_scan_root,
            ipc::scan_commands::remove_scan_root,
            ipc::scan_commands::list_scan_roots,
            ipc::scan_commands::start_scan,
            ipc::scan_commands::stop_scan,
            ipc::scan_commands::clear_database,
            ipc::scan_commands::clear_settings,
            // layout
            // layout
            ipc::layout_commands::compute_layout,
            ipc::layout_commands::get_layout_rows,
            ipc::layout_commands::get_layout_rows_by_y,
            // media
            // media
            ipc::media_commands::get_media_detail,
            ipc::media_commands::get_adjacent_media,
            ipc::media_commands::get_companion_video_url,
            ipc::media_commands::toggle_favorite,
            ipc::media_commands::set_rating,
            ipc::media_commands::soft_delete_items,
            ipc::media_commands::restore_items,
            ipc::media_commands::get_trash,
            ipc::media_commands::get_stats,
            ipc::media_commands::get_directory_tree,
            ipc::media_commands::get_directory_children,
            // thumbnails
            // thumbnails
            ipc::thumbnail_commands::batch_request_thumbnails,
            ipc::thumbnail_commands::start_full_thumbnail_generation,
            ipc::thumbnail_commands::stop_full_thumbnail_generation,
            ipc::thumbnail_commands::cancel_thumbnail_request,
            // search
            // search
            ipc::search_commands::search_media,
            // config
            // config
            ipc::config_commands::get_app_config,
            ipc::config_commands::set_app_config,
            ipc::config_commands::get_thumb_cache_dir,
            ipc::config_commands::get_log_dir,
            // system
            // system
            ipc::system_commands::show_in_explorer,
            ipc::system_commands::open_directory,
            ipc::system_commands::move_to_trash,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Tauri application");
}
