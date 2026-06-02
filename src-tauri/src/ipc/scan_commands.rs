// src-tauri/src/ipc/scan_commands.rs
//! Tauri IPC commands for scan management (§ 6.1 — scan management).

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
#[tauri::command]
pub async fn add_scan_root(
    path: String,
    alias: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<ScanRoot> {
    let norm = normalize_db_path(&path);

    // Check if the root already exists
    {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        let roots = q::list_scan_roots(&pool)?;
        if let Some(existing) = roots.into_iter().find(|r| r.path == norm) {
            info!("Scan root already exists: id={} path={}", existing.id, norm);
            return Ok(existing);
        }
    }

    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    let id = q::insert_scan_root(&conn, &norm, alias.as_deref())?;
    let root = q::get_scan_root(&conn, id)?;
    info!("Scan root added: id={id} path={norm}");
    Ok(root)
}

/// Remove a scan root and all its data (CASCADE).
#[tauri::command]
pub async fn remove_scan_root(id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    state.cancel_scan(id);
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::delete_scan_root(&conn, id)?;
    info!("Scan root removed: id={id}");
    Ok(())
}

/// List all scan roots.
#[tauri::command]
pub async fn list_scan_roots(state: State<'_, Arc<AppState>>) -> Result<Vec<ScanRoot>> {
    let pool = state.db_read_pool.get().map_err(AppError::from)?;
    q::list_scan_roots(&pool)
}

/// Start a scan for a root (both fast scan + background enrichment).
///
/// This command returns when the fast scan completes (UI ready).
/// Background enrichment continues and emits Tauri events.
#[tauri::command]
pub async fn start_scan(
    root_id: i64,
    on_progress: Channel<ScanChannelPayload>,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    // Cancel any existing scan for this root
    state.cancel_scan(root_id);
    let cancel = state.new_scan_token(root_id);

    // Get root path
    let root_path = {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        q::get_scan_root(&pool, root_id)?.path
    };

    info!("start_scan: root_id={root_id} path={root_path}");

    // Clone the Arc so the closure owns an independent reference (no unsafe needed)
    let state_arc = Arc::clone(&*state);
    let cancel_fast = cancel.clone();
    let root_path_clone = root_path.clone();

    // Run fast scan (spawn_blocking so we don't block the async runtime)
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
    {
        let state_arc2 = Arc::clone(&*state);
        let app_clone   = app.clone();
        let cancel_enrich = cancel.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_enrichment(&app_clone, &state_arc2.db_writer, root_id, &cancel_enrich) {
                tracing::error!("Enrichment error for root_id={root_id}: {e}");
            }
        });
    }

    Ok(())
}

/// Stop (cancel) an in-progress scan.
#[tauri::command]
pub async fn stop_scan(root_id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    state.cancel_scan(root_id);
    info!("stop_scan: root_id={root_id}");
    Ok(())
}

/// [Dev] Clear all data — wipe every table and the thumbnail cache directory.
///
/// This is intended for development / QA resets only. It does not delete any
/// original media files on disk.
#[tauri::command]
pub async fn clear_all_data(
    state: State<'_, Arc<AppState>>,
    app:   AppHandle,
) -> Result<()> {
    // Cancel all running scans first
    state.cancel_all_scans();

    // Wipe all DB tables
    {
        let mut conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        let tx = conn.transaction()?;
        tx.execute_batch(
            "DELETE FROM image_meta;
             DELETE FROM media_items;
             DELETE FROM directories;
             DELETE FROM scan_roots;
             DELETE FROM app_config;"
        )?;
        tx.commit()?;
        
        // VACUUM must be run outside of a transaction
        conn.execute("VACUUM", [])?;
    }

    // Drop the thumbnail cache directory
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
    *state.layout_cache.write().unwrap() = None;

    info!("clear_all_data: all data wiped");
    Ok(())
}
