// src-tauri/src/ipc/layout_commands.rs
//! Tauri IPC commands for Justified Layout (§ 6.1 — layout).
//! 针对 Justified Layout（两端对齐布局）的 Tauri IPC 命令（§ 6.1 — 布局）。

use std::sync::Arc;

use tauri::State;

use crate::db::models::MediaFilter;
use crate::db::queries::query_layout_items;
use crate::error::{AppError, Result};
use crate::layout::cache::{
    get_rows, get_summary, get_view_ids as cache_view_ids, store_layout, LayoutSummary,
};
use crate::layout::justified::{
    compute_grid_layout, compute_justified_layout, LayoutParams, LayoutRow,
};
use crate::state::AppState;

/// Parameters for layout computation.
/// 布局计算参数。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputeLayoutParams {
    pub directory_id: Option<i64>,
    pub filters: Option<MediaFilter>,
    pub container_width: f64,
    pub row_height: f64,
    pub gap: f64,
    pub group_by: Option<String>,
    pub sort_within_group: Option<String>,
    pub sort_order: Option<String>,
    pub include_meta: Option<bool>,
    /// 布局模式：None / "justified" = 等高行（默认），"grid" = 均匀宫格（T20）。
    pub layout_mode: Option<String>,
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
    // Mark this as active interaction so background video derivation/AI throttle and don't
    // starve this CPU-bound relayout (布局被视频派生阻塞). See AppState::note_interaction.
    // 标记为主动交互，使后台视频派生/AI 节流，不饿死这次 CPU 密集的重排（布局被视频派生阻塞）。
    state.note_interaction();

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
    let layout_mode = params.layout_mode.clone();

    let (rows, total_height): (Vec<LayoutRow>, f64) =
        tokio::task::spawn_blocking(move || -> Result<(Vec<LayoutRow>, f64)> {
            // Hold a read-pool connection ONLY for the query, then release it before the CPU-bound
            // layout — otherwise one of the (4) pooled connections is pinned for the whole compute,
            // throttling concurrent viewport reads during scroll.
            // 读连接仅在查询期间持有，CPU 密集的布局计算前即释放 —— 否则会把 4 个池连接之一钉住整个计算，
            // 拖慢滚动时并发的可视区读取。
            let items = {
                let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
                query_layout_items(
                    &pool,
                    &filter,
                    group_by.as_deref(),
                    sort_within.as_deref(),
                    sort_order.as_deref(),
                    false,
                )?
            };
            if items.is_empty() {
                return Ok((vec![], 0.0));
            }

            let layout_params = LayoutParams {
                container_width,
                target_row_height,
                gap,
                group_by: group_by.unwrap_or_else(|| "date".to_string()),
                sort_within_group: sort_within.unwrap_or_else(|| "datetime".to_string()),
            };
            // 布局模式分支（T20）：grid 走均匀宫格，其余（含默认/justified）走等高行。
            // 两者产出同一 LayoutRow 枚举，缓存/取行/月桶/虚拟滚动通路完全复用。
            let rows = if layout_mode.as_deref() == Some("grid") {
                compute_grid_layout(&items, &layout_params)
            } else {
                compute_justified_layout(&items, &layout_params)
            };
            let total_height = rows.last().map(|r| r.y() + r.height()).unwrap_or(0.0);
            Ok((rows, total_height))
        })
        .await
        .map_err(|e| AppError::System(e.to_string()))??;

    let version = store_layout(&state.layout_cache, rows, total_height);

    // Single read-lock pass for the summary (was three separate get_summary calls).
    // 单次读锁取摘要（此前是三次独立的 get_summary 调用）。
    Ok(get_summary(&state.layout_cache).unwrap_or(LayoutSummary {
        total_rows: 0,
        total_height,
        layout_version: version,
        total_items: 0,
        separators: vec![],
        month_buckets: vec![],
    }))
}

