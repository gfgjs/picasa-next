// src-tauri/src/ipc/scan_commands.rs
//! Tauri IPC commands for scan management (§ 6.1 — scan management).
//! 用于扫描管理的 Tauri IPC 命令（§ 6.1 — 扫描管理）。

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tauri::ipc::Channel;
use tracing::info;

use crate::db::models::ScanRoot;
use crate::db::queries as q;
use crate::error::{AppError, Result};
use crate::scanner::fast_scan::ScanChannelPayload;
use crate::scanner::{run_enrichment, run_fast_scan};
use crate::state::AppState;
use crate::utils::path::normalize_db_path;

/// Add a new scan root directory.
/// 添加新的扫描根目录。
#[tauri::command]
pub async fn add_scan_root(
    path: String,
    alias: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<ScanRoot> {
    let norm = normalize_db_path(&path);

    // Check if the root already exists
    // 检查根目录是否已存在
    {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        let roots = q::list_scan_roots(&pool)?;
        if let Some(existing) = roots.into_iter().find(|r| r.path == norm) {
            info!("Scan root already exists: id={} path={} | 扫描根目录已存在: id={} path={}", existing.id, norm, existing.id, norm);
            return Ok(existing);
        }
    }

    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    let id = q::insert_scan_root(&conn, &norm, alias.as_deref())?;

    // 立即创建顶级目录记录，以便前端可以立刻加载和选中
    // Immediately create the top-level directory record so the frontend can load and select it immediately
    let dir_name = alias.clone().unwrap_or_else(|| {
        std::path::Path::new(&norm)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    });
    q::upsert_directory(&conn, id, None, "", &dir_name, 0, None)?;

    let root = q::get_scan_root(&conn, id)?;
    info!("Scan root added: id={id} path={norm} | 已添加扫描根目录: id={id} path={norm}");
    Ok(root)
}

/// Remove a scan root and all its data (CASCADE).
/// 移除扫描根目录及其所有数据 (CASCADE)。
#[tauri::command]
pub async fn remove_scan_root(
    id: i64, 
    clear_thumbnails: Option<bool>,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<()> {
    let clear = clear_thumbnails.unwrap_or(false);
    info!("User action: Removing scan root ID: {} (clear_thumbnails={}) | 用户操作：正在移除扫描根目录 ID: {}", id, clear, id);
    
    state.cancel_scan(id);

    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Io(e.to_string()))?
        .join("cache")
        .join("thumbnails");

    let mut thumb_paths_to_delete = Vec::new();

    if clear {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        let sql = "
            SELECT m.thumb_path 
            FROM media_items m
            JOIN directories d ON m.directory_id = d.id
            WHERE d.root_id = ? AND m.thumb_path IS NOT NULL
        ";
        let mut stmt = pool.prepare(sql).map_err(|e| AppError::Db(e.to_string()))?;
        let rows = stmt.query_map([id], |row| row.get::<_, String>(0)).map_err(|e| AppError::Db(e.to_string()))?;
        for path in rows.flatten() {
            thumb_paths_to_delete.push(path);
        }
    }

    {
        let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        q::delete_scan_root(&conn, id)?;
    }
    
    if clear && !thumb_paths_to_delete.is_empty() {
        tokio::spawn(async move {
            info!("Removing {} thumbnails for root id={} | 正在为 root id={} 删除 {} 个缩略图", thumb_paths_to_delete.len(), id, id, thumb_paths_to_delete.len());
            let mut deleted = 0;
            for tp in thumb_paths_to_delete {
                let full_path = cache_dir.join(tp);
                if let Err(e) = std::fs::remove_file(&full_path) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!("Failed to delete thumbnail {:?}: {}", full_path, e);
                    }
                } else {
                    deleted += 1;
                }
            }
            info!("Successfully deleted {} thumbnails | 成功删除 {} 个缩略图", deleted, deleted);
        });
    }

    info!("Scan root removed: id={id} | 已移除扫描根目录: id={id}");
    Ok(())
}

/// List all scan roots.
/// 列出所有扫描根目录。
#[tauri::command]
pub async fn list_scan_roots(state: State<'_, Arc<AppState>>) -> Result<Vec<ScanRoot>> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::list_scan_roots(&pool)
}

