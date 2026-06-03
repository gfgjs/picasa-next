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

