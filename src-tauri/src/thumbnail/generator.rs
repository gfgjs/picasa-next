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
use tracing::{debug, info, trace, warn};

use crate::db::models::ThumbResult;
use crate::engine::EngineArena;
use crate::error::{AppError, Result};
use crate::thumbnail::cache::{
    ai_cache_path, ensure_ai_cache_dir, ensure_thumb_dir, thumb_db_path, thumb_path,
    AI_CACHE_SHORT_EDGE,
};
use crate::thumbnail::thumbhash::generate_thumbhash;

/// Valid thumbnail size tiers
/// 有效缩略图尺寸档位
/// ⚠️ **未经多设备/多库规模实测对比**——改档位会使全库已生成缓存作废需重产,属高代价变更;
/// 当前 4 档是经验值,开发期若要调应在发版前(避免缓存大面积失效)。
const THUMB_TIERS: [u32; 4] = [120, 240, 480, 960];

/// Snap an arbitrary thumbnail size to the nearest valid tier.
/// 将任意缩略图尺寸就近取整到最近的有效档位。
pub fn snap_to_tier(size: u32) -> u32 {
    THUMB_TIERS
        .iter()
        .copied()
        .min_by_key(|&t| (t as i64 - size as i64).unsigned_abs())
        .unwrap_or(240)
}

/// Run a decode/encode step under `catch_unwind` so a panic in a third-party
/// image codec (corrupt / malformed file) fails just that one item instead of
/// aborting the whole process. Requires `panic = "unwind"` (see Cargo.toml).
///
/// `AssertUnwindSafe` is sound here: a panic mid-decode leaves no shared mutable
/// state inconsistent — we only lose the in-flight image and report it as failed.
///
/// 在 `catch_unwind` 下运行解码/编码步骤，使第三方图像编解码器在处理
/// 损坏/畸形文件时的 panic 仅令该项失败，而非中止整个进程。
/// 需要 `panic = "unwind"`（见 Cargo.toml）。
fn panic_guard<T>(label: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            warn!("[ThumbGen] PANIC caught in {label} — item failed, process kept alive | 已捕获 panic，单项失败但进程存活");
            Err(AppError::Internal(format!("panic during {label}")))
        }
    }
}

/// Atomically persist a derived artifact: write `<name>.<seq>.tmp` in the same directory, then
/// `rename` over the final path (same-volume rename is atomic; on Windows `std::fs::rename`
/// replaces the destination via MOVEFILE_REPLACE_EXISTING).
///
/// 原子落盘(审查 R0-4 / CLAUDE.md「派生产物一律原子落盘」):缓存命中判定只查 `exists()`
/// (见 decode_media_step 的 CACHE_HIT 分支),若直写最终路径,崩溃/断电会把半截文件留在
/// 正式路径上 → 之后每次都命中「缓存」→ UI 永久裂图。先写同目录 tmp 再 rename,保证正式
/// 路径上只可能出现完整文件。tmp 名带进程级序号:两个批次并发生成同一 cache_key 时各写各的
/// tmp、rename 后到者胜,不会交错写同一文件。rename 失败即清 tmp;进程中途崩溃遗留的孤儿
/// tmp 不影响正确性(命中判定不认 .tmp),随缓存清理/GC(Part3 缓存治理)一并回收。
/// (T18 起 pub(crate):derive::image 的 ai_cache 生成共用,消除其直写红线违例。)
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!("{file_name}.{seq}.tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[derive(Clone)]
pub struct ThumbConfig {
    pub cache_dir: std::path::PathBuf,
    pub size: u32,
    pub skip_max_bytes: u64,
    pub strategy: String,
    pub gpu_engine: String,
    /// When true, the image thumbnail path produces the AI-analysis cache (short-edge≥336 WebP)
    /// **from the same source decode** — one decode, two outputs — so building it for newly
    /// thumbnailed images is nearly free. Off for cover derivations (video/doc/audio).
    /// 为 true 时，图像缩略图路径**用同一次源解码**顺带产出 AI 分析缓存（短边≥336 的 WebP）——
    /// 一次解码两份产物 —— 使新生成缩略图的图片几乎免费获得 AI 缓存。封面派生（视频/文档/音频）为 false。
    pub ai_hq_cache: bool,
}

// 各变体仅作解码分派的单实例消息穿过 channel/单值传递、不批量收集进 Vec，
// 变体间尺寸差带来的内存浪费可忽略，Box 化反增解引用成本与噪声。
#[allow(clippy::large_enum_variant)]
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

// 同 DecodeResult：单实例分派消息、不批量收集，变体尺寸差可忽略。
#[allow(clippy::large_enum_variant)]
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
        DecodeResult::ToEncode {
            item_id,
            cache_key,
            decoded,
        } => Ok(ThumbResultOrDeferred::Done(encode_media_step(
            item_id, cache_key, decoded, config,
        )?)),
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
    panic_guard("process_deferred_cpu", || {
        process_deferred_cpu_inner(item, abs_path, arena, config)
    })
}

