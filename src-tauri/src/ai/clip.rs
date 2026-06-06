// src-tauri/src/ai/clip.rs
//! Chinese-CLIP inference: image preprocessing and text tokenisation.
//! Chinese-CLIP 推理：图像预处理与文本分词。
//!
//! Model: eisneim/cn-clip_vit-b-16 (FP16 external-data format, ORT 1.26+)
//! 模型：eisneim/cn-clip_vit-b-16（FP16 外部数据格式，需要 ORT 1.26+）
//!
//! Image encoder: RGB → Resize(shortest_edge=224, Bicubic) → CenterCrop(224)
//!   → Normalise(CLIP mean/std) → CHW f32 tensor → "image" input
//!   → "unnorm_image_features" [1,512] → L2 normalise → 512-d unit vector
//!
//! Text encoder:  BERT tokeniser (vocab.txt, 21128 tokens) → token_ids i64[1,52]
//!   → "text" input → "unnorm_text_features" [1,512] → L2 normalise → 512-d unit vector
//!
//! # 踩坑记录 — 接入 CLIP 类多模态模型的经验总结
//!
//! 以下经验适用于所有 CLIP 变体（OpenAI CLIP / Chinese-CLIP / SigLIP / EVA-CLIP 等）
//! 和任何使用 ONNX Runtime 进行跨模态（图像-文本）语义搜索的场景。
//!
//! ## 坑1（致命）：vocab.txt 必须与模型的文本编码器严格匹配
//!
//! 这是我们遇到的**准确率低至 30% 的根因**。
//!
//! `eisneim/cn-clip_vit-b-16` 仓库附带的 `vocab.txt` 是 **英文 OpenAI CLIP**
//! 的 BPE 词表（~5594 tokens），而 Chinese-CLIP 的文本编码器是
//! `RoBERTa-wwm-ext-base-chinese`（= BERT），需要 `bert-base-chinese` 的
//! WordPiece 词表（**21128 tokens**）。
//!
//! 用错误的词表时，所有中文字符都被编码为 `[UNK]`：
//! ```text
//! 错误："小猫" → [CLS=2, UNK=1, UNK=1, SEP=3]
//!       "花朵" → [CLS=2, UNK=1, UNK=1, SEP=3]   // 完全相同！
//!       cosine_similarity = 1.0000                 // 无法区分任何查询
//!
//! 正确："小猫" → [CLS=101, 小=2207, 猫=4344, SEP=102]
//!       "花朵" → [CLS=101, 花=5709, 朵=3321, SEP=102]
//!       cosine_similarity = 0.8499                 // 合理的语义相似度
//! ```
//!
//! **检查方法**：加载 vocab 后检查 `vocab_size`，BERT 系列应 ≥ 20000。
//! **正确来源**：`OFA-Sys/chinese-clip-vit-base-patch16` 的 `vocab.txt`。
//! **通用规则**：永远从模型原始作者（OFA-Sys）获取 vocab，而不是第三方 ONNX 导出者。
//!
//! ## 坑2：图像预处理顺序不能搞反 — 先 Resize 后 CenterCrop
//!
//! CLIP 系列的标准预处理是：
//! ```text
//! 正确：Resize(shortest_edge=224, BICUBIC) → CenterCrop(224) → Normalize
//! 错误：CenterCrop(最大正方形)              → Resize(224)     → Normalize
//! ```
//!
//! 区别示例（1920×1080 照片）：
//! - 错误做法：先裁成 1080×1080（丢失左右各 420px），再缩到 224
//! - 正确做法：先缩到 398×224（保持比例），再裁成 224×224（只丢各 87px）
//!
//! 错误做法丢失大量语义信息，图像特征与训练时期望的特征偏离。
//!
//! **检查方法**：找一张宽幅全景照（如 16:9），分别用两种方式预处理后做推理，
//! 比较与 Python `cn_clip.clip.image_transform` 的输出差异。
//!
//! **通用规则**：查看模型仓库的 `preprocessor_config.json` 或 Python 推理代码，
//! 严格按照 `Resize → CenterCrop` 的顺序实现。
//!
//! ## 坑3：插值方法必须一致 — CatmullRom ≈ PIL.Image.BICUBIC
//!
//! CLIP 训练时使用 PIL 的 BICUBIC 插值。Rust `image` crate 中：
//! - `CatmullRom` ≈ Bicubic（推荐，与 PIL 最接近）
//! - `Lanczos3` = sinc 插值（高频响应不同，会产生微妙的特征偏移）
//!
//! **通用规则**：查看 `preprocessor_config.json` 的 `"resample"` 字段，
//! PIL resample 枚举：0=NEAREST, 1=LANCZOS, 2=BILINEAR, 3=BICUBIC。
//!
//! ## 坑4：tokenizers crate 不会自动添加 [CLS]/[SEP]
//!
//! 与 Python `transformers.BertTokenizer` 不同，Rust `tokenizers` crate 的
//! `encode(text, true)` 中 `true` 只是个提示，如果没有配置 PostProcessor，
//! 它不知道要添加什么特殊 token。
//!
//! **必须手动配置** `TemplateProcessing` 或 `BertProcessing`：
//! ```rust
//! TemplateProcessing::builder()
//!     .try_single("[CLS]:0 $A:0 [SEP]:0")
//!     .special_tokens(vec![("[CLS]", 101), ("[SEP]", 102)])
//! ```
//!
//! **检查方法**：编码 "小猫" 后打印 token IDs，确认首位是 101、末位是 102。
//!
//! ## 坑5：手动截断会丢失 [SEP] — 使用 TruncationParams
//!
//! 如果在 `encode()` 返回后手动 `ids[..MAX_SEQ_LEN]` 截断，
//! 超长文本的最后一个 token（[SEP]=102）会被截掉，破坏 BERT 的句边界语义。
//!
//! **正确做法**：配置 `tokenizer.with_truncation(Some(TruncationParams { ... }))`，
//! tokenizers crate 会智能截断内容 token 并保留 [CLS] 和 [SEP]。
//! 同时配置 `with_padding(Some(PaddingParams { ... }))` 让 crate 自动填充到固定长度。
//!
//! ## 坑6：ONNX 模型输出可能未归一化
//!
//! 部分导出的 ONNX 模型输出的是**未归一化**的原始特征向量（如 `unnorm_*`），
//! 必须在推理后手动 L2 归一化才能用于余弦相似度搜索。
//! 也有些模型（如旧版 cn-clip-vit-b16-*.onnx）输出已归一化的向量。
//!
//! **检查方法**：看输出 tensor 名称是否有 `unnorm` 前缀，
//! 或检查输出向量的 L2 范数是否 ≈ 1.0。
//!
//! ## 诊断技巧：写 Python 对比脚本
//!
//! 当搜索准确率不符合预期时，最有效的诊断方法是写一个 Python 脚本：
//! 1. 用 `tokenizers` Python 库（与 Rust crate 同源）加载 vocab 并编码
//! 2. 用 `onnxruntime` 加载同一个 ONNX 模型并推理
//! 3. 比较文本间的余弦相似度是否合理
//! 4. 关键判据：语义不同的文本（如"小猫"vs"花朵"）相似度不应该 > 0.95
//!
//! 如果 Python 也产出错误结果，说明是数据问题（vocab/模型文件）；
//! 如果 Python 正确但 Rust 不对，说明是代码问题（预处理/tokenizer 配置）。

