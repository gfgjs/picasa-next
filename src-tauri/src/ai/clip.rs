// src-tauri/src/ai/clip.rs
//! Chinese-CLIP inference: image preprocessing and text tokenisation.
//! Chinese-CLIP 推理：图像预处理与文本分词。
//!
//! Image encoder: RGB → resize 224×224 (bicubic centre-crop) → normalise → CHW tensor
//!   → 512-d normalised vector
//! 图像编码器：RGB → 缩放 224×224（双三次中心裁剪）→ 归一化 → CHW 张量 → 512-d 归一化向量
//!
//! Text encoder:  BERT tokeniser (vocab.txt) → input_ids / attention_mask / token_type_ids
//!   → 512-d normalised vector
//! 文本编码器：BERT 分词器（vocab.txt）→ input_ids / attention_mask / token_type_ids
//!   → 512-d 归一化向量

use std::path::Path;
use std::sync::Arc;

use image::{DynamicImage, RgbImage};
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;
use std::sync::Mutex;
use tracing::debug;

use crate::error::{AppError, Result};

// ── Constants ─────────────────────────────────────────────────────────────────
// ── 常量 ─────────────────────────────────────────────────────────────────────

/// Chinese-CLIP ViT-B/16 image size.
/// Chinese-CLIP ViT-B/16 图像尺寸。
const IMG_SIZE: u32 = 224;

/// Embedding dimension for Chinese-CLIP ViT-B/16.
/// Chinese-CLIP ViT-B/16 的嵌入维度。
pub const EMBED_DIM: usize = 512;

/// Maximum sequence length for text tokens.
/// 文本 token 的最大序列长度。
pub const MAX_SEQ_LEN: usize = 52;

/// Model name used as key in `ai_embeddings`.
/// 在 `ai_embeddings` 中用作键的模型名称。
pub const MODEL_NAME: &str = "cn-clip-vit-b16";

/// ImageNet-style normalisation parameters used by Chinese-CLIP.
/// Chinese-CLIP 使用的 ImageNet 风格归一化参数。
const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
const STD:  [f32; 3] = [0.26862954, 0.26130258, 0.27577711];

// ── Image encoding ────────────────────────────────────────────────────────────
// ── 图像编码 ────────────────────────────────────────────────────────────────

/// Encode a JPEG/PNG thumbnail byte slice into a 512-d unit vector.
/// 将 JPEG/PNG 缩略图字节切片编码为 512-d 单位向量。
pub fn encode_image_bytes(
    session: &Arc<Mutex<Session>>,
    image_bytes: &[u8],
) -> Result<Vec<f32>> {
    let img = image::load_from_memory(image_bytes)
        .map_err(|e| AppError::Engine(format!("Image decode failed | 图像解码失败: {e}")))?;

    encode_image(session, &img)
}

/// Encode a `DynamicImage` into a 512-d unit vector.
/// 将 `DynamicImage` 编码为 512-d 单位向量。
pub fn encode_image(
    session: &Arc<Mutex<Session>>,
    img: &DynamicImage,
) -> Result<Vec<f32>> {
    let array = preprocess_image(img);

    // Convert ndarray to ort Tensor
    // 将 ndarray 转换为 ort Tensor
    let shape: [i64; 4] = [1, 3, IMG_SIZE as i64, IMG_SIZE as i64];
    let (flat_data, _offset) = array.into_raw_vec_and_offset();
    let flat_data: Vec<f32> = flat_data;
    let tensor = Tensor::from_array((shape, flat_data))
        .map_err(|e| AppError::Ai(format!("Build image tensor failed | 构建图像张量失败: {e}")))?;

    let mut guard = session.lock().unwrap();
    let outputs = guard
        .run(ort::inputs!["pixel_values" => tensor])
        .map_err(|e| AppError::Ai(format!("CLIP image inference failed | CLIP 图像推理失败: {e}")))?;

    // The CLIP image encoder outputs a single tensor
    // CLIP 图像编码器输出单个张量
    let raw = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Ai(format!("Extract image tensor failed | 提取图像张量失败: {e}")))?;

    let (_shape, raw_slice) = raw;
    let embedding: Vec<f32> = raw_slice.iter().copied().collect();
    Ok(l2_normalize(embedding))
}

/// Preprocess an image to a [1, 3, 224, 224] f32 array.
/// 将图像预处理为 [1, 3, 224, 224] f32 数组。
fn preprocess_image(img: &DynamicImage) -> Array4<f32> {
    // 1. Convert to RGB
    // 1. 转换为 RGB
    let rgb = img.to_rgb8();

    // 2. Centre-crop to square
    // 2. 中心裁剪为正方形
    let cropped = centre_crop_square(rgb);

    // 3. Resize to 224×224 (Lanczos3)
    // 3. 缩放至 224×224（Lanczos3）
    let resized = image::imageops::resize(&cropped, IMG_SIZE, IMG_SIZE, image::imageops::FilterType::Lanczos3);

    // 4. HWC → CHW, normalise using CLIP mean/std
    // 4. HWC → CHW，使用 CLIP 均值/标准差归一化
    let mut tensor = Array4::<f32>::zeros((1, 3, IMG_SIZE as usize, IMG_SIZE as usize));
    let w = IMG_SIZE as usize;
    let h = IMG_SIZE as usize;

    for y in 0..h {
        for x in 0..w {
            let px = resized.get_pixel(x as u32, y as u32);
            for c in 0..3usize {
                let val = px[c] as f32 / 255.0;
                tensor[[0, c, y, x]] = (val - MEAN[c]) / STD[c];
            }
        }
    }

    tensor
}

