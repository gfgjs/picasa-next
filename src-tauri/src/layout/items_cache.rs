// src-tauri/src/layout/items_cache.rs
//! S1 视图取数缓存（Part2 重排提速，2026-07-04）。
//!
//! 病根：compute_layout 每次触发都全量重跑「1M 行 SQL → 布局 → 物化」三段 O(N) 流水线，
//! 而滑块/窗宽/布局模式/分组轴这些**几何交互根本不改变视图集合**（WHERE 子句不变）。
//! 本缓存把「取数」从「几何」中拆出：
//!
//! - 命中键 = `filter_key`（MediaFilter 的 canonical JSON）+ `data_version`（AppState 全局
//!   数据版本，任何成员/几何/顺序写路径 bump）+ 序形态匹配（见 [`CachedOrder`]）。
//! - `sort_within = datetime`（默认）家族存**基准序**（`sort_datetime DESC, id DESC`，
//!   经 `query_layout_items_canonical` 免 JOIN 取回）；date/none/folder 三轴与 asc/desc
//!   方向全部由 [`derive_order`] 内存派生 —— 轴切换从「5s 级 SQL 字符串排序」变为亚秒内存排序。
//! - filename/similarity 特殊排序按 SQL 序原样缓存（签名含全部排序参数），只加速
//!   滑块/窗宽等「序不变」交互。
//! - ai_search 视图**不缓存**（ai_search_results 随每次搜索整表重写）。
//!
//! **序等价契约（刚性）**：`derive_order` 的输出必须与 `push_query_body` 的 SQL ORDER 逐项
//! 等价 —— `get_view_ids`（flat_ids）与 `view_to_sql`（SelectAll 解析）分别源于两条路径，
//! 错位即选区漂移。等价性由 queries.rs 的对拍测试锁定。
//!
//! **锁纪律**：本缓存与 layout_cache 是两把独立 RwLock，任何路径不得同时持有两把
//! （compute_layout 在 items 读锁内跑布局，出锁后才 store_layout）。
//!
//! **S3（几何/载荷分离）后的双重身份**：本缓存同时是**布局行的载荷源**——布局行仅存
//! id + 几何，get_layout_rows 系命令出口经 [`hydrate_rows`] 从这里取载荷拼装线上行。
//! 因此「视图敏感写」不再整体置 None（会饿死出口拼装），而是降级 `reusable = false`：
//! compute 端视同 MISS 重查换代，本快照继续服务旧布局的取行（值照常 patch，视觉即时）。

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use rustc_hash::FxHashMap;

use crate::db::models::{DirLabel, LayoutItem, MediaFilter, ThumbResult};
use crate::layout::justified::{hydrate_item, placeholder_item, HydratedRow, LayoutRow};

/// 缓存内容的序形态。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CachedOrder {
    /// 基准序（`sort_datetime DESC, id DESC`）—— `sort_within=datetime` 家族，
    /// 分组轴/方向由 [`derive_order`] 内存派生。
    Canonical,
    /// 特殊排序（filename/similarity）：按 SQL ORDER 原样缓存，任一排序参数变化即 miss。
    Sql {
        group_by: String,
        sort_within: String,
        sort_order: String,
    },
}

/// [`derive_order`] folder 轴的排序置换 memo（items 下标序列）。
pub struct PermMemo {
    pub group_by: String,
    pub sort_order: String,
    /// 派生序 → 基准序下标的置换。
    pub perm: Vec<u32>,
}

