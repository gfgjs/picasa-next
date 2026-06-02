// src-tauri/src/layout/cache.rs
//! Layout cache stored in `AppState`.
//!
//! `compute_layout` stores the result here; `get_layout_rows` reads slices.
//! A `layout_version` counter prevents stale reads.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::layout::justified::LayoutRow;

static LAYOUT_VERSION_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSummary {
    pub total_rows:   usize,
    pub total_height: f64,
    pub layout_version: u64,
}

/// Data stored in the in-memory layout cache.
pub struct LayoutCacheData {
    pub rows:           Vec<LayoutRow>,
    pub total_height:   f64,
    pub layout_version: u64,
}

/// The layout cache — stored behind an `RwLock` in `AppState`.
pub type LayoutCache = RwLock<Option<LayoutCacheData>>;

/// Create a fresh layout cache (initially empty).
pub fn new_layout_cache() -> LayoutCache {
    RwLock::new(None)
}

/// Store a new layout, atomically incrementing the version.
pub fn store_layout(cache: &LayoutCache, rows: Vec<LayoutRow>, total_height: f64) -> u64 {
    let version = LAYOUT_VERSION_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
    let mut guard = cache.write().unwrap();
    *guard = Some(LayoutCacheData {
        rows,
        total_height,
        layout_version: version,
    });
    version
}

/// Retrieve a slice of rows from the cache.
/// Returns `None` if the cache is empty or the version doesn't match.
pub fn get_rows(
    cache: &LayoutCache,
    start_row: usize,
    end_row: usize,
    expected_version: Option<u64>,
) -> Option<Vec<LayoutRow>> {
    let guard = cache.read().unwrap();
    let data = guard.as_ref()?;

    if let Some(ver) = expected_version {
        if data.layout_version != ver {
            return None;
        }
    }

    let end = end_row.min(data.rows.len());
    if start_row >= end {
        return Some(vec![]);
    }
    Some(data.rows[start_row..end].to_vec())
}

/// Retrieve a slice of rows intersecting [top_y, bottom_y] from the cache.
pub fn get_rows_by_y(
    cache: &LayoutCache,
    top_y: f64,
    bottom_y: f64,
    expected_version: Option<u64>,
) -> Option<Vec<LayoutRow>> {
    let guard = cache.read().unwrap();
    let data = guard.as_ref()?;

    if let Some(ver) = expected_version {
        if data.layout_version != ver {
            return None;
        }
    }

    let start_idx = match data.rows.binary_search_by(|r| r.y().partial_cmp(&top_y).unwrap()) {
        Ok(idx) => idx,
        Err(idx) => idx.saturating_sub(1),
    };

    let mut end_idx = start_idx;
    while end_idx < data.rows.len() && data.rows[end_idx].y() <= bottom_y {
        end_idx += 1;
    }

    Some(data.rows[start_idx..end_idx].to_vec())
}

/// Get the layout summary (row count + total height + version).
pub fn get_summary(cache: &LayoutCache) -> Option<LayoutSummary> {
    let guard = cache.read().unwrap();
    let data = guard.as_ref()?;
    Some(LayoutSummary {
        total_rows:     data.rows.len(),
        total_height:   data.total_height,
        layout_version: data.layout_version,
    })
}

/// Get the adjacent item ID from the cached layout
pub fn get_adjacent_item(cache: &LayoutCache, current_id: i64, offset: isize) -> Option<i64> {
    let guard = cache.read().unwrap();
    let data = guard.as_ref()?;
    
    // Flatten all items
    let mut all_ids = Vec::new();
    for row in &data.rows {
        if let LayoutRow::Normal { items, .. } = row {
            for item in items {
                all_ids.push(item.id);
            }
        }
    }
    
    let current_idx = all_ids.iter().position(|&id| id == current_id)?;
    let target_idx = (current_idx as isize + offset) as usize;
    
    all_ids.get(target_idx).copied()
}
