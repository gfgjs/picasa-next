// src-tauri/src/lib.rs
// src-tauri/src/lib.rs
//! Library entry point — module declarations and Tauri app builder.
//! 库入口点 — 模块声明和 Tauri 应用程序构建器。

pub mod ai;
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

use tauri::{Manager, Emitter};
use tauri_plugin_window_state::StateFlags;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::db::{create_read_pool, create_write_connection};
use crate::db::migration::run_migrations;
use crate::db::queries::get_config;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // ── Plugins ───────────────────────────────────────────────────────
        // ── 插件 ───────────────────────────────────────────────────────
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Only persist geometry — never persist VISIBLE state.
                // If VISIBLE were saved, the plugin would restore main window as
                // visible on the NEXT launch (overriding visible:false in config),
                // causing it to flash before setup() can hide it.
                //
                // 只持久化窗口几何信息，绝不持久化 VISIBLE 状态。
                // 否则插件会在下次启动时恢复 visible:true，导致主窗口在
                // splashscreen 出现前闪烁（setup() 来不及 hide() 它）。
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .skip_initial_state("splashscreen")
                .build()
        )
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

            // ── ORT DLL 路径解析 ──────────────────────────────────────────────
            // 【踩坑1】WebView2 在 Windows 上可能在我们加载之前就把 System32 里的
            //   onnxruntime.dll（通常是 ORT 1.17）加载进进程空间。
            //   设置 ORT_DYLIB_PATH 强制 ort crate 从指定路径加载，绕过系统版本。
            //
            // 【踩坑2】`load-dynamic` 与 `download-binaries` 互斥：
            //   load-dynamic 激活 ort-sys/disable-linking，build.rs 提前退出，
            //   download-binaries 完全不运行。必须手动管理 DLL。
            //
            // 【踩坑3】ORT 版本要求：
            //   - ONNX IR v10（PyTorch 2.11 导出）要求 ORT >= 1.19
            //   - eisneim FP16 外部数据格式模型要求 ORT >= 1.26
            //   - 使用 onnxruntime-node@1.26.0 自带的 DLL（bin/napi-v6/win32/x64/）
            //
            // 优先级：
            //   1. ORT_DYLIB_PATH 已设置（.cargo/config.toml 或环境变量）→ 保留，不覆盖
            //   2. 可执行文件旁边的 onnxruntime.dll（生产/打包版本）→ 使用
            //   3. 都没有 → ORT 自行搜索（可能加载到错误版本）
            if std::env::var("ORT_DYLIB_PATH").is_err() {
                // Only set it ourselves if NOT already configured by the build system
                // 只有在构建系统未配置时才自行设置
                if let Ok(exe_path) = std::env::current_exe() {
                    if let Some(exe_dir) = exe_path.parent() {
                        let ort_dylib = exe_dir.join("onnxruntime.dll");
                        if ort_dylib.exists() {
                            std::env::set_var("ORT_DYLIB_PATH", ort_dylib.to_str().unwrap());
                            info!("Set ORT_DYLIB_PATH to exe-relative path (production mode): {:?}", ort_dylib);
                        } else {
                            info!("onnxruntime.dll not found next to exe, ORT will search system PATH");
                        }
                    }
                }
            } else {
                info!("ORT_DYLIB_PATH already set (by build system): {}", std::env::var("ORT_DYLIB_PATH").unwrap_or_default());
            }



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
            let (thumb_size, thumb_skip_max_kb, thumb_strategy, gpu_engine, custom_cache_dir, log_level, custom_log_dir, thumb_cache_max_mb) = {
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
                let strategy: String = get_config(&pool, "thumb_strategy")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "cpu".to_string());
                let gpu_eng: String = get_config(&pool, "gpu_engine")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "wic".to_string());
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
                let max_mb: u64 = get_config(&pool, "thumb_cache_max_mb")
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1024);
                (size, skip, strategy, gpu_eng, cache_dir, lvl, l_dir, max_mb)
            };

            let cache_dir = custom_cache_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| app_data_dir.join("cache"));
            std::fs::create_dir_all(&cache_dir).unwrap_or_default();

            // ── Asset-protocol scope (E1) ─────────────────────────────────────
            // Grant read access only to the thumbnail cache + actual scan roots, rather
            // than blanket whole-drive globs. Source images live under arbitrary scan-root
            // paths, so they're granted at runtime here (and on add_scan_root).
            // 仅授予缩略图缓存 + 实际扫描根目录的读取权限，取代整盘通配。
            // 源图位于任意扫描根路径下，故在此（及 add_scan_root 时）运行时授权。
            {
                let scope = app.asset_protocol_scope();
                if let Err(e) = scope.allow_directory(&cache_dir, true) {
                    tracing::warn!("Failed to allow cache_dir in asset scope | 缓存目录授权失败: {}", e);
                }
                if let Ok(pool) = db_read_pool.get() {
                    if let Ok(roots) = crate::db::queries::list_scan_roots(&pool) {
                        for r in &roots {
                            if let Err(e) = scope.allow_directory(&r.path, true) {
                                tracing::warn!("Failed to allow scan root {} in asset scope | 扫描根授权失败: {}", r.path, e);
                            }
                        }
                        info!("Asset scope granted for {} scan root(s) + cache dir | 已为 {} 个扫描根 + 缓存目录授予 asset 权限", roots.len(), roots.len());
                    }
                }
            }

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

            let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level));
            let (filter, reload_handle) = tracing_subscriber::reload::Layer::new(env_filter);
            let _ = crate::ipc::config_commands::LOG_RELOAD.set(reload_handle);

            // Use Local time for log statements | 使用本地时间格式化日志
            let timer = tracing_subscriber::fmt::time::ChronoLocal::rfc_3339();

            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_timer(timer.clone())
                        .with_ansi(true)
                )
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_timer(timer)
                        .with_writer(non_blocking)
                        .with_ansi(false)
                )
                .init();

            info!("Picasa Next starting up, database path: {:?} | Picasa Next 正在启动，数据库路径: {:?}", db_path, db_path);
            info!("Log level set to: {} | 日志级别已设置为: {}", log_level, log_level);

            // ── Build AppState ─────────────────────────────────────────────
            // ── 构建 AppState ─────────────────────────────────────────────
            let cache_dir_for_task = cache_dir.clone();
            let app_state = Arc::new(AppState::new(
                db_writer,
                db_read_pool,
                cache_dir,
                log_dir,
                thumb_size,
                thumb_skip_max_kb,
                thumb_strategy,
                gpu_engine,
            ));

            // ── Pre-warm one read-pool connection ─────────────────────────────
            // Eagerly acquire (and immediately release) one read connection so the pool
            // establishes its first SQLite file handle during WebView2 cold-start, which
            // runs concurrently. By the time the frontend's first IPC calls arrive the
            // connection is already open and warmed up, saving ~50-100ms per call.
            //
            // ── 预热一个读连接池连接 ──────────────────────────────────────────
            // 提前获取（立即释放）一个读连接，让连接池在 WebView2 冷启动（并发进行）期间
            // 建立第一个 SQLite 文件句柄。等前端首批 IPC 调用到达时，连接已就绪，
            // 每次调用可节省约 50-100ms。
            drop(app_state.db_read_pool.get());

            let app_state_for_task = app_state.clone();
            app.manage(app_state);
            info!("AppState initialised | 应用状态 (AppState) 初始化完成");

            // ── Background Tasks ──────────────────────────────────────────
            // ── 后台任务 ──────────────────────────────────────────
            let handles_pool: Arc<std::sync::Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
            app.manage(handles_pool.clone());

            let h1 = tauri::async_runtime::spawn(async move {
                // Delay first run by 3 minutes so it doesn't block cold start | 延迟 3 分钟执行，避免影响冷启动
                tokio::time::sleep(std::time::Duration::from_secs(3 * 60)).await;
                loop {
                    tracing::info!("Running PRAGMA optimize for database | 正在执行数据库碎片优化");
                    if let Ok(conn) = app_state_for_task.db_writer.lock() {
                        if let Err(e) = conn.execute_batch("PRAGMA optimize;") {
                            tracing::warn!("Failed to run PRAGMA optimize | 执行数据库碎片优化失败: {}", e);
                        }
                    } else {
                        tracing::warn!("Failed to lock db_writer for PRAGMA optimize | 无法获取写入锁进行碎片优化");
                    }
                    // Run every 24 hours | 每 24 小时执行一次
                    tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
                }
            });
            handles_pool.lock().unwrap().push(h1);

            let cache_dir_clone = cache_dir_for_task;
            let h2 = tauri::async_runtime::spawn(async move {
                // Delay cache enforcement by 1 minute | 延迟 1 分钟执行缓存清理
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                crate::thumbnail::cache::enforce_cache_limit(&cache_dir_clone, thumb_cache_max_mb);
            });
            handles_pool.lock().unwrap().push(h2);

            // Force-hide the main window regardless of what tauri-plugin-window-state
            // may have restored from the previous session (it saves visible:true after
            // close_splashscreen shows the window). The splashscreen is visible:true by
            // default; main window will be revealed only when close_splashscreen is invoked
            // by the frontend after App.vue onMounted completes.
            //
            // 强制隐藏主窗口，覆盖 tauri-plugin-window-state 可能恢复的上次 visible:true 状态。
            // 主窗口仅在前端 App.vue onMounted 完成并调用 close_splashscreen 后才显示。
            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.hide();
            }

            // ── System Tray ──────────────────────────────────────────────────
            // ── 系统托盘 ──────────────────────────────────────────────────
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{TrayIconBuilder, MouseButton, TrayIconEvent};

            let show_i = MenuItem::with_id(app, "show", "显示主界面 | Show Window", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出应用 | Exit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "quit" => {
                            tracing::info!("Quit clicked from tray menu | 用户从托盘菜单点击了退出");
                            app.exit(0);
                        }
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                });
                
            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            let _tray = tray_builder.build(app)?;

            Ok(())
        })
        // ── IPC command handlers ───────────────────────────────────────────
        // ── IPC 命令处理器 ───────────────────────────────────────────
        .invoke_handler(tauri::generate_handler![
            // scan
            // scan
            ipc::scan_commands::add_scan_root,
            ipc::scan_commands::remove_scan_root,
            ipc::scan_commands::remove_scan_root_with_options,
            ipc::scan_commands::check_folder_overlap,
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
            ipc::layout_commands::get_separator_y_by_label,
            // media
            // media
            ipc::media_commands::get_media_detail,
            ipc::media_commands::get_meta_for_viewport,
            ipc::media_commands::get_adjacent_media,
            ipc::media_commands::get_companion_video_url,
            ipc::media_commands::toggle_favorite,
            ipc::media_commands::batch_toggle_favorite,
            ipc::media_commands::set_rating,
            ipc::media_commands::soft_delete_items,
            ipc::media_commands::restore_items,
            ipc::media_commands::get_trash,
            ipc::media_commands::get_stats,
            ipc::media_commands::get_directory_tree,
            ipc::media_commands::get_directory_children,
            ipc::media_commands::get_directory_ancestors,
            // thumbnails
            // thumbnails
            ipc::thumbnail_commands::batch_request_thumbnails,
            ipc::thumbnail_commands::start_full_thumbnail_generation,
            ipc::thumbnail_commands::stop_full_thumbnail_generation,
            ipc::thumbnail_commands::cancel_thumbnail_request,
            ipc::thumbnail_commands::clear_all_thumbnails,
            // search
            // search
            ipc::search_commands::search_media,
            // config
            // config
            ipc::config_commands::get_app_config,
            ipc::config_commands::get_startup_config,
            ipc::config_commands::set_app_config,
            ipc::config_commands::get_thumb_cache_dir,
            ipc::config_commands::get_log_dir,
            // system
            // system
            ipc::system_commands::show_in_explorer,
            ipc::system_commands::open_directory,
            ipc::system_commands::move_to_trash,
            ipc::system_commands::close_splashscreen,
            ipc::system_commands::set_window_theme,
            ipc::system_commands::clear_logs,
            // AI
            // AI
            ipc::ai_commands::detect_ai_provider,
            ipc::ai_commands::get_ai_status,
            ipc::ai_commands::semantic_search_cmd,
            ipc::ai_commands::start_ai_analysis,
            ipc::ai_commands::stop_ai_analysis,
            ipc::ai_commands::rebuild_embeddings,
            ipc::ai_commands::list_ai_models,
            ipc::ai_commands::import_ai_model,
            ipc::ai_commands::reload_ai_engine,
            ipc::system_commands::exit_app,
            ipc::system_commands::hide_window,
            ipc::system_commands::set_as_wallpaper,
            ipc::system_commands::copy_image_to_clipboard,
            // file ops
            ipc::file_ops_commands::create_physical_folder,
            ipc::file_ops_commands::move_media_items,
            ipc::file_ops_commands::copy_media_items,
            ipc::file_ops_commands::move_directory,
            ipc::file_ops_commands::copy_directory,
            ipc::file_ops_commands::delete_directory_to_trash,
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // Prevent the default window close behavior
                    // 阻止默认的窗口关闭物理行为
                    api.prevent_close();
                    // Emit an event to the frontend to handle it according to user settings
                    // 向前端发送事件，由前端根据用户设置处理（最小化到托盘、退出或询问）
                    if let Err(e) = window.emit("window-close-requested", ()) {
                        tracing::warn!("Failed to emit window-close-requested event: {}", e);
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("Error while building Tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit => {
                    info!("Application exiting — checkpointing WAL before termination | 退出前检查点 WAL");
                    // Truncate the WAL so it doesn't grow unbounded across sessions.
                    // `process::exit` skips Drop, so we must checkpoint explicitly here.
                    // 截断 WAL，避免跨会话无限增长。process::exit 会跳过 Drop，
                    // 因此必须在此显式检查点。
                    if let Some(state) = app_handle.try_state::<Arc<AppState>>() {
                        if let Ok(conn) = state.db_writer.lock() {
                            if let Err(e) = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);") {
                                tracing::warn!("WAL checkpoint on exit failed | 退出时 WAL 检查点失败: {}", e);
                            }
                        }
                    }

                    if let Some(handles_pool) = app_handle.try_state::<Arc<std::sync::Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>>>() {
                        if let Ok(mut lock) = handles_pool.lock() {
                            let handles: Vec<_> = lock.drain(..).collect();
                            for h in &handles {
                                h.abort();
                            }
                            let _ = tauri::async_runtime::block_on(async move {
                                let _ = tokio::time::timeout(
                                    std::time::Duration::from_secs(3),
                                    async {
                                        for h in handles {
                                            let _ = h.await;
                                        }
                                    }
                                ).await;
                            });
                            info!("Background tasks gracefully stopped | 后台任务已优雅停止");
                        }
                    }

                    std::process::exit(0);
                }
                _ => {}
            }
        });
}