/// 驻留的视图取数缓存体。
pub struct ItemsCacheData {
    /// MediaFilter 的 canonical JSON —— 视图集合签名（WHERE 子句的等价物）。
    pub filter_key: String,
    pub order: CachedOrder,
    /// 填充时的全局数据版本（`AppState::data_version`）：写路径 bump 后不再命中。
    pub data_version: u64,
    pub items: Vec<LayoutItem>,
    /// id → items 下标：thumb/favorite/rating/color 的 O(1) 就地 patch。
    /// S3.2：FxHashMap（同 cache.rs id_to_flat——1M 级整数键构建提速数倍）。
    pub id_to_idx: FxHashMap<i64, u32>,
    pub dir_labels: HashMap<i64, DirLabel>,
    /// dir_id → rel_path 序秩。rel_path 去重后按字节序排秩，**同 rel_path 同秩** ——
    /// 复刻 SQLite BINARY `ORDER BY d.rel_path ASC`：多根同 rel_path 的并列项由后续
    /// (sort_datetime, id) 键裁决，与 SQL 行为逐项一致（S1 序等价契约）。
    pub dir_rank: HashMap<i64, u32>,
    /// 缓存 filter 本体：favorite/rating/color patch 时判断「该字段是否影响本视图成员」
    /// （如 favoritedOnly 视图下取消收藏 = 成员变化 → 整体失效而非 patch）。
    pub filter: MediaFilter,
    /// 是否可作为 compute_layout 的命中源（S3）。false = 仅作载荷源服务出口拼装：
    /// ①视图敏感写后（成员已变，须重查换代）②ai_search 视图（结果表随每次搜索整表重写）。
    pub reusable: bool,
    /// 已测量项宽高比中位数的惰性缓存（S3.5）：只依赖项集、不依赖布局参数，justified
    /// 每次重排免 O(N) 重算。就地 patch（尺寸回填）有意不失效——中位数轻微漂移仅影响
    /// 0×0 占位项的形状，容差内；随缓存体整体换代自动重算。
    pub median_aspect: std::sync::OnceLock<f64>,
    /// folder 轴派生序的置换 memo：同 (group_by, sort_order) 的后续派生免排序、O(N) 还原。
    /// 就地 patch 均不触碰排序键 (dir_id, sort_datetime, id)，故 memo 只需随缓存体
    /// 整体换代（MISS 重建）自动失效，无需单独失效逻辑。
    pub perm_memo: Mutex<Option<PermMemo>>,
}

pub type ItemsCache = RwLock<Option<ItemsCacheData>>;

pub fn new_items_cache() -> ItemsCache {
    RwLock::new(None)
}

/// 存入新缓存体（整体替换）。
pub fn store_items(cache: &ItemsCache, data: ItemsCacheData) {
    *cache.write().unwrap() = Some(data);
}

/// 显式整体清空（硬失效）。S3 后本缓存是布局行的载荷源——仅用于布局缓存同步清空的场景
/// （如 clear_database），否则会让出口拼装全部退化为占位行项；常规失效走 data_version
/// bump，视图敏感降级走 `reusable = false`（见 patch_or_degrade）。
pub fn invalidate(cache: &ItemsCache) {
    *cache.write().unwrap() = None;
}

/// S3.1 幂等去重预检：镜像 compute_layout 的 HIT 守卫（reusable + 序形态 + 数据代 +
/// 过滤器键），只回答「快照当前是否可作命中源」，不派生序、不触碰 items。
/// 与 compute_layout 内的 HIT 判定必须保持同一判据——改一处必改另一处。
pub fn is_hit_valid(
    cache: &ItemsCache,
    filter_key: &str,
    data_version: u64,
    group_by: &str,
    sort_within: &str,
    sort_order: &str,
) -> bool {
    let guard = cache.read().unwrap();
    let Some(data) = guard.as_ref() else {
        return false;
    };
    let order_ok = match &data.order {
        CachedOrder::Canonical => sort_within == "datetime",
        CachedOrder::Sql {
            group_by: g,
            sort_within: s,
            sort_order: o,
        } => g.as_str() == group_by && s.as_str() == sort_within && o.as_str() == sort_order,
    };
    data.reusable && order_ok && data.data_version == data_version && data.filter_key == filter_key
}

/// 构建 id → 下标索引（O(N) 一次，patch O(1)）。S3.2：预留容量 + FxHash——
/// MISS 换代路径同享构建提速。
pub fn build_id_index(items: &[LayoutItem]) -> FxHashMap<i64, u32> {
    let mut m: FxHashMap<i64, u32> =
        FxHashMap::with_capacity_and_hasher(items.len(), Default::default());
    for (i, it) in items.iter().enumerate() {
        m.insert(it.id, i as u32);
    }
    m
}

