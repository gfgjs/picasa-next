// src-tauri/src/layout/cache.rs
//! Layout cache stored in `AppState`.
//! 存储在 `AppState` 中的布局缓存。
//!
//! `compute_layout` stores the result here; `get_layout_rows` reads slices.
//! `compute_layout` 将结果存储于此；`get_layout_rows` 读取切片。
//! A `layout_version` counter prevents stale reads.
//! `layout_version` 计数器用于防止读取过期数据。

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
/// 存储在内存布局缓存中的数据。
pub struct LayoutCacheData {
    pub rows:           Vec<LayoutRow>,
    pub total_height:   f64,
    pub layout_version: u64,
}

/// The layout cache — stored behind an `RwLock` in `AppState`.
/// 布局缓存 — 存储在 `AppState` 中的 `RwLock` 后面。
pub type LayoutCache = RwLock<Option<LayoutCacheData>>;

/// Create a fresh layout cache (initially empty).
/// 创建一个全新的布局缓存（初始为空）。
pub fn new_layout_cache() -> LayoutCache {
    RwLock::new(None)
}

/// Store a new layout, atomically incrementing the version.
/// 存储新的布局，自动递增版本号。
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
/// 从缓存中检索行切片。
/// Returns `None` if the cache is empty or the version doesn't match.
/// 如果缓存为空或版本不匹配，则返回 `None`。
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
/// 从缓存中检索与 [top_y, bottom_y] 相交的行切片。
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
/// 获取布局摘要（行数 + 总高度 + 版本）。
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
/// 从缓存布局中获取相邻项 ID
pub fn get_adjacent_item(cache: &LayoutCache, current_id: i64, offset: isize) -> Option<i64> {
    let guard = cache.read().unwrap();
    let data = guard.as_ref()?;
    
    // Flatten all items
    // 展平所有项目
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

/// Get all item IDs from the cached layout
/// 从缓存布局中获取所有项 ID
pub fn get_all_item_ids(cache: &LayoutCache, expected_version: Option<u64>) -> Option<Vec<i64>> {
    let guard = cache.read().unwrap();
    let data = guard.as_ref()?;
    
    if let Some(ver) = expected_version {
        if data.layout_version != ver {
            return None;
        }
    }
    
    let mut all_ids = Vec::new();
    for row in &data.rows {
        if let LayoutRow::Normal { items, .. } = row {
            for item in items {
                all_ids.push(item.id);
            }
        }
    }
    Some(all_ids)
}
