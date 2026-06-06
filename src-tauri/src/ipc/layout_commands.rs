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

    // Query layout items from the read pool
    // 从读取池查询布局项
    let items = {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        query_layout_items(&pool, &filter, params.group_by.as_deref(), params.sort_within_group.as_deref(), params.sort_order.as_deref())?
    };

    if items.is_empty() {
        // Store empty layout
        // 存储空布局
        let version = store_layout(&state.layout_cache, vec![], 0.0);
        return Ok(LayoutSummary {
            total_rows: 0,
            total_height: 0.0,
            layout_version: version,
            total_items: 0,
            separators: vec![],
        });
    }

    // Run layout algorithm (CPU-bound) in a blocking task
    // 在阻塞任务中运行布局算法（受限于 CPU）
    let layout_params = LayoutParams {
        container_width:   params.container_width.max(100.0),
        target_row_height: params.row_height.max(50.0),
        gap:               params.gap.max(0.0),
        group_by:          params.group_by.unwrap_or_else(|| "date".to_string()),
        sort_within_group: params.sort_within_group.unwrap_or_else(|| "datetime".to_string()),
    };

    let rows: Vec<LayoutRow> = tokio::task::spawn_blocking(move || {
        compute_justified_layout(&items, &layout_params)
    })
    .await
    .map_err(|e| AppError::Engine(e.to_string()))?;

    let total_height: f64 = rows.last().map(|r| r.y() + r.height()).unwrap_or(0.0);
    let version = store_layout(&state.layout_cache, rows, total_height);

    Ok(LayoutSummary {
        total_rows:     get_summary(&state.layout_cache).map(|s| s.total_rows).unwrap_or(0),
        total_height,
        layout_version: version,
        total_items:    get_summary(&state.layout_cache).map(|s| s.total_items).unwrap_or(0),
        separators:     get_summary(&state.layout_cache).map(|s| s.separators.clone()).unwrap_or_default(),
    })
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