fn process_deferred_cpu_inner(
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
        DecodeResult::ToEncode {
            item_id,
            cache_key,
            decoded,
        } => encode_media_step(item_id, cache_key, decoded, config),
        DecodeResult::DeferredToCpu { .. } => unreachable!("CPU decode cannot return Deferred"),
    }
}

pub fn decode_media_step(
    item: &crate::db::models::MediaItem,
    abs_path: &Path,
    arena: &EngineArena,
    config: &ThumbConfig,
) -> Result<DecodeResult> {
    panic_guard("decode_media_step", || {
        decode_media_step_inner(item, abs_path, arena, config)
    })
}

fn decode_media_step_inner(
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
    } else if is_web_safe
        && item.file_size as u64 <= config.skip_max_bytes
        && item.media_type == "image"
    {
        is_direct = true;
        direct_reason = "file_size<=skip_max_bytes";
    }

    if is_direct {
        info!(
            "[ThumbGen] DIRECT_DISPLAY: id={} reason={} format={} size={} skip_max_bytes={} | 跳过生成，直接使用源文件",
            item_id, direct_reason, item.file_format, item.file_size, config.skip_max_bytes
        );
        // 占位图（thumbhash）是体验底线，不应因 strategy=="direct" 而丢失（Part3 Q14 / §3.1.3）：
        // 去掉原 `strategy != "direct"` 短路，两条 direct 路径均在文件 ≤500KB 时生成占位
        // （500KB 守卫保留：避免仅为占位图去全解码大文件）。
        let mut hash = None;
        if item.file_size <= 500 * 1024 {
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
                info!(
                    "[ThumbGen] GPU_DECODE: id={} format={} size={} | 使用 GPU 解码",
                    item_id, item.file_format, item.file_size
                );
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
                info!(
                    "[ThumbGen] CPU_DECODE: id={} format={} size={} | 使用 CPU 解码",
                    item_id, item.file_format, item.file_size
                );
                try_cpu_decode(item, abs_path, arena, config)
            }
        }
        _ => {
            debug!(
                "[ThumbGen] UNSUPPORTED_TYPE: id={} media_type={} | 非图像类型，跳过",
                item_id, item.media_type
            );
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
        .ok_or_else(|| AppError::Internal(format!("Unknown GPU engine: {}", config.gpu_engine)))?;

    if !gpu_engine.can_handle(&item.file_format) {
        return Err(AppError::UnsupportedFormat(item.file_format.clone()));
    }

    let decoded = gpu_engine.decode(
        abs_path,
        Some(crate::engine::traits::ResizeHint::LongEdge(
            decode_long_edge(config, item),
        )),
    )?;
    Ok(DecodeResult::ToEncode {
        item_id: item.id,
        cache_key: item.cache_key,
        decoded,
    })
}

