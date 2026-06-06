// src-tauri/src/thumbnail/generator.rs
//! Unified thumbnail generation entry point (§ 8.1).
//! 统一的缩略图生成入口点（§ 8.1）。
//!
//! Pipeline:
//! 管道：
//!   1. Cache hit check
//!   2. Small file direct display (thumb_status = 3)
//!   3. Dispatch by media_type
//!   4. ThumbHash generation
//!   5. Write to disk + DB update

use std::path::Path;
use tracing::{debug, info, warn, trace};

use crate::db::models::ThumbResult;
use crate::engine::EngineArena;
use crate::error::{AppError, Result};
use crate::thumbnail::cache::{ensure_thumb_dir, thumb_db_path, thumb_path};
use crate::thumbnail::thumbhash::generate_thumbhash;

/// Valid thumbnail size tiers
/// 有效缩略图尺寸档位
const THUMB_TIERS: [u32; 4] = [120, 240, 480, 960];

/// Snap an arbitrary thumbnail size to the nearest valid tier.
/// 将任意缩略图尺寸就近取整到最近的有效档位。
pub fn snap_to_tier(size: u32) -> u32 {
    THUMB_TIERS.iter()
        .copied()
        .min_by_key(|&t| (t as i64 - size as i64).unsigned_abs())
        .unwrap_or(240)
}

#[derive(Clone)]
pub struct ThumbConfig {
    pub cache_dir:       std::path::PathBuf,
    pub size:            u32,
    pub skip_max_bytes:  u64,
    pub strategy:        String,
    pub gpu_engine:      String,
}

pub enum DecodeResult {
    Ready(ThumbResult),
    ToEncode {
        item_id: i64,
        cache_key: i64,
        decoded: crate::engine::traits::DecodedImage,
    },
    DeferredToCpu {
        item: crate::db::models::MediaItem,
        abs_path: std::path::PathBuf,
    },
}

pub enum ThumbResultOrDeferred {
    Done(ThumbResult),
    Deferred {
        item: crate::db::models::MediaItem,
        abs_path: std::path::PathBuf,
    },
}

pub fn generate_thumbnail(
    item: &crate::db::models::MediaItem,
    abs_path: &Path,
    arena: &EngineArena,
    config: &ThumbConfig,
) -> Result<ThumbResultOrDeferred> {
    let mut snapped_config = config.clone();
    snapped_config.size = snap_to_tier(config.size);
    let config = &snapped_config;

    match decode_media_step(item, abs_path, arena, config)? {
        DecodeResult::Ready(res) => Ok(ThumbResultOrDeferred::Done(res)),
        DecodeResult::ToEncode { item_id, cache_key, decoded } => {
            Ok(ThumbResultOrDeferred::Done(encode_media_step(item_id, cache_key, decoded, config)?))
        }
        DecodeResult::DeferredToCpu { item, abs_path } => {
            Ok(ThumbResultOrDeferred::Deferred { item, abs_path })
        }
    }
}

pub fn process_deferred_cpu(
    item: &crate::db::models::MediaItem,
    abs_path: &Path,
    arena: &EngineArena,
    config: &ThumbConfig,
) -> Result<ThumbResult> {
    let mut snapped_config = config.clone();
    snapped_config.size = snap_to_tier(config.size);
    let config = &snapped_config;

    match try_cpu_decode(item, abs_path, arena, config)? {
        DecodeResult::Ready(res) => Ok(res),
        DecodeResult::ToEncode { item_id, cache_key, decoded } => {
            encode_media_step(item_id, cache_key, decoded, config)
        }
        DecodeResult::DeferredToCpu { .. } => unreachable!("CPU decode cannot return Deferred"),
    }
}

