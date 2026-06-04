// src-tauri/src/scanner/video_meta.rs
//! Extract video width, height and duration from MP4/MOV containers using mp4parse.
//! 使用 mp4parse 从 MP4/MOV 容器中提取视频宽度、高度和时长。
//!
//! Only supports ISO Base Media File Format (MP4, MOV, M4V).
//! 仅支持 ISO 基础媒体文件格式（MP4、MOV、M4V）。
//! Other containers (AVI, MKV, etc.) return `None` and should be handled later via FFprobe.
//! 其他容器（AVI、MKV 等）返回 `None`，将来通过 FFprobe 处理。

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use tracing::warn;

/// Video metadata extracted by mp4parse.
/// mp4parse 提取的视频元数据。
#[derive(Debug, Clone)]
pub struct VideoMeta {
    /// Width in pixels | 宽度（像素）
    pub width:       i64,
    /// Height in pixels | 高度（像素）
    pub height:      i64,
    /// Duration in milliseconds | 时长（毫秒）
    pub duration_ms: i64,
}

/// Try to parse a file as an MP4/MOV and extract width, height, duration.
/// 尝试将文件解析为 MP4/MOV，提取宽度、高度和时长。
///
/// Returns `None` if the file is not a supported container or parsing fails.
/// 如果文件不是受支持的容器或解析失败，返回 `None`。
pub fn extract_video_meta(path: &Path) -> Option<VideoMeta> {
    // 只处理已知的 MP4/MOV 后缀 | Only handle known MP4/MOV extensions
    let ext = path.extension()?.to_str()?.to_lowercase();
    if !matches!(ext.as_str(), "mp4" | "mov" | "m4v") {
        return None;
    }

    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);

    let context = mp4parse::read_mp4(&mut reader)
        .map_err(|e| {
            warn!("[VideoMeta] mp4parse error for {:?}: {:?} | mp4parse 解析失败", path.file_name().unwrap_or_default(), e);
        })
        .ok()?;

    // 找到第一个视频 track | Find first video track
    let video_track = context.tracks.iter().find(|t| {
        t.track_type == mp4parse::TrackType::Video
    })?;

    // 提取时长（timescale 单位转换为毫秒）| Convert timescale ticks to milliseconds
    let duration_ms = video_track.duration.map(|d| {
        let timescale = video_track.timescale
            .map(|ts| ts.0 as i64)
            .unwrap_or(1000);
        let timescale = timescale.max(1);
        d.0 as i64 * 1000 / timescale
    }).unwrap_or(0);

    // 提取宽高（从 stsd sample entry）| Extract width and height from stsd
    let (width, height) = video_track.stsd.as_ref().and_then(|stsd| {
        stsd.descriptions.first().and_then(|desc| {
            match desc {
                mp4parse::SampleEntry::Video(v) => Some((v.width as i64, v.height as i64)),
                _ => None,
            }
        })
    }).unwrap_or((0, 0));

    if width == 0 && height == 0 && duration_ms == 0 {
        return None;
    }

    Some(VideoMeta { width, height, duration_ms })
}
