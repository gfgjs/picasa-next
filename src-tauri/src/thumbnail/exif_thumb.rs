// src-tauri/src/thumbnail/exif_thumb.rs
//! EXIF embedded thumbnail extraction (fast path).
//!
//! For JPEG files with an embedded EXIF thumbnail:
//!   1. Extract the embedded JPEG (~5ms)
//!   2. Validate size is ≥ target dimension
//!   3. Resize if needed and encode as WebP
//!
//! Falls back to full decode if the embedded thumb is too small or absent.

use std::path::Path;

use crate::engine::traits::ImageEngine;
use crate::error::{AppError, Result};

use crate::scanner::metadata::read_jpeg_orientation;

/// Attempt the EXIF fast path. Returns encoded WebP bytes, or `None` to fall back.
pub fn try_exif_thumb(
    engine: &dyn ImageEngine,
    path: &Path,
    target_size: u32,
) -> Option<Vec<u8>> {
    let embedded = engine.extract_embedded_thumb(path).ok()??;

    // Decode the embedded JPEG
    let mut img = image::load_from_memory(&embedded).ok()?;

    // The embedded EXIF thumbnail usually shares the physical orientation of the main image.
    // We must apply the EXIF orientation rotation before saving it as WebP,
    // because WebP won't carry the EXIF metadata to the browser.
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
    if w < target_size && h < target_size {
        return None;
    }

    // Resize and encode
    encode_webp_from_dynamic(img, target_size).ok()
}

/// Resize a `DynamicImage` to fit within `target_size` (longest edge) and encode as WebP.
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
pub fn encode_as_jpeg(rgba: &image::RgbaImage) -> Result<Vec<u8>> {
    let rgb = image::DynamicImage::ImageRgba8(rgba.clone()).to_rgb8();
    let mut buf = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85);
    encoder
        .encode_image(&image::DynamicImage::ImageRgb8(rgb))
        .map_err(|e| AppError::Engine(format!("JPEG encode failed: {e}")))?;
    Ok(buf)
}
