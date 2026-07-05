// src-tauri/src/layout/justified.rs
//! Justified Layout algorithm (Rust implementation, § 10.1).
//! 两端对齐布局算法（Rust 实现，§ 10.1）。
//!
//! Input: flat list of `LayoutItem` sorted by `sort_datetime DESC`.
//! 输入：按 `sort_datetime DESC` 排序的 `LayoutItem` 扁平列表。
//! Output: `Vec<LayoutRow>` — each row is either a normal image row or a date separator.
//! 输出：`Vec<LayoutRow>` — 每一行是普通图像行或日期分隔符。
//!
//! Algorithm (Google Photos / Flickr justified layout):
//! 算法（Google Photos / Flickr 两端对齐布局）：
//!   - For each item, compute the aspect ratio.
//!   - 对于每个项目，计算宽高比。
//!   - Pack items into a row by scaling them to a common height.
//!   - 将项目缩放到公共高度，将它们打包成一行。
//!   - When the row width reaches `container_width ± tolerance`, commit the row.
//!   - 当行宽达到 `container_width ± tolerance` 时，提交该行。
//!   - When the `sort_datetime` crosses a day boundary, insert a separator row first.
//!   - 当 `sort_datetime` 跨越日期边界时，首先插入分隔符行。

use std::borrow::Borrow;
use std::collections::HashMap;

use chrono::{Datelike, TimeZone, Utc};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::db::models::{DirLabel, LayoutItem};

// ── Output types ─────────────────────────────────────────────────────────────
// ── 输出类型 ─────────────────────────────────────────────────────────────

/// 常驻布局行（S3 几何/载荷分离）：Normal 行仅存瘦行项（id + 几何），厚载荷在出口
/// （get_layout_rows 系命令）经 items 取数缓存按可视区拼装为 [`HydratedRow`]。
/// 百万项下常驻 ~32B/项（原 ~200B + 5 段堆载荷），重排零载荷克隆、旧代 drop 近零。
#[derive(Debug, Clone)]
pub enum LayoutRow {
    Separator {
        y: f64,
        height: f64,
        separator_label: String,
        group_id: Option<String>,
    },
    Normal {
        y: f64,
        height: f64,
        items: Vec<SlimRowItem>,
    },
}

impl LayoutRow {
    pub fn y(&self) -> f64 {
        match self {
            LayoutRow::Separator { y, .. } => *y,
            LayoutRow::Normal { y, .. } => *y,
        }
    }

    pub fn height(&self) -> f64 {
        match self {
            LayoutRow::Separator { height, .. } => *height,
            LayoutRow::Normal { height, .. } => *height,
        }
    }
}

/// 常驻瘦行项（S3）：id + 行内几何，Copy、零堆载荷。
#[derive(Debug, Clone, Copy)]
pub struct SlimRowItem {
    pub id: i64,
    pub x: f64,
    pub w: f64,
    pub h: f64,
}

