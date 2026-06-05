// src-tauri/src/ai/search.rs
//! In-memory cosine similarity search over CLIP embeddings.
//! 基于 CLIP 嵌入向量的内存余弦相似度搜索。
//!
//! Phase 4A: Load all embeddings from SQLite BLOB, compute cosine similarity
//! with the query vector, return Top-K results sorted by similarity.
//!
//! Phase 4A：从 SQLite BLOB 加载所有嵌入向量，与查询向量计算余弦相似度，
//! 返回按相似度排序的 Top-K 结果。

use std::sync::Arc;

use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use tracing::{debug, info};

use crate::ai::clip::{bytes_to_embedding, encode_text, ClipTokenizer, MODEL_NAME};
use crate::db::queries::get_all_embeddings;
use crate::error::{AppError, Result};

/// Perform semantic search: encode the query, compare against all stored
/// embeddings, return top-K results with similarity scores.
///
/// 执行语义搜索：编码查询，与所有存储的嵌入向量比较，返回带相似度分数的 Top-K 结果。
pub fn semantic_search(
    conn: &mut rusqlite::Connection,
    text_session: &Arc<std::sync::Mutex<ort::session::Session>>,
    tokenizer: &ClipTokenizer,
    query: &str,
    top_k: usize,
) -> Result<usize> {
    info!("Semantic search started | 语义搜索开始: {:?}", query);

    // 1. Encode the text query into a 512-d unit vector
    // 1. 将文本查询编码为 512-d 单位向量
    let query_vec = encode_text(text_session, tokenizer, query)?;

    // 2. Load all embeddings from SQLite
    // 2. 从 SQLite 加载所有嵌入向量
    let all_embeddings = get_all_embeddings(conn, MODEL_NAME)?;
    debug!(
        "Loaded {} embeddings for cosine search | 已加载 {} 个嵌入向量用于余弦搜索",
        all_embeddings.len(),
        all_embeddings.len()
    );

    if all_embeddings.is_empty() {
        // Clear previous results
        let tx = conn.transaction().map_err(AppError::from)?;
        tx.execute("DELETE FROM ai_search_results", []).map_err(AppError::from)?;
        tx.commit().map_err(AppError::from)?;
        return Ok(0);
    }

    // 3. Compute cosine similarity for every embedding
    // 3. 计算每个嵌入向量的余弦相似度
    let mut scored: Vec<(i64, f32)> = all_embeddings
        .into_iter()
        .map(|(item_id, blob)| {
            let emb = bytes_to_embedding(&blob);
            let sim = cosine_similarity(&query_vec, &emb);
            (item_id, sim)
        })
        .collect();

    // 4. Sort descending by similarity, take top-K
    // 4. 按相似度降序排序，取 Top-K
    scored.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    info!(
        "Cosine search done, top-{} results | 余弦搜索完成，前 {} 个结果",
        scored.len(),
        scored.len()
    );

    // 5. Store the results in the ai_search_results table
    // 5. 将结果存入 ai_search_results 表
    let tx = conn.transaction().map_err(AppError::from)?;
    tx.execute("DELETE FROM ai_search_results", []).map_err(AppError::from)?;

    {
        let mut stmt = tx.prepare("INSERT INTO ai_search_results (file_id, similarity) VALUES (?1, ?2)").map_err(AppError::from)?;
        for (id, sim) in &scored {
            stmt.execute(rusqlite::params![id, sim]).map_err(AppError::from)?;
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
    debug_assert_eq!(a.len(), b.len(), "Embedding dimension mismatch | 嵌入向量维度不匹配");

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    // Clamp to [-1, 1] — both vectors should already be unit-normalised,
    // but floating point errors can push slightly outside.
    // 截断到 [-1, 1] — 两个向量应该已经是单位归一化的，
    // 但浮点误差可能会稍微超出范围。
    dot.clamp(-1.0, 1.0)
}