/// 构建 dir_rank（语义见 [`ItemsCacheData::dir_rank`] 字段文档）。
pub fn build_dir_rank(dir_labels: &HashMap<i64, DirLabel>) -> HashMap<i64, u32> {
    let mut paths: Vec<&str> = dir_labels.values().map(|d| d.rel_path.as_str()).collect();
    paths.sort_unstable();
    paths.dedup();
    let rank_of: HashMap<&str, u32> = paths
        .iter()
        .enumerate()
        .map(|(i, p)| (*p, i as u32))
        .collect();
    dir_labels
        .iter()
        .map(|(id, d)| (*id, rank_of[d.rel_path.as_str()]))
        .collect()
}

/// datetime 家族的序派生（基准序 = `sort_datetime DESC, id DESC`）：
/// - date/none + desc：恒等；
/// - date/none + asc：整体反转（(DESC,DESC) 反转恰为 (ASC,ASC)，与 SQL asc 序一致）；
/// - folder：(rel_path 秩 ASC, sort_datetime, id) 全键排序 —— rel_path 恒 ASC、方向仅作用于
///   ts/id 次键，与 SQL `ORDER BY d.rel_path ASC, m.sort_datetime {dir}, m.id {dir}` 一致。
///
/// 注：缓存查询免 directories JOIN，故孤儿 dir_id（目录行已删但媒体残留，FK 级联下不应
/// 出现）在此排 u32::MAX 末尾而非像 INNER JOIN 那样被剔除 —— 防御性差异，正常库无此形态。
///
/// 性能（S1.1，2026-07-04 真机回归修复）：folder 轴走「装饰-排序-还原 + 置换 memo」。
/// 直接对 `&LayoutItem` 排序时每次比较 = 2 次查秩 + 2 次对 ~200B 大结构体的随机访存，
/// 1M 项 ≈ 2000 万次比较实测 5-7s；排序键抽进紧凑连续元组后亚秒，置换存入
/// [`ItemsCacheData::perm_memo`]，同轴同向的后续交互（滑块/窗宽）免排序。
pub fn derive_order<'a>(
    data: &'a ItemsCacheData,
    group_by: &str,
    sort_order: &str,
) -> Vec<&'a LayoutItem> {
    let asc = sort_order == "asc";
    match group_by {
        "folder" => {
            // 置换 memo 命中：免排序，按下标 O(N) 还原引用序。
            {
                let memo = data.perm_memo.lock().unwrap();
                if let Some(m) = memo.as_ref() {
                    if m.group_by == group_by && m.sort_order == sort_order {
                        return m.perm.iter().map(|&i| &data.items[i as usize]).collect();
                    }
                }
            }
            let rank = |it: &LayoutItem| -> u32 {
                it.dir_id
                    .and_then(|id| data.dir_rank.get(&id).copied())
                    .unwrap_or(u32::MAX)
            };
            // 装饰-排序-还原：一次线性扫描抽键（顺序访存，预取友好），在 32B 紧凑元组的
            // 连续数组上排序 —— 比较器内零哈希查找、零大结构体随机访存。
            let mut keys: Vec<(u32, i64, i64, u32)> = data
                .items
                .iter()
                .enumerate()
                .map(|(i, it)| (rank(it), it.sort_datetime, it.id, i as u32))
                .collect();
            keys.sort_unstable_by(|a, b| {
                a.0.cmp(&b.0).then_with(|| {
                    let key = (a.1, a.2).cmp(&(b.1, b.2));
                    if asc {
                        key
                    } else {
                        key.reverse()
                    }
                })
            });
            let perm: Vec<u32> = keys.iter().map(|k| k.3).collect();
            let refs: Vec<&LayoutItem> = perm.iter().map(|&i| &data.items[i as usize]).collect();
            *data.perm_memo.lock().unwrap() = Some(PermMemo {
                group_by: group_by.to_string(),
                sort_order: sort_order.to_string(),
                perm,
            });
            refs
        }
        _ if asc => data.items.iter().rev().collect(),
        _ => data.items.iter().collect(),
    }
}

