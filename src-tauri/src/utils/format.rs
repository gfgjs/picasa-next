// src-tauri/src/utils/format.rs
//! Media format classification.
//!
//! `classify_media_type` maps a lowercase file extension → `MediaType`.
//! Phase 1 fully supports Image. Phase 2 extensions are registered but return
//! the appropriate type so the scanner can enable them with a flag.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Video,
    Audio,
    Document,
}

impl MediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Image    => "image",
            MediaType::Video    => "video",
            MediaType::Audio    => "audio",
            MediaType::Document => "document",
        }
    }
}

/// Returns the `MediaType` for a given lowercase file extension, or `None` if not supported.
pub fn classify_media_type(ext: &str) -> Option<MediaType> {
    match ext {
        // ── Phase 1 image formats ─────────────────────────────────────────
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "gif" | "tif" | "tiff" => {
            Some(MediaType::Image)
        }

        // ── Phase 2 image extensions ──────────────────────────────────────
        "heic" | "heif" | "avif"
        | "cr2" | "cr3" | "nef" | "arw" | "dng" | "raf" | "orf" | "rw2" | "pef" | "srw"
        | "psd" => Some(MediaType::Image),

        // ── Phase 2 video ─────────────────────────────────────────────────
        "mp4" | "m4v" | "mov" | "avi" | "mkv" | "webm" | "wmv" | "flv"
        | "mpg" | "mpeg" | "3gp" | "3g2" | "ts" | "mts" | "m2ts" | "ogv" | "asf" => {
            Some(MediaType::Video)
        }

        // ── Phase 2 audio ─────────────────────────────────────────────────
        "mp3" | "flac" | "wav" | "aac" | "m4a" | "ogg" | "oga" | "opus"
        | "wma" | "aiff" | "aif" | "ape" | "alac" => Some(MediaType::Audio),

        // ── Phase 2 document ──────────────────────────────────────────────
        "pdf" | "svg" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
        | "txt" | "md" | "rtf" | "odt" | "ods" | "odp" => Some(MediaType::Document),

        _ => None,
    }
}

/// Returns `true` if the format is a Phase 1 image (fully supported in fast scan).
pub fn is_phase1_image(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "webp" | "bmp" | "gif" | "tif" | "tiff")
}

/// Returns `true` if the extension might be the MOV companion of an Apple Live Photo.
pub fn is_live_photo_companion_ext(ext: &str) -> bool {
    ext == "mov"
}

/// Document sub-type for the `document_meta` table.
pub fn doc_subtype(ext: &str) -> &'static str {
    match ext {
        "pdf"  => "pdf",
        "svg"  => "svg",
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" => "office",
        "txt" | "md" | "rtf" => "text",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_jpeg() {
        assert_eq!(classify_media_type("jpg"), Some(MediaType::Image));
        assert_eq!(classify_media_type("jpeg"), Some(MediaType::Image));
    }

    #[test]
    fn classify_video() {
        assert_eq!(classify_media_type("mp4"), Some(MediaType::Video));
        assert_eq!(classify_media_type("mov"), Some(MediaType::Video));
    }

    #[test]
    fn classify_unknown() {
        assert_eq!(classify_media_type("xyz"), None);
    }

    #[test]
    fn phase1_image() {
        assert!(is_phase1_image("jpg"));
        assert!(is_phase1_image("tiff"));
        assert!(!is_phase1_image("heic"));
    }
}