pub fn decode_media_step(
    item: &crate::db::models::MediaItem,
    abs_path: &Path,
    arena: &EngineArena,
    config: &ThumbConfig,
) -> Result<DecodeResult> {
    let item_id = item.id;
    trace!(
        "[ThumbGen] decode_media_step: id={} status={} format={} size={} media_type={} path={:?} | strategy={} skip_max_bytes={}",
        item_id, item.thumb_status, item.file_format, item.file_size, item.media_type,
        abs_path.file_name().unwrap_or_default(), config.strategy, config.skip_max_bytes
    );

    // ── 1. Cache hit ──────────────────────────────────────────────────────
    if item.thumb_status == 1 {
        if let Some(ref tp) = item.thumb_path {
            let full = config.cache_dir.join("thumbnails").join(tp);
            if full.exists() {
                debug!("[ThumbGen] CACHE_HIT: id={item_id} path={tp}");
                return Ok(DecodeResult::Ready(ThumbResult {
                    item_id,
                    thumb_status: 1,
                    thumb_path: item.thumb_path.clone(),
                    thumbhash: item.thumbhash.clone(),
                }));
            } else {
                debug!("[ThumbGen] CACHE_MISS: id={item_id} thumb_path={tp} but file does not exist on disk");
            }
        } else {
            debug!("[ThumbGen] CACHE_MISS: id={item_id} thumb_status=1 but thumb_path is NULL");
        }
    }

    // ── 2. Small file direct display ─────────────────────────────────────
    let web_safe_formats = ["jpg", "jpeg", "png", "webp", "gif", "svg", "avif"];
    let is_web_safe = web_safe_formats.contains(&item.file_format.to_lowercase().as_str());

    let mut is_direct = false;
    let mut direct_reason = "";
    if config.strategy == "direct" && is_web_safe && item.media_type == "image" {
        is_direct = true;
        direct_reason = "strategy=direct";
    } else if is_web_safe && item.file_size as u64 <= config.skip_max_bytes && item.media_type == "image" {
        is_direct = true;
        direct_reason = "file_size<=skip_max_bytes";
    }

    if is_direct {
        info!(
            "[ThumbGen] DIRECT_DISPLAY: id={} reason={} format={} size={} skip_max_bytes={} | 跳过生成，直接使用源文件",
            item_id, direct_reason, item.file_format, item.file_size, config.skip_max_bytes
        );
        let mut hash = None;
        if config.strategy != "direct" && item.file_size <= 500 * 1024 {
            if let Some(engine) = arena.engine_for(&item.file_format) {
                if let Ok(decoded) = engine.decode(abs_path, None) {
                    hash = generate_thumbhash(&decoded).ok();
                }
            }
        }

        let abs_path_str = abs_path.to_string_lossy().replace('\\', "/");
        return Ok(DecodeResult::Ready(ThumbResult {
            item_id,
            thumb_status: 3,
            thumb_path: Some(abs_path_str),
            thumbhash: hash,
        }));
    }

    // ── 3. Dispatch by media_type ─────────────────────────────────────────
    match item.media_type.as_str() {
        "image" => {
            if config.strategy == "gpu" {
                info!("[ThumbGen] GPU_DECODE: id={} format={} size={} | 使用 GPU 解码", item_id, item.file_format, item.file_size);
                match try_gpu_decode(item, abs_path, config) {
                    Ok(res) => {
                        info!("[ThumbGen] GPU_DECODE_OK: id={} | GPU 解码成功", item_id);
                        Ok(res)
                    }
                    Err(e) => {
                        warn!("[ThumbGen] GPU_DECODE_FAIL: id={} err={}, deferring to CPU | GPU 解码失败，推迟至 CPU 跑道", item_id, e);
                        Ok(DecodeResult::DeferredToCpu {
                            item: item.clone(),
                            abs_path: abs_path.to_path_buf(),
                        })
                    }
                }
            } else {
                info!("[ThumbGen] CPU_DECODE: id={} format={} size={} | 使用 CPU 解码", item_id, item.file_format, item.file_size);
                try_cpu_decode(item, abs_path, arena, config)
            }
        }
        _ => {
            debug!("[ThumbGen] UNSUPPORTED_TYPE: id={} media_type={} | 非图像类型，跳过", item_id, item.media_type);
            // Phase 2: video/audio/document
            Ok(DecodeResult::Ready(ThumbResult {
                item_id,
                thumb_status: 2,
                thumb_path: None,
                thumbhash: None,
            }))
        }
    }
}

