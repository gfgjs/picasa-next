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

use chrono::{Datelike, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::db::models::LayoutItem;

// ── Output types ─────────────────────────────────────────────────────────────
// ── 输出类型 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "rowType")]
pub enum LayoutRow {
    #[serde(rename = "separator")]
    Separator {
        y:               f64,
        height:          f64,
        separator_label: String,
    },
    #[serde(rename = "normal")]
    Normal {
        y:      f64,
        height: f64,
        items:  Vec<LayoutRowItem>,
    },
}

impl LayoutRow {
    pub fn y(&self) -> f64 {
        match self {
            LayoutRow::Separator { y, .. } => *y,
            LayoutRow::Normal { y, .. }    => *y,
        }
    }

    pub fn height(&self) -> f64 {
        match self {
            LayoutRow::Separator { height, .. } => *height,
            LayoutRow::Normal { height, .. }    => *height,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRowItem {
    pub id:            i64,
    pub x:             f64,
    pub w:             f64,
    pub h:             f64,
    pub file_size:     i64,
    pub file_format:   String,
    pub media_type:    String,
    pub is_live_photo: bool,
    pub duration_ms:   Option<i64>,
    pub thumb_status:  i64,
    pub thumb_path:    Option<String>,
    pub thumbhash:     Option<Vec<u8>>,
    pub similarity:    Option<f64>,
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

// ── Main algorithm ────────────────────────────────────────────────────────────
// ── 主算法 ────────────────────────────────────────────────────────────

pub fn compute_justified_layout(items: &[LayoutItem], params: &LayoutParams) -> Vec<LayoutRow> {
    let mut rows: Vec<LayoutRow> = Vec::new();
    let mut current_y = 0.0f64;

    let mut pending_items: Vec<&LayoutItem> = Vec::new();
    let mut pending_ar_sum = 0.0f64; // sum of (w/h) aspect ratios
                                     // (w/h) 宽高比总和
    let mut last_label: Option<String> = None;

    let emit_separator = |label: &str, y: &mut f64, rows: &mut Vec<LayoutRow>| {
        rows.push(LayoutRow::Separator {
            y:               *y,
            height:          SEPARATOR_HEIGHT,
            separator_label: label.to_string(),
        });
        *y += SEPARATOR_HEIGHT + params.gap;
    };

    let commit_row = |pending: &mut Vec<&LayoutItem>,
                      ar_sum: &mut f64,
                      y: &mut f64,
                      target_h: f64,
                      rows: &mut Vec<LayoutRow>,
                      params: &LayoutParams,
                      is_last: bool| {
        if pending.is_empty() {
            return;
        }

        // Compute actual row height
        // 计算实际行高
        let total_gaps = params.gap * (pending.len().saturating_sub(1)) as f64;
        let available_w = params.container_width - total_gaps;

        // Determine if the row is actually filling the width, or if it's an incomplete last row.
        // 确定该行是实际填满了宽度，还是不完整的最后一行。
        let is_incomplete = is_last && *ar_sum * target_h < available_w * LAST_ROW_JUSTIFY_THRESHOLD;
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

        let mut unrounded_widths: Vec<f64> = pending.iter().map(|item| aspect_ratio(item) * row_h).collect();
        
        // Only adjust to exactly fill the container if it's a fully justified row
        // 仅在完全两端对齐的行时调整以精确填充容器
        if should_snap_last && pending.len() > 1 {
            let total_unrounded: f64 = unrounded_widths.iter().sum();
            let target_total_w = available_w;
            // Distribute the difference proportionally to avoid dumping rounding errors on the last item
            // 按比例分配差异，以避免将舍入误差倾倒在最后一个项目上
            if total_unrounded > 0.0 {
                let scale = target_total_w / total_unrounded;
                for w in unrounded_widths.iter_mut() {
                    *w *= scale;
                }
            }
        }

        // Now round the widths and distribute any remaining integer pixel difference
        // 现在对宽度进行舍入并分配任何剩余的整数像素差异
        let mut final_widths: Vec<f64> = unrounded_widths.iter().map(|w| w.round()).collect();
        
        if should_snap_last && pending.len() > 1 {
            let current_total: f64 = final_widths.iter().sum();
            let mut diff = (available_w.round() - current_total) as i32;
            
            // Distribute the 1px differences across items until diff is 0
            // 在项目之间分配 1px 差异，直到 diff 为 0
            // We can distribute from largest to smallest to minimize visual impact, 
            // 我们可以从最大到最小进行分配以最小化视觉影响，
            // or just left-to-right. Left-to-right is fine.
            // 或者只是从左到右。从左到右也可以。
            let mut i = 0;
            let len = final_widths.len();
            while diff != 0 {
                if diff > 0 {
                    final_widths[i % len] += 1.0;
                    diff -= 1;
                } else {
                    final_widths[i % len] -= 1.0;
                    diff += 1;
                }
                i += 1;
            }
        }

        let mut x = 0.0f64;
        let mut row_items: Vec<LayoutRowItem> = Vec::with_capacity(pending.len());

        for (i, item) in pending.iter().enumerate() {
            let item_w = final_widths[i];

            row_items.push(LayoutRowItem {
                id:            item.id,
                x:             x.round(),
                w:             item_w.max(1.0),
                h:             row_h.round(),
                file_size:     item.file_size,
                file_format:   item.file_format.clone(),
                media_type:    item.media_type.clone(),
                is_live_photo: item.is_live_photo,
                duration_ms:   item.duration_ms,
                thumb_status:  item.thumb_status,
                thumb_path:    item.thumb_path.clone(),
                thumbhash:     item.thumbhash.clone(),
                similarity:    item.similarity,
            });

            x += item_w + params.gap;
        }

        rows.push(LayoutRow::Normal {
            y:      *y,
            height: row_h.ceil(),
            items:  row_items,
        });

        *y += row_h.ceil() + params.gap;
        pending.clear();
        *ar_sum = 0.0;
    };

    for item in items {
        // Grouping separator check
        // 分组分隔符检查
        let current_label = match params.group_by.as_str() {
            "folder" => item.dir_name.clone().unwrap_or_else(|| item.dir_path.clone().unwrap_or_else(|| "Unknown".to_string())),
            "none" => "".to_string(),
            _ => timestamp_to_date_label(item.sort_datetime), // "date" default
        };

        let needs_separator = if params.group_by == "none" {
            false
        } else {
            match &last_label {
                None    => true,
                Some(prev) => *prev != current_label,
            }
        };

        if needs_separator {
            // Commit any pending row before the separator
            // 提交分隔符之前的任何挂起行
            let _pending_clone = pending_items.clone();
            commit_row(
                &mut pending_items,
                &mut pending_ar_sum,
                &mut current_y,
                params.target_row_height,
                &mut rows,
                params,
                true, // Treat rows before a separator as the "last row" of that group
                      // 将分隔符之前的行视为该组的“最后一行”
            );
            emit_separator(&current_label, &mut current_y, &mut rows);
            last_label = Some(current_label);
        }

        let ar = aspect_ratio(item);
        pending_items.push(item);
        pending_ar_sum += ar;

        // Check if row is full
        // 检查行是否已满
        let total_gaps = params.gap * (pending_items.len().saturating_sub(1)) as f64;
        let available_w = params.container_width - total_gaps;
        let current_row_w = pending_ar_sum * params.target_row_height;

        if current_row_w >= available_w {
            commit_row(
                &mut pending_items,
                &mut pending_ar_sum,
                &mut current_y,
                params.target_row_height,
                &mut rows,
                params,
                false,
            );
        }
    }

    // Commit last partial row
    // 提交最后的非满行
    if !pending_items.is_empty() {
        commit_row(
            &mut pending_items,
            &mut pending_ar_sum,
            &mut current_y,
            params.target_row_height,
            &mut rows,
            params,
            true,
        );
    }

    rows
}

// ── Helpers ───────────────────────────────────────────────────────────────────
// ── 辅助函数 ───────────────────────────────────────────────────────────────────

fn aspect_ratio(item: &LayoutItem) -> f64 {
    let w = item.width.max(1) as f64;
    let h = item.height.max(1) as f64;
    (w / h).clamp(0.2, 5.0) // clamp to prevent extreme ratios
                            // 限制以防止极端的比例
}

fn timestamp_to_date_label(ts: i64) -> String {
    let dt = Utc.timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(Utc::now);

    // Format: "2024年3月15日"  (Chinese date, as specified in the plan)
    // 格式: "2024年3月15日" (中文日期，如计划所指定)
    // Adjust: use local timezone for display — for simplicity use UTC here.
    // 调整：使用本地时区进行显示 — 为简单起见，此处使用 UTC。
    format!("{}年{}月{}日", dt.year(), dt.month(), dt.day())
}
