// src-tauri/src/video/mod.rs
//! Video backend abstraction (§1.4.1 / §3.2) — the capability-trait layer for video
//! probing & frame extraction, plus a tiny runtime registry that picks the best backend
//! for a given extension.
//!
//! This is the first concrete backend trait of the §1.4 plan: rather than landing an empty
//! abstraction in P0, the trait is introduced here together with its first real
//! implementation (`MediaFoundationBackend`, Windows, zero-bundle). FFmpeg (feature `ffmpeg`,
//! Perf-only) and AVFoundation (macOS) slot in later behind the same trait.
//!
//! 视频后端抽象（§1.4.1 / §3.2）—— 视频探测与取帧的能力 trait 层，外加一个极小的运行期注册表，
//! 按扩展名挑选最佳后端。这是 §1.4 计划落地的第一个具体后端 trait：不在 P0 留空抽象，
//! 而是与首个真实实现（`MediaFoundationBackend`，Windows，零捆绑）一并引入。FFmpeg
//! （feature `ffmpeg`，仅 Perf）与 AVFoundation（macOS）后续按同一 trait 接入。

use std::path::Path;

use crate::engine::traits::DecodedImage;
use crate::error::Result;

#[cfg(windows)]
pub mod media_foundation;

/// Cheaply-probed video metadata (no full decode). Dimensions are **display** dimensions —
/// rotation already applied, so `width`/`height` are what the upright frame measures
/// (mirrors how image EXIF orientation swaps w/h). Layout depends on these (§3.2).
/// 廉价探测的视频元数据（不全解码）。宽高为**显示**尺寸 —— 已应用 rotation，
/// 即 `width`/`height` 是正立帧的尺寸（与图片 EXIF orientation 交换宽高同理）。布局强依赖之（§3.2）。
#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    /// Rotation metadata in degrees (0/90/180/270). Stored for reference; the display
    /// dimensions above already account for it.
    /// 旋转元数据（度，0/90/180/270）。仅作记录；上面的显示尺寸已计入。
    pub rotation: i32,
    pub fps: f32,
    /// Average bitrate in bits/sec (0 if unknown).
    /// 平均比特率（比特/秒，未知为 0）。
    pub bitrate: u32,
    pub has_audio: bool,
    /// Short codec label (e.g. "H264", "HEVC"), `None` if unrecognised.
    /// 简短编解码标签（如 "H264"、"HEVC"），无法识别为 `None`。
    pub codec: Option<String>,
}

/// Video capability backend (§1.4.1). Each implementation handles a set of containers and
/// returns **upright** RGBA frames (rotation applied) so callers never re-handle orientation.
/// 视频能力后端（§1.4.1）。每个实现处理一组容器，返回**正立**的 RGBA 帧（已应用旋转），
/// 调用方无需再处理方向。
pub trait VideoBackend: Send + Sync {
    /// Stable backend id, e.g. "media-foundation" | "ffmpeg".
    /// 稳定后端 id，如 "media-foundation" | "ffmpeg"。
    fn name(&self) -> &'static str;

    /// Whether this backend can (likely) handle the given lowercase extension.
    /// 本后端是否（很可能）能处理给定的小写扩展名。
    fn can_handle(&self, ext: &str) -> bool;

    /// Probe dimensions / duration / rotation / fps / audio without a full decode.
    /// 在不全解码的情况下探测宽高 / 时长 / 旋转 / 帧率 / 是否含音频。
    fn probe(&self, path: &Path) -> Result<VideoInfo>;

    /// Decode one upright cover frame near `t_ms` (with simple black-frame avoidance).
    /// 解码 `t_ms` 附近的一帧正立封面（含简单的非黑帧规避）。
    fn cover(&self, path: &Path, t_ms: u64) -> Result<DecodedImage>;

    /// Decode `n` upright, uniformly-sized sample frames evenly spaced across the video,
    /// for a hover/scrub sprite (§3.3). Returned frames share one cell size.
    /// 解码 `n` 张正立、等尺寸、跨视频均匀采样的帧，用于悬停/进度条 scrub 雪碧图（§3.3）。
    /// 返回帧共享同一格尺寸。
    fn keyframes(&self, path: &Path, n: usize) -> Result<Vec<DecodedImage>>;
}

/// Pick the best available backend for a lowercase extension (§1.4.3). P2 ships only the
/// Media Foundation backend (Windows). When none match, the caller marks the item
/// `unsupported` (Lite + mkv/webm/flv → needs Perf/FFmpeg, §9).
/// 为小写扩展名挑选最佳可用后端（§1.4.3）。P2 仅交付 Media Foundation 后端（Windows）。
/// 无匹配时调用方标记 `unsupported`（Lite + mkv/webm/flv → 需 Perf/FFmpeg，§9）。
pub fn backend_for(ext: &str) -> Option<Box<dyn VideoBackend>> {
    let ext = ext.to_ascii_lowercase();
    #[cfg(windows)]
    {
        let mf = media_foundation::MediaFoundationBackend;
        if mf.can_handle(&ext) {
            return Some(Box::new(mf));
        }
    }
    // FfmpegBackend（feature = "ffmpeg"，仅 Perf）将在后续阶段于此接入，覆盖 MF 不支持的容器。
    let _ = ext;
    None
}