/// Centre-crop an RgbImage to a square (largest possible).
/// 将 RgbImage 中心裁剪为正方形（尽可能大）。
fn centre_crop_square(img: RgbImage) -> RgbImage {
    let w = img.width();
    let h = img.height();
    let size = w.min(h);
    let x = (w - size) / 2;
    let y = (h - size) / 2;
    image::imageops::crop_imm(&img, x, y, size, size).to_image()
}

// ── Text encoding ─────────────────────────────────────────────────────────────
// ── 文本编码 ─────────────────────────────────────────────────────────────────

/// Simple BERT WordPiece tokeniser for Chinese-CLIP.
/// Chinese-CLIP 的简易 BERT WordPiece 分词器。
pub struct ClipTokenizer {
    inner: tokenizers::Tokenizer,
}

impl ClipTokenizer {
    /// Load the tokenizer from a `vocab.txt` file.
    /// 从 `vocab.txt` 文件加载分词器。
    pub fn from_vocab(vocab_path: &Path) -> Result<Self> {
        use tokenizers::models::wordpiece::WordPiece;
        use tokenizers::normalizers::bert::BertNormalizer;
        use tokenizers::pre_tokenizers::bert::BertPreTokenizer;

        let wp = WordPiece::from_file(vocab_path.to_str().unwrap_or("vocab.txt"))
            .unk_token("[UNK]".to_string())
            .max_input_chars_per_word(100)
            .build()
            .map_err(|e| AppError::Ai(format!("Tokenizer build failed | 分词器构建失败: {e}")))?;

        let mut tokenizer = tokenizers::Tokenizer::new(wp);
        tokenizer.with_normalizer(Some(BertNormalizer::default()));
        tokenizer.with_pre_tokenizer(Some(BertPreTokenizer));

        Ok(Self { inner: tokenizer })
    }

    /// Tokenise text and return (input_ids, attention_mask, token_type_ids) as i64 vectors.
    /// 将文本分词，返回 (input_ids, attention_mask, token_type_ids) 的 i64 向量。
    pub fn encode(&self, text: &str) -> Result<(Vec<i64>, Vec<i64>, Vec<i64>)> {
        let encoding = self.inner
            .encode(text, true)
            .map_err(|e| AppError::Ai(format!("Tokenize failed | 分词失败: {e}")))?;

        let raw_ids  = encoding.get_ids();
        let raw_mask = encoding.get_attention_mask();
        let raw_type = encoding.get_type_ids();

        // Truncate or pad to MAX_SEQ_LEN
        // 截断或填充至 MAX_SEQ_LEN
        let mut ids   = vec![0i64; MAX_SEQ_LEN];
        let mut mask  = vec![0i64; MAX_SEQ_LEN];
        let mut types = vec![0i64; MAX_SEQ_LEN];

        let len = raw_ids.len().min(MAX_SEQ_LEN);
        for i in 0..len {
            ids[i]   = raw_ids[i] as i64;
            mask[i]  = raw_mask[i] as i64;
            types[i] = raw_type[i] as i64;
        }

        Ok((ids, mask, types))
    }
}

/// Encode a text query into a 512-d unit vector using the CLIP text encoder.
/// 使用 CLIP 文本编码器将文本查询编码为 512-d 单位向量。
pub fn encode_text(
    session: &Arc<Mutex<Session>>,
    tokenizer: &ClipTokenizer,
    text: &str,
) -> Result<Vec<f32>> {
    debug!("Encoding text query | 正在编码文本查询: {:?}", text);

    let (ids, mask, types) = tokenizer.encode(text)?;

    // Build [1, MAX_SEQ_LEN] i64 Tensors
    // 构建 [1, MAX_SEQ_LEN] i64 Tensor
    let shape = [1i64, MAX_SEQ_LEN as i64];
    let input_ids_tensor = Tensor::from_array((shape, ids))
        .map_err(|e| AppError::Ai(e.to_string()))?;
    let attention_mask_tensor = Tensor::from_array((shape, mask))
        .map_err(|e| AppError::Ai(e.to_string()))?;
    let token_type_ids_tensor = Tensor::from_array((shape, types))
        .map_err(|e| AppError::Ai(e.to_string()))?;

    let mut guard = session.lock().unwrap();
    let outputs = guard
        .run(ort::inputs![
            "input_ids"      => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor
        ])
        .map_err(|e| AppError::Ai(format!("CLIP text inference failed | CLIP 文本推理失败: {e}")))?;

    let raw = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Ai(format!("Extract text tensor failed | 提取文本张量失败: {e}")))?;

    let (_shape, raw_slice) = raw;
    let embedding: Vec<f32> = raw_slice.iter().copied().collect();
    Ok(l2_normalize(embedding))
}

// ── Vector utilities ──────────────────────────────────────────────────────────
// ── 向量工具函数 ──────────────────────────────────────────────────────────────

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
