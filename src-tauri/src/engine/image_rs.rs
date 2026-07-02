// src-tauri/src/engine/image_rs.rs
// src-tauri/src/engine/image_rs.rs
//! `ImageRsEngine`: uses the `image` crate to decode Phase 1 formats.
//! `ImageRsEngine`：使用 `image` crate 解码阶段 1 的格式。
//! Supported: jpg, jpeg, png, webp, bmp, gif, tif, tiff
//! 支持的格式：jpg, jpeg, png, webp, bmp, gif, tif, tiff

use image::ImageReader;
use std::path::Path;

use crate::engine::traits::{DecodedImage, ImageEngine, ResizeHint};
use crate::error::AppError;
use crate::scanner::metadata::read_jpeg_orientation;

pub struct ImageRsEngine;

impl ImageEngine for ImageRsEngine {
    fn name(&self) -> &str {
        "image-rs"
    }

    fn supported_formats(&self) -> &[&str] {
        &["jpg", "jpeg", "png", "webp", "bmp", "gif", "tif", "tiff"]
    }

    fn decode(
        &self,
        file_path: &Path,
        resize: Option<ResizeHint>,
    ) -> Result<DecodedImage, AppError> {
        let img = ImageReader::open(file_path)
            .map_err(AppError::Io)?
            .with_guessed_format()
            .map_err(AppError::Io)?
            .decode()
            .map_err(AppError::Engine)?;

        // Apply EXIF orientation for JPEG
        // 为 JPEG 应用 EXIF 方向
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        let img = if matches!(ext.as_str(), "jpg" | "jpeg") {
            apply_exif_orientation(img, file_path)
        } else {
            img
        };

        // Apply resize if requested
        // 如果有缩放请求则进行缩放
        let img = if let Some(hint) = resize {
            let (w, h) = (img.width(), img.height());
            match hint {
                ResizeHint::LongEdge(target) => {
                    if w > target || h > target {
                        let (nw, nh) = if w >= h {
                            (target, (h as f32 * target as f32 / w as f32).round() as u32)
                        } else {
                            ((w as f32 * target as f32 / h as f32).round() as u32, target)
                        };
                        img.resize_exact(
                            nw.max(1),
                            nh.max(1),
                            image::imageops::FilterType::CatmullRom,
                        )
                    } else {
                        img
                    }
                }
                ResizeHint::ShortEdge(target) => {
                    let short = w.min(h);
                    if short != target {
                        let scale = target as f32 / short as f32;
                        let nw = (w as f32 * scale).round() as u32;
                        let nh = (h as f32 * scale).round() as u32;
                        img.resize_exact(
                            nw.max(1),
                            nh.max(1),
                            image::imageops::FilterType::CatmullRom,
                        )
                    } else {
                        img
                    }
                }
            }
        } else {
            img
        };

        let rgba = img.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();

        Ok(DecodedImage {
            pixels: rgba.into_raw(),
            width,
            height,
        })
    }

    /// Try to extract the embedded JPEG thumbnail from EXIF (fast path).
    /// 尝试从 EXIF 中提取嵌入的 JPEG 缩略图（快速路径）。
    fn extract_embedded_thumb(&self, file_path: &Path) -> Result<Option<Vec<u8>>, AppError> {
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !matches!(ext.as_str(), "jpg" | "jpeg") {
            return Ok(None);
        }

        let file = std::fs::File::open(file_path).map_err(AppError::from)?;
        let mut reader = std::io::BufReader::new(file);
        let exif = exif::Reader::new().read_from_container(&mut reader).ok();

        let Some(exif) = exif else { return Ok(None) };

        // Look for the IFD1 (thumbnail) JPEG data
        // 查找 IFD1（缩略图）JPEG 数据
        if let Some(field) = exif.get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL) {
            if let exif::Value::Long(ref offsets) = field.value {
                if let Some(&offset) = offsets.first() {
                    // Get the thumbnail length
                    // 获取缩略图长度
                    if let Some(len_field) =
                        exif.get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL)
                    {
                        if let exif::Value::Long(ref lengths) = len_field.value {
                            if let Some(&length) = lengths.first() {
                                if length > 0 && length < 1_000_000 {
                                    // Re-open file and seek to thumbnail
                                    // 重新打开文件并查找到缩略图位置
                                    use std::io::{Read, Seek, SeekFrom};
                                    let mut f =
                                        std::fs::File::open(file_path).map_err(AppError::from)?;
                                    f.seek(SeekFrom::Start(offset as u64))
                                        .map_err(AppError::from)?;
                                    let mut buf = vec![0u8; length as usize];
                                    f.read_exact(&mut buf).map_err(AppError::from)?;
                                    // Validate JPEG magic
                                    // 验证 JPEG 魔数
                                    if buf.starts_with(&[0xFF, 0xD8]) {
                                        return Ok(Some(buf));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }
}

/// Apply EXIF orientation correction to a decoded image.
/// 将 EXIF 方向校正应用于解码后的图像。
fn apply_exif_orientation(img: image::DynamicImage, path: &Path) -> image::DynamicImage {
    let orientation = read_jpeg_orientation(path);
    match orientation {
        1 => img,
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}
