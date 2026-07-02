// src-tauri/src/ai/search.rs
//! In-memory cosine similarity search over CLIP embeddings (C1).
//! 基于 CLIP 嵌入向量的内存余弦相似度搜索（C1）。
//!
//! Embeddings are loaded from SQLite ONCE into a resident, half-precision (f16)
//! contiguous buffer (`EmbeddingCache`) and reused across queries. Cosine similarity
//! is computed with rayon across all rows. This replaces the previous design that
//! re-read every embedding from SQLite (≈2GB at 1M items) on every single query.
//!
//! 嵌入向量从 SQLite **一次性**载入常驻的半精度（f16）连续缓冲区（`EmbeddingCache`）
//! 并跨查询复用；余弦相似度用 rayon 跨全部行并行计算。此设计取代了此前"每次查询都
//! 从 SQLite 重读全部嵌入向量（百万项约 2GB）"的实现。

use half::f16;
use rayon::prelude::*;
use tracing::{debug, info};

use crate::ai::clip::{encode_text, ClipTokenizer};
use crate::ai::profile::ModelProfile;
use crate::db::queries::get_all_embeddings;
use crate::error::{AppError, Result};
use crate::state::AppState;

/// Resident, half-precision embedding store kept in `AppState`.
/// `data` is row-major: row `i` occupies `data[i*dim .. (i+1)*dim]`, paired with `ids[i]`.
///
/// 常驻于 `AppState` 的半精度嵌入向量存储。
/// `data` 行主序：第 `i` 行占 `data[i*dim .. (i+1)*dim]`，与 `ids[i]` 配对。
pub struct EmbeddingCache {
    pub model_name: String,
    pub ids: Vec<i64>,
    pub data: Vec<f16>,
    pub dim: usize,
}