/// 返回当前视图**按布局序的全集 id**（T14.5 / T18 选择契约的前端前置）。
///
/// 解锁 Part5 T4「选区脱离 DOM」：Shift-range 跨视口、框选命中判定基于 flat_ids 序号而非可视 DOM；
/// Ctrl+A 全选亦据此（前端只持「全选标记 + 排除集」，批量写再走 `SelectionDescriptor::SelectAll`）。
///
/// 直接返回缓存内已物化的 `flat_ids`（O(1)，无 DB 往返）。`layout_version` 与当前布局不一致 →
/// `ViewStale`（前端重算 layout 重取）；压根无布局 → `LayoutNotReady`。
#[tauri::command]
pub async fn get_view_ids(
    layout_version: Option<u64>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<i64>> {
    match cache_view_ids(&state.layout_cache, layout_version) {
        Some(ids) => Ok(ids),
        // None 二义：无布局 vs 版本不符。无版本约束再取一次以区分，给前端可分流的错误码。
        None => {
            if cache_view_ids(&state.layout_cache, None).is_some() {
                Err(AppError::ViewStale)
            } else {
                Err(AppError::LayoutNotReady)
            }
        }
    }
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
    // Scrolling = active interaction → throttle background decode (布局被视频派生阻塞).
    // 滚动 = 主动交互 → 节流后台解码。
    state.note_interaction();
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
    // Scrolling = active interaction → throttle background decode (布局被视频派生阻塞).
    // 滚动 = 主动交互 → 节流后台解码。
    state.note_interaction();
    crate::layout::cache::get_rows_by_y(&state.layout_cache, top_y, bottom_y, layout_version)
        .ok_or(AppError::LayoutNotReady)
}

/// Find the Y coordinate of a separator row by its group id (the unique directory id).
/// 通过分组 id（唯一目录 id）查找分隔符行的 Y 坐标。
#[tauri::command]
pub async fn get_separator_y_by_group_id(
    group_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<f64>> {
    Ok(crate::layout::cache::get_separator_y_by_group_id(
        &state.layout_cache,
        &group_id,
    ))
}

/// Find the Y coordinate of the row containing the given item id (for re-anchoring
/// the viewport to the previously-viewed item after a row-height reflow — 问题1).
/// 查找包含给定项 id 的行的 Y 坐标（用于行高重排后把视口重新锚定到之前浏览的项 — 问题1）。
#[tauri::command]
pub async fn get_item_y_by_id(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<f64>> {
    Ok(crate::layout::cache::get_item_y_by_id(
        &state.layout_cache,
        item_id,
    ))
}

/// Where to scroll when a folder is clicked (folder grouping): the folder's own separator
/// if it has direct media, otherwise the first descendant subfolder (in layout order) that
/// does — so clicking an "empty" parent jumps to its first media-bearing child instead of
/// doing nothing. Returns the matched directory id + y, or null if the whole subtree has
/// no media in the current view.
/// 点击文件夹（按文件夹分组）时的滚动目标：若该文件夹有直接媒体则用它自己的分隔符，否则用
/// 布局顺序中其首个「有媒体」的后代子文件夹——这样点击「空」父文件夹会跳到首个含媒体的子项，
/// 而非毫无反应。返回命中的目录 id + y；若整棵子树在当前视图无媒体则返回 null。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtreeScrollTarget {
    pub dir_id: i64,
    pub y: f64,
}

#[tauri::command]
pub async fn get_subtree_scroll_target(
    dir_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<SubtreeScrollTarget>> {
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<Option<SubtreeScrollTarget>> {
        let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
        let ids = crate::db::queries::get_directory_descendant_ids(&pool, dir_id)?;
        let set: std::collections::HashSet<String> =
            ids.into_iter().map(|i| i.to_string()).collect();
        Ok(
            crate::layout::cache::get_first_separator_y_in_set(&state_arc.layout_cache, &set).map(
                |(gid, y)| SubtreeScrollTarget {
                    dir_id: gid.parse().unwrap_or(dir_id),
                    y,
                },
            ),
        )
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}
