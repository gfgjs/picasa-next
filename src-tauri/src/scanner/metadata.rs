// src-tauri/src/scanner/metadata.rs
//! EXIF and XMP metadata parsing.
//! EXIF 和 XMP 元数据解析。
//!
//! Uses `kamadak-exif` for EXIF and `quick-xml` for XMP (Motion Photo detection).
//! 使用 `kamadak-exif` 解析 EXIF，使用 `quick-xml` 解析 XMP（动态照片检测）。

use std::io::BufReader;
use std::path::Path;

use crate::db::models::ImageMeta;
use crate::error::{AppError, Result};

// ── EXIF orientation (fast path — for quick scan) ────────────────────────────
// ── EXIF 方向（快速路径 — 用于快速扫描） ────────────────────────────

/// Read only the EXIF Orientation tag from a JPEG.
/// 仅读取 JPEG 的 EXIF 方向标签。
/// Returns the orientation value (1-8), or `1` if not present / on error.
/// 返回方向值 (1-8)，如果不存在 / 出错则返回 `1`。
/// This is lightweight: kamadak-exif reads just enough bytes to find the tag.
/// 这是轻量级的：kamadak-exif 仅读取足够的字节来寻找标签。
pub fn read_jpeg_orientation(path: &Path) -> u32 {
    read_orientation_inner(path).unwrap_or(1)
}

fn read_orientation_inner(path: &Path) -> Option<u32> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = exif::Reader::new()
        .read_from_container(&mut reader)
        .ok()?;
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    match field.value {
        exif::Value::Short(ref v) => v.first().copied().map(|n| n as u32),
        _ => None,
    }
}

/// Returns `true` if the orientation value requires 90° / 270° rotation
/// 如果方向值需要 90° / 270° 旋转，则返回 `true`
/// (i.e., width and height should be swapped).
/// （即宽度和高度应该互换）。
pub fn orientation_needs_swap(orientation: u32) -> bool {
    matches!(orientation, 5..=8)
}

// ── Full EXIF parse (enrichment phase) ───────────────────────────────────────
// ── 完整 EXIF 解析（丰富信息阶段） ───────────────────────────────────────

/// Parse full EXIF metadata from an image file.
/// 从图像文件解析完整的 EXIF 元数据。
/// Returns a partially-filled `ImageMeta` (item_id will be set by the caller).
/// 返回部分填充的 `ImageMeta`（item_id 将由调用者设置）。
pub fn parse_exif_meta(path: &Path) -> Result<ImageMeta> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let exif = exif::Reader::new()
        .read_from_container(&mut reader)
        .map_err(AppError::from)?;

    let mut meta = ImageMeta::default();

    // Orientation
    // 方向
    if let Some(f) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
        if let exif::Value::Short(ref v) = f.value {
            meta.orientation = v.first().copied().unwrap_or(1) as i64;
        }
    }

    // DateTime (original → digitised → modified)
    // 日期时间 (原始 → 数字化 → 修改)
    for tag in [
        exif::Tag::DateTimeOriginal,
        exif::Tag::DateTimeDigitized,
        exif::Tag::DateTime,
    ] {
        if let Some(f) = exif.get_field(tag, exif::In::PRIMARY) {
            if let exif::Value::Ascii(ref v) = f.value {
                if let Some(dt_str) = v.first().and_then(|b| std::str::from_utf8(b).ok()) {
                    if let Some(ts) = parse_exif_datetime(dt_str) {
                        meta.exif_datetime = Some(ts);
                        break;
                    }
                }
            }
        }
    }

    // Camera make / model / lens
    // 相机制造商 / 型号 / 镜头
    meta.exif_make  = get_ascii_field(&exif, exif::Tag::Make);
    meta.exif_model = get_ascii_field(&exif, exif::Tag::Model);
    meta.exif_lens  = get_ascii_field(&exif, exif::Tag::LensModel);

    // Focal length (mm)
    // 焦距 (mm)
    if let Some(f) = exif.get_field(exif::Tag::FocalLength, exif::In::PRIMARY) {
        meta.exif_focal_length = rational_to_f64(&f.value);
    }

    // Aperture (F-number)
    // 光圈 (F 值)
    if let Some(f) = exif.get_field(exif::Tag::FNumber, exif::In::PRIMARY) {
        meta.exif_aperture = rational_to_f64(&f.value);
    }

    // Shutter speed (ExposureTime as "1/200" string)
    // 快门速度 (ExposureTime 作为 "1/200" 字符串)
    if let Some(f) = exif.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY) {
        if let exif::Value::Rational(ref v) = f.value {
            if let Some(r) = v.first() {
                meta.exif_shutter = Some(format!("{}/{}", r.num, r.denom));
            }
        }
    }

    // ISO
    // ISO
    if let Some(f) = exif.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY) {
        if let exif::Value::Short(ref v) = f.value {
            meta.exif_iso = v.first().copied().map(|n| n as i64);
        }
    }

    // GPS
    // GPS
    if let (Some(lat), Some(lat_ref), Some(lng), Some(lng_ref)) = (
        exif.get_field(exif::Tag::GPSLatitude,     exif::In::PRIMARY),
        exif.get_field(exif::Tag::GPSLatitudeRef,  exif::In::PRIMARY),
        exif.get_field(exif::Tag::GPSLongitude,    exif::In::PRIMARY),
        exif.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY),
    ) {
        if let (Some(lat_dd), Some(lng_dd)) = (
            dms_to_decimal(&lat.value),
            dms_to_decimal(&lng.value),
        ) {
            let lat_sign = if get_ascii_field(&exif, exif::Tag::GPSLatitudeRef)
                .as_deref() == Some("S") { -1.0 } else { 1.0 };
            let lng_sign = if get_ascii_field(&exif, exif::Tag::GPSLongitudeRef)
                .as_deref() == Some("W") { -1.0 } else { 1.0 };
            meta.exif_gps_lat = Some(lat_dd * lat_sign);
            meta.exif_gps_lng = Some(lng_dd * lng_sign);
        }
        let _ = (lat_ref, lng_ref); // suppress unused warning
                                    // 抑制未使用警告
    }

    Ok(meta)
}

