// src-tauri/src/ipc/scan_commands.rs
//! Tauri IPC commands for scan management (§ 6.1 — scan management).

use tauri::{AppHandle, State};
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
    state: State<'_, AppState>,
) -> Result<ScanRoot> {
    let norm = normalize_db_path(&path);
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    let id = q::insert_scan_root(&conn, &norm, alias.as_deref())?;
    let root = q::get_scan_root(&conn, id)?;
    info!("Scan root added: id={id} path={norm}");
    Ok(root)
}

/// Remove a scan root and all its data (CASCADE).
#[tauri::command]
pub async fn remove_scan_root(id: i64, state: State<'_, AppState>) -> Result<()> {
    state.cancel_scan(id);
    let conn = state.db_writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    q::delete_scan_root(&conn, id)?;
    info!("Scan root removed: id={id}");
    Ok(())
}

/// List all scan roots.
#[tauri::command]
pub async fn list_scan_roots(state: State<'_, AppState>) -> Result<Vec<ScanRoot>> {
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
    state: State<'_, AppState>,
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

    // Run fast scan (spawn_blocking so we don't block the async runtime)
    let cancel_clone = cancel.clone();
    let root_path_clone = root_path.clone();
    let writer_ref = &state.db_writer;

    tokio::task::spawn_blocking({
        let db_writer = unsafe {
            // SAFETY: AppState lives for the duration of the app; we hold the State ref.
            &*(writer_ref as *const _)
        };
        let on_progress = on_progress.clone();
        let cancel = cancel_clone.clone();
        move || run_fast_scan(db_writer, root_id, &root_path_clone, &on_progress, &cancel)
    })
    .await
    .map_err(|e| AppError::Io(e.to_string()))??;

    // Spawn background enrichment (fire-and-forget, emits events)
    {
        let app_clone = app.clone();
        let cancel_enrich = cancel.clone();
        let root_path_clone2 = root_path.clone();
        let db_writer = unsafe {
            &*(writer_ref as *const _)
        };
        tokio::task::spawn_blocking(move || {
            if let Err(e) = run_enrichment(&app_clone, db_writer, root_id, &cancel_enrich) {
                tracing::error!("Enrichment error for root_id={root_id}: {e}");
            }
        });
    }

    Ok(())
}

/// Stop (cancel) an in-progress scan.
#[tauri::command]
pub async fn stop_scan(root_id: i64, state: State<'_, AppState>) -> Result<()> {
    state.cancel_scan(root_id);
    info!("stop_scan: root_id={root_id}");
    Ok(())
}
