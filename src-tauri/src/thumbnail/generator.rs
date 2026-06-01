// src-tauri/src/thumbnail/generator.rs
//! Unified thumbnail generation entry point (§ 8.1).
//!
//! Pipeline:
//!   1. Cache hit check
//!   2. Small file direct display (thumb_status = 3)
//!   3. Dispatch by media_type
//!   4. ThumbHash generation
//!   5. Write to disk + DB update

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use tracing::{debug, warn};

use crate::db::models::ThumbResult;
use crate::db::queries::{get_item_path_info, get_media_item, update_thumb_result};
use crate::engine::EngineArena;
use crate::error::{AppError, Result};
use crate::thumbnail::cache::{ensure_thumb_dir, thumb_db_path, thumb_path};
use crate::thumbnail::exif_thumb::{encode_as_jpeg, encode_as_webp, try_exif_thumb};
use crate::thumbnail::thumbhash::generate_thumbhash;

/// Configuration for thumbnail generation.
#[derive(Clone)]
pub struct ThumbConfig {
    pub cache_dir:       std::path::PathBuf,
    pub size:            u32,
    pub skip_max_bytes:  u64,
}

/// Generate a thumbnail for a single media item.
///
/// Returns a `ThumbResult` that can be sent directly to the frontend.
pub fn generate_thumbnail(
    writer: &Mutex<Connection>,
    arena: &EngineArena,
    item_id: i64,
    config: &ThumbConfig,
) -> Result<ThumbResult> {
    let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
    let item = get_media_item(&conn, item_id)?;
    let (root_path, rel_path, file_name) = get_item_path_info(&conn, item_id)?;
    drop(conn);

    let abs_path_str = crate::utils::path::resolve_media_path(&root_path, &rel_path, &file_name);
    let abs_path = Path::new(&abs_path_str);

    // ── 1. Cache hit ──────────────────────────────────────────────────────
    if item.thumb_status == 1 {
        if let Some(ref tp) = item.thumb_path {
            let full = config.cache_dir.join("thumbnails").join(tp);
            if full.exists() {
                debug!("Cache hit: item_id={item_id}");
                return Ok(ThumbResult {
                    item_id,
                    thumb_status: 1,
                    thumb_path: item.thumb_path.clone(),
                    thumbhash: item.thumbhash.clone(),
                });
            }
        }
    }

    // ── 2. Small file direct display ─────────────────────────────────────
    if item.file_size as u64 <= config.skip_max_bytes && item.media_type == "image" {
        // Still generate ThumbHash for the placeholder
        let hash = generate_thumbhash_from_file(arena, &item.file_format, abs_path)?;
        // Store the absolute path as thumb_path so the frontend can load the
        // original file directly via convertFileSrc without an extra IPC call.
        let abs_path_str = abs_path.to_string_lossy().replace('\\', "/");
        {
            let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
            update_thumb_result(&conn, item_id, 3, Some(abs_path_str.as_str()), hash.as_deref())?;
        }
        return Ok(ThumbResult {
            item_id,
            thumb_status: 3,
            thumb_path: Some(abs_path_str),
            thumbhash: hash,
        });
    }

    // ── 3. Dispatch by media_type ─────────────────────────────────────────
    match item.media_type.as_str() {
        "image" => {
            generate_image_thumb(writer, arena, item_id, item.cache_key, abs_path, &item.file_format, config)
        }
        _ => {
            // Phase 2: video/audio/document — mark as failed for now
            let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
            update_thumb_result(&conn, item_id, 2, None, None)?;
            Ok(ThumbResult {
                item_id,
                thumb_status: 2,
                thumb_path: None,
                thumbhash: None,
            })
        }
    }
}

