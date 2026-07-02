// src-tauri/src/derive/image.rs
//! Image derivation: the AI-analysis cache (§ AI pipeline). A pure `run(ctx) -> Result<Output>`;
//! the generic pipeline handles scheduling / resume / yield / orphan recovery.
//!
//! 图像派生：AI 分析缓存。纯函数 `run(ctx) -> Result<Output>`；通用流水线负责调度/续传/让步/孤儿恢复。
//!
//! # 为什么需要它
//! CLIP 分析按短边裁到 `image_size`（224/336）。若每次都解码全分辨率原图，24MP JPEG 的熵解码
//! 会把 CPU 全核占满、把 GPU 饿死（实测 CPU 99% / GPU 45%）。本派生预先把每张图缩成一份
//! **短边≥336** 的小 WebP，分析阶段只解这份小缓存 —— CPU 解码量降两个数量级，GPU 得以吃满。
//!
//! 短边取 336 而非 224：分析只会把短边**下采样**到 image_size、绝不上采样，故 336 同时覆盖
//! B/16·L/14（224）与 L/14@336（336）；做 224 则无法服务 336 模型且白占空间（用户要求不做 224）。

use crate::derive::kind::{DerivationContext, DerivationOutput};
use crate::engine::gpu::get_gpu_engine;
use crate::engine::image_rs::ImageRsEngine;
use crate::engine::traits::{DecodedImage, ImageEngine, ResizeHint};
use crate::error::{AppError, Result};
use crate::thumbnail::cache::{
    ai_cache_db_path, ai_cache_path, ensure_ai_cache_dir, AI_CACHE_SHORT_EDGE,
};

/// Decode the source at short-edge 336 (WIC GPU path, CPU `image` crate fallback), encode a
/// WebP, and write it to the AI cache dir keyed by `cache_key`. Skips the work (and re-decode)
/// if the cache file already exists — e.g. the thumbnail pipeline produced it in its own decode
/// pass (`generator.rs`), so this derivation only fills the gaps for already-thumbnailed images.
/// 按短边 336 解码源图（WIC GPU 路径，CPU `image` crate 回退），编码 WebP，按 `cache_key` 写入 AI 缓存目录。
/// 若缓存文件已存在则跳过（免去重复解码）—— 例如缩略图流水线已在自己的解码里顺带产出（见 generator.rs），
/// 本派生只为「已生成缩略图、缺 AI 缓存」的存量图补齐。
pub fn run_ai_thumb(ctx: &DerivationContext) -> Result<DerivationOutput> {
    // Already on disk (thumbnail pipeline or a previous run) → just record the path, don't re-decode.
    // 已在磁盘（缩略图流水线或上次运行所产）→ 仅记录路径，不重复解码。
    if ai_cache_path(&ctx.cache_dir, ctx.cache_key).exists() {
        return Ok(DerivationOutput {
            payload_path: Some(ai_cache_db_path(ctx.cache_key)),
            thumbhash: None,
            page_count: None,
        });
    }

    let decoded = decode_short_edge(&ctx.file_format, &ctx.abs_path, AI_CACHE_SHORT_EDGE)?;

    let (w, h) = (decoded.width, decoded.height);
    let rgba = image::RgbaImage::from_raw(w, h, decoded.pixels).ok_or_else(|| {
        AppError::Internal("AI cache buffer size mismatch | AI 缓存缓冲尺寸不符".into())
    })?;

    // Reuse the thumbnail WebP/JPEG encoders (same quality knobs as display thumbnails).
    // 复用缩略图的 WebP/JPEG 编码器（与显示缩略图同质量参数）。
    let bytes = crate::thumbnail::exif_thumb::encode_as_webp(&rgba, w, h)
        .or_else(|_| crate::thumbnail::exif_thumb::encode_as_jpeg(&rgba))
        .map_err(|_| {
            AppError::Internal("AI cache WebP encode failed | AI 缓存 WebP 编码失败".into())
        })?;

    ensure_ai_cache_dir(&ctx.cache_dir, ctx.cache_key).map_err(AppError::Io)?;
    let disk = ai_cache_path(&ctx.cache_dir, ctx.cache_key);
    std::fs::write(&disk, &bytes).map_err(AppError::from)?;

    Ok(DerivationOutput {
        payload_path: Some(ai_cache_db_path(ctx.cache_key)),
        thumbhash: None,
        page_count: None,
    })
}

/// Decode an image to a `DecodedImage` with short edge resized to `target`, preferring the
/// GPU (WIC) engine and falling back to the CPU `image` crate engine — mirrors the AI
/// pipeline's own decode so the cache matches what analysis would otherwise produce.
/// 把图像解码为短边缩到 `target` 的 `DecodedImage`，优先 GPU（WIC）引擎、回退 CPU `image` crate
/// 引擎 —— 与 AI 流水线自身解码一致，使缓存与"直接分析"产物相同。
fn decode_short_edge(
    file_format: &str,
    path: &std::path::Path,
    target: u32,
) -> Result<DecodedImage> {
    let hint = Some(ResizeHint::ShortEdge(target));

    if let Some(gpu) = get_gpu_engine("wic") {
        if gpu.can_handle(file_format) {
            match gpu.decode(path, hint) {
                Ok(d) => return Ok(d),
                Err(e) => tracing::debug!(
                    "AI cache GPU decode failed, falling back to CPU | AI 缓存 GPU 解码失败，回退 CPU: {}",
                    e
                ),
            }
        }
    }

    if !ImageRsEngine.can_handle(file_format) {
        return Err(AppError::UnsupportedFormat(file_format.to_string()));
    }
    ImageRsEngine.decode(path, hint)
}
