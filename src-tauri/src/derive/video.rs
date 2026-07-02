// src-tauri/src/derive/video.rs
//! Video derivations: poster cover (§3.2) and keyframe sprite (§3.3), driven by the
//! `VideoBackend` (Media Foundation on Windows). Each is a pure `run(ctx) -> Result<Output>`;
//! the pipeline handles scheduling / resume / yield / orphan recovery.
//!
//! 视频派生：封面帧（§3.2）与关键帧雪碧图（§3.3），由 `VideoBackend`（Windows 下为 Media
//! Foundation）驱动。每个都是纯函数 `run(ctx) -> Result<Output>`；流水线负责调度/续传/让步/孤儿恢复。

use crate::derive::kind::{DerivationContext, DerivationOutput};
use crate::error::{AppError, Result};
use crate::thumbnail::cache::{ensure_sprite_dir, keyframe_sprite_db_path, keyframe_sprite_path};
use crate::thumbnail::generator::{encode_media_step, snap_to_tier, ThumbConfig};
use crate::video::backend_for;

/// Number of sample frames packed into one keyframe scrub sprite (§3.3). Kept in sync with the
/// frontend, which derives cell width from `spriteNaturalWidth / KEYFRAME_COUNT`.
/// 拼入一张关键帧 scrub 雪碧图的采样帧数（§3.3）。与前端保持一致 —— 前端用
/// `雪碧图原始宽度 / KEYFRAME_COUNT` 推导每格宽度。
pub const KEYFRAME_COUNT: usize = 10;

/// Extract a poster frame and write it as a WebP cover (§3.2). The cover is written to the exact
/// thumbnail cache path/key, so the pipeline can mirror `thumb_status=1 / thumb_path` onto
/// `media_items` and `MediaThumb` shows it with zero frontend changes.
/// 抽取封面帧并写为 WebP 封面（§3.2）。封面写入精确的缩略图缓存路径/键，使流水线能把
/// `thumb_status=1 / thumb_path` 回填到 `media_items`，`MediaThumb` 零改动即可显示。
pub fn run_cover(ctx: &DerivationContext) -> Result<DerivationOutput> {
    let backend = backend_for(&ctx.file_format)
        .ok_or_else(|| AppError::UnsupportedFormat(ctx.file_format.clone()))?;

    // Probe duration to pick a cover time = min(1s, 10% of duration); avoids the often-black
    // very first frame. Probe is cheap (metadata only). If probe fails, fall back to 1s.
    // 探测时长以选取封面时间 = min(1s, 时长的 10%)；避开常为黑帧的最初一帧。探测很廉价（仅元数据）。
    // 探测失败则回退到 1s。
    let cover_t_ms = match backend.probe(&ctx.abs_path) {
        Ok(info) if info.duration_ms > 0 => 1000u64.min(info.duration_ms / 10),
        _ => 1000,
    };

    let decoded = backend.cover(&ctx.abs_path, cover_t_ms)?;

    // Reuse the thumbnail encoder: resize → WebP → write to thumb cache (by cache_key) → thumbhash.
    // 复用缩略图编码器：缩放 → WebP → 写入缩略图缓存（按 cache_key）→ thumbhash。
    let cfg = ThumbConfig {
        cache_dir: ctx.cache_dir.clone(),
        size: snap_to_tier(ctx.thumb_size),
        skip_max_bytes: 0,
        strategy: String::new(),
        gpu_engine: String::new(),
        ai_hq_cache: false, // 视频封面非 CLIP 分析对象，不产 AI 缓存
    };
    let res = encode_media_step(ctx.item_id, ctx.cache_key, decoded, &cfg)?;

    Ok(DerivationOutput {
        payload_path: res.thumb_path,
        thumbhash: res.thumbhash,
        page_count: None,
    })
}

/// Sample N frames and pack them into one horizontal keyframe sprite (§3.3) for hover/scrub.
/// 采样 N 帧并拼为一张水平关键帧雪碧图（§3.3），用于悬停/进度条 scrub。
pub fn run_keyframes(ctx: &DerivationContext) -> Result<DerivationOutput> {
    let backend = backend_for(&ctx.file_format)
        .ok_or_else(|| AppError::UnsupportedFormat(ctx.file_format.clone()))?;

    let frames = backend.keyframes(&ctx.abs_path, KEYFRAME_COUNT)?;
    if frames.is_empty() {
        return Err(AppError::Internal("no keyframes | 无关键帧".into()));
    }

    // All frames share one cell size (uniform aspect). Pack left→right into a single strip.
    // 所有帧共享一个格尺寸（统一比例）。从左到右拼成单条带。
    let cell_w = frames[0].width;
    let cell_h = frames[0].height;
    let cols = frames.len() as u32;
    let sprite_w = cell_w * cols;
    let sprite_h = cell_h;

    let mut sprite = image::RgbaImage::new(sprite_w, sprite_h);
    for (i, f) in frames.iter().enumerate() {
        // Skip any frame whose size drifted (defensive — backend resizes uniformly).
        // 跳过尺寸漂移的帧（防御性 —— 后端按统一尺寸缩放）。
        if f.width != cell_w || f.height != cell_h {
            continue;
        }
        let Some(frame_img) = image::RgbaImage::from_raw(f.width, f.height, f.pixels.clone())
        else {
            continue;
        };
        let x0 = i as u32 * cell_w;
        image::imageops::overlay(&mut sprite, &frame_img, x0 as i64, 0);
    }

    let webp = crate::thumbnail::exif_thumb::encode_as_webp(&sprite, sprite_w, sprite_h)
        .or_else(|_| crate::thumbnail::exif_thumb::encode_as_jpeg(&sprite))
        .map_err(|_| AppError::Internal("sprite WebP encode failed | 雪碧图编码失败".into()))?;

    ensure_sprite_dir(&ctx.cache_dir, ctx.cache_key).map_err(AppError::Io)?;
    let disk_path = keyframe_sprite_path(&ctx.cache_dir, ctx.cache_key);
    std::fs::write(&disk_path, &webp).map_err(AppError::from)?;

    Ok(DerivationOutput {
        payload_path: Some(keyframe_sprite_db_path(ctx.cache_key)),
        thumbhash: None,
        page_count: None,
    })
}
