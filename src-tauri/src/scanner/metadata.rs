// src-tauri/src/scanner/metadata.rs
//! EXIF and XMP metadata parsing.
//! EXIF 和 XMP 元数据解析。
//!
//! Uses `kamadak-exif` for EXIF and `quick-xml` for XMP (Motion Photo detection).
//! 使用 `kamadak-exif` 解析 EXIF，使用 `quick-xml` 解析 XMP（动态照片检测）。

use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

use crate::db::models::ImageMeta;
use crate::error::{AppError, Result};

/// TIFF 维度解析的硬超时上限。读文件头本应亚秒级完成；给 5s 余量以容忍慢盘 /
/// 合法大文件，同时对畸形 TIFF 的无限阻塞兜底。
const TIFF_DIMENSION_TIMEOUT: Duration = Duration::from_secs(5);

/// 在 detached 线程上运行 `f`，最多等待 `timeout`；超时 / 线程 panic 返回 `None`。
///
/// 用于给可能无限阻塞的第三方解析（如畸形 TIFF 的 `image::image_dimensions`）设硬上限：
/// 超时即放弃等待、立即返回，让出当前工作线程。落单线程在后台自行跑完退出——
/// 最坏情况泄漏一个线程，但远胜永久阻塞 enrich 工作者。
///
/// 注意：`std::thread::scope` 无法实现真超时——其 drop 必须 join 完所有子线程才返回，
/// 与「超时即放弃」语义互斥；故此处用 detached spawn + `recv_timeout`。
fn run_with_timeout<T, F>(timeout: Duration, f: F) -> Option<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // 接收端可能已超时丢弃 rx → send 失败属预期，忽略。
        let _ = tx.send(f());
    });
    // Err 同时覆盖 Timeout 与 Disconnected（线程 panic 未发送即丢弃 tx）——都按失败处理。
    rx.recv_timeout(timeout).ok()
}

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
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
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

/// Header-only pixel dimensions, WITHOUT orientation correction and WITHOUT any
/// EXIF read (TIFF gets a scoped-thread timeout guard). Returns `(0, 0)` on failure.
/// 仅读文件头的像素尺寸：不做方向校正、不读 EXIF（TIFF 用作用域线程加超时保护）。失败返回 `(0, 0)`。
pub fn read_raw_dimensions(abs_path: &Path, ext: &str) -> (i64, i64) {
    // TIFF: 解析可能读取大量字节、且畸形文件可能无限阻塞 — 用真超时守卫兜底。
    // TIFF: parsing can read many bytes and a malformed file may hang — guard with a real timeout.
    if ext == "tif" || ext == "tiff" {
        let path = abs_path.to_path_buf();
        return run_with_timeout(TIFF_DIMENSION_TIMEOUT, move || {
            image::image_dimensions(&path).ok()
        })
        .flatten()
        .map(|(w, h)| (w as i64, h as i64))
        .unwrap_or((0, 0));
    }

    image::image_dimensions(abs_path)
        .map(|(w, h)| (w as i64, h as i64))
        .unwrap_or((0, 0))
}

/// Swap `(w, h)` when the EXIF orientation indicates a 90°/270° rotation.
/// 当 EXIF 方向表示 90°/270° 旋转时交换 `(w, h)`。
pub fn apply_orientation_swap(dims: (i64, i64), orientation: u32) -> (i64, i64) {
    if orientation_needs_swap(orientation) {
        (dims.1, dims.0)
    } else {
        dims
    }
}

