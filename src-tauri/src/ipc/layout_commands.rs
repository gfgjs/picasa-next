// src-tauri/src/ipc/layout_commands.rs
//! Tauri IPC commands for Justified Layout (§ 6.1 — layout).
//! 针对 Justified Layout（两端对齐布局）的 Tauri IPC 命令（§ 6.1 — 布局）。

use std::sync::Arc;

use tauri::State;

use crate::db::models::{LayoutItem, MediaFilter};
use crate::db::queries::{query_dir_labels, query_layout_items, query_layout_items_canonical};
use crate::error::{AppError, Result};
use crate::layout::cache::{
    dedup_summary, get_rows, get_summary, get_view_ids as cache_view_ids, store_layout,
    LayoutSummary,
};
use crate::layout::items_cache::{self, CachedOrder, ItemsCacheData};
use crate::layout::justified::{
    compute_grid_layout, compute_justified_layout, median_measured_aspect, HydratedRow,
    LayoutParams, LayoutRow,
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

    // Run BOTH the (potentially million-row) query and the CPU-bound layout algorithm
    // inside one blocking task, so neither blocks a tokio worker.
    // 把（可能百万行的）查询与受限于 CPU 的布局算法放进同一个阻塞任务，
    // 二者均不阻塞 tokio 工作线程。
    let state_arc = state.inner().clone();
    let group_by = params
        .group_by
        .clone()
        .unwrap_or_else(|| "date".to_string());
    let sort_within = params
        .sort_within_group
        .clone()
        .unwrap_or_else(|| "datetime".to_string());
    let sort_order = params
        .sort_order
        .clone()
        .unwrap_or_else(|| "desc".to_string());
    let container_width = params.container_width.max(100.0);
    let target_row_height = params.row_height.max(50.0);
    let gap = params.gap.max(0.0);
    let layout_mode = params.layout_mode.clone();

    // ── S3.1 幂等去重（前端重复触发治理）────────────────────────────────────────
    // 挂载/统计返回/尺寸观察等前端触发源可能以完全相同的输入连发 compute（mediaStore
    // 的在飞合并只排队、不比对参数）。布局是「有序快照 × 参数」的纯函数：快照可命中
    // （同 HIT 守卫判据）且现行布局代的构建指纹一致 ⇒ 输出必然逐项相同——直接复用现行
    // 摘要，免全量重排；**版本不换代**，前端 bucket 段表亦免于虚假重建。
    // 锁纪律：items 读锁与 layout 读锁先后独立取放，绝不重叠（S1）。
    let filter_key_probe =
        serde_json::to_string(&filter).map_err(|e| AppError::System(e.to_string()))?;
    let dv_probe = state.data_version();
    let gen_key = format!(
        "{}|{}|{}|{}|w{:.2}|h{:.2}|g{:.2}|m{}|meta{}|dv{}",
        filter_key_probe,
        group_by,
        sort_within,
        sort_order,
        container_width,
        target_row_height,
        gap,
        layout_mode.as_deref().unwrap_or("justified"),
        params.include_meta.unwrap_or(false),
        dv_probe
    );
    if filter.ai_search != Some(true)
        && items_cache::is_hit_valid(
            &state.layout_items_cache,
            &filter_key_probe,
            dv_probe,
            &group_by,
            &sort_within,
            &sort_order,
        )
    {
        if let Some(summary) = dedup_summary(&state.layout_cache, &gen_key) {
            tracing::info!(
                "compute_layout DEDUP: v{} unchanged, {} rows | 同参数同数据代——复用现行布局(免重排免换代)",
                summary.layout_version,
                summary.total_rows
            );
            return Ok(summary);
        }
    }

    let (rows, total_height): (Vec<LayoutRow>, f64) =
        tokio::task::spawn_blocking(move || -> Result<(Vec<LayoutRow>, f64)> {
            let t0 = std::time::Instant::now();
            let layout_params = LayoutParams {
                container_width,
                target_row_height,
                gap,
                group_by: group_by.clone(),
                sort_within_group: sort_within.clone(),
            };
            // ai 搜索视图不可复用（S1/S3）：ai_search_results 随每次搜索整表重写，命中
            // 徒增失效面——每次全量重查；但快照仍驻留（reusable=false）作出口拼装的载荷源。
            let cacheable = filter.ai_search != Some(true);
            let is_datetime = sort_within == "datetime";
            // 去重预检已序列化过一次，闭包直接接管该键（同一 filter，键必同一）。
            let filter_key = filter_key_probe;
            let dv = state_arc.data_version();

            // ── ① 命中：免 SQL —— items 读锁内派生序 + 布局（S1 锁纪律：items 读锁与
            //    layout_cache 写锁绝不重叠，store_layout 在本闭包返回后才执行）────────────
            if cacheable {
                let guard = state_arc.layout_items_cache.read().unwrap();
                if let Some(data) = guard.as_ref() {
                    let order_ok = match &data.order {
                        CachedOrder::Canonical => is_datetime,
                        CachedOrder::Sql {
                            group_by: g,
                            sort_within: s,
                            sort_order: o,
                        } => *g == group_by && *s == sort_within && *o == sort_order,
                    };
                    if data.reusable
                        && order_ok
                        && data.data_version == dv
                        && data.filter_key == filter_key
                    {
                        let ordered: Vec<&LayoutItem> = match data.order {
                            CachedOrder::Canonical => {
                                items_cache::derive_order(data, &group_by, &sort_order)
                            }
                            CachedOrder::Sql { .. } => data.items.iter().collect(),
                        };
                        let t_derive = t0.elapsed();
                        // S3.5：中位数缓存于快照（OnceLock,首次现算后驻留）。
                        let aspect = *data
                            .median_aspect
                            .get_or_init(|| median_measured_aspect(&data.items));
                        let (rows, total_height) = run_layout(
                            &ordered,
                            &layout_params,
                            layout_mode.as_deref(),
                            &data.dir_labels,
                            Some(aspect),
                        );
                        tracing::info!(
                            "compute_layout HIT: {} items, {} rows; derive {:.0}ms + layout {:.0}ms (axis={}/{}) | 取数缓存命中(免 SQL)",
                            data.items.len(),
                            rows.len(),
                            t_derive.as_secs_f64() * 1000.0,
                            (t0.elapsed() - t_derive).as_secs_f64() * 1000.0,
                            group_by,
                            sort_order
                        );
                        return Ok((rows, total_height));
                    }
                }
            }

            // ── ② miss：查询（datetime 家族走基准序免 JOIN 查询）→ 布局 → 回填缓存 ──────
            // Hold a read-pool connection ONLY for the query, then release it before the
            // CPU-bound layout — otherwise one of the (4) pooled connections is pinned for
            // the whole compute, throttling concurrent viewport reads during scroll.
            // 读连接仅在查询期间持有，CPU 密集的布局计算前即释放 —— 否则会把 4 个池连接之一
            // 钉住整个计算，拖慢滚动时并发的可视区读取。
            let (items, dir_labels) = {
                let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
                let items = if cacheable && is_datetime {
                    query_layout_items_canonical(&pool, &filter)?
                } else {
                    query_layout_items(
                        &pool,
                        &filter,
                        Some(&group_by),
                        Some(&sort_within),
                        Some(&sort_order),
                        false,
                    )?
                };
                // 目录标签映射恒取（量级 10^3 的小查询）：缓存驻留后可直接服务后续
                // folder 轴的内存派生，无需再碰 DB。
                let dir_labels = query_dir_labels(&pool)?;
                (items, dir_labels)
            };
            let t_query = t0.elapsed();

            let order = if cacheable && is_datetime {
                CachedOrder::Canonical
            } else {
                CachedOrder::Sql {
                    group_by: group_by.clone(),
                    sort_within: sort_within.clone(),
                    sort_order: sort_order.clone(),
                }
            };
            let data = ItemsCacheData {
                filter_key,
                order,
                data_version: dv,
                id_to_idx: items_cache::build_id_index(&items),
                dir_rank: items_cache::build_dir_rank(&dir_labels),
                items,
                dir_labels,
                filter: filter.clone(),
                reusable: cacheable,
                median_aspect: std::sync::OnceLock::new(),
                perm_memo: std::sync::Mutex::new(None),
            };
            let ordered: Vec<&LayoutItem> = match &data.order {
                CachedOrder::Canonical => items_cache::derive_order(&data, &group_by, &sort_order),
                CachedOrder::Sql { .. } => data.items.iter().collect(),
            };
            let aspect = *data
                .median_aspect
                .get_or_init(|| median_measured_aspect(&data.items));
            let (rows, total_height) = run_layout(
                &ordered,
                &layout_params,
                layout_mode.as_deref(),
                &data.dir_labels,
                Some(aspect),
            );
            drop(ordered);
            let item_count = data.items.len();
            // S3：无条件驻留（含 ai_search）——快照同时是布局行的载荷源（出口拼装），
            // 不可复用视图（reusable=false）只是不参与命中，仍服务 get_*_rows 取载荷。
            items_cache::store_items(&state_arc.layout_items_cache, data);
            tracing::info!(
                "compute_layout MISS: {} items, {} rows; query {:.0}ms, total {:.0}ms | 取数缓存未命中(重查)",
                item_count,
                rows.len(),
                t_query.as_secs_f64() * 1000.0,
                t0.elapsed().as_secs_f64() * 1000.0
            );
            Ok((rows, total_height))
        })
        .await
        .map_err(|e| AppError::System(e.to_string()))??;

    // S3 布局换代计时（S3.2 后=索引物化+指针交换；旧代 drop 已卸后台线程）。1M 级索引
    // 物化是 CPU 工作,同样不占 tokio worker——挪进 spawn_blocking(锁纪律不变:此处
    // 不持任何 items 锁,layout 写锁在 store_layout 内短窗取放)。
    let t_store = std::time::Instant::now();
    let state_store = state.inner().clone();
    let version = tokio::task::spawn_blocking(move || {
        store_layout(&state_store.layout_cache, rows, total_height, gen_key)
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?;
    tracing::info!(
        "store_layout: v{} in {:.0}ms | 布局换代(索引物化;旧代已后台释放)",
        version,
        t_store.elapsed().as_secs_f64() * 1000.0
    );

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

/// 布局段（纯 CPU）：模式分支 + 总高。入参为引用序 —— S1 命中路径直接引用缓存内 items，
/// 零拷贝；布局模式分支见 T20（grid 均匀宫格 / 其余等高行，产出同一 LayoutRow 枚举，
/// 缓存/取行/月桶/虚拟滚动通路完全复用）。
fn run_layout(
    ordered: &[&LayoutItem],
    params: &LayoutParams,
    layout_mode: Option<&str>,
    dir_labels: &std::collections::HashMap<i64, crate::db::models::DirLabel>,
    placeholder_aspect: Option<f64>,
) -> (Vec<LayoutRow>, f64) {
    let rows = if layout_mode == Some("grid") {
        compute_grid_layout(ordered, params, dir_labels)
    } else {
        compute_justified_layout(ordered, params, dir_labels, placeholder_aspect)
    };
    let total_height = rows.last().map(|r| r.y() + r.height()).unwrap_or(0.0);
    (rows, total_height)
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
) -> Result<Vec<HydratedRow>> {
    // Scrolling = active interaction → throttle background decode (布局被视频派生阻塞).
    // 滚动 = 主动交互 → 节流后台解码。
    state.note_interaction();
    // S3 出口拼装：瘦行几何 + items 取数缓存载荷 → 线上行（两把锁先后独立取放，不重叠）。
    let rows = get_rows(&state.layout_cache, start_row, end_row, layout_version)
        .ok_or(AppError::LayoutNotReady)?;
    Ok(items_cache::hydrate_rows(&state.layout_items_cache, rows))
}

/// Fetch a slice of layout rows intersecting [top_y, bottom_y] from the in-memory cache.
/// 从内存缓存中获取与 [top_y, bottom_y] 相交的布局行的切片。
#[tauri::command]
pub async fn get_layout_rows_by_y(
    top_y: f64,
    bottom_y: f64,
    layout_version: Option<u64>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<HydratedRow>> {
    // Scrolling = active interaction → throttle background decode (布局被视频派生阻塞).
    // 滚动 = 主动交互 → 节流后台解码。
    state.note_interaction();
    let rows =
        crate::layout::cache::get_rows_by_y(&state.layout_cache, top_y, bottom_y, layout_version)
            .ok_or(AppError::LayoutNotReady)?;
    Ok(items_cache::hydrate_rows(&state.layout_items_cache, rows))
}

/// Fetch one bucket segment's rows: y in [start_y, end_y) — exact membership,
/// unlike the intersect semantics of `get_layout_rows_by_y`.
/// 取单个 bucket 段的行:y 落在 [start_y, end_y) 的行——精确归属(半开区间),区别于
/// `get_layout_rows_by_y` 的视口相交语义。边界来自 summary 的 month_buckets 相邻 y
/// (末桶用 total_height)。T16 方案 B(bucket 分段虚拟滚动)B0。
#[tauri::command]
pub async fn get_bucket_rows(
    start_y: f64,
    end_y: f64,
    layout_version: Option<u64>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<HydratedRow>> {
    // Scrolling = active interaction → throttle background decode.
    // 滚动 = 主动交互 → 节流后台解码。
    state.note_interaction();
    let rows =
        crate::layout::cache::get_bucket_rows(&state.layout_cache, start_y, end_y, layout_version)
            .ok_or(AppError::LayoutNotReady)?;
    Ok(items_cache::hydrate_rows(&state.layout_items_cache, rows))
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
