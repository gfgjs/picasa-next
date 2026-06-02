// src-tauri/src/db/models.rs
//! Rust structs that mirror database rows.
//! All structs implement `serde::{Serialize, Deserialize}` for IPC.

use serde::{Deserialize, Serialize};

// ── Scan root ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRoot {
    pub id:            i64,
    pub path:          String,
    pub alias:         Option<String>,
    pub scan_status:   String,
    pub scan_progress: i64,
    pub total_files:   i64,
    pub last_scan_at:  Option<i64>,
    pub is_active:     bool,
    pub created_at:    i64,
    pub updated_at:    i64,
}

// ── Directory ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    pub id:          i64,
    pub root_id:     i64,
    pub parent_id:   Option<i64>,
    pub rel_path:    String,
    pub name:        String,
    pub depth:       i64,
    pub media_count: i64,
    pub mtime:       Option<i64>,
    pub created_at:  i64,
}

/// Lightweight node used in the sidebar folder tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirNode {
    pub id:           i64,
    pub root_id:      i64,
    pub parent_id:    Option<i64>,
    pub name:         String,
    pub rel_path:     String,
    pub depth:        i64,
    pub media_count:  i64,
    pub has_children: bool,
}

// ── Media item ───────────────────────────────────────────────────────────────

/// Core media item (all fields from `media_items` table).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id:               i64,
    pub directory_id:     i64,
    pub file_name:        String,
    pub file_size:        i64,
    pub file_mtime:       i64,
    pub file_format:      String,
    pub media_type:       String,
    pub width:            i64,
    pub height:           i64,
    pub duration_ms:      Option<i64>,
    pub sort_datetime:    i64,
    pub cache_key:        i64,
    pub thumb_status:     i64,
    pub thumb_path:       Option<String>,
    pub thumbhash:        Option<Vec<u8>>,
    pub is_favorited:     bool,
    pub is_deleted:       bool,
    pub deleted_at:       Option<i64>,
    pub rating:           i64,
    pub is_live_photo:    bool,
    pub has_embedded_video: bool,
    pub companion_of:     Option<i64>,
    pub content_hash:     Option<String>,
    pub created_at:       i64,
    pub updated_at:       i64,
}

/// Minimal item used for layout computation (only fields Justified Layout needs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutItem {
    pub id:            i64,
    pub width:         i64,
    pub height:        i64,
    pub sort_datetime: i64,
    pub file_format:   String,
    pub media_type:    String,
    pub is_live_photo: bool,
    pub duration_ms:   Option<i64>,
    pub thumb_status:  i64,
    pub thumb_path:    Option<String>,
    pub thumbhash:     Option<Vec<u8>>,
}

// ── Image meta ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageMeta {
    pub item_id:           i64,
    pub orientation:       i64,
    pub exif_datetime:     Option<i64>,
    pub exif_make:         Option<String>,
    pub exif_model:        Option<String>,
    pub exif_lens:         Option<String>,
    pub exif_focal_length: Option<f64>,
    pub exif_aperture:     Option<f64>,
    pub exif_shutter:      Option<String>,
    pub exif_iso:          Option<i64>,
    pub exif_gps_lat:      Option<f64>,
    pub exif_gps_lng:      Option<f64>,
    pub dominant_hue:      Option<i64>,
    pub dominant_sat:      Option<i64>,
    pub dominant_lum:      Option<i64>,
    pub dominant_hex:      Option<String>,
    pub is_monochrome:     bool,
}

// ── Media detail (full) ──────────────────────────────────────────────────────

/// Full detail returned to the frontend when the user opens a media item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDetail {
    #[serde(flatten)]
    pub item:       MediaItem,
    pub abs_path:   String,
    pub image_meta: Option<ImageMeta>,
}

// ── Search result ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id:           i64,
    pub file_name:    String,
    pub media_type:   String,
    pub thumb_path:   Option<String>,
    pub thumbhash:    Option<Vec<u8>>,
    pub thumb_status: i64,
}

// ── App stats ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStats {
    pub total_items:    i64,
    pub total_images:   i64,
    pub total_videos:   i64,
    pub total_audios:   i64,
    pub total_documents: i64,
    pub total_favorited: i64,
    pub total_deleted:   i64,
    pub total_live_photos: i64,
}

// ── Media filter ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaFilter {
    pub media_types:      Option<Vec<String>>,
    pub live_photo_only:  Option<bool>,
    pub favorited_only:   Option<bool>,
    pub min_rating:       Option<i64>,
    pub date_range:       Option<DateRange>,
    pub directory_id:     Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRange {
    pub from: i64,
    pub to:   i64,
}

// ── Thumbnail result ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbResult {
    pub item_id:      i64,
    pub thumb_status: i64,
    pub thumb_path:   Option<String>,
    pub thumbhash:    Option<Vec<u8>>,
}
