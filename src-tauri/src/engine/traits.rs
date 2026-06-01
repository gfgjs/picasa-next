// src-tauri/src/engine/traits.rs
//! `ImageEngine` trait definition as specified in § 7.1 of the implementation plan.

use std::path::Path;
use crate::error::AppError;

/// A decoded image ready for processing (thumbnail generation, ThumbHash, etc.).
pub struct DecodedImage {
    /// Raw RGBA pixel data.
    pub pixels: Vec<u8>,
    pub width:  u32,
    pub height: u32,
}

/// Trait that all image decoding backends must implement.
pub trait ImageEngine: Send + Sync {
    fn name(&self) -> &str;

    fn supported_formats(&self) -> &[&str];

    fn can_handle(&self, format: &str) -> bool {
        self.supported_formats().contains(&format)
    }

    /// Fully decode the image at `file_path` into RGBA pixels.
    fn decode(&self, file_path: &Path) -> Result<DecodedImage, AppError>;

    /// Attempt to extract an embedded thumbnail (e.g. EXIF JPEG thumbnail).
    /// Returns `Ok(None)` if no embedded thumb is available.
    fn extract_embedded_thumb(&self, _file_path: &Path) -> Result<Option<Vec<u8>>, AppError> {
        Ok(None)
    }
}
