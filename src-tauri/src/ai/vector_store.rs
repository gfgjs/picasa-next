// src-tauri/src/ai/vector_store.rs
//! 向量存储抽象（Part1 §3.4）。
//!
//! 把「暴力内积 ↔ ANN」做成可热切换的 `VectorStore` trait，并为「AI/人脸推理迁入插件 worker
//! （Part4）」预留干净边界。本 Part 只落 **阶段一**：`BruteForceStore`——封装现有 f16 常驻缓存 +
//! rayon 暴力点积（与 `search::semantic_search` **同一套数学**，零行为变更），并补**增量
//! `upsert`/`remove`**（取代「每次搜索全量重载」）。
//!
//! **本 store 刻意 DB 无关**：加载（从 SQLite 读 embedding 灌入）由调用方做（未来接线层 / Part4），
//! store 只管内存中的向量与检索。如此可纯单测、且不与现有 `AppState.ai_embedding_cache` 热路径耦合
//! （热路径的重接是 Part4 消费点，本 Part 不动，避免检索回归）。
//!
//! 阶段二（ANN：`sqlite-vec`/`usearch`）后置——到 `ANN_THRESHOLD` 量级再实装，可随 Part4 一并做。

//!
//! ## 状态：dormant（休眠、有主）——R2-2 裁决（2026-07-02）
//!
//! 全仓零生产消费者是**有意状态**而非遗漏：生产语义检索仍走 `AppState.ai_embedding_cache` 热路径。
//! **启用条件**（满足其一即接线，owner = Part4）：
//! 1. Part4 AI worker 化落地时，由其完成 DB 灌入 + AppState 多 store 重接（本模块即成热路径）；
//! 2. embedding 行数达 `ANN_THRESHOLD` 或检索 p95 > 300ms 时,按同一 trait 换 ANN 实现(阶段二)。
//!
//! 在此之前**不得以「未使用」为由清理本模块**（含其单测）。

use std::collections::HashMap;
use std::sync::RwLock;

use half::f16;
use rayon::prelude::*;

use crate::error::{AppError, Result};

/// ANN 触发阈值（**embedding 行数**）。低于此用暴力检索；达到则考虑切 ANN。
/// CLIP 一图一向量（行数=item 数）；人脸一图多脸、faces 行数 >> item 数，可能更早触发（§3.4）。
pub const ANN_THRESHOLD: usize = 500_000;

/// 向量存储：暴力 / ANN 实现共用此接口，让查询/写入与底层算法解耦。
///
/// 约定：所有写入的向量应为**预归一化单位向量**（与现有 CLIP/人脸 pipeline 一致），
/// 故 `search` 的相似度按**点积** == 余弦计算。
pub trait VectorStore: Send + Sync {
    /// 向量维度（架构 id 绑定；不同维度的向量不可混入同一 store）。
    fn dim(&self) -> usize;

    /// 插入或更新一条向量（`id` 已存在则覆盖其向量，行数不变）。
    fn upsert(&self, id: i64, vec: &[f32]) -> Result<()>;

    /// 批量 upsert。默认逐条；实现可覆写为更高效的一次性写入。
    fn upsert_batch(&self, pairs: &[(i64, Vec<f32>)]) -> Result<()> {
        for (id, v) in pairs {
            self.upsert(*id, v)?;
        }
        Ok(())
    }

    /// 移除一条向量（不存在则 no-op）。
    fn remove(&self, id: i64) -> Result<()>;

    /// 检索最相似的 top_k（**降序**返回 `(id, similarity)`）。空库 / `top_k=0` 返回空。
    fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<(i64, f32)>>;

    /// 当前向量条数。
    fn len(&self) -> usize;

    /// 是否为空。
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 落盘（持久化实现用；内存暴力 store 为空操作）。
    fn flush(&self) -> Result<()> {
        Ok(())
    }

    /// 从持久化重载（usearch 落 `.usearch`；内存暴力 store 为空操作）。
    fn reload(&self) -> Result<()> {
        Ok(())
    }
}

/// 共享句柄：AppState 按 `model_name`（架构 id）持多个（Part4 消费）。
pub type DynVectorStore = std::sync::Arc<dyn VectorStore>;