use std::path::Path;
use std::sync::Arc;

use image::DynamicImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::Tensor;
use std::sync::Mutex;
use tracing::debug;

use crate::engine::traits::DecodedImage;
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
    session_pool: &crate::ai::engine::SessionPool,
    image_bytes: &[u8],
) -> Result<Vec<f32>> {
    let img = image::load_from_memory(image_bytes)
        .map_err(|e| AppError::Engine(format!("Image decode failed | 图像解码失败: {e}")))?;

    encode_image(session_pool, &img)
}

/// Encode a `DynamicImage` into a 512-d unit vector.
/// 将 `DynamicImage` 编码为 512-d 单位向量。
pub fn encode_image(
    session_pool: &crate::ai::engine::SessionPool,
    img: &DynamicImage,
) -> Result<Vec<f32>> {
    let array = preprocess_image(img);
    run_image_inference(session_pool, array)
}

/// Encode a pre-decoded image (RGBA u8 pixels) into a 512-d unit vector.
/// 将预解码的图像（RGBA u8 像素）编码为 512-d 单位向量。
///
/// The image should already be resized to `short_edge=224` by the `ImageEngine`.
/// This function only performs lightweight CenterCrop + Normalize + CHW conversion
/// on the small (~336×224) image before running CLIP inference.
///
/// 图像应已由 `ImageEngine` 缩放至 `short_edge=224`。
/// 本函数仅在小图（约 336×224）上执行轻量的 CenterCrop + 归一化 + CHW 转换，
/// 然后运行 CLIP 推理。
pub fn encode_image_from_decoded(
    session_pool: &crate::ai::engine::SessionPool,
    decoded: &DecodedImage,
) -> Result<Vec<f32>> {
    let array = preprocess_decoded(decoded);
    run_image_inference(session_pool, array)
}