fn generate_image_thumb(
    writer: &Mutex<Connection>,
    arena: &EngineArena,
    item_id: i64,
    cache_key: i64,
    abs_path: &Path,
    format: &str,
    config: &ThumbConfig,
) -> Result<ThumbResult> {
    let engine = arena
        .engine_for(format)
        .ok_or_else(|| AppError::UnsupportedFormat(format.to_string()))?;

    // Try EXIF fast path first
    let webp_bytes = if let Some(bytes) = try_exif_thumb(engine.as_ref(), abs_path, config.size) {
        bytes
    } else {
        // Full decode
        let decoded = engine.decode(abs_path)?;
        let hash_result = generate_thumbhash(&decoded);

        // Resize with fast_image_resize
        let webp = resize_and_encode(&decoded.pixels, decoded.width, decoded.height, config.size)?;

        // Write WebP to disk
        ensure_thumb_dir(&config.cache_dir, config.size, cache_key)
            .map_err(|e| AppError::Io(e.to_string()))?;
        let disk_path = thumb_path(&config.cache_dir, config.size, cache_key);
        std::fs::write(&disk_path, &webp).map_err(AppError::from)?;

        let db_path = thumb_db_path(config.size, cache_key);
        let hash = hash_result.ok();

        {
            let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
            update_thumb_result(&conn, item_id, 1, Some(&db_path), hash.as_deref())?;
        }

        return Ok(ThumbResult {
            item_id,
            thumb_status: 1,
            thumb_path: Some(db_path),
            thumbhash: hash,
        });
    };

    // Write EXIF-path WebP to disk
    ensure_thumb_dir(&config.cache_dir, config.size, cache_key)
        .map_err(|e| AppError::Io(e.to_string()))?;
    let disk_path = thumb_path(&config.cache_dir, config.size, cache_key);
    std::fs::write(&disk_path, &webp_bytes).map_err(AppError::from)?;

    let db_path = thumb_db_path(config.size, cache_key);

    // Generate ThumbHash from the written WebP
    let hash = generate_thumbhash_from_webp_bytes(&webp_bytes).ok();

    {
        let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        update_thumb_result(&conn, item_id, 1, Some(&db_path), hash.as_deref())?;
    }

    Ok(ThumbResult {
        item_id,
        thumb_status: 1,
        thumb_path: Some(db_path),
        thumbhash: hash,
    })
}

fn resize_and_encode(pixels: &[u8], w: u32, h: u32, target: u32) -> Result<Vec<u8>> {
    use fast_image_resize::{images::Image as FirImage, Resizer, ResizeOptions};
    use fast_image_resize::pixels::PixelType;

    let (new_w, new_h) = if w >= h {
        let r = target as f32 / w as f32;
        (target, (h as f32 * r).round() as u32)
    } else {
        let r = target as f32 / h as f32;
        ((w as f32 * r).round() as u32, target)
    };

    // fast_image_resize v4: Image::from_slice_u8 / Image::new take u32 directly
    let mut pixels_vec = pixels.to_vec();
    let src = FirImage::from_slice_u8(
        w.max(1),
        h.max(1),
        &mut pixels_vec,
        PixelType::U8x4,
    )
    .map_err(|e| AppError::Engine(e.to_string()))?;

    let mut dst = FirImage::new(new_w.max(1), new_h.max(1), PixelType::U8x4);

    let mut resizer = Resizer::new();
    resizer
        .resize(&src, &mut dst, &ResizeOptions::default())
        .map_err(|e| AppError::Engine(e.to_string()))?;

    let rgba_img = image::RgbaImage::from_raw(new_w, new_h, dst.into_vec())
        .ok_or_else(|| AppError::Engine("resize buffer mismatch".into()))?;

    encode_as_webp(&rgba_img, new_w, new_h).or_else(|_| {
        warn!("WebP encode failed, falling back to JPEG");
        encode_as_jpeg(&rgba_img)
    })
}

fn generate_thumbhash_from_file(
    arena: &EngineArena,
    format: &str,
    path: &Path,
) -> Result<Option<Vec<u8>>> {
    let Some(engine) = arena.engine_for(format) else {
        return Ok(None);
    };
    let decoded = engine.decode(path)?;
    Ok(generate_thumbhash(&decoded).ok())
}

fn generate_thumbhash_from_webp_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| AppError::Engine(e.to_string()))?
        .to_rgba8();
    let w = img.width();
    let h = img.height();
    let pixels = img.into_raw();
    Ok(thumbhash::rgba_to_thumb_hash(w as usize, h as usize, &pixels))
}