/// 线上（IPC）布局行：serde 形状与 S3 前的 LayoutRow 完全一致（rowType 标签、camelCase），
/// **前端零改动**。由出口拼装产生（[`crate::layout::items_cache::hydrate_rows`]），不常驻。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "rowType")]
pub enum HydratedRow {
    #[serde(rename = "separator")]
    Separator {
        y: f64,
        height: f64,
        #[serde(rename = "separatorLabel")]
        separator_label: String,
        #[serde(rename = "groupId")]
        group_id: Option<String>,
    },
    #[serde(rename = "normal")]
    Normal {
        y: f64,
        height: f64,
        items: Vec<LayoutRowItem>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// 线上（IPC）逐项行数据（S3 后不再常驻）：由出口拼装（[`hydrate_item`]）按可视区从
/// items 取数缓存现场组装，serde 形状与 S3 前完全一致（前端零改动）。重型元数据
/// （文件名、目录路径、EXIF、GPS）仍在 `MediaMeta`，仅为可视区按需拉取。
pub struct LayoutRowItem {
    pub id: i64,
    pub x: f64,
    pub w: f64,
    pub h: f64,
    pub file_size: i64,
    pub file_format: String,
    pub media_type: String,
    pub is_live_photo: bool,
    pub duration_ms: Option<i64>,
    pub thumb_status: i64,
    pub thumb_path: Option<String>,
    pub thumbhash: Option<Vec<u8>>,
    pub is_favorited: bool,
    /// 用户评分 0-5（0 = 未评分）。与 is_favorited 同类的逐项小标量，供网格星级显示 + hover 快捷评分。
    pub rating: i64,
    /// 用户颜色标签 0-7（0 = 未标）。与 rating 同类的逐项小标量，供网格 swatch 显示 + 按色筛选（T16）。
    pub color_label: i64,
    /// 系统可用态 'online'|'offline'|'missing'（前端置灰+角标；缺失检测 Part2 §3.2）。
    pub availability: String,
    pub similarity: Option<f64>,
    pub original_width: i64,
    pub original_height: i64,
    pub sort_datetime: i64,
}

// ── Layout parameters ─────────────────────────────────────────────────────────
// ── 布局参数 ─────────────────────────────────────────────────────────

pub struct LayoutParams {
    pub container_width: f64,
    pub target_row_height: f64,
    pub gap: f64,
    pub group_by: String,
    pub sort_within_group: String,
}

const SEPARATOR_HEIGHT: f64 = 36.0;
/// How much we allow the last row to be shorter before we "justify" it vs. leave as-is.
/// 在我们将最后一行“两端对齐”与保持原样之前，允许其短多少。
const LAST_ROW_JUSTIFY_THRESHOLD: f64 = 0.6;
/// Maximum row height multiplier — prevents a single portrait image from
/// stretching to fill the entire container width (e.g. 1200 / 0.2 = 6000px).
/// 最大行高乘数 — 防止单张肖像图像拉伸以填充整个容器宽度（例如 1200 / 0.2 = 6000px）。
/// If computed row_h > target_h * MAX_ROW_HEIGHT_FACTOR, clamp to this factor.
/// 如果计算的 row_h > target_h * MAX_ROW_HEIGHT_FACTOR，则将其限制在该系数。
const MAX_ROW_HEIGHT_FACTOR: f64 = 2.0;

// ── Shared helpers (justified + grid) ──────────────────────────────────────────
// ── 共享辅助（justified + grid 两种布局共用）────────────────────────────────────

/// 出口拼装（S3 几何/载荷分离）：瘦行项几何 + `LayoutItem` 载荷 → 线上行项。字段集与
/// S3 前的常驻行完全一致（serde 形状不变，前端零改动）；仅对可视区行调用（10^2 级），
/// 载荷克隆成本无关紧要。新增 LayoutRowItem 字段只需改这一处（与 placeholder_item 成对）。
pub fn hydrate_item(item: &LayoutItem, slot: &SlimRowItem) -> LayoutRowItem {
    LayoutRowItem {
        id: slot.id,
        x: slot.x,
        w: slot.w,
        h: slot.h,
        file_size: item.file_size,
        file_format: item.file_format.clone(),
        media_type: item.media_type.clone(),
        is_live_photo: item.is_live_photo,
        duration_ms: item.duration_ms,
        thumb_status: item.thumb_status,
        thumb_path: item.thumb_path.clone(),
        thumbhash: item.thumbhash.clone(),
        is_favorited: item.is_favorited,
        rating: item.rating,
        color_label: item.color_label,
        availability: item.availability.clone(),
        similarity: item.similarity,
        original_width: item.width,
        original_height: item.height,
        sort_datetime: item.sort_datetime,
    }
}

/// items 取数缓存查无此 id 时的占位行项（布局与快照换代的竞态窗口/清库后未重算的瞬态）：
/// 几何保留使行形不塌（虚拟滚动行高稳定），载荷置空，thumb_status=0 → 前端按待生成
/// 骨架渲染，下次布局换代自愈。availability 置空串（≠ "offline"，不触发置灰样式）。
pub fn placeholder_item(slot: &SlimRowItem) -> LayoutRowItem {
    LayoutRowItem {
        id: slot.id,
        x: slot.x,
        w: slot.w,
        h: slot.h,
        file_size: 0,
        file_format: String::new(),
        media_type: String::new(),
        is_live_photo: false,
        duration_ms: None,
        thumb_status: 0,
        thumb_path: None,
        thumbhash: None,
        is_favorited: false,
        rating: 0,
        color_label: 0,
        availability: String::new(),
        similarity: None,
        original_width: 0,
        original_height: 0,
        sort_datetime: 0,
    }
}

/// 分组边界的逐项廉价键（S2 布局消脂）：date = UTC 日序数（`div_euclid(86400)` 与
/// `timestamp_to_date_label` 的 Utc 日界严格一致），folder = 目录 id。百万项下逐项
/// chrono 格式化 + 2 个 String 分配曾占布局段大头，现降为纯整数比较，标签仅在边界
/// 变化处构造（调用量级 10^6 → 10^3，见 group_label）。
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupMark {
    Day(i64),
    Dir(i64),
    /// folder 分组但 dir_id 缺失（列 NOT NULL，理论死分支，防御保留）：标签走 "Unknown"。
    DirUnknown,
}

fn group_mark(item: &LayoutItem, group_by: &str) -> GroupMark {
    match group_by {
        "folder" => item
            .dir_id
            .map(GroupMark::Dir)
            .unwrap_or(GroupMark::DirUnknown),
        _ => GroupMark::Day(item.sort_datetime.div_euclid(86400)),
    }
}

/// 分组标签：`(分隔符标签, group_id)`。justified 与 grid 共用，确保两种布局的分隔符/月桶
/// 边界严格一致（grid 复用同样的 `YYYY-MM` group_id → get_summary 月桶推导零改动）。
/// **仅在 GroupMark 边界变化时调用**（原 group_key 逐项调用，S2 优化）。
/// - folder：展示路径经 DirLabel 映射还原（语义 = 原 SQL 逐行拼接的 dir_path；空路径回退
///   目录名/Root，查无映射回退 "Unknown"），group_id = 目录 id 字符串。
/// - none：空标签、无 group_id（调用方据此不产分隔符）。
/// - date（默认）：中文日标签 + `YYYY-MM` group_id（T14 §3.8.2）。
fn group_label(
    item: &LayoutItem,
    group_by: &str,
    dir_labels: &HashMap<i64, DirLabel>,
) -> (String, Option<String>) {
    match group_by {
        "folder" => {
            let Some(dir_id) = item.dir_id else {
                return ("Unknown".to_string(), None);
            };
            let name = match dir_labels.get(&dir_id) {
                Some(dl) if dl.display.is_empty() => {
                    if dl.name.is_empty() {
                        "Root".to_string()
                    } else {
                        dl.name.clone()
                    }
                }
                Some(dl) => dl.display.clone(),
                None => "Unknown".to_string(),
            };
            (name, Some(dir_id.to_string()))
        }
        "none" => ("".to_string(), None),
        _ => (
            timestamp_to_date_label(item.sort_datetime),
            Some(timestamp_to_year_month(item.sort_datetime)),
        ),
    }
}

// ── 并行骨架（S3.4）───────────────────────────────────────────────────────────

/// S3.4 组间并行布局骨架（justified / grid 共用）：按 GroupMark 切连续段 → rayon 并行
/// 逐组打包（组内 y 从局部 0 起）→ 组高前缀和缝合绝对 y。
///
/// **顺序等价**：分组语义/行几何与原顺序实现逐行相同；y 均为整数和（分隔符 36 + gap、
/// 行高 ceil + gap），「基线 + 局部 y」与顺序累加**位级一致**（整数在 f64 中精确，
/// 特征化测试锁定）。none 分组 = 单段——行打包链式依赖（断行取决于前一行终点），
/// 组内不可切分，该轴无并行收益（维持原顺序耗时）。
fn layout_groups_parallel<I, F>(items: &[I], group_by: &str, pack_group: F) -> Vec<LayoutRow>
where
    I: Borrow<LayoutItem> + Sync,
    F: Fn(&[I]) -> (Vec<LayoutRow>, f64) + Sync,
{
    if items.is_empty() {
        return Vec::new();
    }
    // 段表：连续同 GroupMark 区间（整数比较 O(N) 轻扫，百万项数 ms 级）。
    let mut segments: Vec<(usize, usize)> = Vec::new();
    if group_by == "none" {
        segments.push((0, items.len()));
    } else {
        let mut start = 0usize;
        let mut last = group_mark(items[0].borrow(), group_by);
        for (i, it) in items.iter().enumerate().skip(1) {
            let mark = group_mark(it.borrow(), group_by);
            if mark != last {
                segments.push((start, i));
                start = i;
                last = mark;
            }
        }
        segments.push((start, items.len()));
    }

    // 并行逐组打包（rayon 全局池；调用方已运行在 spawn_blocking 阻塞线程上）。
    let mut outs: Vec<(Vec<LayoutRow>, f64)> = segments
        .par_iter()
        .map(|&(start, end)| pack_group(&items[start..end]))
        .collect();

    // 组高前缀和 → 并行偏移各组局部 y → 顺序拼接（容量一次预留）。
    let mut bases: Vec<f64> = Vec::with_capacity(outs.len());
    let mut acc = 0.0f64;
    for (_, h) in &outs {
        bases.push(acc);
        acc += *h;
    }
    outs.par_iter_mut()
        .zip(bases.par_iter())
        .for_each(|((rows, _), &base)| {
            if base != 0.0 {
                for row in rows.iter_mut() {
                    match row {
                        LayoutRow::Separator { y, .. } | LayoutRow::Normal { y, .. } => *y += base,
                    }
                }
            }
        });
    let total_rows: usize = outs.iter().map(|(rows, _)| rows.len()).sum();
    let mut all_rows: Vec<LayoutRow> = Vec::with_capacity(total_rows);
    for (rows, _) in outs {
        all_rows.extend(rows);
    }
    all_rows
}

// ── Main algorithm ────────────────────────────────────────────────────────────
// ── 主算法 ────────────────────────────────────────────────────────────

pub fn compute_justified_layout<I: Borrow<LayoutItem> + Sync>(
    items: &[I],
    params: &LayoutParams,
    dir_labels: &HashMap<i64, DirLabel>,
    placeholder_aspect: Option<f64>,
) -> Vec<LayoutRow> {
    // Placeholder aspect for not-yet-measured (0×0) items: the median of the
    // measured items, so deferred-dimension photos render at a plausible shape
    // (not a square) and reflow only slightly when their real dims arrive.
    // 未测量(0×0)项的占位宽高比：取已测量项的中位数，使延后取尺寸的照片以合理形状
    // （而非正方形）渲染，真实尺寸到达时只发生轻微重排。
    // S3.5：中位数只依赖项集、不依赖布局参数——调用方传快照缓存值（OnceLock）免每次
    // 重排 O(N) 重算；None = 现算（测试/独立调用）。
    let placeholder_aspect = placeholder_aspect.unwrap_or_else(|| median_measured_aspect(items));

    let emit_separator =
        |label: &str, group_id: Option<String>, y: &mut f64, rows: &mut Vec<LayoutRow>| {
            rows.push(LayoutRow::Separator {
                y: *y,
                height: SEPARATOR_HEIGHT,
                separator_label: label.to_string(),
                group_id,
            });
            *y += SEPARATOR_HEIGHT + params.gap;
        };

    // S3.5：宽度暂存由调用方复用（每组一个，跨行 clear）——原每行 2 个临时 Vec，真机
    // 行密度（1-3 项/行、数十万行）下即数百万次堆分配，占 layout 段大头。
    let commit_row = |pending: &mut Vec<&LayoutItem>,
                      ar_sum: &mut f64,
                      y: &mut f64,
                      target_h: f64,
                      rows: &mut Vec<LayoutRow>,
                      params: &LayoutParams,
                      is_last: bool,
                      widths: &mut Vec<f64>| {
        if pending.is_empty() {
            return;
        }

        // Compute actual row height
        // 计算实际行高
        let total_gaps = params.gap * (pending.len().saturating_sub(1)) as f64;
        let available_w = params.container_width - total_gaps;

        // Determine if the row is actually filling the width, or if it's an incomplete last row.
        // 确定该行是实际填满了宽度，还是不完整的最后一行。
        let is_incomplete =
            is_last && *ar_sum * target_h < available_w * LAST_ROW_JUSTIFY_THRESHOLD;
        let ideal_h = available_w / *ar_sum;

        let row_h = if is_incomplete {
            // Last row — don't stretch; use target height
            // 最后一行 — 不要拉伸；使用目标高度
            target_h
        } else {
            // Normal row: scale to fill width, but cap at MAX_ROW_HEIGHT_FACTOR
            // 普通行：缩放以填充宽度，但上限为 MAX_ROW_HEIGHT_FACTOR
            ideal_h.min(target_h * MAX_ROW_HEIGHT_FACTOR)
        };

        let hit_cap = ideal_h > target_h * MAX_ROW_HEIGHT_FACTOR;
        let should_snap_last = !is_incomplete && !hit_cap;

        widths.clear();
        widths.extend(
            pending
                .iter()
                .map(|item| aspect_ratio(item, placeholder_aspect) * row_h),
        );

        // Only adjust to exactly fill the container if it's a fully justified row
        // 仅在完全两端对齐的行时调整以精确填充容器
        if should_snap_last && pending.len() > 1 {
            let total_unrounded: f64 = widths.iter().sum();
            let target_total_w = available_w;
            // Distribute the difference proportionally to avoid dumping rounding errors on the last item
            // 按比例分配差异，以避免将舍入误差倾倒在最后一个项目上
            if total_unrounded > 0.0 {
                let scale = target_total_w / total_unrounded;
                for w in widths.iter_mut() {
                    *w *= scale;
                }
            }
        }

        // Round in place and distribute any remaining integer pixel difference
        // 就地舍入并分配剩余整数像素差异（S3.5：未舍入值此后不再使用，单缓冲复用）
        for w in widths.iter_mut() {
            *w = w.round();
        }

        if should_snap_last && pending.len() > 1 {
            let current_total: f64 = widths.iter().sum();
            let mut diff = (available_w.round() - current_total) as i32;

            // Distribute the 1px differences across items until diff is 0
            // 在项目之间分配 1px 差异，直到 diff 为 0
            // We can distribute from largest to smallest to minimize visual impact,
            // 我们可以从最大到最小进行分配以最小化视觉影响，
            // or just left-to-right. Left-to-right is fine.
            // 或者只是从左到右。从左到右也可以。
            let mut i = 0;
            let len = widths.len();
            while diff != 0 {
                if diff > 0 {
                    widths[i % len] += 1.0;
                    diff -= 1;
                } else {
                    widths[i % len] -= 1.0;
                    diff += 1;
                }
                i += 1;
            }
        }

        let mut x = 0.0f64;
        let mut row_items: Vec<SlimRowItem> = Vec::with_capacity(pending.len());

        for (i, item) in pending.iter().enumerate() {
            let item_w = widths[i];

            // x/w/h 语义不变（x 取整、w 保底 1、h 取整）；S3：行内仅存 id + 几何，零载荷克隆。
            row_items.push(SlimRowItem {
                id: item.id,
                x: x.round(),
                w: item_w.max(1.0),
                h: row_h.round(),
            });

            x += item_w + params.gap;
        }

        rows.push(LayoutRow::Normal {
            y: *y,
            height: row_h.ceil(),
            items: row_items,
        });

        *y += row_h.ceil() + params.gap;
        pending.clear();
        *ar_sum = 0.0;
    };

    // S3.4 单组打包（组内 y 从局部 0 起算）：分组语义与顺序版逐行相同——组首发分隔符
    //（none 分组无），组内满行 is_last=false、组末不满行 is_last=true（原「分隔符前冲掉
    // 挂起行」即组末提交）。边界标签仅每组构造一次（承接 S2：标签构造量级 10^3 不变）。
    let pack_group = |group: &[I]| -> (Vec<LayoutRow>, f64) {
        let mut rows: Vec<LayoutRow> = Vec::new();
        let mut y = 0.0f64;
        let mut pending: Vec<&LayoutItem> = Vec::new();
        let mut ar_sum = 0.0f64;
        let mut widths_scratch: Vec<f64> = Vec::new();

        if params.group_by != "none" {
            let (label, group_id) = group_label(group[0].borrow(), &params.group_by, dir_labels);
            emit_separator(&label, group_id, &mut y, &mut rows);
        }
        for item in group {
            let item = item.borrow();
            pending.push(item);
            ar_sum += aspect_ratio(item, placeholder_aspect);

            // Check if row is full
            // 检查行是否已满
            let total_gaps = params.gap * (pending.len().saturating_sub(1)) as f64;
            let available_w = params.container_width - total_gaps;
            if ar_sum * params.target_row_height >= available_w {
                commit_row(
                    &mut pending,
                    &mut ar_sum,
                    &mut y,
                    params.target_row_height,
                    &mut rows,
                    params,
                    false,
                    &mut widths_scratch,
                );
            }
        }
        // Commit last partial row of the group
        // 提交组末的非满行
        if !pending.is_empty() {
            commit_row(
                &mut pending,
                &mut ar_sum,
                &mut y,
                params.target_row_height,
                &mut rows,
                params,
                true,
                &mut widths_scratch,
            );
        }
        (rows, y)
    };

    layout_groups_parallel(items, &params.group_by, pack_group)
}

/// 均匀宫格布局（T20，方案 a「后端 uniform-packing」）。与 justified 共享分组/分隔符/月桶逻辑
/// （group_key + SEPARATOR_HEIGHT），仅"行内打包"不同：固定列数、方格单元、等高行；单元撑满
/// 容器宽（消除右侧空隙），方图由前端 `object-fit: cover` 裁切。产出同一 `LayoutRow` 枚举，故
/// 时间轴 / 虚拟滚动 / 分隔符联动全部原样工作。
///
/// 列数 = `⌊(W+gap) / (cell_target+gap)⌋`（至少 1）；实际单元边长 = `(W - (cols-1)·gap)/cols`
/// 撑满宽。`target_row_height` 复用作单元目标边长（即工具栏 gridRowHeight 密度滑块）。
pub fn compute_grid_layout<I: Borrow<LayoutItem> + Sync>(
    items: &[I],
    params: &LayoutParams,
    dir_labels: &HashMap<i64, DirLabel>,
) -> Vec<LayoutRow> {
    let gap = params.gap.max(0.0);
    let target = params.target_row_height.max(1.0);
    let width = params.container_width.max(1.0);

    // 列数：容器宽内能放下几个 (target+gap)，至少 1 列；随后把单元放大到精确撑满宽。
    let cols = (((width + gap) / (target + gap)).floor() as usize).max(1);
    let cell = ((width - gap * (cols as f64 - 1.0)) / cols as f64).max(1.0);

    let emit_separator =
        |label: &str, group_id: Option<String>, y: &mut f64, rows: &mut Vec<LayoutRow>| {
            rows.push(LayoutRow::Separator {
                y: *y,
                height: SEPARATOR_HEIGHT,
                separator_label: label.to_string(),
                group_id,
            });
            *y += SEPARATOR_HEIGHT + gap;
        };

    // 提交一整（或末尾不满）行：方格、x 按列均布。
    let commit_grid_row =
        |pending: &mut Vec<&LayoutItem>, y: &mut f64, rows: &mut Vec<LayoutRow>| {
            if pending.is_empty() {
                return;
            }
            let mut row_items: Vec<SlimRowItem> = Vec::with_capacity(pending.len());
            for (col, item) in pending.iter().enumerate() {
                let x = (col as f64) * (cell + gap);
                // 方格：w = h = cell；x/w/h 取整与 justified 同款，避免亚像素缝。S3：仅 id + 几何。
                row_items.push(SlimRowItem {
                    id: item.id,
                    x: x.round(),
                    w: cell.round(),
                    h: cell.round(),
                });
            }
            rows.push(LayoutRow::Normal {
                y: *y,
                height: cell.ceil(),
                items: row_items,
            });
            *y += cell.ceil() + gap;
            pending.clear();
        };

    // S3.4 单组打包（同 justified 的组间并行骨架）：组间不共享行、组末冲掉不满行,
    // 与顺序版逐行相同。
    let pack_group = |group: &[I]| -> (Vec<LayoutRow>, f64) {
        let mut rows: Vec<LayoutRow> = Vec::new();
        let mut y = 0.0f64;
        let mut pending: Vec<&LayoutItem> = Vec::new();

        if params.group_by != "none" {
            let (label, group_id) = group_label(group[0].borrow(), &params.group_by, dir_labels);
            emit_separator(&label, group_id, &mut y, &mut rows);
        }
        for item in group {
            pending.push(item.borrow());
            if pending.len() >= cols {
                commit_grid_row(&mut pending, &mut y, &mut rows);
            }
        }
        // 末尾不满行
        if !pending.is_empty() {
            commit_grid_row(&mut pending, &mut y, &mut rows);
        }
        (rows, y)
    };

    layout_groups_parallel(items, &params.group_by, pack_group)
}

// ── Helpers ───────────────────────────────────────────────────────────────────
// ── 辅助函数 ───────────────────────────────────────────────────────────────────

// pub(crate) 仅为 horizontal.rs(H-Lab 实验布局)复用纯几何辅助,零行为变更。
pub(crate) fn aspect_ratio(item: &LayoutItem, placeholder_aspect: f64) -> f64 {
    // Not-yet-measured items (deferred dimensions) carry 0×0 → use the supplied
    // placeholder aspect rather than 1.0 (square).
    // 未测量项（延后取尺寸）为 0×0 → 使用传入的占位宽高比，而非 1.0（正方形）。
    if item.width <= 0 || item.height <= 0 {
        return placeholder_aspect.clamp(0.2, 5.0);
    }
    let w = item.width.max(1) as f64;
    let h = item.height.max(1) as f64;
    (w / h).clamp(0.2, 5.0) // clamp to prevent extreme ratios
                            // 限制以防止极端的比例
}

/// Median aspect ratio of the measured items in the set (those with real w/h),
/// used as the placeholder shape for 0×0 items. Falls back to 3:2 when nothing
/// is measured yet. O(n) via `select_nth_unstable` (no full sort).
/// 集合中已测量项（有真实宽高）的中位宽高比，作为 0×0 项的占位形状。尚无测量时
/// 回退到 3:2。借 `select_nth_unstable` 实现 O(n)（无需完整排序）。
pub(crate) fn median_measured_aspect<I: Borrow<LayoutItem>>(items: &[I]) -> f64 {
    let mut ars: Vec<f64> = items
        .iter()
        .map(Borrow::borrow)
        .filter(|it| it.width > 0 && it.height > 0)
        .map(|it| (it.width as f64 / it.height as f64).clamp(0.2, 5.0))
        .collect();
    if ars.is_empty() {
        return 1.5; // 3:2 landscape default | 默认 3:2 横向
    }
    let mid = ars.len() / 2;
    ars.select_nth_unstable_by(mid, |a, b| {
        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
    });
    ars[mid]
}

fn timestamp_to_date_label(ts: i64) -> String {
    let dt = Utc.timestamp_opt(ts, 0).single().unwrap_or_else(Utc::now);

    // Format: "2024年3月15日"  (Chinese date, as specified in the plan)
    // 格式: "2024年3月15日" (中文日期，如计划所指定)
    // Adjust: use local timezone for display — for simplicity use UTC here.
    // 调整：使用本地时区进行显示 — 为简单起见，此处使用 UTC。
    format!("{}年{}月{}日", dt.year(), dt.month(), dt.day())
}

/// 时间戳 → `"YYYY-MM"`（月零填充）。date 分组日分隔符的 `group_id`（T14 §3.8.2）。
///
/// **与 `timestamp_to_date_label` 同用 `Utc` 基准**——确保「月」边界与「日」分隔符严格对齐
/// （否则同一张照片可能落在 UTC 的 3 月 1 日却被算进 2 月桶）。这也契合 T9 核验结论：
/// `sort_datetime` 的 EXIF 分量按「墙钟时间当 UTC 存」，故 UTC 格式化对绝大多数照片即正确。
fn timestamp_to_year_month(ts: i64) -> String {
    let dt = Utc.timestamp_opt(ts, 0).single().unwrap_or_else(Utc::now);
    format!("{}-{:02}", dt.year(), dt.month())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `timestamp_to_year_month`：UTC 基准 + 月零填充（date 分组 group_id 的事实源，T14 §3.8.2）。
    #[test]
    fn year_month_is_utc_and_zero_padded() {
        // epoch → 1970-01（验证个位月零填充）。
        assert_eq!(timestamp_to_year_month(0), "1970-01");
        // 已知 UTC 时刻：用 chrono 构造，避免硬编码脆弱的魔数。
        let mar = Utc
            .with_ymd_and_hms(2024, 3, 5, 10, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(timestamp_to_year_month(mar), "2024-03");
        let dec = Utc
            .with_ymd_and_hms(2023, 12, 31, 23, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(timestamp_to_year_month(dec), "2023-12", "两位月不补零");
        // 与同基准的日标签同月——锁住「月桶边界 = 日分隔符边界」的对齐前提。
        assert!(timestamp_to_date_label(mar).starts_with("2024年3月"));
    }

    /// 最小 LayoutItem fixture：仅关心 id / 宽高 / 时间戳，其余取无害默认。
    fn mk_item(id: i64, w: i64, h: i64, ts: i64) -> LayoutItem {
        LayoutItem {
            id,
            width: w,
            height: h,
            file_size: 0,
            sort_datetime: ts,
            file_format: "jpg".into(),
            media_type: "image".into(),
            is_live_photo: false,
            duration_ms: None,
            thumb_status: 1,
            thumb_path: None,
            thumbhash: None,
            is_favorited: false,
            rating: 0,
            color_label: 0,
            availability: "online".into(),
            dir_id: None,
            similarity: None,
        }
    }

    /// 取出 Normal 行的 (y, height, items)，分隔符行返回 None——测试断言用。
    fn as_normal(row: &LayoutRow) -> Option<(f64, f64, &Vec<SlimRowItem>)> {
        match row {
            LayoutRow::Normal { y, height, items } => Some((*y, *height, items)),
            LayoutRow::Separator { .. } => None,
        }
    }

    fn grid_params(container_width: f64, target: f64, gap: f64, group_by: &str) -> LayoutParams {
        LayoutParams {
            container_width,
            target_row_height: target,
            gap,
            group_by: group_by.to_string(),
            sort_within_group: "datetime".to_string(),
        }
    }

    /// grid（none 分组）：固定列数、方格单元、撑满宽、x 按列均布、y 逐行推进。
    #[test]
    fn grid_none_packs_uniform_square_rows() {
        // 容器 300 / 目标 100 / gap 0 → cols=3, cell=100。7 项 → 行 [3,3,1]，无分隔符。
        let items: Vec<LayoutItem> = (1..=7).map(|i| mk_item(i, 160, 90, i)).collect();
        let rows = compute_grid_layout(
            &items,
            &grid_params(300.0, 100.0, 0.0, "none"),
            &HashMap::new(),
        );

        // 全是 Normal 行（none 分组无分隔符）。
        assert!(
            rows.iter().all(|r| as_normal(r).is_some()),
            "none 分组不应有分隔符"
        );
        assert_eq!(rows.len(), 3, "7 项 / 3 列 = 3 行");

        let (y0, h0, r0) = as_normal(&rows[0]).unwrap();
        assert_eq!(r0.len(), 3);
        assert_eq!((y0, h0), (0.0, 100.0));
        // 方格：w == h == cell(100)；x 按列 0/100/200。
        for it in r0 {
            assert_eq!((it.w, it.h), (100.0, 100.0), "单元应为方格");
        }
        assert_eq!((r0[0].x, r0[1].x, r0[2].x), (0.0, 100.0, 200.0));

        // 第二行 y 推进到 100；末行仅 1 项、x=0。
        let (y1, _, r1) = as_normal(&rows[1]).unwrap();
        assert_eq!((y1, r1.len()), (100.0, 3));
        let (y2, _, r2) = as_normal(&rows[2]).unwrap();
        assert_eq!((y2, r2.len()), (200.0, 1));
        assert_eq!(r2[0].x, 0.0);
    }

    /// grid 单元撑满容器宽：cols·cell + (cols-1)·gap ≈ 容器宽（消除右侧空隙）。
    #[test]
    fn grid_cell_fills_width_with_gap() {
        // 容器 320 / 目标 100 / gap 10 → cols=⌊330/110⌋=3, cell=(320-20)/3=100。
        let items: Vec<LayoutItem> = (1..=3).map(|i| mk_item(i, 100, 100, i)).collect();
        let rows = compute_grid_layout(
            &items,
            &grid_params(320.0, 100.0, 10.0, "none"),
            &HashMap::new(),
        );
        let (_, _, r0) = as_normal(&rows[0]).unwrap();
        assert_eq!(r0.len(), 3);
        // x: 0 / (100+10)=110 / 220；末单元右缘 = 220+100 = 320 = 容器宽。
        assert_eq!((r0[0].x, r0[1].x, r0[2].x), (0.0, 110.0, 220.0));
        assert_eq!(r0[2].x + r0[2].w, 320.0, "末单元右缘应贴容器右沿");
    }

    /// grid（date 分组）：跨天插分隔符、组间不共享行，group_id 为 "YYYY-MM"（与 justified 同源）。
    #[test]
    fn grid_inserts_separators_between_days() {
        // 2 项 1970-01-01 + 2 项约 2 天后；cols≥3 故每组 1 个不满行。
        let day2 = 2 * 86_400;
        let items = vec![
            mk_item(1, 100, 100, 10),
            mk_item(2, 100, 100, 20),
            mk_item(3, 100, 100, day2 + 10),
            mk_item(4, 100, 100, day2 + 20),
        ];
        let rows = compute_grid_layout(
            &items,
            &grid_params(400.0, 100.0, 0.0, "date"),
            &HashMap::new(),
        );

        // 期望序列：Sep, Normal(2), Sep, Normal(2)。
        let sep_count = rows
            .iter()
            .filter(|r| matches!(r, LayoutRow::Separator { .. }))
            .count();
        assert_eq!(sep_count, 2, "两天 → 两个日分隔符");
        // 首行是分隔符且带 YYYY-MM group_id。
        match &rows[0] {
            LayoutRow::Separator { group_id, .. } => {
                assert_eq!(group_id.as_deref(), Some("1970-01"))
            }
            _ => panic!("首行应为分隔符"),
        }
        // 两个 Normal 行各 2 项（组间不共享行）。
        let normal_lens: Vec<usize> = rows
            .iter()
            .filter_map(as_normal)
            .map(|(_, _, its)| its.len())
            .collect();
        assert_eq!(normal_lens, vec![2, 2]);
    }

    /// S3.4 并行缝合特征化：多组绝对 y 与顺序累加位级一致（组高前缀和 + 组内局部 y）。
    #[test]
    fn parallel_stitch_yields_sequential_y_positions() {
        let day2 = 2 * 86_400;
        // grid/date/gap0:Sep(0,36)+Normal(36,100)+Sep(136,36)+Normal(172,100)。
        let items = vec![
            mk_item(1, 100, 100, 10),
            mk_item(2, 100, 100, 20),
            mk_item(3, 100, 100, day2 + 10),
            mk_item(4, 100, 100, day2 + 20),
        ];
        let rows = compute_grid_layout(
            &items,
            &grid_params(400.0, 100.0, 0.0, "date"),
            &HashMap::new(),
        );
        let ys: Vec<f64> = rows.iter().map(|r| r.y()).collect();
        assert_eq!(ys, vec![0.0, 36.0, 136.0, 172.0]);

        // justified/date/gap4(单张 ar2 不满行 → 行高=目标 100):
        // Sep(0)→y40 + Row(40,100)→y144 + Sep(144)→y184 + Row(184)。
        let items = vec![mk_item(1, 200, 100, 10), mk_item(2, 200, 100, day2 + 10)];
        let rows = compute_justified_layout(
            &items,
            &grid_params(400.0, 100.0, 4.0, "date"),
            &HashMap::new(),
            None,
        );
        let ys: Vec<f64> = rows.iter().map(|r| r.y()).collect();
        assert_eq!(ys, vec![0.0, 40.0, 144.0, 184.0]);
    }

    /// justified 几何特征化：锁住 make_row_item / group_key 提取后的输出不变（该路径此前无单测）。
    /// 单张 ar=2.0 图、容器 400 / 目标 100 / none：不满末行不拉伸 → 行高=目标，宽=ar·行高。
    #[test]
    fn justified_geometry_unchanged_characterization() {
        let items = vec![mk_item(1, 200, 100, 5)]; // ar = 2.0
        let rows = compute_justified_layout(
            &items,
            &grid_params(400.0, 100.0, 0.0, "none"),
            &HashMap::new(),
            None,
        );
        assert_eq!(rows.len(), 1);
        let (y, h, its) = as_normal(&rows[0]).unwrap();
        assert_eq!((y, h), (0.0, 100.0));
        assert_eq!(its.len(), 1);
        // is_incomplete（200 < 400·0.6=240）→ row_h=target=100；w=ar·row_h=200；x=0。
        assert_eq!((its[0].x, its[0].w, its[0].h), (0.0, 200.0, 100.0));
        // 字段透传职责已随 S3 移到出口拼装：hydrate_item 保 slot 几何 + item 载荷逐字段还原
        //（original 尺寸 = 原始宽高、rating/color_label 默认 0）——继续锁同一契约。
        let wire = hydrate_item(&items[0], &its[0]);
        assert_eq!((wire.id, wire.x, wire.w, wire.h), (1, 0.0, 200.0, 100.0));
        assert_eq!((wire.original_width, wire.original_height), (200, 100));
        assert_eq!((wire.rating, wire.color_label), (0, 0));
        assert_eq!(wire.file_format, "jpg");
    }

    /// S3.5 拆帐基准(非门控,--release + --ignored 手动跑):1M 合成项,分相计时定位
    /// layout 段的 150ms 地板。用法:
    /// cargo test --release --lib layout::justified::tests::bench_layout_1m -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_layout_1m() {
        use std::time::Instant;
        const N: i64 = 1_000_000;
        // 1000 目录 × 连续块(与真实序一致:folder 轴按目录聚簇);时间戳跨 ~200 天;
        // 4 种宽高比轮转 + 5% 未测量(0×0)项走占位比。
        let items: Vec<LayoutItem> = (0..N)
            .map(|i| {
                let (w, h) = match i % 20 {
                    0 => (0, 0),
                    x if x % 4 == 1 => (1600, 1200),
                    x if x % 4 == 2 => (1200, 1600),
                    x if x % 4 == 3 => (1920, 1080),
                    _ => (1500, 1000),
                };
                let mut it = mk_item(i + 1, w, h, (N - i) * 17);
                it.dir_id = Some(i / 1000);
                it
            })
            .collect();
        let dir_labels: HashMap<i64, DirLabel> = (0..1000)
            .map(|d| {
                (
                    d,
                    DirLabel {
                        rel_path: format!("dir/{d:04}"),
                        display: format!("D:/photos/dir/{d:04}"),
                        name: format!("{d:04}"),
                    },
                )
            })
            .collect();

        let t = Instant::now();
        let med = median_measured_aspect(&items);
        println!(
            "median_measured_aspect: {:.1}ms (med={med:.3})",
            t.elapsed().as_secs_f64() * 1e3
        );

        // 复现真机行密度(1.2-2.4 项/行 → 41-83 万行):target 拉大。
        for (axis, target) in [
            ("folder", 100.0),
            ("folder", 240.0),
            ("folder", 400.0),
            ("folder", 700.0),
            ("date", 100.0),
            ("none", 100.0),
        ] {
            let p = grid_params(1200.0, target, 4.0, axis);
            // 预热一次(rayon 池/页错误),再取三次最小值。
            let _ = compute_justified_layout(&items, &p, &dir_labels, Some(med));
            let mut best = f64::MAX;
            let mut rows_n = 0usize;
            for _ in 0..3 {
                let t = Instant::now();
                let rows = compute_justified_layout(&items, &p, &dir_labels, Some(med));
                best = best.min(t.elapsed().as_secs_f64() * 1e3);
                rows_n = rows.len();
                drop(rows);
            }
            println!("justified axis={axis} target={target}: {best:.1}ms ({rows_n} rows)");
        }
        for (axis, target) in [("folder", 100.0), ("date", 100.0)] {
            let p = grid_params(1200.0, target, 4.0, axis);
            let _ = compute_grid_layout(&items, &p, &dir_labels);
            let mut best = f64::MAX;
            let mut rows_n = 0usize;
            for _ in 0..3 {
                let t = Instant::now();
                let rows = compute_grid_layout(&items, &p, &dir_labels);
                best = best.min(t.elapsed().as_secs_f64() * 1e3);
                rows_n = rows.len();
                drop(rows);
            }
            println!("grid axis={axis} target={target}: {best:.1}ms ({rows_n} rows)");
        }
    }
}
