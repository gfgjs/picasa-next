// src-tauri/src/ipc/derive_commands.rs
//! IPC commands for the background derivation pipeline (§5): start / pause / stop / status.
//! Mirrors the AI 3-button control surface (开始/暂停/停止) and its resume semantics.
//! 后台派生流水线的 IPC 命令（§5）：开始 / 暂停 / 停止 / 状态。
//! 与 AI 三按钮控制面板（开始/暂停/停止）及其续传语义同构。

use std::sync::Arc;

use tauri::{AppHandle, State};
use tracing::info;

use crate::db::models::DerivationStatusSummary;
use crate::db::queries::{get_config, set_config};
use crate::derive::derivation_counts;
use crate::derive::{start_derivation_pipeline, DerivationKind};
use crate::state::AppState;

/// Persist the "derivation desired" flag and launch the pipeline.
/// 持久化「期望运行」标志并启动流水线。
fn launch_derivation_pipeline(
    app: AppHandle,
    state: &Arc<AppState>,
    kind_filter: Option<DerivationKind>,
) {
    {
        let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let _ = set_config(&conn, "derivation_active", "1");
    }
    let token = state.new_derivation_token();
    start_derivation_pipeline(app, Arc::clone(state), token, kind_filter);
}

/// Start (or resume) the derivation pipeline. `kind` optionally restricts to a single kind
/// (e.g. "video_cover"); omit to process all kinds. Already-done items are skipped; only
/// pending / interrupted tasks run (orphan recovery + backfill happen inside the pipeline).
/// 启动（或续传）派生流水线。`kind` 可选地限定单一 kind（如 "video_cover"）；省略则处理所有。
/// 已完成项跳过，仅运行待处理 / 被中断任务（孤儿恢复 + backfill 在流水线内部完成）。
#[tauri::command]
pub async fn start_derivation(
    app: AppHandle,
    kind: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    let state = Arc::clone(&state);
    let kind_filter = match kind.as_deref() {
        Some(s) => Some(
            DerivationKind::from_str(s)
                .ok_or_else(|| format!("Unknown derivation kind: {s} | 未知派生 kind"))?,
        ),
        None => None,
    };

    // Cancel any existing run, then start fresh (resume picks up pending/interrupted).
    // 取消任何现有运行，然后重新开始（续传会接续待处理/被中断项）。
    state.cancel_derivation();
    info!(
        "Starting/resuming derivation pipeline (filter={:?}) | 启动/续传派生流水线",
        kind_filter.map(|k| k.as_str())
    );
    // launch 内含 set_config 落库（R1-3：rusqlite 离开 tokio worker）。
    tokio::task::spawn_blocking(move || launch_derivation_pipeline(app, &state, kind_filter))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Pause the running pipeline: cancel but KEEP the active flag so it can be resumed later
/// (incl. auto-resume on next launch). In-flight tasks are recovered to pending next run.
/// 暂停运行中的流水线：取消但保留 active 标志，以便之后续传（含下次启动自动续传）。
/// 在途任务下次运行时恢复为待处理。
#[tauri::command]
pub async fn pause_derivation(state: State<'_, Arc<AppState>>) -> std::result::Result<(), String> {
    info!("Pausing derivation pipeline (keeps resume flag) | 暂停派生流水线（保留续传标志）");
    state.cancel_derivation();
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let _ = set_config(&conn, "derivation_active", "1");
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Stop the pipeline AND clear the resume flag (no auto-resume). Progress is preserved
/// (completed derivations are kept) — only the auto-continue intent is dropped.
/// 停止流水线并清除续传标志（不再自动续传）。进度保留（已完成派生不删），仅放弃「自动继续」意图。
#[tauri::command]
pub async fn stop_derivation(state: State<'_, Arc<AppState>>) -> std::result::Result<(), String> {
    info!("Stopping derivation pipeline (clears resume flag) | 停止派生流水线（清除续传标志）");
    state.cancel_derivation();
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let _ = set_config(&conn, "derivation_active", "0");
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Status summary for the derivation UI: counts by status + running/active flags.
/// 派生 UI 的状态摘要：按状态计数 + 运行/期望标志。
#[tauri::command]
pub async fn derivation_status(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<DerivationStatusSummary, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(
        move || -> std::result::Result<DerivationStatusSummary, String> {
            let (pending, processing, done, error) =
                derivation_counts(&state).map_err(|e| e.to_string())?;

            let is_running = state.is_derivation_running();
            let conn = state.db_read_pool.get().map_err(|e| e.to_string())?;
            let active = get_config(&conn, "derivation_active")
                .unwrap_or_default()
                .map(|v| v == "1")
                .unwrap_or(false);

            Ok(DerivationStatusSummary {
                pending,
                processing,
                done,
                error,
                is_running,
                active,
            })
        },
    )
    .await
    .map_err(|e| e.to_string())?
}
