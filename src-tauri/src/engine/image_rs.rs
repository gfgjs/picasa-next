// src-tauri/src/engine/image_rs.rs
//! `ImageRsEngine`: uses the `image` crate to decode Phase 1 formats.
//! Supported: jpg, jpeg, png, webp, bmp, gif, tif, tiff

use std::path::Path;
use image::ImageReader;

use crate::engine::traits::{DecodedImage, ImageEngine};
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

    fn decode(&self, file_path: &Path) -> Result<DecodedImage, AppError> {
        let img = ImageReader::open(file_path)
            .map_err(|e| AppError::Engine(e.to_string()))?
            .with_guessed_format()
            .map_err(|e| AppError::Engine(e.to_string()))?
            .decode()
            .map_err(|e| AppError::Engine(e.to_string()))?;

        // Apply EXIF orientation for JPEG
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

        let rgba = img.to_rgba8();
        let width  = rgba.width();
        let height = rgba.height();

        Ok(DecodedImage {
            pixels: rgba.into_raw(),
            width,
            height,
        })
    }

    /// Try to extract the embedded JPEG thumbnail from EXIF (fast path).
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
        let exif = exif::Reader::new()
            .read_from_container(&mut reader)
            .ok();

        let Some(exif) = exif else { return Ok(None) };

        // Look for the IFD1 (thumbnail) JPEG data
        if let Some(field) = exif.get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL) {
            if let exif::Value::Long(ref offsets) = field.value {
                if let Some(&offset) = offsets.first() {
                    // Get the thumbnail length
                    if let Some(len_field) = exif.get_field(
                        exif::Tag::JPEGInterchangeFormatLength,
                        exif::In::THUMBNAIL,
                    ) {
                        if let exif::Value::Long(ref lengths) = len_field.value {
                            if let Some(&length) = lengths.first() {
                                if length > 0 && length < 1_000_000 {
                                    // Re-open file and seek to thumbnail
                                    use std::io::{Read, Seek, SeekFrom};
                                    let mut f = std::fs::File::open(file_path)
                                        .map_err(AppError::from)?;
                                    f.seek(SeekFrom::Start(offset as u64))
                                        .map_err(AppError::from)?;
                                    let mut buf = vec![0u8; length as usize];
                                    f.read_exact(&mut buf).map_err(AppError::from)?;
                                    // Validate JPEG magic
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