/// LongEdge target for the (GPU) source decode. Normally just `config.size`. But when the AI HQ
/// cache is on and the image is "wide" — its thumbnail short edge would fall below the AI cache
/// short edge — decode slightly larger so the SAME buffer can yield BOTH the thumbnail (downscale
/// to `size`) and the AI cache (downscale to short-edge `AI_CACHE_SHORT_EDGE`), avoiding a second
/// full source decode by the `ai_thumb` derivation. Never upscales (WIC LongEdge only downscales).
///
/// 用于（GPU）源解码的 LongEdge 目标。常态即 `config.size`。但当 AI 高清缓存开启且图像为「宽幅」
/// （其缩略图短边会低于 AI 缓存短边）时，把解码长边略放大，使同一缓冲既能产出缩略图（降采样到
/// `size`）又能产出 AI 缓存（降采样到短边 `AI_CACHE_SHORT_EDGE`），免去 `ai_thumb` 派生再做一次
/// 全分辨率源解码。绝不上采样（WIC LongEdge 仅下采样）。
fn decode_long_edge(config: &ThumbConfig, item: &crate::db::models::MediaItem) -> u32 {
    let (w, h) = (item.width as u32, item.height as u32);
    let (long, short) = (w.max(h), w.min(h));
    if !config.ai_hq_cache || long == 0 || short == 0 {
        return config.size;
    }
    let thumb_short = (short as f32 * config.size as f32 / long as f32).round() as u32;
    if thumb_short >= AI_CACHE_SHORT_EDGE {
        config.size // 缩略图短边已≥336，无需为 AI 缓存放大解码
    } else {
        ((AI_CACHE_SHORT_EDGE as f32 * long as f32 / short as f32).ceil() as u32).max(config.size)
    }
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
    if let Some((webp, hash)) =
        crate::thumbnail::exif_thumb::try_exif_thumb(engine.as_ref(), abs_path, config.size)
    {
        ensure_thumb_dir(&config.cache_dir, config.size, item.cache_key).map_err(AppError::Io)?;
        let disk_path = thumb_path(&config.cache_dir, config.size, item.cache_key);
        write_atomic(&disk_path, &webp).map_err(AppError::from)?;

        let db_path = thumb_db_path(config.size, item.cache_key);
        return Ok(DecodeResult::Ready(ThumbResult {
            item_id: item.id,
            thumb_status: 1,
            thumb_path: Some(db_path),
            thumbhash: hash,
        }));
    }

    // Full decode fallback —— 解码期即降采样到目标档位（Part3 Q1 / §3.1.1）。
    // 复用 GPU 路径同一 `decode_long_edge`（而非裸 `snap_to_tier`）：AI 高清缓存开启且宽幅图时
    // 解码略大，使 encode 阶段同一缓冲既出缩略图又出 AI 缓存（一次解码两份产物），否则即 config.size。
    // image crate 的 LongEdge 仅下采样不上采样（image_rs.rs:57）→ 小图不被放大；常见情形下
    // `resize_to_rgba` 因 w/h<=target 短路返回，省掉二次缩放（与 GPU 路径输出语义一致，同 CatmullRom）。
    let decoded = engine.decode(
        abs_path,
        Some(crate::engine::traits::ResizeHint::LongEdge(
            decode_long_edge(config, item),
        )),
    )?;
    Ok(DecodeResult::ToEncode {
        item_id: item.id,
        cache_key: item.cache_key,
        decoded,
    })
}

pub fn encode_media_step(
    item_id: i64,
    cache_key: i64,
    decoded: crate::engine::traits::DecodedImage,
    config: &ThumbConfig,
) -> Result<ThumbResult> {
    panic_guard("encode_media_step", move || {
        encode_media_step_inner(item_id, cache_key, decoded, config)
    })
}