/// Run CLIP image encoder inference on a preprocessed [1,3,224,224] f32 tensor.
/// 在预处理后的 [1,3,224,224] f32 张量上运行 CLIP 图像编码器推理。
fn run_image_inference(
    session_pool: &crate::ai::engine::SessionPool,
    array: Array4<f32>,
) -> Result<Vec<f32>> {
    let shape: [i64; 4] = [1, 3, IMG_SIZE as i64, IMG_SIZE as i64];
    let (flat_data, _offset) = array.into_raw_vec_and_offset();
    let tensor = Tensor::from_array((shape, flat_data))
        .map_err(|e| AppError::Ai(format!("Build image tensor failed | 构建图像张量失败: {e}")))?;

    let mut guard = session_pool.get();
    let outputs = guard
        .run(ort::inputs!["pixel_values" => tensor])
        .map_err(|e| AppError::Ai(format!("CLIP image inference failed | CLIP 图像推理失败: {e}")))?;

    // Output: "unnorm_image_features" [1, 512] — model does not L2-normalise, we do it here
    // 输出："unnorm_image_features" [1, 512] — 模型不做 L2 归一化，在此处手动归一化
    let raw = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::Ai(format!("Extract image tensor failed | 提取图像张量失败: {e}")))?;

    let (_shape, raw_slice) = raw;
    let embedding: Vec<f32> = raw_slice.iter().copied().collect();
    Ok(l2_normalize(embedding))
}

/// Preprocess an image to a [1, 3, 224, 224] f32 array.
/// 将图像预处理为 [1, 3, 224, 224] f32 数组。
///
/// Pipeline matches official Chinese-CLIP (cn_clip):
///   RGB → Resize(shortest_edge=224, Bicubic) → CenterCrop(224) → Normalize
///
/// 流水线与官方 Chinese-CLIP (cn_clip) 一致：
///   RGB → 按短边缩放到224(Bicubic) → 中心裁剪224 → 归一化
///
/// 【关键修复】旧代码先 CenterCrop(最大正方形) 再 Resize(224, Lanczos3)，
/// 导致宽幅图像丢失大量语义内容（如 1920×1080 图片会先裁掉左右各 420px）。
/// 正确做法是先按短边等比缩放，再裁剪，最大限度保留图像内容。
fn preprocess_image(img: &DynamicImage) -> Array4<f32> {
    // 1. Convert to RGB
    // 1. 转换为 RGB
    let rgb = img.to_rgb8();

    // 2. Resize: scale shortest edge to 224, keep aspect ratio.
    //    Use CatmullRom (= Bicubic interpolation, matching PIL.Image.BICUBIC).
    // 2. 按短边缩放到 224，保持长宽比。
    //    使用 CatmullRom（= Bicubic 插值，匹配 PIL.Image.BICUBIC）。
    let (w, h) = (rgb.width(), rgb.height());
    let short_edge = w.min(h) as f32;
    let scale = IMG_SIZE as f32 / short_edge;
    let new_w = (w as f32 * scale).round() as u32;
    let new_h = (h as f32 * scale).round() as u32;
    let resized = image::imageops::resize(
        &rgb, new_w, new_h,
        image::imageops::FilterType::CatmullRom,
    );

    // 3. CenterCrop to 224×224
    // 3. 中心裁剪到 224×224
    let cx = (resized.width() - IMG_SIZE) / 2;
    let cy = (resized.height() - IMG_SIZE) / 2;
    let cropped = image::imageops::crop_imm(&resized, cx, cy, IMG_SIZE, IMG_SIZE)
        .to_image();

    // 4. HWC → CHW, normalise using CLIP mean/std
    // 4. HWC → CHW，使用 CLIP 均值/标准差归一化
    let mut tensor = Array4::<f32>::zeros((1, 3, IMG_SIZE as usize, IMG_SIZE as usize));
    let iw = IMG_SIZE as usize;
    let ih = IMG_SIZE as usize;

    for y in 0..ih {
        for x in 0..iw {
            let px = cropped.get_pixel(x as u32, y as u32);
            for c in 0..3usize {
                let val = px[c] as f32 / 255.0;
                tensor[[0, c, y, x]] = (val - MEAN[c]) / STD[c];
            }
        }
    }

    tensor
}

