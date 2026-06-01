// src-tauri/src/thumbnail/thumbhash.rs
//! ThumbHash generation from decoded pixels.
//!
//! Input: RGBA pixel buffer (any size).
//! Output: ~28 bytes of ThumbHash, stored as BLOB in the DB.
//! Frontend receives it as `number[]` → `Uint8Array` → renders 32×32 placeholder.

use crate::engine::traits::DecodedImage;
use crate::error::{AppError, Result};

/// Maximum dimension to scale the image before hashing (ThumbHash works well at 100×100 or smaller).
const HASH_MAX_DIM: u32 = 100;

/// Generate a ThumbHash for a decoded image.
pub fn generate_thumbhash(decoded: &DecodedImage) -> Result<Vec<u8>> {
    // Scale down if needed
    let (pixels, width, height) = if decoded.width > HASH_MAX_DIM || decoded.height > HASH_MAX_DIM {
        let ratio = (HASH_MAX_DIM as f32) / (decoded.width.max(decoded.height) as f32);
        let new_w = ((decoded.width  as f32) * ratio).round() as u32;
        let new_h = ((decoded.height as f32) * ratio).round() as u32;

        // Use fast_image_resize v4 for downscaling
        use fast_image_resize::{images::Image as FirImage, Resizer, ResizeOptions};
        use fast_image_resize::pixels::PixelType;

        let src = FirImage::from_vec_u8(
            decoded.width.max(1),
            decoded.height.max(1),
            decoded.pixels.clone(),
            PixelType::U8x4,
        )
        .map_err(|e| AppError::Engine(e.to_string()))?;

        let mut dst = FirImage::new(new_w.max(1), new_h.max(1), PixelType::U8x4);

        let mut resizer = Resizer::new();
        resizer
            .resize(&src, &mut dst, &ResizeOptions::default())
            .map_err(|e| AppError::Engine(e.to_string()))?;

        (dst.into_vec(), new_w, new_h)
    } else {
        (decoded.pixels.clone(), decoded.width, decoded.height)
    };

    let hash = thumbhash::rgba_to_thumb_hash(width as usize, height as usize, &pixels);
    Ok(hash)
}
