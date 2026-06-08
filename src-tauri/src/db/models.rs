// src-tauri/src/db/models.rs
// src-tauri/src/db/models.rs
//! Rust structs that mirror database rows.
//! 镜像数据库行的 Rust 结构体。
//! All structs implement `serde::{Serialize, Deserialize}` for IPC.
//! 所有结构体都实现 `serde::{Serialize, Deserialize}` 以用于 IPC。

use serde::{Deserialize, Serialize};

// ── Scan root ────────────────────────────────────────────────────────────────
// ── 扫描根目录 ────────────────────────────────────────────────────────────────

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
// ── 目录 ────────────────────────────────────────────────────────────────

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
/// 侧边栏文件夹树中使用的轻量级节点。
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
// ── 媒体项 ───────────────────────────────────────────────────────────────

/// Core media item (all fields from `media_items` table).
/// 核心媒体项（来自 `media_items` 表的所有字段）。
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
/// 用于布局计算的最小化项（仅 Justified Layout 需要的字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutItem {
    pub id:            i64,
    pub width:         i64,
    pub height:        i64,
    pub file_size:     i64,
    pub sort_datetime: i64,
    pub file_format:   String,
    pub media_type:    String,
    pub is_live_photo: bool,
    pub duration_ms:   Option<i64>,
    pub thumb_status:  i64,
    pub thumb_path:    Option<String>,
    pub thumbhash:     Option<Vec<u8>>,
    pub is_favorited:  bool,
    pub dir_path:      Option<String>,
    pub dir_name:      Option<String>,
    pub file_name:     String,
    pub dir_id:        Option<i64>,
    pub similarity:    Option<f64>,
    pub gps_lat:       Option<f64>,
    pub gps_lng:       Option<f64>,
    pub exif_make:     Option<String>,
    pub exif_model:    Option<String>,
    pub exif_lens:     Option<String>,
    pub exif_focal_length: Option<f64>,
    pub exif_aperture: Option<f64>,
    pub exif_shutter:  Option<String>,
    pub exif_iso:      Option<i64>,
}

// ── Image meta ───────────────────────────────────────────────────────────────
// ── 图像元数据 ───────────────────────────────────────────────────────────────

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
// ── 媒体详情（完整） ──────────────────────────────────────────────────────

/// Full detail returned to the frontend when the user opens a media item.
/// 用户打开媒体项时返回给前端的完整详情。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDetail {
    #[serde(flatten)]
    pub item:       MediaItem,
    pub abs_path:   String,
    pub image_meta: Option<ImageMeta>,
}

// ── Search result ─────────────────────────────────────────────────────────────
// ── 搜索结果 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id:           i64,
    pub file_name:    String,
    pub media_type:   String,
    pub width:        i64,
    pub height:       i64,
    pub thumb_path:   Option<String>,
    pub thumbhash:    Option<Vec<u8>>,
    pub thumb_status: i64,
}

// ── App stats ────────────────────────────────────────────────────────────────
// ── 应用程序统计 ────────────────────────────────────────────────────────────────

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
// ── 媒体过滤器 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaFilter {
    pub media_types:      Option<Vec<String>>,
    pub live_photo_only:  Option<bool>,
    pub favorited_only:   Option<bool>,
    pub min_rating:       Option<i64>,
    pub date_range:       Option<DateRange>,
    pub directory_id:     Option<i64>,
    pub search_query:     Option<String>,
    pub search_scope:     Option<String>,
    pub ai_search:        Option<bool>,
    pub ai_threshold:     Option<f64>,
    pub trashed_only:     Option<bool>,
    pub recent_only:      Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRange {
    pub from: i64,
    pub to:   i64,
}

// ── Thumbnail result ─────────────────────────────────────────────────────────
/// Thumbnail result returned after thumb generation.
/// 缩略图生成后返回的缩略图结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbResult {
    pub item_id:      i64,
    pub thumb_status: i64,
    pub thumb_path:   Option<String>,
    pub thumbhash:    Option<Vec<u8>>,
}

// ── AI ───────────────────────────────────────────────────────────────────────
// ── AI ───────────────────────────────────────────────────────────────────────

/// AI processing status codes stored in `media_items.ai_status`.
/// 存储在 `media_items.ai_status` 中的 AI 处理状态码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i64)]
pub enum AiStatus {
    /// Not yet analysed | 尚未分析
    Pending    = 0,
    /// Currently being processed | 当前正在处理
    Processing = 1,
    /// Embedding stored | 嵌入向量已存储
    Done       = 2,
    /// Analysis failed (image unreadable etc.) | 分析失败
    Error      = 3,
}

impl AiStatus {
    pub fn as_i64(self) -> i64 { self as i64 }

    pub fn from_i64(v: i64) -> Self {
        match v {
            0 => AiStatus::Pending,
            1 => AiStatus::Processing,
            2 => AiStatus::Done,
            3 => AiStatus::Error,
            _ => AiStatus::Error,
        }
    }
}

/// A single stored CLIP embedding row.
/// 单条存储的 CLIP 嵌入向量行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiEmbedding {
    pub item_id:    i64,
    pub model_name: String,
    /// Raw f32 bytes (512 × 4 = 2048 bytes for ViT-B/16).
    /// 原始 f32 字节（ViT-B/16 为 512 × 4 = 2048 字节）。
    #[serde(skip)]
    pub embedding:  Vec<u8>,
    pub version:    i64,
    pub created_at: i64,
}

/// Semantic search result with similarity score.
/// 带相似度分数的语义搜索结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchResult {
    pub id:           i64,
    pub file_name:    String,
    pub media_type:   String,
    pub width:        i64,
    pub height:       i64,
    pub thumb_path:   Option<String>,
    pub thumbhash:    Option<Vec<u8>>,
    pub thumb_status: i64,
    /// Cosine similarity in [0, 1] range.
    /// [0, 1] 范围内的余弦相似度。
    pub similarity:   f32,
}

/// AI status summary returned to the frontend.
/// 返回给前端的 AI 状态摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatusSummary {
    pub provider:        String,
    pub gpu_name:        String,
    pub vram_gb:         Option<i64>,
    pub batch_size:      i64,
    pub clip_loaded:     bool,
    pub total_items:     i64,
    pub analyzed_items:  i64,
    pub pending_items:   i64,
    pub is_analyzing:    bool,
}