/// 出口拼装（S3 几何/载荷分离）：瘦布局行 → 线上行。逐 id 经 id_to_idx 取载荷；查无此 id
/// （布局与快照换代的竞态窗口/清库后未重算的瞬态）→ 占位行项（几何保留、载荷置空），
/// 下次布局换代自愈——不丢行不 panic，保虚拟滚动行形稳定。仅对可视区调用（10^2 级行项），
/// 读锁 + 载荷克隆成本无关紧要。
pub fn hydrate_rows(cache: &ItemsCache, rows: Vec<LayoutRow>) -> Vec<HydratedRow> {
    let guard = cache.read().unwrap();
    let data = guard.as_ref();
    rows.into_iter()
        .map(|row| match row {
            LayoutRow::Separator {
                y,
                height,
                separator_label,
                group_id,
            } => HydratedRow::Separator {
                y,
                height,
                separator_label,
                group_id,
            },
            LayoutRow::Normal { y, height, items } => HydratedRow::Normal {
                y,
                height,
                items: items
                    .iter()
                    .map(|slot| {
                        data.and_then(|d| {
                            d.id_to_idx
                                .get(&slot.id)
                                .and_then(|&i| d.items.get(i as usize))
                        })
                        .map(|it| hydrate_item(it, slot))
                        .unwrap_or_else(|| placeholder_item(slot))
                    })
                    .collect(),
            },
        })
        .collect()
}

// ── 就地 patch(S3 后唯一 patch 目标,经 AppState 组合调用) ─────────────

/// 通用 patch：对每个命中 id 应用 `f`；若 `sensitive`（该写会改变本缓存视图的**成员**）
/// 则同时降级 `reusable = false`——下次 compute 视同 MISS 重查。S3 后不可整体置 None：
/// 本缓存还是现存布局行的载荷源，置 None 会让出口拼装全部退化为占位行项（可视区破图）；
/// 值仍照常 patch，使旧布局在重查换代前的展示即时正确。
fn patch_or_degrade<F: Fn(&mut LayoutItem)>(
    cache: &ItemsCache,
    ids: &[i64],
    sensitive: impl FnOnce(&MediaFilter) -> bool,
    f: F,
) {
    if ids.is_empty() {
        return;
    }
    let mut guard = cache.write().unwrap();
    let Some(data) = guard.as_mut() else { return };
    if sensitive(&data.filter) {
        data.reusable = false;
    }
    for id in ids {
        let Some(&i) = data.id_to_idx.get(id) else {
            continue;
        };
        if let Some(it) = data.items.get_mut(i as usize) {
            f(it);
        }
    }
}

/// 缩略图结果就地 patch。**不失效不 bump**：缩略图不改变视图成员/几何，仅展示字段 ——
/// 这是 S1 的关键设计（浏览时的持续缩略图生成若走失效，缓存将长期冰冷）。
pub fn apply_thumb_results(cache: &ItemsCache, results: &[ThumbResult]) {
    if results.is_empty() {
        return;
    }
    let mut guard = cache.write().unwrap();
    let Some(data) = guard.as_mut() else { return };
    for r in results {
        let Some(&i) = data.id_to_idx.get(&r.item_id) else {
            continue;
        };
        if let Some(it) = data.items.get_mut(i as usize) {
            it.thumb_status = r.thumb_status;
            it.thumb_path = r.thumb_path.clone();
            it.thumbhash = r.thumbhash.clone();
        }
    }
}

/// 尺寸回填 patch（0×0 占位 → 真实尺寸；守卫同 SQL update_media_dimensions：仅 0×0 项）。
/// 尺寸是布局几何**输入**，patch 后下次重排（缓存命中）即产出正确比例——不失效不 bump
/// （补尺寸阶段的高频写走失效会让缓存长期冰冷）。dims = (id, w, h)。
pub fn set_dimensions(cache: &ItemsCache, dims: &[(i64, i64, i64)]) {
    if dims.is_empty() {
        return;
    }
    let mut guard = cache.write().unwrap();
    let Some(data) = guard.as_mut() else { return };
    for &(id, w, h) in dims {
        let Some(&i) = data.id_to_idx.get(&id) else {
            continue;
        };
        if let Some(it) = data.items.get_mut(i as usize) {
            if it.width <= 0 || it.height <= 0 {
                it.width = w;
                it.height = h;
            }
        }
    }
}

/// 收藏 patch；favoritedOnly 视图（写改成员）→ 降级不可复用（reusable=false）。
pub fn set_favorite(cache: &ItemsCache, ids: &[i64], value: bool) {
    patch_or_degrade(
        cache,
        ids,
        |flt| flt.favorited_only == Some(true),
        |it| it.is_favorited = value,
    );
}