/// 内部缓冲：行主序 f16 连续存储 + `id→行号` 索引（支撑 O(1) upsert/remove）。
struct Inner {
    /// 第 `i` 条向量的 id；与 `data` 的第 `i` 行配对。
    ids: Vec<i64>,
    /// 行主序：第 `i` 行占 `data[i*dim .. (i+1)*dim]`。f16 半精度（省内存，百万级 ≈ 一半 RAM）。
    data: Vec<f16>,
    /// `id → 行号`，让 upsert（覆盖）/remove 免线性扫描。
    pos: HashMap<i64, usize>,
}

/// 阶段一暴力向量存储：f16 常驻 + rayon 并行点积。封装原 `EmbeddingCache` + `semantic_search`
/// 的检索数学，额外提供增量写入。线程安全（内部 `RwLock`），可放入 `Arc` 跨线程共享。
pub struct BruteForceStore {
    dim: usize,
    inner: RwLock<Inner>,
}

impl BruteForceStore {
    /// 新建空 store（指定维度）。
    pub fn new(dim: usize) -> Self {
        Self::with_capacity(dim, 0)
    }

    /// 预分配 `n` 条容量（已知规模时减少重分配）。
    pub fn with_capacity(dim: usize, n: usize) -> Self {
        BruteForceStore {
            dim,
            inner: RwLock::new(Inner {
                ids: Vec::with_capacity(n),
                data: Vec::with_capacity(n * dim),
                pos: HashMap::with_capacity(n),
            }),
        }
    }

    /// 维度校验：写入/查询向量长度必须等于 store 维度（防混入异构架构向量）。
    fn check_dim(&self, len: usize) -> Result<()> {
        if len != self.dim {
            return Err(AppError::Internal(format!(
                "向量维度不匹配：store={} 输入={}",
                self.dim, len
            )));
        }
        Ok(())
    }
}

impl VectorStore for BruteForceStore {
    fn dim(&self) -> usize {
        self.dim
    }

    fn upsert(&self, id: i64, vec: &[f32]) -> Result<()> {
        self.check_dim(vec.len())?;
        let dim = self.dim;
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        match inner.pos.get(&id).copied() {
            // 已存在：原地覆盖该行（行数不变）。
            Some(row) => {
                for (k, &x) in vec.iter().enumerate() {
                    inner.data[row * dim + k] = f16::from_f32(x);
                }
            }
            // 新行：追加 id + 数据，登记行号。
            None => {
                let row = inner.ids.len();
                inner.ids.push(id);
                for &x in vec {
                    inner.data.push(f16::from_f32(x));
                }
                inner.pos.insert(id, row);
            }
        }
        Ok(())
    }

    fn remove(&self, id: i64) -> Result<()> {
        let dim = self.dim;
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());

