// src-tauri/src/engine/traits.rs
// src-tauri/src/engine/traits.rs
//! `ImageEngine` trait definition as specified in § 7.1 of the implementation plan.
//! 实施计划第 7.1 节中指定的 `ImageEngine` trait 定义。

use std::path::Path;
use crate::error::AppError;

/// Resize strategy hint for `ImageEngine::decode()`.
/// `ImageEngine::decode()` 的缩放策略提示。
#[derive(Debug, Clone, Copy)]
pub enum ResizeHint {
    /// Scale so the **long** edge matches `target`, preserving aspect ratio (thumbnails).
    /// 按**长边**适配到 `target`，保持纵横比（缩略图用）。
    /// Example: 6000×4000 + LongEdge(300) → 300×200
    LongEdge(u32),
    /// Scale so the **short** edge matches `target`, preserving aspect ratio (CLIP preprocessing).
    /// 按**短边**适配到 `target`，保持纵横比（CLIP 预处理用）。
    /// Example: 6000×4000 + ShortEdge(224) → 336×224
    ShortEdge(u32),
}

/// A decoded image ready for processing (thumbnail generation, ThumbHash, etc.).
/// 准备处理（缩略图生成、ThumbHash 等）的解码图像。
pub struct DecodedImage {
    /// Raw RGBA pixel data.
    /// 原始 RGBA 像素数据。
    pub pixels: Vec<u8>,
    pub width:  u32,
    pub height: u32,
}

/// Trait that all image decoding backends must implement.
/// 所有图像解码后端必须实现的 Trait。
pub trait ImageEngine: Send + Sync {
    fn name(&self) -> &str;

    fn supported_formats(&self) -> &[&str];

    fn can_handle(&self, format: &str) -> bool {
        self.supported_formats().contains(&format)
    }

    /// Fully decode the image at `file_path` into RGBA pixels,
    /// optionally resizing according to the given `ResizeHint`.
    /// 将 `file_path` 处的图像完全解码为 RGBA 像素，
    /// 可选地根据给定的 `ResizeHint` 进行缩放。
    fn decode(&self, file_path: &Path, resize: Option<ResizeHint>) -> Result<DecodedImage, AppError>;

    /// Attempt to extract an embedded thumbnail (e.g. EXIF JPEG thumbnail).
    /// 尝试提取嵌入的缩略图（例如 EXIF JPEG 缩略图）。
    /// Returns `Ok(None)` if no embedded thumb is available.
    /// 如果没有可用的嵌入缩略图，则返回 `Ok(None)`。
    fn extract_embedded_thumb(&self, _file_path: &Path) -> Result<Option<Vec<u8>>, AppError> {
        Ok(None)
    }
}