/// 评分 patch；minRating 过滤视图 → 降级不可复用。
pub fn set_rating(cache: &ItemsCache, ids: &[i64], rating: i64) {
    patch_or_degrade(
        cache,
        ids,
        |flt| flt.min_rating.is_some(),
        |it| it.rating = rating,
    );
}

/// 色标 patch；colorLabel 过滤视图 → 降级不可复用。
pub fn set_color_label(cache: &ItemsCache, ids: &[i64], color_label: i64) {
    patch_or_degrade(
        cache,
        ids,
        |flt| flt.color_label.is_some(),
        |it| it.color_label = color_label,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_item(id: i64, ts: i64, dir_id: i64) -> LayoutItem {
        LayoutItem {
            id,
            width: 100,
            height: 100,
            file_size: 0,
            sort_datetime: ts,
            file_format: "jpg".into(),
            media_type: "image".into(),
            is_live_photo: false,
            duration_ms: None,
            thumb_status: 0,
            thumb_path: None,
            thumbhash: None,
            is_favorited: false,
            rating: 0,
            color_label: 0,
            availability: "online".into(),
            dir_id: Some(dir_id),
            similarity: None,
        }
    }

    fn dl(rel: &str) -> DirLabel {
        DirLabel {
            rel_path: rel.into(),
            display: format!("C:/root/{rel}"),
            name: rel.into(),
        }
    }

    /// 基准序 fixture：ts DESC, id DESC(含同 ts 的 id tiebreaker)。
    fn canonical_data() -> ItemsCacheData {
        // (id, ts, dir): 基准序按 (ts DESC, id DESC)。
        let items = vec![
            mk_item(5, 300, 2),
            mk_item(4, 200, 1),
            mk_item(3, 200, 2), // 同 ts=200:id DESC → 4 在 3 前
            mk_item(1, 100, 1),
        ];
        let mut dir_labels = HashMap::new();
        dir_labels.insert(1, dl("a"));
        dir_labels.insert(2, dl("b"));
        let dir_rank = build_dir_rank(&dir_labels);
        let id_to_idx = build_id_index(&items);
        ItemsCacheData {
            filter_key: "{}".into(),
            order: CachedOrder::Canonical,
            data_version: 1,
            items,
            id_to_idx,
            dir_labels,
            dir_rank,
            filter: MediaFilter::default(),
            reusable: true,
            median_aspect: std::sync::OnceLock::new(),
            perm_memo: Mutex::new(None),
        }
    }

    fn ids(refs: &[&LayoutItem]) -> Vec<i64> {
        refs.iter().map(|it| it.id).collect()
    }

    #[test]
    fn derive_identity_desc_and_reversed_asc() {
        let data = canonical_data();
        assert_eq!(ids(&derive_order(&data, "date", "desc")), vec![5, 4, 3, 1]);
        // asc = 整体反转 → (ts ASC, id ASC)。
        assert_eq!(ids(&derive_order(&data, "date", "asc")), vec![1, 3, 4, 5]);
        // none 轴同 date(无分隔符差异,序一致)。
        assert_eq!(ids(&derive_order(&data, "none", "desc")), vec![5, 4, 3, 1]);
    }

    #[test]
    fn derive_folder_sorts_by_rel_path_rank_then_ts_id() {
        let data = canonical_data();
        // folder desc:dir a(rank 0)先 → 其内 (ts DESC,id DESC) = [4,1];dir b → [5,3]。
        assert_eq!(
            ids(&derive_order(&data, "folder", "desc")),
            vec![4, 1, 5, 3]
        );
        // folder asc:rel_path 仍 ASC,组内 (ts ASC,id ASC) = [1,4] / [3,5]。
        assert_eq!(ids(&derive_order(&data, "folder", "asc")), vec![1, 4, 3, 5]);
    }

    #[test]
    fn folder_perm_memo_reused_and_replaced() {
        let data = canonical_data();
        assert!(data.perm_memo.lock().unwrap().is_none());
        let first = ids(&derive_order(&data, "folder", "desc"));
        assert!(
            data.perm_memo.lock().unwrap().is_some(),
            "首次 folder 派生应写入置换 memo"
        );
        // memo 命中路径必须还原出与首次完全一致的序。
        assert_eq!(ids(&derive_order(&data, "folder", "desc")), first);
        // 换方向 → memo 替换,序仍正确(等价于 derive_folder 测试的 asc 期望)。
        assert_eq!(ids(&derive_order(&data, "folder", "asc")), vec![1, 4, 3, 5]);
        let memo = data.perm_memo.lock().unwrap();
        assert_eq!(memo.as_ref().unwrap().sort_order, "asc");
    }

    #[test]
    fn same_rel_path_shares_rank_so_ts_breaks_ties() {
        // 两根同 rel_path "x":同秩 → 并列由 (ts,id) 裁决(复刻 SQL 单键 rel_path 排序)。
        let items = vec![mk_item(2, 200, 11), mk_item(1, 100, 10)];
        let mut dir_labels = HashMap::new();
        dir_labels.insert(10, dl("x"));
        dir_labels.insert(11, dl("x"));
        let dir_rank = build_dir_rank(&dir_labels);
        assert_eq!(dir_rank[&10], dir_rank[&11], "同 rel_path 必须同秩");
        let id_to_idx = build_id_index(&items);
        let data = ItemsCacheData {
            filter_key: "{}".into(),
            order: CachedOrder::Canonical,
            data_version: 1,
            items,
            id_to_idx,
            dir_labels,
            dir_rank,
            filter: MediaFilter::default(),
            reusable: true,
            median_aspect: std::sync::OnceLock::new(),
            perm_memo: Mutex::new(None),
        };
        // 同秩 → ts DESC 决定:2(ts200) 在 1(ts100) 前(跨 dir 交错,与 SQL 一致)。
        assert_eq!(ids(&derive_order(&data, "folder", "desc")), vec![2, 1]);
    }

    #[test]
    fn thumb_patch_updates_in_place_without_invalidation() {
        let cache = new_items_cache();
        store_items(&cache, canonical_data());
        apply_thumb_results(
            &cache,
            &[ThumbResult {
                item_id: 3,
                thumb_status: 1,
                thumb_path: Some("t/3.webp".into()),
                thumbhash: Some(vec![9]),
            }],
        );
        let guard = cache.read().unwrap();
        let data = guard.as_ref().expect("thumb patch 不应失效缓存");
        let it = &data.items[data.id_to_idx[&3] as usize];
        assert_eq!(it.thumb_status, 1);
        assert_eq!(it.thumb_path.as_deref(), Some("t/3.webp"));
    }

    #[test]
    fn favorite_patches_normal_view_but_invalidates_favorites_view() {
        // 普通视图:就地 patch。
        let cache = new_items_cache();
        store_items(&cache, canonical_data());
        set_favorite(&cache, &[4], true);
        {
            let guard = cache.read().unwrap();
            let data = guard.as_ref().unwrap();
            assert!(data.items[data.id_to_idx[&4] as usize].is_favorited);
        }
        // favoritedOnly 视图:成员变化 → 降级不可复用(S3:数据保留,继续服务出口拼装)。
        let mut sensitive = canonical_data();
        sensitive.filter.favorited_only = Some(true);
        store_items(&cache, sensitive);
        set_favorite(&cache, &[4], false);
        let guard = cache.read().unwrap();
        let data = guard
            .as_ref()
            .expect("敏感写应保留数据(载荷源),不得置 None");
        assert!(!data.reusable, "敏感写应降级 reusable=false");
        assert!(
            !data.items[data.id_to_idx[&4] as usize].is_favorited,
            "降级同时值仍应被 patch(旧布局展示即时正确)"
        );
    }

    /// S3.1:is_hit_valid 与 HIT 守卫同判据——reusable/序形态/数据代/过滤器键四关全过才 true。
    #[test]
    fn is_hit_valid_mirrors_hit_guard() {
        let cache = new_items_cache();
        assert!(
            !is_hit_valid(&cache, "{}", 1, "date", "datetime", "desc"),
            "空缓存不可命中"
        );

        store_items(&cache, canonical_data());
        // Canonical 序形态:sort_within=datetime 时任意 group_by/sort_order 可派生。
        assert!(is_hit_valid(&cache, "{}", 1, "date", "datetime", "desc"));
        assert!(is_hit_valid(&cache, "{}", 1, "folder", "datetime", "asc"));
        // 非 datetime 家族 → Canonical 不可派生。
        assert!(!is_hit_valid(&cache, "{}", 1, "date", "filename", "desc"));
        // 数据代/过滤器键不符。
        assert!(!is_hit_valid(&cache, "{}", 2, "date", "datetime", "desc"));
        assert!(!is_hit_valid(
            &cache,
            "{\"x\":1}",
            1,
            "date",
            "datetime",
            "desc"
        ));

        // 敏感写降级 reusable=false → 探针同步失效(仍作载荷源,但不可命中)。
        let mut sensitive = canonical_data();
        sensitive.filter.favorited_only = Some(true);
        store_items(&cache, sensitive);
        set_favorite(&cache, &[4], false);
        assert!(!is_hit_valid(&cache, "{}", 1, "date", "datetime", "desc"));

        // Sql 序形态:三元组逐项相等才可命中。
        let mut sql = canonical_data();
        sql.order = CachedOrder::Sql {
            group_by: "date".into(),
            sort_within: "filename".into(),
            sort_order: "desc".into(),
        };
        store_items(&cache, sql);
        assert!(is_hit_valid(&cache, "{}", 1, "date", "filename", "desc"));
        assert!(!is_hit_valid(&cache, "{}", 1, "date", "filename", "asc"));
    }

    #[test]
    fn rating_and_color_sensitivity() {
        let cache = new_items_cache();
        let mut d = canonical_data();
        d.filter.min_rating = Some(3);
        store_items(&cache, d);
        set_rating(&cache, &[1], 5);
        {
            let guard = cache.read().unwrap();
            let data = guard.as_ref().expect("敏感写应保留数据");
            assert!(!data.reusable, "minRating 视图评分写应降级不可复用");
            assert_eq!(data.items[data.id_to_idx[&1] as usize].rating, 5);
        }

        let mut d2 = canonical_data();
        d2.filter.color_label = Some(2);
        store_items(&cache, d2);
        set_color_label(&cache, &[1], 4);
        let guard = cache.read().unwrap();
        let data = guard.as_ref().expect("敏感写应保留数据");
        assert!(!data.reusable, "colorLabel 视图色标写应降级不可复用");
        assert_eq!(data.items[data.id_to_idx[&1] as usize].color_label, 4);
    }

    /// S3 出口拼装:命中 id → 载荷来自 items;未知 id → 占位(几何保留);None 缓存 → 全占位。
    #[test]
    fn hydrate_rows_fills_payload_and_placeholders() {
        use crate::layout::justified::SlimRowItem;
        let cache = new_items_cache();
        store_items(&cache, canonical_data());
        let slot = |id: i64, x: f64| SlimRowItem {
            id,
            x,
            w: 50.0,
            h: 40.0,
        };
        let rows = vec![
            LayoutRow::Separator {
                y: 0.0,
                height: 36.0,
                separator_label: "sep".into(),
                group_id: Some("g".into()),
            },
            LayoutRow::Normal {
                y: 36.0,
                height: 40.0,
                items: vec![slot(4, 0.0), slot(999, 60.0)],
            },
        ];
        let wire = hydrate_rows(&cache, rows.clone());
        assert_eq!(wire.len(), 2);
        match &wire[1] {
            HydratedRow::Normal { items, .. } => {
                assert_eq!(items.len(), 2, "未知 id 不丢行项(占位)");
                assert_eq!(items[0].id, 4);
                assert_eq!(items[0].file_format, "jpg", "载荷应来自 items 缓存");
                assert_eq!((items[0].x, items[0].w, items[0].h), (0.0, 50.0, 40.0));
                assert_eq!(items[1].id, 999);
                assert_eq!(items[1].file_format, "", "未知 id → 占位载荷");
                assert_eq!(items[1].x, 60.0, "占位保留几何");
            }
            _ => panic!("expected normal row"),
        }
        // None 缓存(清库瞬态)→ 全占位,不 panic 不丢行。
        invalidate(&cache);
        let wire2 = hydrate_rows(&cache, rows);
        match &wire2[1] {
            HydratedRow::Normal { items, .. } => {
                assert_eq!(items[0].file_format, "", "None 缓存 → 占位");
            }
            _ => panic!("expected normal row"),
        }
    }
}