// ── XMP Motion Photo detection ────────────────────────────────────────────────
// ── XMP 动态照片检测 ────────────────────────────────────────────────

/// Scan the first 128 KB of a JPEG for XMP Motion Photo markers.
/// 扫描 JPEG 的前 128 KB 以寻找 XMP 动态照片标记。
/// Returns `(is_live_photo, has_embedded_video)`.
/// 返回 `(is_live_photo, has_embedded_video)`。
pub fn detect_motion_photo_xmp(path: &Path) -> (bool, bool) {
    let Ok(mut file) = std::fs::File::open(path) else {
        return (false, false);
    };
    use std::io::Read;
    let mut buf = vec![0u8; 131_072]; // 128 KB
    let n = file.read(&mut buf).unwrap_or(0);
    let text = String::from_utf8_lossy(&buf[..n]);

    // Google Motion Photo marker
    // Google 动态照片标记
    let google = text.contains("GCamera:MotionPhoto=\"1\"")
        || text.contains("Camera:MotionPhoto=\"1\"")
        || text.contains("MotionPhoto=\"1\"");

    // Samsung Motion Photo marker
    // 三星动态照片标记
    let samsung = text.contains("MotionPhoto_Capture_Type")
        || text.contains("com.samsung.android.photo");

    (google || samsung, google || samsung)
}

// ── Helpers ───────────────────────────────────────────────────────────────────
// ── 辅助函数 ───────────────────────────────────────────────────────────────────

fn get_ascii_field(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    exif.get_field(tag, exif::In::PRIMARY).and_then(|f| {
        if let exif::Value::Ascii(ref v) = f.value {
            v.first()
                .and_then(|b| std::str::from_utf8(b).ok())
                .map(|s| s.trim_end_matches('\0').trim().to_string())
        } else {
            None
        }
    })
}

fn rational_to_f64(value: &exif::Value) -> Option<f64> {
    if let exif::Value::Rational(ref v) = value {
        v.first().map(|r| r.num as f64 / r.denom as f64)
    } else {
        None
    }
}

fn dms_to_decimal(value: &exif::Value) -> Option<f64> {
    if let exif::Value::Rational(ref v) = value {
        if v.len() >= 3 {
            let deg  = v[0].num as f64 / v[0].denom as f64;
            let min  = v[1].num as f64 / v[1].denom as f64;
            let sec  = v[2].num as f64 / v[2].denom as f64;
            return Some(deg + min / 60.0 + sec / 3600.0);
        }
    }
    None
}

/// Parse an EXIF datetime string (`"2024:03:15 10:30:00"`) to a Unix timestamp.
/// 将 EXIF 日期时间字符串 (`"2024:03:15 10:30:00"`) 解析为 Unix 时间戳。
fn parse_exif_datetime(s: &str) -> Option<i64> {
    // Format: "YYYY:MM:DD HH:MM:SS"
    // 格式: "YYYY:MM:DD HH:MM:SS"
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let year:   i32 = s[0..4].parse().ok()?;
    let month:  u32 = s[5..7].parse().ok()?;
    let day:    u32 = s[8..10].parse().ok()?;
    let hour:   u32 = s[11..13].parse().ok()?;
    let minute: u32 = s[14..16].parse().ok()?;
    let second: u32 = s[17..19].parse().ok()?;

    // Simple UTC timestamp (ignores timezone)
    // 简单的 UTC 时间戳（忽略时区）
    use chrono::{TimeZone, Utc};
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .map(|dt| dt.timestamp())
}
