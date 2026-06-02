// src-tauri/src/thumbnail/generator.rs
//! Unified thumbnail generation entry point (§ 8.1).
//! 统一的缩略图生成入口点（§ 8.1）。
//!
//! Pipeline:
//! 管道：
//!   1. Cache hit check
//!   1. 缓存命中检查
//!   2. Small file direct display (thumb_status = 3)
//!   2. 小文件直接显示（thumb_status = 3）
//!   3. Dispatch by media_type
//!   3. 根据 media_type 分发
//!   4. ThumbHash generation
//!   4. ThumbHash 生成
//!   5. Write to disk + DB update
//!   5. 写入磁盘 + 数据库更新

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use tracing::{debug, warn};

use crate::db::models::ThumbResult;
use crate::db::queries::{get_item_path_info, get_media_item, update_thumb_result};
use crate::engine::EngineArena;
use crate::error::{AppError, Result};
use crate::thumbnail::cache::{ensure_thumb_dir, thumb_db_path, thumb_path};

use crate::thumbnail::thumbhash::generate_thumbhash;

/// Configuration for thumbnail generation.
/// 缩略图生成的配置。
#[derive(Clone)]
pub struct ThumbConfig {
    pub cache_dir:       std::path::PathBuf,
    pub size:            u32,
    pub skip_max_bytes:  u64,
    pub strategy:        String,
    pub gpu_engine:      String,
}