/// Lightweight preprocessing for a pre-resized `DecodedImage` (RGBA u8 pixels).
/// 对预缩放的 `DecodedImage`（RGBA u8 像素）进行轻量预处理。
///
/// The image is expected to have `short_edge = 224` (e.g. 336×224 or 224×224).
/// Performs: CenterCrop(224×224) → RGBA→RGB → /255 → CLIP Normalize → HWC→CHW.
///
/// 图像预期 `短边 = 224`（如 336×224 或 224×224）。
/// 执行：CenterCrop(224×224) → RGBA→RGB → /255 → CLIP 归一化 → HWC→CHW。
fn preprocess_decoded(decoded: &DecodedImage) -> Array4<f32> {
    let (w, h) = (decoded.width as usize, decoded.height as usize);
    let crop_size = IMG_SIZE as usize;

    // CenterCrop: compute offsets (saturating to 0 for images exactly 224)
    // CenterCrop：计算偏移量（对于恰好 224 的图像饱和到 0）
    let cx = w.saturating_sub(crop_size) / 2;
    let cy = h.saturating_sub(crop_size) / 2;

    let mut tensor = Array4::<f32>::zeros((1, 3, crop_size, crop_size));

    for y in 0..crop_size {
        for x in 0..crop_size {
            let src_x = cx + x;
            let src_y = cy + y;
            // RGBA stride: 4 bytes per pixel
            let idx = (src_y * w + src_x) * 4;
            for c in 0..3usize {
                let val = decoded.pixels[idx + c] as f32 / 255.0;
                tensor[[0, c, y, x]] = (val - MEAN[c]) / STD[c];
            }
        }
    }

    tensor
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
        use tokenizers::processors::template::TemplateProcessing;

        let wp = WordPiece::from_file(vocab_path.to_str().unwrap_or("vocab.txt"))
            .unk_token("[UNK]".to_string())
            .max_input_chars_per_word(100)
            .build()
            .map_err(|e| AppError::Ai(format!("Tokenizer build failed | 分词器构建失败: {e}")))?;

        let mut tokenizer = tokenizers::Tokenizer::new(wp);
        
        // ── Normalizer & PreTokenizer ───────────────────────────────────────
        // ── 归一化器与预分词器 ──────────────────────────────────────────────
        // Chinese-CLIP uses BERT tokenizer which requires lowercase and CJK char handling.
        tokenizer.with_normalizer(Some(
            BertNormalizer::new(true, true, Some(true), true)
        ));
        
        tokenizer.with_pre_tokenizer(Some(
            BertPreTokenizer
        ));

        // ── Vocab sanity check ──────────────────────────────────────────────
        // ── 词表完整性检查 ──────────────────────────────────────────────────
        //
        // Chinese-CLIP uses bert-base-chinese vocab with 21128 tokens.
        // The eisneim/cn-clip_vit-b-16 HuggingFace repo ships the WRONG
        // vocab.txt (English CLIP, ~5594 tokens). If loaded, all Chinese
        // characters become [UNK], making every query produce identical
        // embeddings and destroying search accuracy.
        //
        // 【关键防护】Chinese-CLIP 使用 bert-base-chinese 词表（21128 个 token）。
        //   eisneim/cn-clip_vit-b-16 仓库附带的 vocab.txt 是英文 CLIP 的
        //   （约 5594 个 token），如果误用，所有中文字符都会变成 [UNK]，
        //   导致每个查询产生完全相同的嵌入向量，搜索准确率降为零。
        //   正确的 vocab.txt 应从 OFA-Sys/chinese-clip-vit-base-patch16 获取。
        let vocab_size = tokenizer.get_vocab_size(true);
        if vocab_size < 10000 {
            tracing::error!(
                "vocab.txt has only {} tokens — expected ~21128 (bert-base-chinese). \
                 This is likely the wrong vocab file (English CLIP). \
                 Download the correct one from OFA-Sys/chinese-clip-vit-base-patch16. \
                 | vocab.txt 仅有 {} 个 token，预期约 21128。\
                 可能使用了错误的词表文件（英文 CLIP）。\
                 请从 OFA-Sys/chinese-clip-vit-base-patch16 下载正确的词表。",
                vocab_size, vocab_size
            );
            return Err(AppError::Ai(format!(
                "Wrong vocab.txt: only {} tokens, expected ~21128. \
                 Please replace with bert-base-chinese vocab from \
                 OFA-Sys/chinese-clip-vit-base-patch16. \
                 | 错误的 vocab.txt：仅 {} 个 token，预期约 21128。\
                 请用 OFA-Sys/chinese-clip-vit-base-patch16 的正确词表替换。",
                vocab_size, vocab_size
            )));
        }
        debug!("Loaded vocab with {} tokens | 加载了 {} 个 token 的词表", vocab_size, vocab_size);

        // ── Post-processor: insert [CLS] at start and [SEP] at end ──────────
        // ── 后处理器：在序列前插入 [CLS]，末尾插入 [SEP] ──────────────────────
        //
        // 【关键修复·上轮】没有 TemplateProcessing，tokenizers crate 即使传
        //   add_special_tokens=true 也不知道应该添加什么 token。
        //   Chinese-CLIP 的 BERT 文本编码器依赖 [CLS]（id=101）的输出
        //   作为整句语义表示。缺少它会导致文本嵌入语义错乱，搜索准确率骤降。
        let cls_id = 101u32; // [CLS] in Chinese-CLIP vocab
        let sep_id = 102u32; // [SEP] in Chinese-CLIP vocab
        let post_processor = TemplateProcessing::builder()
            .try_single("[CLS]:0 $A:0 [SEP]:0")
            .map_err(|e| AppError::Ai(format!("Template single failed: {e}")))?
            .try_pair("[CLS]:0 $A:0 [SEP]:0 $B:1 [SEP]:1")
            .map_err(|e| AppError::Ai(format!("Template pair failed: {e}")))?
            .special_tokens(vec![
                (String::from("[CLS]"), cls_id),
                (String::from("[SEP]"), sep_id),
            ])
            .build()
            .map_err(|e| AppError::Ai(format!("Post-processor build failed | 后处理器构建失败: {e}")))?;
        tokenizer.with_post_processor(Some(post_processor));

        // ── Truncation: ensure [SEP] is preserved for long text ─────────────
        // ── 截断配置：确保长文本也能保留 [SEP] ──────────────────────────────
        //
        // 【关键修复】旧代码在 encode() 中手动截断 raw_ids[..MAX_SEQ_LEN]，
        //   这会在超长文本时丢弃末尾的 [SEP]（id=102），破坏 BERT 的句边界语义。
        //   配置 Truncation 后，tokenizers crate 会自动在截断后保留 [SEP]。
        let truncation = tokenizers::TruncationParams {
            max_length: MAX_SEQ_LEN,
            strategy: tokenizers::TruncationStrategy::LongestFirst,
            ..Default::default()
        };
        tokenizer.with_truncation(Some(truncation))
            .map_err(|e| AppError::Ai(format!("Truncation config failed | 截断配置失败: {e}")))?;

        // ── Padding: pad to MAX_SEQ_LEN with 0 (= [PAD]) ────────────────────
        // ── 填充：用 0（= [PAD]）填充到 MAX_SEQ_LEN ──────────────────────────
        tokenizer.with_padding(Some(tokenizers::PaddingParams {
            strategy: tokenizers::PaddingStrategy::Fixed(MAX_SEQ_LEN),
            pad_id: 0,
            pad_token: String::from("[PAD]"),
            ..Default::default()
        }));

        Ok(Self { inner: tokenizer })
    }

    /// Tokenise text and return (input_ids, attention_mask, token_type_ids) as i64 vectors.
    /// 将文本分词，返回 (input_ids, attention_mask, token_type_ids) 的 i64 向量。
    ///
    /// Truncation and padding are handled by the tokenizers crate via the
    /// TruncationParams and PaddingParams configured in `from_vocab()`.
    /// This guarantees [CLS] and [SEP] are always present and the output
    /// length is exactly MAX_SEQ_LEN.
    ///
    /// 截断和填充由 tokenizers crate 通过 `from_vocab()` 中配置的
    /// TruncationParams 和 PaddingParams 自动处理。
    /// 这保证 [CLS] 和 [SEP] 始终存在，输出长度恰好为 MAX_SEQ_LEN。
    pub fn encode(&self, text: &str) -> Result<(Vec<i64>, Vec<i64>, Vec<i64>)> {
        let encoding = self.inner
            .encode(text, true)
            .map_err(|e| AppError::Ai(format!("Tokenize failed | 分词失败: {e}")))?;

        // tokenizers crate has already truncated to MAX_SEQ_LEN and padded with [PAD](0).
        // tokenizers crate 已截断到 MAX_SEQ_LEN 并用 [PAD](0) 填充。
        let ids:   Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let mask:  Vec<i64> = encoding.get_attention_mask().iter().map(|&x| x as i64).collect();
        let types: Vec<i64> = encoding.get_type_ids().iter().map(|&x| x as i64).collect();

        Ok((ids, mask, types))
    }
}