/// Orientation-corrected dimensions. For JPEG this reads the EXIF Orientation tag
/// (one extra file open); callers that have ALREADY parsed EXIF should instead use
/// `read_raw_dimensions` + `apply_orientation_swap` with the known orientation to
/// avoid re-opening the file.
/// 经方向校正的尺寸。JPEG 会读取 EXIF 方向标签（多开一次文件）；已解析过 EXIF 的调用方
/// 应改用 `read_raw_dimensions` + `apply_orientation_swap` 传入已知方向，避免重复打开文件。
///
/// Single-sourced so the fast-scan eager path and the viewport-priority path stay
/// consistent (same orientation handling → no double-flip).
/// 在此单一实现，使快速扫描即时路径与可视窗口优先路径一致（相同方向处理 → 不会双重翻转）。
pub fn read_image_dimensions(abs_path: &Path, ext: &str) -> (i64, i64) {
    let dims = read_raw_dimensions(abs_path, ext);
    if dims == (0, 0) {
        return (0, 0);
    }
    if ext == "jpg" || ext == "jpeg" {
        apply_orientation_swap(dims, read_jpeg_orientation(abs_path))
    } else {
        dims
    }
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
    meta.exif_make = get_ascii_field(&exif, exif::Tag::Make);
    meta.exif_model = get_ascii_field(&exif, exif::Tag::Model);
    meta.exif_lens = get_ascii_field(&exif, exif::Tag::LensModel);

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
        exif.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY),
        exif.get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY),
        exif.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY),
        exif.get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY),
    ) {
        if let (Some(lat_dd), Some(lng_dd)) =
            (dms_to_decimal(&lat.value), dms_to_decimal(&lng.value))
        {
            let lat_sign =
                if get_ascii_field(&exif, exif::Tag::GPSLatitudeRef).as_deref() == Some("S") {
                    -1.0
                } else {
                    1.0
                };
            let lng_sign =
                if get_ascii_field(&exif, exif::Tag::GPSLongitudeRef).as_deref() == Some("W") {
                    -1.0
                } else {
                    1.0
                };
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
    let samsung =
        text.contains("MotionPhoto_Capture_Type") || text.contains("com.samsung.android.photo");

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
            let deg = v[0].num as f64 / v[0].denom as f64;
            let min = v[1].num as f64 / v[1].denom as f64;
            let sec = v[2].num as f64 / v[2].denom as f64;
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
    let year: i32 = s[0..4].parse().ok()?;
    let month: u32 = s[5..7].parse().ok()?;
    let day: u32 = s[8..10].parse().ok()?;
    let hour: u32 = s[11..13].parse().ok()?;
    let minute: u32 = s[14..16].parse().ok()?;
    let second: u32 = s[17..19].parse().ok()?;

    // Simple UTC timestamp (ignores timezone)
    // 简单的 UTC 时间戳（忽略时区）
    use chrono::{TimeZone, Utc};
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .map(|dt| dt.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_with_timeout_returns_value_for_fast_closure() {
        // 快速闭包应在超时前返回其值。
        let r = run_with_timeout(Duration::from_secs(5), || 42);
        assert_eq!(r, Some(42));
    }

    #[test]
    fn run_with_timeout_gives_up_on_slow_closure() {
        // 慢闭包（模拟畸形 TIFF 挂起）：超时即放弃，返回 None，不等满 10s。
        // 关键：本测试自身只阻塞约 50ms（超时时长），不会真等 10s——证明「不 join」生效。
        let r: Option<i32> = run_with_timeout(Duration::from_millis(50), || {
            std::thread::sleep(Duration::from_secs(10));
            42
        });
        assert_eq!(r, None);
    }

    #[test]
    fn run_with_timeout_returns_none_on_panic() {
        // 子线程 panic 未发送即丢弃 tx → recv_timeout 得 Disconnected → None（不传播 panic）。
        let r: Option<i32> = run_with_timeout(Duration::from_secs(5), || panic!("boom"));
        assert_eq!(r, None);
    }

    #[test]
    fn read_raw_dimensions_non_tiff_missing_file_is_zero() {
        // 非 TIFF 缺失文件走直读分支，失败回落 (0,0)，不 panic。
        let (w, h) = read_raw_dimensions(Path::new("/nonexistent/x.jpg"), "jpg");
        assert_eq!((w, h), (0, 0));
    }

    #[test]
    fn read_raw_dimensions_tiff_missing_file_is_zero() {
        // TIFF 缺失文件走超时守卫分支：解析立即失败（非超时），仍回落 (0,0)。
        let (w, h) = read_raw_dimensions(Path::new("/nonexistent/x.tiff"), "tiff");
        assert_eq!((w, h), (0, 0));
    }
}
