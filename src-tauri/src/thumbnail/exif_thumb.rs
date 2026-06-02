use std::path::Path;

use crate::engine::traits::{ImageEngine, DecodedImage};
use crate::error::{AppError, Result};

use crate::scanner::metadata::read_jpeg_orientation;
use crate::thumbnail::thumbhash::generate_thumbhash;

/// Attempt the EXIF fast path. Returns encoded WebP bytes and optional ThumbHash, or `None` to fall back.
/// 尝试 EXIF 快速路径。返回编码后的 WebP 字节和可选的 ThumbHash，如果需要回退则返回 `None`。
pub fn try_exif_thumb(
    engine: &dyn ImageEngine,
    path: &Path,
    target_size: u32,
) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
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

    // Only use if the embedded thumb is reasonably large (e.g. >= 120px on longest edge).
    // Standard camera EXIF thumbnails are typically 160x120 or 256x160.
    // If the target_size is 300, we accept upscaling these to avoid a massive full decode.
    // 仅当内嵌的缩略图足够大时（例如，最长边 >= 120px）才使用。
    // 标准相机的 EXIF 缩略图通常是 160x120 或 256x160。
    // 如果 target_size 是 300，我们允许放大它们以避免进行代价极高的全量解码。
    if w.max(h) < 120 {
        return None;
    }

    // Resize and encode
    // 调整大小并编码
    let (new_w, new_h) = if w >= h {
        let ratio = target_size as f32 / w as f32;
        (target_size, (h as f32 * ratio).round() as u32)
    } else {
        let ratio = target_size as f32 / h as f32;
        ((w as f32 * ratio).round() as u32, target_size)
    };

    let resized = img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();

    let decoded_for_hash = DecodedImage {
        pixels: rgba.clone().into_raw(),
        width: new_w,
        height: new_h,
    };
    let hash = generate_thumbhash(&decoded_for_hash).ok();

    let webp_bytes = encode_as_webp(&rgba, new_w, new_h).ok()?;

    Some((webp_bytes, hash))
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
