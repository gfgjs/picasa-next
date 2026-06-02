// src-tauri/src/engine/gpu/wic_engine.rs
//! WIC (Windows Imaging Component) Engine.
//! Leverages OS-native codecs for extremely fast decoding, often with hardware acceleration (e.g., for JPEG/HEIC).

use std::path::Path;
use windows::core::{HSTRING, Interface};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::Foundation::GENERIC_READ;

use crate::engine::traits::{DecodedImage, ImageEngine};
use crate::error::AppError;
use crate::scanner::metadata::read_jpeg_orientation;

pub struct WicEngine;

impl ImageEngine for WicEngine {
    fn name(&self) -> &str {
        "wic"
    }

    fn supported_formats(&self) -> &[&str] {
        &["jpg", "jpeg", "png", "bmp", "tif", "tiff", "heic", "heif", "avif"]
    }

    fn decode(&self, file_path: &Path, target_size: Option<u32>) -> Result<DecodedImage, AppError> {
        let abs_path = file_path.to_string_lossy().to_string();

        unsafe {
            // Initialize COM. It might already be initialized by Tauri or another thread, so we ignore errors.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let factory: IWICImagingFactory = windows::Win32::System::Com::CoCreateInstance(
                &CLSID_WICImagingFactory,
                None,
                windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
            ).map_err(|e| AppError::Engine(format!("Failed to create WIC factory: {}", e)))?;

            // Create Decoder
            let decoder = factory.CreateDecoderFromFilename(
                &HSTRING::from(&abs_path),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            ).map_err(|e| AppError::Engine(format!("WIC CreateDecoderFromFilename failed: {}", e)))?;

            // Get first frame
            let frame = decoder.GetFrame(0)
                .map_err(|e| AppError::Engine(format!("WIC GetFrame failed: {}", e)))?;

            // Get dimensions
            let mut width = 0;
            let mut height = 0;
            frame.GetSize(&mut width, &mut height)
                .map_err(|e| AppError::Engine(format!("WIC GetSize failed: {}", e)))?;

            // Convert to 32bppRGBA
            let converter = factory.CreateFormatConverter()
                .map_err(|e| AppError::Engine(format!("WIC CreateFormatConverter failed: {}", e)))?;

            let mut scaled_width = width;
            let mut scaled_height = height;

            let source: IWICBitmapSource = if let Some(target) = target_size {
                if width > target || height > target {
                    if width >= height {
                        let r = target as f32 / width as f32;
                        scaled_width = target;
                        scaled_height = (height as f32 * r).round() as u32;
                    } else {
                        let r = target as f32 / height as f32;
                        scaled_height = target;
                        scaled_width = (width as f32 * r).round() as u32;
                    }

                    let scaler = factory.CreateBitmapScaler()
                        .map_err(|e| AppError::Engine(format!("WIC CreateBitmapScaler failed: {}", e)))?;
                    scaler.Initialize(
                        &frame,
                        scaled_width,
                        scaled_height,
                        WICBitmapInterpolationModeFant,
                    ).map_err(|e| AppError::Engine(format!("WIC scaler initialization failed: {}", e)))?;

                    scaler.cast().map_err(|e| AppError::Engine(format!("WIC cast failed: {}", e)))?
                } else {
                    frame.cast().map_err(|e| AppError::Engine(format!("WIC cast failed: {}", e)))?
                }
            } else {
                frame.cast().map_err(|e| AppError::Engine(format!("WIC cast failed: {}", e)))?
            };

            converter.Initialize(
                &source,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            ).map_err(|e| AppError::Engine(format!("WIC format conversion failed: {}", e)))?;

            // Copy pixels
            let stride = scaled_width * 4;
            let buffer_size = stride * scaled_height;
            let mut pixels = vec![0u8; buffer_size as usize];

            converter.CopyPixels(
                std::ptr::null(),
                stride,
                &mut pixels,
            ).map_err(|e| AppError::Engine(format!("WIC CopyPixels failed: {}", e)))?;

            // Apply EXIF orientation if needed (since WIC sometimes doesn't automatically apply it based on codec)
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if matches!(ext.as_str(), "jpg" | "jpeg" | "heic" | "heif") {
                let orientation = read_jpeg_orientation(file_path);
                if orientation > 1 {
                    // Fallback to image crate for orientation rotation logic, or we could use fast_image_resize/wgpu in the future.
                    // For now, doing it via `image` crate RgbaImage to keep it simple.
                    if let Some(rgba) = image::RgbaImage::from_raw(scaled_width, scaled_height, pixels.clone()) {
                        let img = image::DynamicImage::ImageRgba8(rgba);
                        let img = match orientation {
                            2 => img.fliph(),
                            3 => img.rotate180(),
                            4 => img.flipv(),
                            5 => img.rotate90().fliph(),
                            6 => img.rotate90(),
                            7 => img.rotate270().fliph(),
                            8 => img.rotate270(),
                            _ => img,
                        };
                        let rgba = img.to_rgba8();
                        return Ok(DecodedImage {
                            pixels: rgba.into_raw(),
                            width: img.width(),
                            height: img.height(),
                        });
                    }
                }
            }

            Ok(DecodedImage {
                pixels,
                width: scaled_width,
                height: scaled_height,
            })
        }
    }
}