/// Encode a text query into a 512-d unit vector using the CLIP text encoder.
/// 使用 CLIP 文本编码器将文本查询编码为 512-d 单位向量。
pub fn encode_text(
    session_pool: &crate::ai::engine::SessionPool,
    tokenizer: &ClipTokenizer,
    text: &str,
) -> Result<Vec<f32>> {
    debug!("Encoding text query | 正在编码文本查询: {:?}", text);

    let (ids, mask, types) = tokenizer.encode(text)?;

    // ── Diagnostic: print token IDs for debugging ───────────────────────
    // ── 诊断：打印 token IDs 用于调试 ───────────────────────────────────
    let non_pad: Vec<i64> = ids.iter().copied().filter(|&x| x != 0).collect();
    debug!(
        "Token IDs for {:?}: {:?} (total={}, non-pad={})",
        text, non_pad, ids.len(), non_pad.len()
    );

    let shape = [1i64, MAX_SEQ_LEN as i64];
    let input_ids = Tensor::from_array((shape, ids))
        .map_err(|e| AppError::Ai(e.to_string()))?;
    let attention_mask = Tensor::from_array((shape, mask))
        .map_err(|e| AppError::Ai(e.to_string()))?;
    let token_type_ids = Tensor::from_array((shape, types))
        .map_err(|e| AppError::Ai(e.to_string()))?;

    let mut guard = session_pool.get();
    let outputs = guard
        .run(ort::inputs![
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
            "token_type_ids" => token_type_ids
        ])
        .map_err(|e| AppError::Ai(format!("CLIP text inference failed | CLIP 文本推理失败: {e}")))?;

    // Output: "text_features" [1, 512]
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
