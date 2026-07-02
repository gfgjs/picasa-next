use std::path::Path;

use crate::engine::traits::{DecodedImage, ImageEngine};
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

    // 质量守门（Part3 Q4 / §3.1.2）：内嵌图不足目标档位时是否采用，按档位分级——
    // 大档位（480/960）严格拒绝不足档位的内嵌图、回退全解码（避免 160→960 的数倍上采样劣化）；
    // 小档位（120/240）维持宽通道（标准 160×120/256×160 内嵌图通常已够，选片速度优先、容轻度放大）。
    if !embedded_thumb_acceptable(w.max(h), target_size) {
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

/// 内嵌 EXIF 缩略图是否够格直接采用（否则 `try_exif_thumb` 返回 None → 回退全解码）。
///
/// 按目标档位分级（Part3 Q4 / §3.1.2）：
/// - **大档位（480/960）**：严格——`max_edge` 不足档位即拒，避免把 160×120 上采样到 960（数倍劣化）。
/// - **小档位（120/240）**：宽松——`max_edge >= 120` 即可（标准内嵌图通常已够，选片速度优先，容轻度放大）。
///
/// `target_size` 经 `snap_to_tier` 归一到 [120,240,480,960] 再判级（幂等：已是档位则不变）。
fn embedded_thumb_acceptable(max_edge: u32, target_size: u32) -> bool {
    let tier = crate::thumbnail::generator::snap_to_tier(target_size);
    // 小档位维持原 120px 下限；大档位要求内嵌图至少达档位（不放大）。
    let min_edge = if tier >= 480 { tier } else { 120 };
    max_edge >= min_edge
}

/// Resize a `DynamicImage` to fit within `target_size` (longest edge) and encode as WebP.
/// 调整 `DynamicImage` 的大小以适应 `target_size`（最长边），并编码为 WebP。
pub fn encode_webp_from_dynamic(img: image::DynamicImage, target_size: u32) -> Result<Vec<u8>> {
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
    .map_err(|e| AppError::Internal(format!("WebP encode failed: {e}")))?;
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
        .map_err(|e| AppError::Internal(format!("JPEG encode failed: {e}")))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::embedded_thumb_acceptable;

    /// 大档位（480/960）严格：标准内嵌图（160×120/256×160，max_edge≤256）不足档位 → 拒绝回退全解码。
    #[test]
    fn large_tier_rejects_undersized_embedded() {
        // tier=480：256<480 拒；恰达/超档位采用。
        assert!(!embedded_thumb_acceptable(256, 480));
        assert!(embedded_thumb_acceptable(480, 480));
        assert!(embedded_thumb_acceptable(512, 480));
        // tier=960：800<960 拒；≥960 采用。
        assert!(!embedded_thumb_acceptable(800, 960));
        assert!(embedded_thumb_acceptable(960, 960));
        assert!(embedded_thumb_acceptable(1024, 960));
    }

    /// 小档位（120/240）宽松：max_edge≥120 即采用（容轻度放大），仅 <120 才回退。
    #[test]
    fn small_tier_lenient_above_120_floor() {
        // tier=120：160 采用；119 拒（保留原 <120 下限）。
        assert!(embedded_thumb_acceptable(160, 120));
        assert!(!embedded_thumb_acceptable(119, 120));
        // tier=240：160<240 仍采用（宽通道、轻度放大）；119 拒。
        assert!(embedded_thumb_acceptable(160, 240));
        assert!(!embedded_thumb_acceptable(119, 240));
    }

    /// 非档位 target_size 经 snap_to_tier 归一后判级（500→480 严格；300→240 宽松）。
    #[test]
    fn non_tier_target_snaps_before_grading() {
        assert!(!embedded_thumb_acceptable(256, 500)); // snap→480 严格，256<480 拒
        assert!(embedded_thumb_acceptable(160, 300)); // snap→240 宽松，160≥120 采用
    }
}
