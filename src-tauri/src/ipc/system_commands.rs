// src-tauri/src/ipc/system_commands.rs
//! System-level commands (§ 6.1 — system).
//! 系统级命令（§ 6.1 — 系统）。

use std::sync::Arc;

use tauri::{Manager, State};
use tracing::info;

use crate::db::queries::get_item_path_info;
use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::utils::path::resolve_media_path;

/// Reveal a media item in the OS file explorer.
/// 在操作系统文件资源管理器中显示媒体项。
#[tauri::command]
pub async fn show_in_explorer(item_id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    let (root, rel, name) = get_item_path_info(&pool, item_id)?;
    let abs_path = resolve_media_path(&root, &rel, &name);
    info!("show_in_explorer: {abs_path} | 在资源管理器中显示: {abs_path}");

    // Platform-specific file reveal
    // 特定平台的文件显示
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", abs_path.replace('/', "\\")))
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

/// Open an arbitrary directory in the OS file explorer.
/// 在操作系统文件资源管理器中打开任意目录。
#[tauri::command]
pub async fn open_directory(path: String) -> Result<()> {
    info!("open_directory: {path} | 打开目录: {path}");

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path.replace('/', "\\"))
            .spawn()
            .map_err(AppError::from)?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(AppError::from)?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(AppError::from)?;
    }

    Ok(())
}

/// Move items to the system trash (Phase 2 — stub for now).
/// 将项目移至系统垃圾桶（阶段 2 — 暂时为存根）。
#[tauri::command]
pub async fn move_to_trash(item_ids: Vec<i64>, state: State<'_, Arc<AppState>>) -> Result<()> {
    // Phase 2: integrate `trash` crate
    // 阶段 2：集成 `trash` crate
    // For now, fall back to soft delete
    // 目前，退回到软删除
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    crate::db::queries::soft_delete_items(&conn, &item_ids)
}

/// Atomically close the splashscreen window and reveal the main window.
/// Called by the frontend once App.vue onMounted is complete.
///
/// 原子化关闭 Splashscreen 窗口并显示主窗口。
/// 由前端在 App.vue onMounted 完成后调用。
#[tauri::command]
pub async fn close_splashscreen(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    // ── Startup timing ────────────────────────────────────────────────────
    // Measure elapsed time from AppState initialisation to main window reveal.
    // This covers: WebView2 cold-start + Vite bundle load + Vue bootstrap + IPC round-trips.
    //
    // ── 启动耗时统计 ──────────────────────────────────────────────────────
    // 测量从 AppState 初始化完成到主窗口弹出的总耗时。
    // 涵盖：WebView2 冷启动 + Vite 包加载 + Vue 初始化 + IPC 往返。
    let elapsed = state.startup_instant.elapsed();
    info!(
        "⏱  AppState → main window: {:.0?} ({} ms) | ⏱  AppState 初始化完成 → 主界面弹出: {:.0?} ({} ms)",
        elapsed,
        elapsed.as_millis(),
        elapsed,
        elapsed.as_millis(),
    );

    // Close splashscreen first so there is no flash of both windows being visible.
    // 先关闭 splashscreen，避免两个窗口同时可见的闪烁。
    if let Some(splash) = app.get_webview_window("splashscreen") {
        splash.close().map_err(|e| e.to_string())?;
    }
    // Show main window and bring it to focus.
    // 显示主窗口并使其获得焦点。
    if let Some(main_win) = app.get_webview_window("main") {
        main_win.show().map_err(|e| e.to_string())?;
        main_win.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
pub async fn set_window_theme(
    app: tauri::AppHandle,
    theme: String,
    resolved: String,
) -> std::result::Result<(), String> {
    if let Some(main_win) = app.get_webview_window("main") {
        let t = match theme.as_str() {
            "dark" => Some(tauri::Theme::Dark),
            "light" => Some(tauri::Theme::Light),
            _ => None,
        };
        // Set Tauri's theme first
        // 先设置 Tauri 的主题
        let _ = main_win.set_theme(t.clone());

        // Explicitly set DWM title bar theme on Windows
        // 在 Windows 上显式设置 DWM 标题栏主题
        #[cfg(target_os = "windows")]
        {
            if let Ok(hwnd) = main_win.hwnd() {
                use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
                use windows::Win32::Foundation::HWND;

                let is_dark = match resolved.as_str() {
                    "dark" => true,
                    _ => false,
                };

                let dark_mode: i32 = if is_dark { 1 } else { 0 };
                unsafe {
                    let _ = DwmSetWindowAttribute(
                        HWND(hwnd.0 as _),
                        DWMWA_USE_IMMERSIVE_DARK_MODE,
                        &dark_mode as *const i32 as *const _,
                        std::mem::size_of::<i32>() as u32,
                    );
                }
            }
        }
    }
    Ok(())
}

/// Clear all log files.
/// 清除所有日志文件。
#[tauri::command]
pub async fn clear_logs(state: State<'_, Arc<AppState>>) -> Result<()> {
    let log_dir = &state.log_dir;
    if log_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(log_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "log") {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }
    tracing::info!("Logs cleared by user | 用户清除了日志文件");
    Ok(())
}

/// Explicitly exit the application.
/// 明确退出应用程序。
#[tauri::command]
pub async fn exit_app(app: tauri::AppHandle) {
    tracing::info!("exit_app called from frontend, terminating process. | 前端调用了 exit_app，正在终止进程。");
    app.exit(0);
}

/// Hide the main window (minimize to tray).
/// 隐藏主窗口（最小化到托盘）。
#[tauri::command]
pub async fn hide_window(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

/// Set a media item as desktop wallpaper.
/// 将媒体项设置为桌面壁纸。
#[tauri::command]
pub async fn set_as_wallpaper(item_id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    let (root, rel, name) = get_item_path_info(&pool, item_id)?;
    let abs_path = resolve_media_path(&root, &rel, &name);
    info!("set_as_wallpaper: {abs_path} | 设为壁纸: {abs_path}");

    wallpaper::set_from_path(&abs_path).map_err(|e| AppError::Engine(format!("Failed to set wallpaper: {}", e)))?;
    wallpaper::set_mode(wallpaper::Mode::Crop).map_err(|e| AppError::Engine(format!("Failed to set wallpaper mode: {}", e)))?;

    Ok(())
}

/// Copy a media item image to the system clipboard.
/// 复制媒体项图像到系统剪贴板。
#[tauri::command]
pub async fn copy_image_to_clipboard(item_id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    let (root, rel, name) = get_item_path_info(&pool, item_id)?;
    let abs_path = resolve_media_path(&root, &rel, &name);
    info!("copy_image_to_clipboard: {abs_path} | 复制图像到剪贴板: {abs_path}");

    let img = image::open(&abs_path).map_err(|e| AppError::Engine(format!("Failed to open image for clipboard: {}", e)))?;
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    let img_data = arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes: std::borrow::Cow::Borrowed(rgba.as_raw()),
    };

    let mut clipboard = arboard::Clipboard::new().map_err(|e| AppError::Engine(format!("Failed to initialize clipboard: {}", e)))?;
    clipboard.set_image(img_data).map_err(|e| AppError::Engine(format!("Failed to set clipboard image: {}", e)))?;

    Ok(())
}