impl EmbeddingCache {
    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// Ensure the resident embedding cache is populated for `model_name`.
/// Loads from a read-pool connection (no write lock needed); cheap if already loaded.
///
/// 确保 `model_name` 的常驻嵌入缓存已填充。
/// 从读连接池加载（无需写锁）；若已加载则开销极小。
fn ensure_cache(state: &AppState, model_name: &str, dim: usize) -> Result<()> {
    // Fast path: already loaded for this model.
    // 快速路径：该模型已加载。
    {
        let guard = state.ai_embedding_cache.read().unwrap();
        if let Some(c) = guard.as_ref() {
            if c.model_name == model_name {
                return Ok(());
            }
        }
    }

    // Slow path: load all embeddings from SQLite and pack into f16.
    // 慢速路径：从 SQLite 加载全部嵌入向量并打包为 f16。
    let t0 = std::time::Instant::now();
    let raw = {
        let conn = state.db_read_pool.get().map_err(AppError::from)?;
        get_all_embeddings(&conn, model_name)?
    };

    let mut ids: Vec<i64> = Vec::with_capacity(raw.len());
    let mut data: Vec<f16> = Vec::with_capacity(raw.len() * dim);
    for (id, blob) in &raw {
        // Stored as little-endian f32 (4 bytes/elem). Skip any malformed row
        // (incl. vectors from a different model whose dim ≠ this profile's).
        // 以小端 f32 存储（每元素 4 字节）。跳过任何格式异常的行
        // （含维度 ≠ 本 profile 的其它模型向量）。
        if blob.len() != dim * 4 {
            continue;
        }
        ids.push(*id);
        for chunk in blob.chunks_exact(4) {
            let f = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            data.push(f16::from_f32(f));
        }
    }

    info!(
        "Embedding cache loaded: {} vectors ({:.1} MB f16) in {:.0}ms | 嵌入缓存已载入",
        ids.len(),
        (data.len() * 2) as f64 / (1024.0 * 1024.0),
        t0.elapsed().as_secs_f64() * 1000.0
    );

    let mut guard = state.ai_embedding_cache.write().unwrap();
    *guard = Some(EmbeddingCache {
        model_name: model_name.to_string(),
        ids,
        data,
        dim,
    });
    Ok(())
}

/// Perform semantic search: encode the query, compare against the resident cache
/// in parallel, persist the top-K results to `ai_search_results`.
///
/// 执行语义搜索：编码查询，与常驻缓存并行比较，把 Top-K 结果持久化到 `ai_search_results`。
pub fn semantic_search(
    state: &AppState,
    text_session_pool: &crate::ai::engine::SessionPool,
    tokenizer: &ClipTokenizer,
    query: &str,
    top_k: usize,
    profile: &ModelProfile,
) -> Result<usize> {
    info!("Semantic search started | 语义搜索开始: {:?}", query);

    // 1. Encode the text query into an `embed_dim` unit vector (must match the cache's model).
    // 1. 将文本查询编码为 `embed_dim` 维单位向量（须与缓存所用模型一致）。
    let query_vec = encode_text(text_session_pool, tokenizer, query, profile)?;

    // 2. Make sure the resident cache is loaded for THIS model (one-time disk read).
    // 2. 确保当前模型的常驻缓存已加载（一次性磁盘读取）。
    ensure_cache(state, &profile.id, profile.embed_dim)?;

    // 3. Score every embedding in parallel (cosine == dot for unit vectors).
    // 3. 并行为每个嵌入向量打分（单位向量的余弦 == 点积）。
    let mut scored: Vec<(i64, f32)> = {
        let guard = state.ai_embedding_cache.read().unwrap();
        let cache = guard
            .as_ref()
            .ok_or_else(|| AppError::Internal("embedding cache missing".into()))?;

        if cache.is_empty() {
            drop(guard);
            let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            conn.execute("DELETE FROM ai_search_results", [])
                .map_err(AppError::from)?;
            return Ok(0);
        }

        let dim = cache.dim;
        let q = &query_vec;
        cache
            .ids
            .par_iter()
            .enumerate()
            .map(|(i, &id)| {
                let row = &cache.data[i * dim..i * dim + dim];
                let mut dot = 0.0f32;
                for k in 0..dim {
                    dot += q[k] * row[k].to_f32();
                }
                (id, dot.clamp(-1.0, 1.0))
            })
            .collect()
    };

    debug!(
        "Scored {} embeddings | 已为 {} 个嵌入向量打分",
        scored.len(),
        scored.len()
    );

    // 4. Sort descending by similarity, take top-K.
    // 4. 按相似度降序排序，取 Top-K。
    scored.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    info!(
        "Cosine search done, top-{} results | 余弦搜索完成，前 {} 个结果",
        scored.len(),
        scored.len()
    );

    // 5. Persist results (briefly takes the write lock — NOT held during scoring).
    // 5. 持久化结果（短暂持有写锁 —— 打分期间不持有）。
    let mut conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
    let tx = conn.transaction().map_err(AppError::from)?;
    tx.execute("DELETE FROM ai_search_results", [])
        .map_err(AppError::from)?;
    {
        let mut stmt = tx
            .prepare("INSERT INTO ai_search_results (file_id, similarity) VALUES (?1, ?2)")
            .map_err(AppError::from)?;
        for (id, sim) in &scored {
            stmt.execute(rusqlite::params![id, sim])
                .map_err(AppError::from)?;
        }
    }
    tx.commit().map_err(AppError::from)?;

    Ok(scored.len())
}

/// Cosine similarity between two pre-normalised unit vectors.
/// 两个预归一化单位向量之间的余弦相似度。
///
/// For unit vectors: cosine_similarity = dot_product.
/// 对于单位向量：余弦相似度 = 点积。
#[inline]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(
        a.len(),
        b.len(),
        "Embedding dimension mismatch | 嵌入向量维度不匹配"
    );

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    // Clamp to [-1, 1] — both vectors should already be unit-normalised,
    // but floating point errors can push slightly outside.
    // 截断到 [-1, 1] — 两个向量应该已经是单位归一化的，
    // 但浮点误差可能会稍微超出范围。
    dot.clamp(-1.0, 1.0)
}
