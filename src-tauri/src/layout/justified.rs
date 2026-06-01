// src-tauri/src/layout/justified.rs
//! Justified Layout algorithm (Rust implementation, § 10.1).
//!
//! Input: flat list of `LayoutItem` sorted by `sort_datetime DESC`.
//! Output: `Vec<LayoutRow>` — each row is either a normal image row or a date separator.
//!
//! Algorithm (Google Photos / Flickr justified layout):
//!   - For each item, compute the aspect ratio.
//!   - Pack items into a row by scaling them to a common height.
//!   - When the row width reaches `container_width ± tolerance`, commit the row.
//!   - When the `sort_datetime` crosses a day boundary, insert a separator row first.

use chrono::{DateTime, Datelike, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::db::models::LayoutItem;

// ── Output types ─────────────────────────────────────────────────────────────

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
    pub media_type:    String,
    pub is_live_photo: bool,
    pub duration_ms:   Option<i64>,
    pub thumb_status:  i64,
    pub thumb_path:    Option<String>,
    pub thumbhash:     Option<Vec<u8>>,
}

// ── Layout parameters ─────────────────────────────────────────────────────────

pub struct LayoutParams {
    pub container_width: f64,
    pub target_row_height: f64,
    pub gap: f64,
}

const SEPARATOR_HEIGHT: f64 = 36.0;
/// How much we allow the last row to be shorter before we "justify" it vs. leave as-is.
const LAST_ROW_JUSTIFY_THRESHOLD: f64 = 0.6;

// ── Main algorithm ────────────────────────────────────────────────────────────

pub fn compute_justified_layout(items: &[LayoutItem], params: &LayoutParams) -> Vec<LayoutRow> {
    let mut rows: Vec<LayoutRow> = Vec::new();
    let mut current_y = 0.0f64;

    let mut pending_items: Vec<&LayoutItem> = Vec::new();
    let mut pending_ar_sum = 0.0f64; // sum of (w/h) aspect ratios
    let mut last_date_label: Option<String> = None;

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
        let total_gaps = params.gap * (pending.len().saturating_sub(1)) as f64;
        let available_w = params.container_width - total_gaps;

        let row_h = if is_last && *ar_sum * target_h < available_w * LAST_ROW_JUSTIFY_THRESHOLD {
            // Last row — don't stretch; use target height
            target_h
        } else {
            available_w / *ar_sum
        };

        let mut x = 0.0f64;
        let mut row_items: Vec<LayoutRowItem> = Vec::new();

        for (i, item) in pending.iter().enumerate() {
            let ar = aspect_ratio(item);
            let item_w = if i == pending.len() - 1 {
                // Last item in row: use remaining width to avoid float accumulation gaps
                params.container_width - x
            } else {
                (ar * row_h).round()
            };

            row_items.push(LayoutRowItem {
                id:            item.id,
                x:             x.round(),
                w:             item_w.max(1.0),
                h:             row_h.round(),
                media_type:    item.media_type.clone(),
                is_live_photo: item.is_live_photo,
                duration_ms:   item.duration_ms,
                thumb_status:  item.thumb_status,
                thumb_path:    item.thumb_path.clone(),
                thumbhash:     item.thumbhash.clone(),
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
        // Date separator check
        let date_label = timestamp_to_date_label(item.sort_datetime);
        let needs_separator = match &last_date_label {
            None    => true,
            Some(prev) => *prev != date_label,
        };

        if needs_separator {
            // Commit any pending row before the separator
            let pending_clone = pending_items.clone();
            commit_row(
                &mut pending_items,
                &mut pending_ar_sum,
                &mut current_y,
                params.target_row_height,
                &mut rows,
                params,
                false,
            );
            emit_separator(&date_label, &mut current_y, &mut rows);
            last_date_label = Some(date_label);
        }

        let ar = aspect_ratio(item);
        pending_items.push(item);
        pending_ar_sum += ar;

        // Check if row is full
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

fn aspect_ratio(item: &LayoutItem) -> f64 {
    let w = item.width.max(1) as f64;
    let h = item.height.max(1) as f64;
    (w / h).clamp(0.2, 5.0) // clamp to prevent extreme ratios
}

fn timestamp_to_date_label(ts: i64) -> String {
    let dt = Utc.timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(Utc::now);

    // Format: "2024年3月15日"  (Chinese date, as specified in the plan)
    // Adjust: use local timezone for display — for simplicity use UTC here.
    format!("{}年{}月{}日", dt.year(), dt.month(), dt.day())
}