fn encode_media_step_inner(
    item_id: i64,
    cache_key: i64,
    mut decoded: crate::engine::traits::DecodedImage,
    config: &ThumbConfig,
) -> Result<ThumbResult> {
    let t0 = std::time::Instant::now();

    // One-decode-two-outputs: before the thumbnail consumes the buffer, opportunistically emit the
    // AI-analysis cache from this SAME decoded image (best-effort — a failure must not fail the
    // thumbnail). Only for wide images whose thumbnail short edge < the AI short edge (squarer
    // images' thumbnails already satisfy analysis, so an AI cache would just waste disk).
    // 一次解码两份产物：在缩略图消费缓冲前，顺带从同一解码图产出 AI 分析缓存（尽力而为 —— 失败不可
    // 拖垮缩略图）。仅针对缩略图短边 < AI 短边的宽幅图（较方的图其缩略图已满足分析，再建 AI 缓存纯浪费盘）。
    if config.ai_hq_cache {
        if let Err(e) = maybe_write_ai_cache(cache_key, &decoded, config) {
            warn!(
                "[ThumbGen] AI cache emit failed for id={} | AI 缓存顺带产出失败: {}",
                item_id, e
            );
        }
    }

    let rgba_img = resize_to_rgba(
        &mut decoded.pixels,
        decoded.width,
        decoded.height,
        config.size,
    )?;

    let decoded_for_hash = crate::engine::traits::DecodedImage {
        pixels: rgba_img.as_raw().clone(),
        width: rgba_img.width(),
        height: rgba_img.height(),
    };
    let final_hash = generate_thumbhash(&decoded_for_hash).ok();

    let webp = crate::thumbnail::exif_thumb::encode_as_webp(
        &rgba_img,
        rgba_img.width(),
        rgba_img.height(),
    )
    .or_else(|_| crate::thumbnail::exif_thumb::encode_as_jpeg(&rgba_img))
    .map_err(|_| AppError::Internal("WebP encode failed".into()))?;

    ensure_thumb_dir(&config.cache_dir, config.size, cache_key).map_err(AppError::Io)?;
    let disk_path = thumb_path(&config.cache_dir, config.size, cache_key);
    write_atomic(&disk_path, &webp).map_err(AppError::from)?;

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
            .ok_or_else(|| AppError::Internal("resize buffer mismatch".into()));
    }

    use fast_image_resize::pixels::PixelType;
    use fast_image_resize::{images::Image as FirImage, ResizeOptions, Resizer};

    let (new_w, new_h) = if w >= h {
        let r = target as f32 / w as f32;
        (target, (h as f32 * r).round() as u32)
    } else {
        let r = target as f32 / h as f32;
        ((w as f32 * r).round() as u32, target)
    };

    let src = FirImage::from_slice_u8(w.max(1), h.max(1), pixels, PixelType::U8x4)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let mut dst = FirImage::new(new_w.max(1), new_h.max(1), PixelType::U8x4);

    use fast_image_resize::{FilterType, ResizeAlg};
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));

    let mut resizer = Resizer::new();
    resizer
        .resize(&src, &mut dst, &options)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    image::RgbaImage::from_raw(new_w.max(1), new_h.max(1), dst.into_vec())
        .ok_or_else(|| AppError::Internal("resize buffer mismatch".into()))
}

/// One-decode-two-outputs: emit the AI-analysis cache from an ALREADY-decoded image, so building
/// it costs no extra source decode. Only acts on WIDE images whose thumbnail short edge would fall
/// below `AI_CACHE_SHORT_EDGE` (squarer images' thumbnails already satisfy analysis → an AI cache
/// would just waste disk). Requires the decoded buffer's short edge ≥ target (no upscaling). The
/// AI pipeline discovers this file by `cache_key` (see `db::queries::PendingAiItem`). Best-effort.
///
/// 一次解码两份产物：从**已解码**图产出 AI 分析缓存，建它不再额外解码源图。仅处理缩略图短边会低于
/// `AI_CACHE_SHORT_EDGE` 的宽幅图（较方图的缩略图已满足分析 → 再建 AI 缓存纯浪费盘）。要求缓冲短边
/// ≥ 目标（不上采样）。AI 流水线按 `cache_key` 发现此文件（见 `db::queries::PendingAiItem`）。尽力而为。
fn maybe_write_ai_cache(
    cache_key: i64,
    decoded: &crate::engine::traits::DecodedImage,
    config: &ThumbConfig,
) -> Result<()> {
    let (w, h) = (decoded.width, decoded.height);
    let (long, short) = (w.max(h), w.min(h));
    if long == 0 || short == 0 {
        return Ok(());
    }
    // 仅当：缓冲短边已 ≥ 336（够大、无需上采样）且缩略图短边 < 336（较方图不必建 AI 缓存）。
    let thumb_short = (short as f32 * config.size as f32 / long as f32).round() as u32;
    if short < AI_CACHE_SHORT_EDGE || thumb_short >= AI_CACHE_SHORT_EDGE {
        return Ok(());
    }
    let disk = ai_cache_path(&config.cache_dir, cache_key);
    if disk.exists() {
        return Ok(()); // 已存在 → 跳过缩放/编码/写盘
    }

    let rgba = resize_short_edge_rgba(&decoded.pixels, w, h, AI_CACHE_SHORT_EDGE)?;
    let webp = crate::thumbnail::exif_thumb::encode_as_webp(&rgba, rgba.width(), rgba.height())
        .or_else(|_| crate::thumbnail::exif_thumb::encode_as_jpeg(&rgba))
        .map_err(|_| AppError::Internal("AI cache WebP encode failed".into()))?;
    ensure_ai_cache_dir(&config.cache_dir, cache_key).map_err(AppError::Io)?;
    write_atomic(&disk, &webp).map_err(AppError::from)?;
    Ok(())
}

