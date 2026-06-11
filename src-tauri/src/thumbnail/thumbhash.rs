// src-tauri/src/thumbnail/thumbhash.rs
//! ThumbHash generation from decoded pixels.
//! 从解码后的像素生成 ThumbHash。
//!
//! Input: RGBA pixel buffer (any size).
//! 输入：RGBA 像素缓冲区（任何尺寸）。
//! Output: ~28 bytes of ThumbHash, stored as BLOB in the DB.
//! 输出：约 28 字节的 ThumbHash，作为 BLOB 存储在数据库中。
//! Frontend receives it as `number[]` → `Uint8Array` → renders 32×32 placeholder.
//! 前端接收其为 `number[]` → `Uint8Array` → 渲染为 32×32 占位符。

use crate::engine::traits::DecodedImage;
use crate::error::{AppError, Result};

/// Maximum dimension to scale the image before hashing (ThumbHash works well at 100×100 or smaller).
/// 在散列之前缩放图像的最大尺寸（ThumbHash 在 100×100 或更小的尺寸下效果很好）。
const HASH_MAX_DIM: u32 = 100;

/// Generate a ThumbHash for a decoded image.
/// 为解码后的图像生成 ThumbHash。
pub fn generate_thumbhash(decoded: &DecodedImage) -> Result<Vec<u8>> {
    // Scale down if needed
    // 如果需要则缩小
    let (pixels, width, height) = if decoded.width > HASH_MAX_DIM || decoded.height > HASH_MAX_DIM {
        let ratio = (HASH_MAX_DIM as f32) / (decoded.width.max(decoded.height) as f32);
        let new_w = ((decoded.width  as f32) * ratio).round() as u32;
        let new_h = ((decoded.height as f32) * ratio).round() as u32;

        // Use fast_image_resize v4 for downscaling
        // 使用 fast_image_resize v4 进行降采样（缩小）
        use fast_image_resize::{images::Image as FirImage, Resizer, ResizeOptions};
        use fast_image_resize::pixels::PixelType;

        let src = FirImage::from_vec_u8(
            decoded.width.max(1),
            decoded.height.max(1),
            decoded.pixels.clone(),
            PixelType::U8x4,
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut dst = FirImage::new(new_w.max(1), new_h.max(1), PixelType::U8x4);

        let mut resizer = Resizer::new();
        resizer
            .resize(&src, &mut dst, &ResizeOptions::default())
            .map_err(|e| AppError::Internal(e.to_string()))?;

        (dst.into_vec(), new_w, new_h)
    } else {
        (decoded.pixels.clone(), decoded.width, decoded.height)
    };

    let hash = thumbhash::rgba_to_thumb_hash(width as usize, height as usize, &pixels);
    Ok(hash)
}
