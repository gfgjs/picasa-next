// crates/picasa-next-ai-core/src/embedding.rs
//! 嵌入向量的纯字节序/归一化工具(T16 准备:自 clip.rs 外移)。
//!
//! 本模块**永远可用**(不在 `inference` feature 门内):host 在 T16 删 ort 后仍需
//! 读写 DB 里的向量字节(vector_store/聚类/搜索打分),这些与推理无关。
//! 路径兼容:clip.rs 以 `pub use` 原位再导出,既有 `clip::embedding_to_bytes` 引用不变。

/// L2-normalise a vector in-place (returns the input modified).
/// 就地 L2 归一化向量（返回修改后的输入）。
pub fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}

/// Convert a `Vec<f32>` embedding to raw bytes (little-endian IEEE 754).
/// 将 `Vec<f32>` 嵌入向量转换为原始字节（小端 IEEE 754）。
pub fn embedding_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for &f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Convert raw bytes back to a `Vec<f32>` embedding.
/// 将原始字节转换回 `Vec<f32>` 嵌入向量。
pub fn bytes_to_embedding(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
