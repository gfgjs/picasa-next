// src-tauri/src/ipc/layout_commands.rs
//! Tauri IPC commands for Justified Layout (§ 6.1 — layout).
//! 针对 Justified Layout（两端对齐布局）的 Tauri IPC 命令（§ 6.1 — 布局）。

use std::sync::Arc;

use tauri::State;

use crate::db::models::MediaFilter;
use crate::db::queries::query_layout_items;
use crate::error::{AppError, Result};
use crate::layout::cache::{get_rows, get_summary, store_layout, LayoutSummary};
use crate::layout::justified::{compute_justified_layout, LayoutParams, LayoutRow};
use crate::state::AppState;

/// Parameters for layout computation.
/// 布局计算参数。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputeLayoutParams {
    pub directory_id:    Option<i64>,
    pub filters:         Option<MediaFilter>,
    pub container_width: f64,
    pub row_height:      f64,
    pub gap:             f64,
    pub group_by:        Option<String>,
    pub sort_within_group: Option<String>,
    pub sort_order:      Option<String>,
    pub include_meta:    Option<bool>,
}

/// Compute the Justified Layout for the given filters.
/// 计算给定过滤器的 Justified Layout（两端对齐布局）。
/// Returns the layout summary (row count, total height, version).
/// 返回布局摘要（行数、总高度、版本）。
/// The full row data is stored in the in-memory cache.
/// 完整的行数据存储在内存缓存中。
#[tauri::command]
pub async fn compute_layout(
    params: ComputeLayoutParams,
    state: State<'_, Arc<AppState>>,
) -> Result<LayoutSummary> {
    let filter = {
        let mut f = params.filters.unwrap_or_default();
        if let Some(dir_id) = params.directory_id {
            f.directory_id = Some(dir_id);
        }
        f
    };

    // Run BOTH the (potentially multi-hundred-thousand row) query and the CPU-bound
    // layout algorithm inside one blocking task, so neither blocks a tokio worker.
    // 把（可能数十万行的）查询与受限于 CPU 的布局算法放进同一个阻塞任务，
    // 二者均不阻塞 tokio 工作线程。
    let state_arc = state.inner().clone();
    let group_by = params.group_by.clone();
    let sort_within = params.sort_within_group.clone();
    let sort_order = params.sort_order.clone();
    let container_width = params.container_width.max(100.0);
    let target_row_height = params.row_height.max(50.0);
    let gap = params.gap.max(0.0);

    let (rows, total_height): (Vec<LayoutRow>, f64) = tokio::task::spawn_blocking(move || -> Result<(Vec<LayoutRow>, f64)> {
        let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
        let items = query_layout_items(
            &pool,
            &filter,
            group_by.as_deref(),
            sort_within.as_deref(),
            sort_order.as_deref(),
            false,
        )?;
        if items.is_empty() {
            return Ok((vec![], 0.0));
        }

        let layout_params = LayoutParams {
            container_width,
            target_row_height,
            gap,
            group_by:          group_by.unwrap_or_else(|| "date".to_string()),
            sort_within_group: sort_within.unwrap_or_else(|| "datetime".to_string()),
        };
        let rows = compute_justified_layout(&items, &layout_params);
        let total_height = rows.last().map(|r| r.y() + r.height()).unwrap_or(0.0);
        Ok((rows, total_height))
    })
    .await
    .map_err(|e| AppError::Engine(e.to_string()))??;

    let version = store_layout(&state.layout_cache, rows, total_height);

    // Single read-lock pass for the summary (was three separate get_summary calls).
    // 单次读锁取摘要（此前是三次独立的 get_summary 调用）。
    Ok(get_summary(&state.layout_cache).unwrap_or(LayoutSummary {
        total_rows: 0,
        total_height,
        layout_version: version,
        total_items: 0,
        separators: vec![],
    }))
}

/// Fetch a slice of layout rows from the in-memory cache.
/// 从内存缓存中获取布局行的切片。
#[tauri::command]
pub async fn get_layout_rows(
    start_row: usize,
    end_row: usize,
    layout_version: Option<u64>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<LayoutRow>> {
    get_rows(&state.layout_cache, start_row, end_row, layout_version)
        .ok_or(AppError::LayoutNotReady)
}

/// Fetch a slice of layout rows intersecting [top_y, bottom_y] from the in-memory cache.
/// 从内存缓存中获取与 [top_y, bottom_y] 相交的布局行的切片。
#[tauri::command]
pub async fn get_layout_rows_by_y(
    top_y: f64,
    bottom_y: f64,
    layout_version: Option<u64>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<LayoutRow>> {
    crate::layout::cache::get_rows_by_y(&state.layout_cache, top_y, bottom_y, layout_version)
        .ok_or(AppError::LayoutNotReady)
}

/// Find the Y coordinate of a separator row by matching its label.
/// 通过匹配标签查找分隔符行的 Y 坐标。
#[tauri::command]
pub async fn get_separator_y_by_label(
    label: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<f64>> {
    Ok(crate::layout::cache::get_separator_y_by_label(&state.layout_cache, &label))
}
