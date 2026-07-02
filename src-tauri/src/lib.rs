// src-tauri/src/lib.rs
// src-tauri/src/lib.rs
//! Library entry point — module declarations and Tauri app builder.
//! 库入口点 — 模块声明和 Tauri 应用程序构建器。

pub mod ai;
pub mod audio;
pub mod db;
pub mod derive;
pub mod download;
pub mod engine;
pub mod error;
pub mod exotic;
pub mod ipc;
pub mod layout;
pub mod proofread;
pub mod scanner;
pub mod state;
pub mod storage;
pub mod thumbnail;
pub mod utils;
pub mod video;

/// Compile-time build variant marker (§1.4.4) — "lite" (default) or "perf".
/// Surfaced to the UI/telemetry so the app can show a variant badge and gate
/// "needs Perf / missing component" hints.
/// 编译期构建变体标记（§1.4.4）—— "lite"（默认）或 "perf"。
/// 暴露给 UI/埋点，用于显示变体角标并提示「需性能版 / 缺组件」。
pub const BUILD_VARIANT: &str = if cfg!(feature = "perf") {
    "perf"
} else {
    "lite"
};

use std::sync::Arc;

use tauri::{Emitter, Manager};
use tauri_plugin_window_state::StateFlags;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::db::migration::run_migrations;
use crate::db::queries::get_config;
use crate::db::{create_read_pool, create_write_connection};
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
            let app_data_dir = match app.path().app_data_dir() {
                Ok(d) => d,
                Err(e) => fatal_startup_error(
                    app.handle(),
                    "无法获取应用数据目录 / cannot resolve app data dir",
                    &e.to_string(),
                ),
            };
            if let Err(e) = std::fs::create_dir_all(&app_data_dir) {
                fatal_startup_error(
                    app.handle(),
                    "无法创建应用数据目录 / cannot create app data dir",
                    &e.to_string(),
                );
            }

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
                            std::env::set_var(
                                "ORT_DYLIB_PATH",
                                ort_dylib.to_string_lossy().as_ref(),
                            );
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
            let db_writer = match create_write_connection(&db_path) {
                Ok(w) => w,
                Err(e) => fatal_startup_error(
                    app.handle(),
                    "无法打开数据库写入连接 / cannot open DB write connection",
                    &e.to_string(),
                ),
            };

            {
                let conn = db_writer.lock().unwrap();
                if let Err(e) = run_migrations(&conn) {
                    // 迁移已事务化：失败整块回滚、版本号不前进，重启可安全重跑。
                    // 仍失败多为 DB 损坏 / 磁盘 / 权限 → 给可诊断提示而非裸 panic。
                    fatal_startup_error(
                        app.handle(),
                        "数据库迁移失败 / database migration failed",
                        &format!("{e}（数据库 / db: {}）", db_path.display()),
                    );
                }
            }

            // ── Read pool (desktop) ───────────────────────────────────────
            // 8 connections: the foreground interleaves compute_layout + viewport meta +
            // thumbnail batches while background derivation/AI also read — 4 left those queuing
            // (布局被后台读饿死的次因). WAL makes extra read connections cheap.
            // ── 读取池（桌面端） ─────────────────────────────────────
            // 8 个连接：前台会交错 compute_layout + 可视区元数据 + 缩略图批，同时后台派生/AI 也在读
            // —— 4 个会让它们排队（布局被后台读饿死的次因）。WAL 下额外读连接开销很低。
            let db_read_pool = match create_read_pool(&db_path, 8) {
                Ok(p) => p,
                Err(e) => fatal_startup_error(
                    app.handle(),
                    "无法创建数据库读取连接池 / cannot create DB read pool",
                    &e.to_string(),
                ),
            };

            // ── Read persisted config ─────────────────────────────────────
            // ── 读取持久化配置 ─────────────────────────────────────
            let (thumb_size, thumb_skip_max_kb, thumb_strategy, gpu_engine, custom_cache_dir, log_level, custom_log_dir, thumb_cache_max_mb, ai_hq_cache) = {
                let pool = match db_read_pool.get() {
                    Ok(p) => p,
                    Err(e) => fatal_startup_error(
                        app.handle(),
                        "无法从读取池取连接 / cannot acquire read-pool connection",
                        &e.to_string(),
                    ),
                };
                let size: u32 = get_config(&pool, "thumb_size")
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(480);
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
                // AI 高清缓存开关（opt-in，默认关）；驱动缩略图流水线是否顺带产出 AI 缓存。
                let ai_hq: bool = get_config(&pool, "ai_hq_cache_enabled")
                    .ok()
                    .flatten()
                    .map(|v| v == "true")
                    .unwrap_or(false);
                (size, skip, strategy, gpu_eng, cache_dir, lvl, l_dir, max_mb, ai_hq)
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
            // 内置冷门格式能力目录（编译期嵌入）。解析失败不致命：降级为空目录并告警，
            // 主功能不受影响（仅 exotic 识别失效）。
            let exotic_catalog = Arc::new(
                crate::exotic::CatalogStore::from_builtin().unwrap_or_else(|e| {
                    tracing::error!("内置 exotic Catalog 解析失败，降级为空目录 | {e}");
                    crate::exotic::CatalogStore::with_snapshot(
                        crate::exotic::CatalogSnapshot::empty(),
                    )
                }),
            );

            let cache_dir_for_task = cache_dir.clone();
            let app_state = Arc::new(AppState::new(
                db_writer,
                db_read_pool,
                cache_dir,
                log_dir,
                app_data_dir.join("exotic"),
                thumb_size,
                thumb_skip_max_kb,
                thumb_strategy,
                gpu_engine,
                ai_hq_cache,
                exotic_catalog,
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
            let app_state_for_coord = app_state.clone();
            let app_state_for_volwatch = app_state.clone();
            app.manage(app_state);
            info!("AppState initialised | 应用状态 (AppState) 初始化完成");

            // ── exotic（冷门格式插件）Coordinator（Part2 §4.1）──────────────────────
            // 单一调度器：接扫描/安装/激活/配置/重试事件，幂等唤醒唯一 Pipeline。Part2 无真实
            // License → 默认门控为不可领取（除 dev fixture）；有 Worker + 授权时自动出图。
            {
                // 运行期 Host：catalog + 只读连接池安装真相 + keyring 授权真相（Part3 §5）。
                let host = std::sync::Arc::new(app_state_for_coord.exotic_host());
                let coord = crate::exotic::coordinator::ExoticCoordinator::start(
                    app.handle().clone(),
                    app_state_for_coord.clone(),
                    host,
                );
                app_state_for_coord.set_exotic_coordinator(coord);
                // 启动 wake：恢复上次遗留的就绪任务（孤儿恢复 + backfill 后的待处理）。
                app_state_for_coord.wake_exotic(crate::exotic::coordinator::WakeReason::Startup);
                info!("exotic Coordinator 已启动 | exotic Coordinator started");
            }

            // ── Background Tasks ──────────────────────────────────────────
            // ── 后台任务 ──────────────────────────────────────────
            let handles_pool: Arc<std::sync::Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
            app.manage(handles_pool.clone());

            // 卷插拔监听（Part2 T2 / C5 Piece B）：冷启动延迟后每 15s 对账已知卷在线态，
            // 实时维护 availability(online↔offline)，拔盘/插回 ≤15s 反映到画廊。
            let volwatch_handle = crate::scanner::volume_watch::spawn(
                app.handle().clone(),
                app_state_for_volwatch,
            );
            handles_pool.lock().unwrap().push(volwatch_handle);

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
            ipc::layout_commands::get_view_ids, // T14.5/T18：按布局序的视图全集 id（Part5 选区前置）
            ipc::layout_commands::get_layout_rows,
            ipc::layout_commands::get_layout_rows_by_y,
            ipc::layout_commands::get_separator_y_by_group_id,
            ipc::layout_commands::get_item_y_by_id,
            ipc::layout_commands::get_subtree_scroll_target,
            // media
            // media
            ipc::media_commands::get_media_detail,
            ipc::media_commands::get_meta_for_viewport,
            ipc::media_commands::get_adjacent_media,
            ipc::media_commands::get_companion_video_url,
            ipc::media_commands::get_keyframe_sprite,
            // audio player (需求6, §3.6)
            // 音频播放器（需求6, §3.6）
            ipc::audio_commands::get_audio_detail,
            ipc::media_commands::toggle_favorite,
            ipc::media_commands::batch_toggle_favorite,
            ipc::media_commands::set_rating,
            ipc::media_commands::batch_set_rating,
            ipc::media_commands::set_color_label,
            ipc::media_commands::batch_set_color_label,
            ipc::media_commands::soft_delete_items,
            ipc::media_commands::restore_items,
            ipc::media_commands::resolve_selection, // Part5 S4：选择描述符 → id 列表（按视图布局序）
            ipc::media_commands::count_selection,   // Part5 S4：选择描述符精确计数（SelectAll 走 COUNT(*)）
            ipc::media_commands::get_trash,
            ipc::media_commands::get_stats,
            ipc::media_commands::get_directory_tree,
            ipc::media_commands::get_directory_children,
            ipc::media_commands::get_directory_ancestors,
            ipc::media_commands::list_directory_files,
            ipc::media_commands::prioritize_dimensions,
            // thumbnails
            // thumbnails
            ipc::thumbnail_commands::batch_request_thumbnails,
            ipc::thumbnail_commands::start_full_thumbnail_generation,
            ipc::thumbnail_commands::stop_full_thumbnail_generation,
            ipc::thumbnail_commands::cancel_thumbnail_request,
            ipc::thumbnail_commands::clear_all_thumbnails,
            // exotic（冷门格式插件）查询命令（Part1 §2.3）
            ipc::exotic_commands::list_exotic_format_resolutions,
            ipc::exotic_commands::get_exotic_item_state,
            ipc::exotic_commands::list_installed_exotic_plugins,
            ipc::exotic_commands::get_plugin_entitlement,
            // exotic 处理控制命令（Part2 §4.5）
            ipc::exotic_commands::start_exotic_processing,
            ipc::exotic_commands::pause_exotic_processing,
            ipc::exotic_commands::stop_exotic_processing,
            ipc::exotic_commands::get_exotic_processing_status,
            ipc::exotic_commands::retry_exotic_task,
            ipc::exotic_commands::retry_exotic_plugin_failures,
            // exotic 激活 / 移除授权命令（Part3 §6.6）
            ipc::exotic_commands::activate_exotic_plugin,
            ipc::exotic_commands::deactivate_exotic_plugin,
            // exotic 安装 / 卸载 / 修复 / 回滚 / Registry 命令（Part3 §6.4-6.6）
            ipc::exotic_commands::fetch_exotic_registry,
            ipc::exotic_commands::list_exotic_registry,
            ipc::exotic_commands::install_exotic_plugin,
            ipc::exotic_commands::repair_exotic_plugin,
            ipc::exotic_commands::rollback_exotic_plugin,
            ipc::exotic_commands::uninstall_exotic_plugin,
            // volume（已知卷面板，T13 离线 UX）
            // volume
            ipc::volume_commands::list_volumes,
            ipc::volume_commands::rename_volume,
            ipc::volume_commands::forget_volume,
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
            ipc::config_commands::get_cache_stats,
            ipc::config_commands::clear_cache,
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
            ipc::ai_commands::restart_ai_analysis,
            ipc::ai_commands::pause_ai_analysis,
            ipc::ai_commands::stop_ai_analysis,
            ipc::ai_commands::rebuild_embeddings,
            ipc::ai_commands::list_ai_models,
            ipc::ai_commands::import_ai_model,
            ipc::ai_commands::reload_ai_engine,
            ipc::ai_commands::list_model_registry,
            ipc::ai_commands::set_active_model,
            ipc::ai_commands::download_model,
            // Face recognition (F5)
            // 人脸识别（F5）
            ipc::face_commands::get_face_status,
            ipc::face_commands::start_face_analysis,
            ipc::face_commands::restart_face_analysis,
            ipc::face_commands::pause_face_analysis,
            ipc::face_commands::stop_face_analysis,
            ipc::face_commands::list_face_persons,
            ipc::face_commands::get_item_faces,
            ipc::face_commands::rename_face_person,
            ipc::face_commands::set_face_person_hidden,
            ipc::face_commands::merge_face_persons,
            ipc::face_commands::recluster_faces,
            // 批量审批（Part4 T3 / §3.5.1）
            ipc::face_commands::confirm_faces,
            ipc::face_commands::reassign_faces,
            ipc::face_commands::unassign_faces,
            ipc::face_commands::reject_faces,
            ipc::face_commands::create_person,
            ipc::face_commands::list_likely_face_matches,
            ipc::face_commands::list_face_model_registry,
            ipc::face_commands::download_face_model,
            // derivation pipeline (video cover/keyframes, doc thumb, audio cover/meta)
            // 派生流水线（视频封面/关键帧、文档缩略图、音频封面/元数据）
            ipc::derive_commands::start_derivation,
            ipc::derive_commands::pause_derivation,
            ipc::derive_commands::stop_derivation,
            ipc::derive_commands::derivation_status,
            // documents (P4): doc thumbnail frontend-render loop (§3.4)
            // 文档（P4）：文档缩略图前端渲染回环（§3.4）
            ipc::doc_commands::ensure_doc_thumb_queue,
            ipc::doc_commands::list_pending_doc_thumbs,
            ipc::doc_commands::store_doc_thumbnail,
            ipc::doc_commands::get_reading_progress,
            ipc::doc_commands::set_reading_progress,
            ipc::doc_commands::list_replacements,
            ipc::doc_commands::get_effective_replacements,
            ipc::doc_commands::upsert_replacement,
            ipc::doc_commands::delete_replacement,
            ipc::doc_commands::list_versions,
            ipc::doc_commands::get_current_version,
            ipc::doc_commands::get_document_text,
            ipc::doc_commands::get_version_content,
            ipc::doc_commands::save_version,
            ipc::doc_commands::set_current_version,
            ipc::doc_commands::delete_version,
            ipc::doc_commands::diff_versions,
            ipc::doc_commands::diff_texts,
            // documents (P4): remote AI proofreading (§5.4)
            // 文档（P4）：远程 AI 校对（§5.4）
            ipc::proofread_commands::get_proofread_config,
            ipc::proofread_commands::set_proofread_config,
            ipc::proofread_commands::set_proofread_key,
            ipc::proofread_commands::clear_proofread_key,
            ipc::proofread_commands::proofread_chunk,
            // collections / favorites (需求7)
            // 收藏夹（需求7）
            ipc::collection_commands::list_collections,
            ipc::collection_commands::recent_collections,
            ipc::collection_commands::create_collection,
            ipc::collection_commands::delete_collection,
            ipc::collection_commands::rename_collection,
            ipc::collection_commands::add_to_collection,
            ipc::collection_commands::remove_from_collection,
            // storage backends (network drives, 需求8 8B, §3.8)
            // 存储后端（网络盘, 需求8 8B, §3.8）
            ipc::storage_commands::list_backends,
            ipc::storage_commands::add_backend,
            ipc::storage_commands::test_backend,
            ipc::storage_commands::remove_backend,
            ipc::system_commands::exit_app,
            ipc::system_commands::hide_window,
            ipc::system_commands::set_as_wallpaper,
            ipc::system_commands::copy_image_to_clipboard,
            // file ops
            ipc::file_ops_commands::create_physical_folder,
            ipc::file_ops_commands::move_media_items,
            ipc::file_ops_commands::copy_media_items,
            ipc::file_ops_commands::relocate_media_items,
            ipc::file_ops_commands::copy_media_items_db,
            ipc::file_ops_commands::remove_media_items_hard,
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
                            tauri::async_runtime::block_on(async move {
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
                tauri::RunEvent::Ready
                    // 开机冒烟测试：设 PICASA_SMOKE_TEST 时，应用一旦「启动就绪」即退出 0。
                    // CI headless 启动构建产物 + 断言退出码非 101 → 把「开机 panic」
                    //（coordinator 无 reactor / 迁移失败 等）挡在合并前，而非等 run dev 才发现。
                    if std::env::var_os("PICASA_SMOKE_TEST").is_some() => {
                        eprintln!(
                            "[smoke] boot reached RunEvent::Ready — startup OK | 开机就绪，冒烟测试通过"
                        );
                        app_handle.exit(0);
                    }
                _ => {}
            }
        });
}

/// 致命启动错误的统一出口：取代裸 `.expect()` 的不可读 panic。
/// 写清晰可诊断信息到 stderr（日志子系统在 DB 初始化后才装好，此前唯 stderr 保证可见）
/// + 尽力弹原生对话框（失败不影响退出）+ 受控退出码 1（区别于 panic 的 101）。
fn fatal_startup_error(app: &tauri::AppHandle, context: &str, detail: &str) -> ! {
    eprintln!("[FATAL][startup] {context} | 启动失败: {detail}");
    let msg = format!(
        "Picasa Next 启动失败：{context}\n\n详情 / Detail: {detail}\n\n\
         可尝试：检查数据库文件是否损坏（可备份后删除以重置），或查看日志后重试。"
    );
    use tauri_plugin_dialog::DialogExt;
    let _ = app
        .dialog()
        .message(msg)
        .title("Picasa Next 启动失败 / Startup failed")
        .blocking_show();
    std::process::exit(1);
}