/// Generate a thumbnail for a single media item.
/// 为单个媒体项生成缩略图。
///
/// Returns a `ThumbResult` that can be sent directly to the frontend.
/// 返回一个可以直接发送到前端的 `ThumbResult`。
pub fn generate_thumbnail(
    item: &crate::db::models::MediaItem,
    abs_path: &Path,
    arena: &EngineArena,
    config: &ThumbConfig,
) -> Result<ThumbResult> {
    let item_id = item.id;

    // ── 1. Cache hit ──────────────────────────────────────────────────────
    // ── 1. 缓存命中 ────────────────────────────────────────────────────────
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
    // ── 2. 小文件直接显示 ──────────────────────────────────────────────────
    let web_safe_formats = ["jpg", "jpeg", "png", "webp", "gif", "svg", "avif"];
    let is_web_safe = web_safe_formats.contains(&item.file_format.to_lowercase().as_str());

    let mut is_direct = false;
    if config.strategy == "direct" && is_web_safe && item.media_type == "image" {
        is_direct = true;
    } else if is_web_safe && item.file_size as u64 <= config.skip_max_bytes && item.media_type == "image" {
        is_direct = true;
    }

    if is_direct {
        let mut hash = None;
        if config.strategy != "direct" && item.file_size <= 500 * 1024 {
            // Only fall back to full decode for ThumbHash if the file is genuinely small
            // (e.g., < 500KB). Full decoding large files just for a ThumbHash causes CPU spikes.
            // 只有当文件确实很小（例如 < 500KB）时，才回退到完整解码以获取 ThumbHash。
            // 仅仅为了 ThumbHash 而完整解码大文件会导致 CPU 占用率激增。
            hash = generate_thumbhash_from_file(arena, &item.file_format, abs_path).unwrap_or(None);
        }

        // Store the absolute path as thumb_path so the frontend can load the
        // original file directly via convertFileSrc without an extra IPC call.
        // 将绝对路径存储为 thumb_path，以便前端可以通过 convertFileSrc 直接加载原始文件，
        // 而无需额外的 IPC 调用。
        let abs_path_str = abs_path.to_string_lossy().replace('\\', "/");
        return Ok(ThumbResult {
            item_id,
            thumb_status: 3,
            thumb_path: Some(abs_path_str),
            thumbhash: hash,
        });
    }

    // ── 3. Dispatch by media_type ─────────────────────────────────────────
    // ── 3. 根据 media_type 分发 ────────────────────────────────────────────
    match item.media_type.as_str() {
        "image" => {
            if config.strategy == "gpu" {
                match generate_gpu_image_thumb(item_id, item.cache_key, abs_path, &item.file_format, config) {
                    Ok(res) => Ok(res),
                    Err(e) => {
                        tracing::warn!("GPU generation failed for {:?}, falling back to CPU: {}", abs_path.file_name(), e);
                        generate_image_thumb(arena, item_id, item.cache_key, abs_path, &item.file_format, config)
                    }
                }
            } else {
                generate_image_thumb(arena, item_id, item.cache_key, abs_path, &item.file_format, config)
            }
        }
        _ => {
            // Phase 2: video/audio/document — mark as failed for now
            // 第 2 阶段：视频/音频/文档 — 目前标记为失败
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

    let start_total = std::time::Instant::now();
    let final_webp;
    let final_hash;

    // Try fast EXIF path first
    // 首先尝试快速 EXIF 路径
    let start_exif = std::time::Instant::now();
    if let Some((webp, hash)) = crate::thumbnail::exif_thumb::try_exif_thumb(engine.as_ref(), abs_path, config.size) {
        tracing::debug!("try_exif_thumb for {:?} took {:?}", abs_path.file_name(), start_exif.elapsed());
        final_webp = Some(webp);
        final_hash = hash;
    } else {
        tracing::debug!("try_exif_thumb failed or skipped for {:?}, falling back to full decode", abs_path.file_name());
        // Full decode fallback
        // 完整解码回退
        let start_decode = std::time::Instant::now();
        let mut decoded = engine.decode(abs_path, None)?;
        tracing::debug!("engine.decode for {:?} took {:?}", abs_path.file_name(), start_decode.elapsed());

        let start_resize = std::time::Instant::now();
        let rgba_img = resize_to_rgba(&mut decoded.pixels, decoded.width, decoded.height, config.size)?;
        tracing::debug!("resize_to_rgba for {:?} took {:?}", abs_path.file_name(), start_resize.elapsed());

        let start_hash = std::time::Instant::now();
        let decoded_for_hash = crate::engine::traits::DecodedImage {
            pixels: rgba_img.as_raw().clone(),
            width: rgba_img.width(),
            height: rgba_img.height(),
        };
        final_hash = generate_thumbhash(&decoded_for_hash).ok();
        tracing::debug!("generate_thumbhash for {:?} took {:?}", abs_path.file_name(), start_hash.elapsed());

        let start_encode = std::time::Instant::now();
        final_webp = crate::thumbnail::exif_thumb::encode_as_webp(&rgba_img, rgba_img.width(), rgba_img.height())
            .or_else(|_| crate::thumbnail::exif_thumb::encode_as_jpeg(&rgba_img))
            .ok();
        tracing::debug!("encode for {:?} took {:?}", abs_path.file_name(), start_encode.elapsed());
    }

    let webp = final_webp.ok_or_else(|| AppError::Engine("WebP encode failed".into()))?;

    // Write WebP to disk
    // 将 WebP 写入磁盘
    ensure_thumb_dir(&config.cache_dir, config.size, cache_key)
        .map_err(|e| AppError::Io(e.to_string()))?;
    let disk_path = thumb_path(&config.cache_dir, config.size, cache_key);
    std::fs::write(&disk_path, &webp).map_err(AppError::from)?;

    let db_path = thumb_db_path(config.size, cache_key);
    let hash = final_hash;

    tracing::debug!("Total generate_image_thumb for {:?} took {:?}", abs_path.file_name(), start_total.elapsed());

    return Ok(ThumbResult {
        item_id,
        thumb_status: 1,
        thumb_path: Some(db_path),
        thumbhash: hash,
    });


}

fn resize_to_rgba(pixels: &mut [u8], w: u32, h: u32, target: u32) -> Result<image::RgbaImage> {
    if w <= target && h <= target {
        return image::RgbaImage::from_raw(w, h, pixels.to_vec())
            .ok_or_else(|| AppError::Engine("resize buffer mismatch".into()));
    }

    use fast_image_resize::{images::Image as FirImage, Resizer, ResizeOptions};
    use fast_image_resize::pixels::PixelType;

    let (new_w, new_h) = if w >= h {
        let r = target as f32 / w as f32;
        (target, (h as f32 * r).round() as u32)
    } else {
        let r = target as f32 / h as f32;
        ((w as f32 * r).round() as u32, target)
    };

    // fast_image_resize v4 FirImage::from_slice_u8 uses mutable slice to avoid copying
    let src = FirImage::from_slice_u8(
        w.max(1),
        h.max(1),
        pixels,
        PixelType::U8x4,
    )
    .map_err(|e| AppError::Engine(e.to_string()))?;

    let mut dst = FirImage::new(new_w.max(1), new_h.max(1), PixelType::U8x4);

    use fast_image_resize::{ResizeAlg, FilterType};
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));

    let mut resizer = Resizer::new();
    resizer
        .resize(&src, &mut dst, &options)
        .map_err(|e| AppError::Engine(e.to_string()))?;

    image::RgbaImage::from_raw(new_w.max(1), new_h.max(1), dst.into_vec())
        .ok_or_else(|| AppError::Engine("resize buffer mismatch".into()))
}

