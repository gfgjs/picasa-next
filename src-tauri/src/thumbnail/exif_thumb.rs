// src-tauri/src/thumbnail/exif_thumb.rs
//! EXIF embedded thumbnail extraction (fast path).
//! EXIF 内嵌缩略图提取（快速路径）。
//!
//! For JPEG files with an embedded EXIF thumbnail:
//! 对于带有内嵌 EXIF 缩略图的 JPEG 文件：
//!   1. Extract the embedded JPEG (~5ms)
//!   1. 提取内嵌的 JPEG（约 5ms）
//!   2. Validate size is ≥ target dimension
//!   2. 验证尺寸是否 ≥ 目标尺寸
//!   3. Resize if needed and encode as WebP
//!   3. 如果需要，进行调整大小并编码为 WebP
//!
//! Falls back to full decode if the embedded thumb is too small or absent.
//! 如果内嵌的缩略图太小或不存在，则回退到完整解码。

use std::path::Path;

use crate::engine::traits::ImageEngine;
use crate::error::{AppError, Result};

use crate::scanner::metadata::read_jpeg_orientation;

/// Attempt the EXIF fast path. Returns encoded WebP bytes, or `None` to fall back.
/// 尝试 EXIF 快速路径。返回编码后的 WebP 字节，如果需要回退则返回 `None`。
pub fn try_exif_thumb(
    engine: &dyn ImageEngine,
    path: &Path,
    target_size: u32,
) -> Option<Vec<u8>> {
    let embedded = engine.extract_embedded_thumb(path).ok()??;

    // Decode the embedded JPEG
    // 解码内嵌的 JPEG
    let mut img = image::load_from_memory(&embedded).ok()?;

    // The embedded EXIF thumbnail usually shares the physical orientation of the main image.
    // We must apply the EXIF orientation rotation before saving it as WebP,
    // because WebP won't carry the EXIF metadata to the browser.
    // 内嵌的 EXIF 缩略图通常与主图共享物理方向。
    // 在将其保存为 WebP 之前，我们必须应用 EXIF 方向旋转，
    // 因为 WebP 不会将 EXIF 元数据带到浏览器。
    let orientation = read_jpeg_orientation(path);
    img = match orientation {
        1 => img,
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    };

    let (w, h) = (img.width(), img.height());

    // Only use if the embedded thumb is large enough
    // 仅当内嵌的缩略图足够大时使用
    if w < target_size && h < target_size {
        return None;
    }

    // Resize and encode
    // 调整大小并编码
    encode_webp_from_dynamic(img, target_size).ok()
}

/// Resize a `DynamicImage` to fit within `target_size` (longest edge) and encode as WebP.
/// 调整 `DynamicImage` 的大小以适应 `target_size`（最长边），并编码为 WebP。
pub fn encode_webp_from_dynamic(
    img: image::DynamicImage,
    target_size: u32,
) -> Result<Vec<u8>> {
    let (w, h) = (img.width(), img.height());
    let (new_w, new_h) = if w >= h {
        let ratio = target_size as f32 / w as f32;
        (target_size, (h as f32 * ratio).round() as u32)
    } else {
        let ratio = target_size as f32 / h as f32;
        ((w as f32 * ratio).round() as u32, target_size)
    };

    let resized = img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();

    encode_as_webp(&rgba, new_w, new_h)
}

/// Encode RGBA pixel data as WebP using the `image` crate's WebP encoder.
/// Falls back to JPEG if WebP encoding fails.
/// 使用 `image` crate 的 WebP 编码器将 RGBA 像素数据编码为 WebP。
/// 如果 WebP 编码失败，则回退到 JPEG。
pub fn encode_as_webp(rgba: &image::RgbaImage, _w: u32, _h: u32) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    rgba.write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::WebP,
    )
    .map_err(|e| AppError::Engine(format!("WebP encode failed: {e}")))?;
    Ok(buf)
}

/// Encode as JPEG fallback (quality 85).
/// 作为 JPEG 回退进行编码（质量 85）。
pub fn encode_as_jpeg(rgba: &image::RgbaImage) -> Result<Vec<u8>> {
    let rgb = image::DynamicImage::ImageRgba8(rgba.clone()).to_rgb8();
    let mut buf = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85);
    encoder
        .encode_image(&image::DynamicImage::ImageRgb8(rgb))
        .map_err(|e| AppError::Engine(format!("JPEG encode failed: {e}")))?;
    Ok(buf)
}
