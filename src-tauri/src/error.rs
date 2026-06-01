// src-tauri/src/error.rs
//! Unified application error type.
//! All variants are serialisable so they can be forwarded to the frontend via IPC.

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "code", content = "message")]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(String),

    #[error("Database error: {0}")]
    Db(String),

    #[error("Connection pool error: {0}")]
    Pool(String),

    #[error("EXIF parse error: {0}")]
    Exif(String),

    #[error("XMP parse error: {0}")]
    Xmp(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Image engine error: {0}")]
    Engine(String),

    #[error("Path resolution error: {0}")]
    PathResolution(String),

    #[error("FFmpeg error: {0}")]
    FFmpeg(String),

    #[error("Audio metadata error: {0}")]
    AudioMetadata(String),

    #[error("Document render error: {0}")]
    DocumentRender(String),

    #[error("Layout cache not ready — call compute_layout first")]
    LayoutNotReady,

    #[error("Scan root not found: id={0}")]
    ScanRootNotFound(i64),

    #[error("Media item not found: id={0}")]
    MediaNotFound(i64),

    #[error("Operation cancelled")]
    Cancelled,
}

// ── Conversions ────────────────────────────────────────────────────────────

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}

impl From<r2d2::Error> for AppError {
    fn from(e: r2d2::Error) -> Self {
        AppError::Pool(e.to_string())
    }
}

impl From<image::ImageError> for AppError {
    fn from(e: image::ImageError) -> Self {
        AppError::Engine(e.to_string())
    }
}

impl From<exif::Error> for AppError {
    fn from(e: exif::Error) -> Self {
        AppError::Exif(e.to_string())
    }
}

impl From<quick_xml::Error> for AppError {
    fn from(e: quick_xml::Error) -> Self {
        AppError::Xmp(e.to_string())
    }
}

/// Convenience alias used throughout the codebase.
pub type Result<T> = std::result::Result<T, AppError>;