fn generate_thumbhash_from_file(
    arena: &EngineArena,
    format: &str,
    path: &Path,
) -> Result<Option<Vec<u8>>> {
    let Some(engine) = arena.engine_for(format) else {
        return Ok(None);
    };
    let decoded = engine.decode(path, None)?;
    Ok(generate_thumbhash(&decoded).ok())
}

fn generate_gpu_image_thumb(
    item_id: i64,
    cache_key: i64,
    abs_path: &Path,
    format: &str,
    config: &ThumbConfig,
) -> Result<ThumbResult> {
    let gpu_engine = crate::engine::gpu::get_gpu_engine(&config.gpu_engine)
        .ok_or_else(|| AppError::Engine(format!("Unknown GPU engine: {}", config.gpu_engine)))?;

    if !gpu_engine.can_handle(format) {
        return Err(AppError::UnsupportedFormat(format.to_string()));
    }

    let start_total = std::time::Instant::now();
    
    let start_decode = std::time::Instant::now();
    let mut decoded = gpu_engine.decode(abs_path, Some(config.size))?;
    tracing::debug!("GPU decode for {:?} took {:?}", abs_path.file_name(), start_decode.elapsed());

    let start_resize = std::time::Instant::now();
    let rgba_img = resize_to_rgba(&mut decoded.pixels, decoded.width, decoded.height, config.size)?;
    tracing::debug!("resize_to_rgba (CPU) for {:?} took {:?}", abs_path.file_name(), start_resize.elapsed());

    let start_hash = std::time::Instant::now();
    let decoded_for_hash = crate::engine::traits::DecodedImage {
        pixels: rgba_img.as_raw().clone(),
        width: rgba_img.width(),
        height: rgba_img.height(),
    };
    let final_hash = generate_thumbhash(&decoded_for_hash).ok();
    tracing::debug!("generate_thumbhash for {:?} took {:?}", abs_path.file_name(), start_hash.elapsed());

    let start_encode = std::time::Instant::now();
    let webp = crate::thumbnail::exif_thumb::encode_as_webp(&rgba_img, rgba_img.width(), rgba_img.height())
        .or_else(|_| crate::thumbnail::exif_thumb::encode_as_jpeg(&rgba_img))
        .map_err(|_| AppError::Engine("WebP encode failed".into()))?;
    tracing::debug!("encode for {:?} took {:?}", abs_path.file_name(), start_encode.elapsed());

    ensure_thumb_dir(&config.cache_dir, config.size, cache_key)
        .map_err(|e| AppError::Io(e.to_string()))?;
    let disk_path = thumb_path(&config.cache_dir, config.size, cache_key);
    std::fs::write(&disk_path, &webp).map_err(AppError::from)?;

    let db_path = thumb_db_path(config.size, cache_key);

    tracing::debug!("Total generate_gpu_image_thumb for {:?} took {:?}", abs_path.file_name(), start_total.elapsed());

    Ok(ThumbResult {
        item_id,
        thumb_status: 1,
        thumb_path: Some(db_path),
        thumbhash: final_hash,
    })
}
