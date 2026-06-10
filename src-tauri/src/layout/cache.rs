// src-tauri/src/layout/cache.rs
//! Layout cache stored in `AppState`.
//! 存储在 `AppState` 中的布局缓存。
//!
//! `compute_layout` stores the result here; `get_layout_rows` reads slices.
//! `compute_layout` 将结果存储于此；`get_layout_rows` 读取切片。
//! A `layout_version` counter prevents stale reads.
//! `layout_version` 计数器用于防止读取过期数据。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::db::models::ThumbResult;
use crate::layout::justified::LayoutRow;

static LAYOUT_VERSION_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeparatorInfo {
    pub label: String,
    pub y: f64,
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSummary {
    pub total_rows:   usize,
    pub total_height: f64,
    pub layout_version: u64,
    pub total_items:  usize,
    pub separators:   Vec<SeparatorInfo>,
}

/// Data stored in the in-memory layout cache.
/// 存储在内存布局缓存中的数据。
///
/// The flat indices below turn two hot operations from O(N) into O(1):
///   - thumbnail result write-back (was a full rows×items scan under the write lock)
///   - adjacent-item lookup for detail navigation (was a full flatten per arrow key)
///
/// 下面的扁平索引把两个热点操作从 O(N) 降到 O(1)：
///   - 缩略图结果回写（原先在写锁下全表 rows×items 扫描）
///   - 详情页相邻项查找（原先每按一次方向键都展平全表）
pub struct LayoutCacheData {
    pub rows:           Vec<LayoutRow>,
    pub total_height:   f64,
    pub layout_version: u64,
    pub total_items:    usize,

    /// Layout-order item ids (one entry per image item, separators excluded).
    /// 按布局顺序排列的项 id（每个图片项一个，不含分隔符）。
    pub flat_ids:       Vec<i64>,
    /// Parallel to `flat_ids`: flat index → (row index, item index within row).
    /// 与 `flat_ids` 并行：扁平下标 → (行下标, 行内项下标)。
    pub flat_rowcol:    Vec<(u32, u32)>,
    /// item id → flat index. The single source of truth for both hot paths.
    /// 项 id → 扁平下标。两个热点路径的唯一索引来源。
    pub id_to_flat:     HashMap<i64, usize>,
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

    // Build the flat indices in a single pass while we still own `rows`.
    // 在仍持有 `rows` 时一次遍历构建扁平索引。
    let mut flat_ids: Vec<i64> = Vec::new();
    let mut flat_rowcol: Vec<(u32, u32)> = Vec::new();
    let mut id_to_flat: HashMap<i64, usize> = HashMap::new();
    for (ri, row) in rows.iter().enumerate() {
        if let LayoutRow::Normal { items, .. } = row {
            for (ii, item) in items.iter().enumerate() {
                id_to_flat.insert(item.id, flat_ids.len());
                flat_ids.push(item.id);
                flat_rowcol.push((ri as u32, ii as u32));
            }
        }
    }
    let total_items = flat_ids.len();

    let mut guard = cache.write().unwrap();
    *guard = Some(LayoutCacheData {
        rows,
        total_height,
        layout_version: version,
        total_items,
        flat_ids,
        flat_rowcol,
        id_to_flat,
    });
    version
}

/// Apply a batch of thumbnail results to the cached layout in O(batch) using the
/// id index — replaces the previous O(rows × items × results) write-lock scan.
///
/// 使用 id 索引以 O(batch) 复杂度将一批缩略图结果写回缓存布局 —
/// 取代原先 O(行数 × 项数 × 结果数) 的写锁全表扫描。
pub fn apply_thumb_results(cache: &LayoutCache, results: &[ThumbResult]) {
    if results.is_empty() {
        return;
    }
    let mut guard = cache.write().unwrap();
    let Some(data) = guard.as_mut() else { return };
    for r in results {
        let Some(&flat) = data.id_to_flat.get(&r.item_id) else { continue };
        let Some(&(ri, ii)) = data.flat_rowcol.get(flat) else { continue };
        if let Some(LayoutRow::Normal { items, .. }) = data.rows.get_mut(ri as usize) {
            if let Some(item) = items.get_mut(ii as usize) {
                item.thumb_status = r.thumb_status;
                item.thumb_path = r.thumb_path.clone();
                item.thumbhash = r.thumbhash.clone();
            }
        }
    }
}

/// O(1)-per-id update of `is_favorited` in the cached layout, mirroring a DB write so
/// `get_rows_by_y` returns fresh state after the row scrolls out and back in (D3).
///
/// 以每 id O(1) 更新缓存布局中的 is_favorited，与数据库写入保持一致，
/// 使行滚出再滚回时 `get_rows_by_y` 仍返回最新状态（D3）。
pub fn set_favorite_in_cache(cache: &LayoutCache, ids: &[i64], value: bool) {
    if ids.is_empty() {
        return;
    }
    let mut guard = cache.write().unwrap();
    let Some(data) = guard.as_mut() else { return };
    for &id in ids {
        let Some(&flat) = data.id_to_flat.get(&id) else { continue };
        let Some(&(ri, ii)) = data.flat_rowcol.get(flat) else { continue };
        if let Some(LayoutRow::Normal { items, .. }) = data.rows.get_mut(ri as usize) {
            if let Some(item) = items.get_mut(ii as usize) {
                item.is_favorited = value;
            }
        }
    }
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
    
    let mut separators = Vec::new();
    for row in &data.rows {
        if let LayoutRow::Separator { y, separator_label, group_id, .. } = row {
            separators.push(SeparatorInfo {
                label: separator_label.clone(),
                y: *y,
                group_id: group_id.clone(),
            });
        }
    }

    Some(LayoutSummary {
        total_rows:     data.rows.len(),
        total_height:   data.total_height,
        layout_version: data.layout_version,
        total_items:    data.total_items,
        separators,
    })
}

/// Get the adjacent item ID from the cached layout
/// 从缓存布局中获取相邻项 ID
pub fn get_adjacent_item(cache: &LayoutCache, current_id: i64, offset: isize) -> Option<i64> {
    let guard = cache.read().unwrap();
    let data = guard.as_ref()?;

    // O(1) via the id index — no full flatten per navigation step.
    // 通过 id 索引 O(1) 完成 — 不再每步导航都展平全表。
    let current_idx = *data.id_to_flat.get(&current_id)?;
    let target_idx = current_idx as isize + offset;
    if target_idx < 0 {
        return None;
    }
    data.flat_ids.get(target_idx as usize).copied()
}

/// Find the Y coordinate of a separator row by matching its label
/// 通过匹配标签查找分隔符行的 Y 坐标
pub fn get_separator_y_by_label(cache: &LayoutCache, label_substring: &str) -> Option<f64> {
    let guard = cache.read().unwrap();
    let data = guard.as_ref()?;
    
    for row in &data.rows {
        if let LayoutRow::Separator { y, separator_label, .. } = row {
            if separator_label.contains(label_substring) {
                return Some(*y);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::justified::{LayoutRow, LayoutRowItem};

    fn mk_item(id: i64) -> LayoutRowItem {
        LayoutRowItem {
            id,
            x: 0.0,
            w: 100.0,
            h: 100.0,
            file_size: 0,
            file_format: String::new(),
            media_type: "image".into(),
            is_live_photo: false,
            duration_ms: None,
            thumb_status: 0,
            thumb_path: None,
            thumbhash: None,
            is_favorited: false,
            similarity: None,
            original_width: 100,
            original_height: 100,
            sort_datetime: 0,
        }
    }

    /// Separator + two Normal rows; flat item order is [10, 11, 12].
    /// 分隔符 + 两个普通行；扁平项顺序为 [10, 11, 12]。
    fn sample_layout() -> Vec<LayoutRow> {
        vec![
            LayoutRow::Separator { y: 0.0, height: 36.0, separator_label: "d1".into(), group_id: None },
            LayoutRow::Normal { y: 36.0, height: 100.0, items: vec![mk_item(10), mk_item(11)] },
            LayoutRow::Normal { y: 140.0, height: 100.0, items: vec![mk_item(12)] },
        ]
    }

    #[test]
    fn test_apply_thumb_results_updates_correct_item() {
        let cache = new_layout_cache();
        store_layout(&cache, sample_layout(), 240.0);

        apply_thumb_results(&cache, &[ThumbResult {
            item_id: 11,
            thumb_status: 1,
            thumb_path: Some("a/b.webp".into()),
            thumbhash: Some(vec![1, 2, 3]),
        }]);

        let guard = cache.read().unwrap();
        let data = guard.as_ref().unwrap();
        assert_eq!(data.flat_ids, vec![10, 11, 12]);
        assert_eq!(data.total_items, 3);
        match &data.rows[1] {
            LayoutRow::Normal { items, .. } => {
                assert_eq!(items[1].id, 11);
                assert_eq!(items[1].thumb_status, 1);
                assert_eq!(items[1].thumb_path.as_deref(), Some("a/b.webp"));
                assert_eq!(items[1].thumbhash, Some(vec![1, 2, 3]));
                // Sibling untouched.
                assert_eq!(items[0].thumb_status, 0);
            }
            _ => panic!("expected normal row"),
        }
    }

    #[test]
    fn test_get_adjacent_item_is_correct_at_boundaries() {
        let cache = new_layout_cache();
        store_layout(&cache, sample_layout(), 240.0);

        assert_eq!(get_adjacent_item(&cache, 10, 1), Some(11));
        assert_eq!(get_adjacent_item(&cache, 11, 1), Some(12));
        assert_eq!(get_adjacent_item(&cache, 12, 1), None); // past end
        assert_eq!(get_adjacent_item(&cache, 11, -1), Some(10));
        assert_eq!(get_adjacent_item(&cache, 10, -1), None); // before start
        assert_eq!(get_adjacent_item(&cache, 999, 1), None); // unknown id
    }

    #[test]
    fn test_set_favorite_in_cache_targets_correct_items() {
        let cache = new_layout_cache();
        store_layout(&cache, sample_layout(), 240.0);

        set_favorite_in_cache(&cache, &[11, 12], true);

        let guard = cache.read().unwrap();
        let data = guard.as_ref().unwrap();
        match &data.rows[1] {
            LayoutRow::Normal { items, .. } => {
                assert!(!items[0].is_favorited, "id 10 should be untouched");
                assert!(items[1].is_favorited, "id 11 should be favorited");
            }
            _ => panic!("expected normal row"),
        }
        match &data.rows[2] {
            LayoutRow::Normal { items, .. } => assert!(items[0].is_favorited, "id 12 should be favorited"),
            _ => panic!("expected normal row"),
        }
    }
}