        let Some(row) = inner.pos.get(&id).copied() else {
            return Ok(()); // 不存在 → no-op
        };
        let last = inner.ids.len() - 1;
        // swap-remove：把末行搬到待删行位，再截断（O(dim)，不挪动其余行）。
        if row != last {
            for k in 0..dim {
                inner.data[row * dim + k] = inner.data[last * dim + k];
            }
            let moved_id = inner.ids[last];
            inner.ids[row] = moved_id;
            inner.pos.insert(moved_id, row); // 修正被搬动行的行号
        }
        inner.ids.pop();
        inner.data.truncate(last * dim);
        inner.pos.remove(&id);
        Ok(())
    }

    fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<(i64, f32)>> {
        self.check_dim(query.len())?;
        if top_k == 0 {
            return Ok(Vec::new());
        }
        let dim = self.dim;
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        if inner.ids.is_empty() {
            return Ok(Vec::new());
        }

        // 并行点积（单位向量的点积 == 余弦；与 semantic_search 同一数学）。
        let mut scored: Vec<(i64, f32)> = inner
            .ids
            .par_iter()
            .enumerate()
            .map(|(i, &id)| {
                let rowv = &inner.data[i * dim..i * dim + dim];
                let mut dot = 0.0f32;
                for k in 0..dim {
                    dot += query[k] * rowv[k].to_f32();
                }
                (id, dot.clamp(-1.0, 1.0))
            })
            .collect();

        // 降序取 top_k。次键 id 降序作确定性 tiebreaker（同分时稳定序）。
        scored.sort_unstable_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.0.cmp(&a.0))
        });
        scored.truncate(top_k);
        Ok(scored)
    }

    fn len(&self) -> usize {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .ids
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单位向量构造（dim=2，便于手算余弦）。
    fn store_2d() -> BruteForceStore {
        let s = BruteForceStore::new(2);
        s.upsert(1, &[1.0, 0.0]).unwrap(); // 东
        s.upsert(2, &[0.0, 1.0]).unwrap(); // 北
        s.upsert(3, &[0.707_106_77, 0.707_106_77]).unwrap(); // 东北 45°
        s
    }

    /// 检索按相似度降序：query=东 → 东(1.0) > 东北(0.707) > 北(0.0)。
    #[test]
    fn search_ranks_by_cosine_descending() {
        let s = store_2d();
        let top = s.search(&[1.0, 0.0], 3).unwrap();
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].0, 1, "最相似应是同向向量");
        assert_eq!(top[1].0, 3, "次相似 45°");
        assert_eq!(top[2].0, 2, "最不相似正交");
        assert!((top[0].1 - 1.0).abs() < 1e-2);
        assert!((top[2].1 - 0.0).abs() < 1e-2);

        // top_k 截断。
        assert_eq!(s.search(&[1.0, 0.0], 1).unwrap().len(), 1);
        // top_k=0 / 空库 → 空。
        assert!(s.search(&[1.0, 0.0], 0).unwrap().is_empty());
        assert!(BruteForceStore::new(2)
            .search(&[1.0, 0.0], 5)
            .unwrap()
            .is_empty());
    }

    /// 增量 upsert：同 id 覆盖（行数不变、向量更新影响排序）。
    #[test]
    fn upsert_same_id_overwrites_in_place() {
        let s = store_2d();
        assert_eq!(s.len(), 3);

        // 把 id=2 从「北」改成「东」——现在它应与 id=1 并列最相似。
        s.upsert(2, &[1.0, 0.0]).unwrap();
        assert_eq!(s.len(), 3, "覆盖不得新增行");

        let top = s.search(&[1.0, 0.0], 2).unwrap();
        let ids: Vec<i64> = top.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&1) && ids.contains(&2), "1 和 2 现都最相似");
        assert!(top[0].1 > 0.99 && top[1].1 > 0.99);
    }

    /// remove：swap-remove 正确删除且不破坏其余行的检索。
    #[test]
    fn remove_drops_vector_and_keeps_rest_searchable() {
        let s = store_2d();
        s.remove(1).unwrap(); // 删「东」
        assert_eq!(s.len(), 2);

        let top = s.search(&[1.0, 0.0], 3).unwrap();
        let ids: Vec<i64> = top.iter().map(|(id, _)| *id).collect();
        assert!(!ids.contains(&1), "已删向量不应出现");
        assert_eq!(top[0].0, 3, "剩余中 45° 最相似");
        assert_eq!(top.len(), 2);

        // 删不存在的 id → no-op，不报错、不改变。
        s.remove(999).unwrap();
        assert_eq!(s.len(), 2);
    }

    /// upsert_batch 默认实现等价逐条。
    #[test]
    fn upsert_batch_inserts_all() {
        let s = BruteForceStore::new(2);
        s.upsert_batch(&[(10, vec![1.0, 0.0]), (20, vec![0.0, 1.0])])
            .unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s.search(&[0.0, 1.0], 1).unwrap()[0].0, 20);
    }

    /// 维度不匹配 → Err（防异构架构向量混入）。
    #[test]
    fn dim_mismatch_is_rejected() {
        let s = BruteForceStore::new(2);
        assert!(
            s.upsert(1, &[1.0, 0.0, 0.0]).is_err(),
            "3 维写入 2 维 store 应拒"
        );
        assert!(s.search(&[1.0], 1).is_err(), "1 维查询 2 维 store 应拒");
    }
}