fn try_gpu_decode(
    item: &crate::db::models::MediaItem,
    abs_path: &Path,
    config: &ThumbConfig,
) -> Result<DecodeResult> {
    let gpu_engine = crate::engine::gpu::get_gpu_engine(&config.gpu_engine)
        .ok_or_else(|| AppError::Engine(format!("Unknown GPU engine: {}", config.gpu_engine)))?;

    if !gpu_engine.can_handle(&item.file_format) {
        return Err(AppError::UnsupportedFormat(item.file_format.clone()));
    }

    let decoded = gpu_engine.decode(abs_path, Some(crate::engine::traits::ResizeHint::LongEdge(config.size)))?;
    Ok(DecodeResult::ToEncode {
        item_id: item.id,
        cache_key: item.cache_key,
        decoded,
    })
}

fn try_cpu_decode(
    item: &crate::db::models::MediaItem,
    abs_path: &Path,
    arena: &EngineArena,
    config: &ThumbConfig,
) -> Result<DecodeResult> {
    let engine = arena
        .engine_for(&item.file_format)
        .ok_or_else(|| AppError::UnsupportedFormat(item.file_format.clone()))?;

    // Try fast EXIF path first
    if let Some((webp, hash)) = crate::thumbnail::exif_thumb::try_exif_thumb(engine.as_ref(), abs_path, config.size) {
        ensure_thumb_dir(&config.cache_dir, config.size, item.cache_key)
            .map_err(|e| AppError::Io(e.to_string()))?;
        let disk_path = thumb_path(&config.cache_dir, config.size, item.cache_key);
        std::fs::write(&disk_path, &webp).map_err(AppError::from)?;

        let db_path = thumb_db_path(config.size, item.cache_key);
        return Ok(DecodeResult::Ready(ThumbResult {
            item_id: item.id,
            thumb_status: 1,
            thumb_path: Some(db_path),
            thumbhash: hash,
        }));
    }

    // Full decode fallback
    let decoded = engine.decode(abs_path, None)?;
    Ok(DecodeResult::ToEncode {
        item_id: item.id,
        cache_key: item.cache_key,
        decoded,
    })
}

pub fn encode_media_step(
    item_id: i64,
    cache_key: i64,
    mut decoded: crate::engine::traits::DecodedImage,
    config: &ThumbConfig,
) -> Result<ThumbResult> {
    let t0 = std::time::Instant::now();
    let rgba_img = resize_to_rgba(&mut decoded.pixels, decoded.width, decoded.height, config.size)?;

    let decoded_for_hash = crate::engine::traits::DecodedImage {
        pixels: rgba_img.as_raw().clone(),
        width: rgba_img.width(),
        height: rgba_img.height(),
    };
    let final_hash = generate_thumbhash(&decoded_for_hash).ok();

    let webp = crate::thumbnail::exif_thumb::encode_as_webp(&rgba_img, rgba_img.width(), rgba_img.height())
        .or_else(|_| crate::thumbnail::exif_thumb::encode_as_jpeg(&rgba_img))
        .map_err(|_| AppError::Engine("WebP encode failed".into()))?;

    ensure_thumb_dir(&config.cache_dir, config.size, cache_key)
        .map_err(|e| AppError::Io(e.to_string()))?;
    let disk_path = thumb_path(&config.cache_dir, config.size, cache_key);
    std::fs::write(&disk_path, &webp).map_err(AppError::from)?;

    let db_path = thumb_db_path(config.size, cache_key);
    info!(
        "[ThumbGen] ENCODE_OK: id={} cache_key={} disk={:?} db_path={} size={}B elapsed={:.1}ms | 编码完成",
        item_id, cache_key, disk_path, db_path, webp.len(), t0.elapsed().as_secs_f64() * 1000.0
    );

    Ok(ThumbResult {
        item_id,
        thumb_status: 1,
        thumb_path: Some(db_path),
        thumbhash: final_hash,
    })
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