/// Start a scan for a root (both fast scan + background enrichment).
/// 启动根目录扫描（包括快速扫描和后台内容丰富）。
///
/// This command returns when the fast scan completes (UI ready).
/// 此命令在快速扫描完成时返回（UI 准备就绪）。
/// Background enrichment continues and emits Tauri events.
/// 后台内容丰富继续进行并发出 Tauri 事件。
#[tauri::command]
pub async fn start_scan(
    root_id: i64,
    on_progress: Channel<ScanChannelPayload>,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    info!("User action: Starting scan for root ID: {} | 用户操作：开始扫描根目录 ID: {}", root_id, root_id);
    // Cancel any existing scan for this root
    // 取消该根目录任何现有的扫描
    state.cancel_scan(root_id);
    let cancel = state.new_scan_token(root_id);

    // Get root path
    // 获取根目录路径
    let root_path = {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        q::get_scan_root(&pool, root_id)?.path
    };

    info!("start_scan: root_id={root_id} path={root_path} | 开始扫描: root_id={root_id} path={root_path}");

    // Clone the Arc so the closure owns an independent reference (no unsafe needed)
    // 克隆 Arc 以便闭包拥有独立的引用（不需要 unsafe）
    let state_arc = Arc::clone(&*state);
    let cancel_fast = cancel.clone();
    let root_path_clone = root_path.clone();

    // Run fast scan (spawn_blocking so we don't block the async runtime)
    // 运行快速扫描（使用 spawn_blocking，因此我们不会阻塞异步运行时）
    tokio::task::spawn_blocking(move || {
        run_fast_scan(
            &state_arc.db_writer,
            root_id,
            &root_path_clone,
            &on_progress,
            &cancel_fast,
        )
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))??;

    // Spawn background enrichment (fire-and-forget, emits events)
    // After enrichment completes, remove the scan token so the AI pipeline
    // can start without waiting forever (was the root cause of infinite yielding).
    // 生成后台内容丰富任务（触发后不管，发出事件）
    // Enrichment 完成后移除 scan token，AI pipeline 才能正常启动（否则会无限让步）。
    {
        let state_arc2 = Arc::clone(&*state);
        let app_clone   = app.clone();
        let cancel_enrich = cancel.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_enrichment(&app_clone, &state_arc2.db_writer, root_id, &cancel_enrich) {
                tracing::error!("Enrichment error for root_id={root_id}: {e}");
            }
            // Remove the token for this root — work is done (or was cancelled).
            // This prevents should_yield_to_higher_priority() from returning true forever.
            // 移除此根目录的 token — 工作已完成（或已取消）。
            // 这可以防止 should_yield_to_higher_priority() 永远返回 true。
            state_arc2.scan_tokens.lock().unwrap().remove(&root_id);
            tracing::info!("Scan token cleared for root_id={root_id} | 已清除扫描 token: root_id={root_id}");
        });
    }

    Ok(())
}

/// Stop (cancel) an in-progress scan.
/// 停止（取消）正在进行的扫描。
#[tauri::command]
pub async fn stop_scan(root_id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    info!("User action: Stopping scan for root ID: {} | 用户操作：停止扫描根目录 ID: {}", root_id, root_id);
    state.cancel_scan(root_id);
    info!("stop_scan: root_id={root_id} | 停止扫描: root_id={root_id}");
    Ok(())
}

#[tauri::command]
pub async fn clear_database(
    state: State<'_, Arc<AppState>>,
    app:   AppHandle,
) -> Result<()> {
    info!("User action: Clearing database | 用户操作：正在清除数据库");
    // Cancel all running scans first
    // 首先取消所有正在运行的扫描
    state.cancel_all_scans();

    // Wipe all DB tables
    // 擦除所有数据库表
    {
        let mut conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        let tx = conn.transaction()?;
        tx.execute_batch(
            "DELETE FROM image_meta;
             DELETE FROM media_items;
             DELETE FROM directories;
             DELETE FROM scan_roots;"
        )?;
        tx.commit()?;
        
        // VACUUM must be run outside of a transaction
        // VACUUM 必须在事务外运行
        conn.execute("VACUUM", [])?;
    }

    // Drop the thumbnail cache directory
    // 删除缩略图缓存目录
    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Io(e.to_string()))?
        .join("cache")
        .join("thumbnails");

    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir)
            .map_err(|e| AppError::Io(format!("Failed to remove thumbnail cache: {e}")))?;
    }

    // Reset the layout cache in memory
    // 重置内存中的布局缓存
    *state.layout_cache.write().unwrap() = None;

    info!("clear_database: all media data wiped | 清除数据库：所有媒体数据已擦除");
    Ok(())
}

#[tauri::command]
pub async fn clear_settings(
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    info!("User action: Clearing settings | 用户操作：正在清除设置");
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    conn.execute("DELETE FROM app_config", [])?;
    info!("clear_settings: settings wiped | 清除设置：设置项已擦除");
    Ok(())
}