/// Downscale RGBA pixels so the SHORT edge becomes `target_short` (aspect preserved). Never
/// upscales — callers guarantee short ≥ target. Bilinear via fast_image_resize (matches `resize_to_rgba`).
/// 按短边缩放 RGBA 到 `target_short`（保持比例）。绝不放大 —— 调用方保证短边≥目标。
/// fast_image_resize 双线性（与 `resize_to_rgba` 一致）。
fn resize_short_edge_rgba(
    pixels: &[u8],
    w: u32,
    h: u32,
    target_short: u32,
) -> Result<image::RgbaImage> {
    if w.min(h) <= target_short {
        return image::RgbaImage::from_raw(w, h, pixels.to_vec())
            .ok_or_else(|| AppError::Internal("ai cache buffer mismatch".into()));
    }

    use fast_image_resize::pixels::PixelType;
    use fast_image_resize::{
        images::Image as FirImage, FilterType, ResizeAlg, ResizeOptions, Resizer,
    };

    let scale = target_short as f32 / w.min(h) as f32;
    let new_w = ((w as f32 * scale).round() as u32).max(1);
    let new_h = ((h as f32 * scale).round() as u32).max(1);

    // fast_image_resize wants a `&mut [u8]`; the decoded buffer is borrowed immutably here, so copy.
    // fast_image_resize 需要 `&mut [u8]`；此处解码缓冲为不可变借用，故复制一份。
    let mut buf = pixels.to_vec();
    let src = FirImage::from_slice_u8(w.max(1), h.max(1), &mut buf, PixelType::U8x4)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let mut dst = FirImage::new(new_w, new_h, PixelType::U8x4);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
    Resizer::new()
        .resize(&src, &mut dst, &options)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    image::RgbaImage::from_raw(new_w, new_h, dst.into_vec())
        .ok_or_else(|| AppError::Internal("ai cache buffer mismatch".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R0-4:成功路径——最终文件内容完整、目录内不残留任何 .tmp。
    #[test]
    fn write_atomic_leaves_complete_file_and_no_tmp() {
        let dir = std::env::temp_dir().join(format!("picasa_wa_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("a1b2c3.webp");

        write_atomic(&target, b"hello-webp").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"hello-webp");

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .collect();
        assert!(leftovers.is_empty(), "no .tmp may remain after success");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// R0-4:覆盖路径——目标已存在(含模拟的「半截旧文件」)时 rename 原子替换为新完整内容。
    /// 这正是崩溃恢复场景:上一次直写留下的截断文件,重生成后必须被完整文件顶掉。
    #[test]
    fn write_atomic_replaces_existing_truncated_file() {
        let dir = std::env::temp_dir().join(format!("picasa_wa_rep_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("d4e5f6.webp");

        std::fs::write(&target, b"trunc").unwrap(); // 模拟半截旧缓存
        write_atomic(&target, b"full-new-content").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"full-new-content");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// snap_to_tier 回归:就近取整到 4 档。
    #[test]
    fn snap_to_tier_picks_nearest() {
        assert_eq!(snap_to_tier(100), 120);
        assert_eq!(snap_to_tier(200), 240);
        assert_eq!(snap_to_tier(400), 480);
        assert_eq!(snap_to_tier(2000), 960);
    }
}
