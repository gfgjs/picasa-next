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
    pub id: i64,
    pub path: String,
    pub alias: Option<String>,
    pub scan_status: String,
    pub scan_progress: i64,
    pub total_files: i64,
    pub last_scan_at: Option<i64>,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
    /// 存储后端归属（V7）：`None`=本地 / OS 挂载盘（走 `LocalFs`），`Some`=`storage_backends.id`（网络盘等）。
    pub backend_id: Option<i64>,
}

// ── Directory ────────────────────────────────────────────────────────────────
// ── 目录 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directory {
    pub id: i64,
    pub root_id: i64,
    pub parent_id: Option<i64>,
    pub rel_path: String,
    pub name: String,
    pub depth: i64,
    pub media_count: i64,
    pub mtime: Option<i64>,
    pub created_at: i64,
}

/// Lightweight node used in the sidebar folder tree.
/// 侧边栏文件夹树中使用的轻量级节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirNode {
    pub id: i64,
    pub root_id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub rel_path: String,
    pub depth: i64,
    pub media_count: i64,
    pub has_children: bool,
}

/// Lightweight media-file row shown as a leaf under a directory in the sidebar tree.
/// Only the few fields the tree's file list needs (name + type + favorite flag); the
/// full item is fetched lazily when the file is actually opened.
/// 侧边栏文件夹树中作为目录叶子显示的轻量媒体文件行。仅含文件列表所需的少量字段
///（名称 + 类型 + 收藏标志）；点击打开时再按需拉取完整项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirFile {
    pub id: i64,
    pub file_name: String,
    pub media_type: String,
    pub is_favorited: bool,
}

// ── Media item ───────────────────────────────────────────────────────────────
// ── 媒体项 ───────────────────────────────────────────────────────────────

/// Core media item (all fields from `media_items` table).
/// 核心媒体项（来自 `media_items` 表的所有字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: i64,
    pub directory_id: i64,
    pub file_name: String,
    pub file_size: i64,
    pub file_mtime: i64,
    pub file_format: String,
    pub media_type: String,
    pub width: i64,
    pub height: i64,
    pub duration_ms: Option<i64>,
    pub sort_datetime: i64,
    pub cache_key: i64,
    pub thumb_status: i64,
    pub thumb_path: Option<String>,
    pub thumbhash: Option<Vec<u8>>,
    pub is_favorited: bool,
    pub is_deleted: bool,
    pub deleted_at: Option<i64>,
    pub rating: i64,
    /// 用户颜色标签 0-7（0=未标）。与 rating 同类的逐项小标量，供详情页单项设色（T16）。
    pub color_label: i64,
    pub is_live_photo: bool,
    pub has_embedded_video: bool,
    pub companion_of: Option<i64>,
    pub content_hash: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Minimal item used for layout computation (only fields Justified Layout +
/// card-skeleton rendering need). Heavy metadata (file name, dir path, EXIF, GPS)
/// is intentionally excluded and fetched on demand via `get_meta_for_viewport`.
///
/// 用于布局计算的最小化项（仅 Justified Layout 与卡片骨架渲染所需字段）。
/// 重型元数据（文件名、目录路径、EXIF、GPS）有意排除，按需经
/// `get_meta_for_viewport` 拉取。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutItem {
    pub id: i64,
    pub width: i64,
    pub height: i64,
    pub file_size: i64,
    pub sort_datetime: i64,
    pub file_format: String,
    pub media_type: String,
    pub is_live_photo: bool,
    pub duration_ms: Option<i64>,
    pub thumb_status: i64,
    pub thumb_path: Option<String>,
    pub thumbhash: Option<Vec<u8>>,
    pub is_favorited: bool,
    /// 用户评分 0-5（0 = 未评分）。与 is_favorited 同为逐项小标量，随布局行常驻，
    /// 供网格直接显示星级 / hover 快捷评分 / 「≥N 星」筛选，无需懒加载重元数据。
    pub rating: i64,
    /// 用户颜色标签 0-7（0 = 未标）。与 rating 同为逐项小标量，随布局行常驻，供网格 swatch 显示
    /// 与按色筛选（T16）。色档颜色映射在前端（schema.rs:582 仅存档位）。
    pub color_label: i64,
    /// 系统可用态：'online' | 'offline' | 'missing'（卷/扫描驱动，与 is_deleted 正交）。
    /// 前端据此置灰 + 角标（缺失检测 Part2 §3.2）。与 media_type 同为逐项小串。
    pub availability: String,
    // Grouping fields — used by the layout algorithm (folder separators), not
    // copied into the resident per-item row data.
    // 分组字段 — 供布局算法使用（文件夹分隔符），不复制进常驻的逐项行数据。
    pub dir_path: Option<String>,
    pub dir_name: Option<String>,
    pub dir_id: Option<i64>,
    pub similarity: Option<f64>,
}

/// Heavy per-item metadata fetched lazily for the visible viewport only.
/// 仅为可视区按需拉取的逐项重型元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaMeta {
    pub id: i64,
    pub file_name: String,
    pub dir_path: Option<String>,
    pub gps_lat: Option<f64>,
    pub gps_lng: Option<f64>,
    pub exif_make: Option<String>,
    pub exif_model: Option<String>,
    pub exif_lens: Option<String>,
    pub exif_focal_length: Option<f64>,
    pub exif_aperture: Option<f64>,
    pub exif_shutter: Option<String>,
    pub exif_iso: Option<i64>,
}

// ── Image meta ───────────────────────────────────────────────────────────────
// ── 图像元数据 ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageMeta {
    pub item_id: i64,
    pub orientation: i64,
    pub exif_datetime: Option<i64>,
    pub exif_make: Option<String>,
    pub exif_model: Option<String>,
    pub exif_lens: Option<String>,
    pub exif_focal_length: Option<f64>,
    pub exif_aperture: Option<f64>,
    pub exif_shutter: Option<String>,
    pub exif_iso: Option<i64>,
    pub exif_gps_lat: Option<f64>,
    pub exif_gps_lng: Option<f64>,
    pub dominant_hue: Option<i64>,
    pub dominant_sat: Option<i64>,
    pub dominant_lum: Option<i64>,
    pub dominant_hex: Option<String>,
    pub is_monochrome: bool,
}

// ── Audio meta (§3.6) ──────────────────────────────────────────────────────────
// ── 音频元数据（§3.6） ──────────────────────────────────────────────────────────

/// Tags/properties row from `audio_meta` (artist/album/track/year/genre + lyrics provenance).
/// 来自 `audio_meta` 的标签/属性行（艺术家/专辑/音轨/年份/流派 + 歌词来源）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioMeta {
    pub item_id: i64,
    pub audio_codec: Option<String>,
    pub artist: Option<String>,
    pub album_title: Option<String>,
    pub track_title: Option<String>,
    pub track_no: Option<i64>,
    pub year: Option<i64>,
    pub genre: Option<String>,
    /// 'embedded' | 'lrc' | 'none' — where the lyrics come from (text read lazily by source).
    /// 'embedded' | 'lrc' | 'none' —— 歌词来源（文本按来源懒加载）。
    pub lyrics_source: Option<String>,
    pub lyrics_path: Option<String>,
}

/// Full audio detail for the player (`/audio/:id`): core item + abs path + tags + cover + lyrics.
/// Tags/lyrics are read lazily from the file so existing libraries work without a rescan (§3.6).
/// 播放器（`/audio/:id`）的完整音频详情：核心项 + 绝对路径 + 标签 + 封面 + 歌词。
/// 标签/歌词按需从文件读取，使既有库无需重扫即可工作（§3.6）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDetail {
    #[serde(flatten)]
    pub item: MediaItem,
    pub abs_path: String,
    pub meta: AudioMeta,
    /// Full-resolution embedded cover, extracted to the cache on demand (`convertFileSrc`-able).
    /// `None` → no embedded art (frontend shows a music-note placeholder).
    /// 全分辨率内嵌封面，按需抽取至缓存（可 `convertFileSrc`）。`None` → 无内嵌封面（前端显示占位）。
    pub cover_path: Option<String>,
    /// Resolved lyrics text (embedded or `.lrc`); `None` if neither.
    /// 解析出的歌词文本（内嵌或 `.lrc`）；都没有则为 `None`。
    pub lyrics: Option<String>,
    /// True if `lyrics` carries `[mm:ss]` LRC timestamps (frontend syncs to playback).
    /// `lyrics` 是否带 `[mm:ss]` LRC 时间轴（前端随播放同步）为真。
    pub lyrics_synced: bool,
}

// ── Media detail (full) ──────────────────────────────────────────────────────
// ── 媒体详情（完整） ──────────────────────────────────────────────────────

/// Full detail returned to the frontend when the user opens a media item.
/// 用户打开媒体项时返回给前端的完整详情。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDetail {
    #[serde(flatten)]
    pub item: MediaItem,
    pub abs_path: String,
    pub image_meta: Option<ImageMeta>,
    /// 系统可用态（缺失检测 Part2 §3.2）：'online' | 'offline' | 'missing'。
    /// 大图查看器据此对「卷离线/文件缺失」给出明确提示，而非任由 <img> 显示 broken 图标。
    pub availability: String,
}

// ── Search result ─────────────────────────────────────────────────────────────
// ── 搜索结果 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: i64,
    pub file_name: String,
    pub media_type: String,
    pub width: i64,
    pub height: i64,
    pub thumb_path: Option<String>,
    pub thumbhash: Option<Vec<u8>>,
    pub thumb_status: i64,
}

// ── Collection (favorites) ────────────────────────────────────────────────────
// ── 收藏夹 ────────────────────────────────────────────────────────────────────

/// A favorites collection, backed by the `albums` table (§3.7).
/// 由 `albums` 表承载的收藏夹（§3.7）。
///
/// `kind='system'` → one of the 4 seeded type folders (image/video/audio/document);
/// membership is virtual (`media_type_filter` + is_favorited). `kind='user'` → membership
/// stored in `album_items`, may mix types.
/// `kind='system'` → 播种的 4 个类型夹之一；成员虚拟（`media_type_filter` + is_favorited）。
/// `kind='user'` → 成员存 `album_items`，可跨类型混装。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub media_type_filter: Option<String>,
    pub icon: Option<String>,
    /// Cover item for the card thumbnail (latest member); resolved by the frontend.
    /// 卡片缩略图的封面项（最新成员）；由前端解析缩略图。
    pub cover_item_id: Option<i64>,
    pub item_count: i64,
    pub sort_order: i64,
}

// ── Storage backend (network drives, §3.8 8B) ─────────────────────────────────
// ── 存储后端（网络盘，§3.8 8B） ───────────────────────────────────────────────

/// A configured storage backend row from `storage_backends` (§3.8). The password is never here —
/// only `cred_ref` (a keyring lookup key). `has_password` tells the UI whether one is stored.
/// 来自 `storage_backends` 的已配置存储后端行（§3.8）。密码绝不在此 —— 仅 `cred_ref`（keyring 查找键）。
/// `has_password` 告知 UI 是否已存密码。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageBackendInfo {
    pub id: i64,
    pub kind: String, // 'local' | 'smb' | 'webdav'
    pub name: String,
    pub host: Option<String>,
    pub base_path: Option<String>,
    pub username: Option<String>,
    pub has_password: bool,
    pub created_at: i64,
}

// ── App stats ────────────────────────────────────────────────────────────────
// ── 应用程序统计 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStats {
    pub total_items: i64,
    pub total_images: i64,
    pub total_videos: i64,
    pub total_audios: i64,
    pub total_documents: i64,
    pub total_favorited: i64,
    pub total_deleted: i64,
    pub total_live_photos: i64,
}

// ── Media filter ─────────────────────────────────────────────────────────────
// ── 媒体过滤器 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaFilter {
    pub media_types: Option<Vec<String>>,
    pub live_photo_only: Option<bool>,
    pub favorited_only: Option<bool>,
    pub min_rating: Option<i64>,
    /// 颜色标签筛选：精确匹配某色档（1-7；0=未标）。与 min_rating 同为逐项小标量筛选（T16）。
    pub color_label: Option<i64>,
    pub date_range: Option<DateRange>,
    pub directory_id: Option<i64>,
    /// Filter to members of a user collection (album_items). System collections instead
    /// use `media_types` + `favorited_only`, so they need no album JOIN.
    /// 过滤为某用户收藏夹（album_items）的成员。系统夹改用 `media_types` + `favorited_only`，无需 JOIN。
    pub album_id: Option<i64>,
    /// Filter to images containing a face assigned to this person cluster (F6 people wall →
    /// person's photos). Mutually exclusive with the other view filters frontend-side.
    /// 过滤为包含归属此人物簇人脸的图像（F6 人物墙 → 某人物的照片）。前端与其它视图筛选互斥。
    pub person_id: Option<i64>,
    pub search_query: Option<String>,
    pub search_scope: Option<String>,
    pub ai_search: Option<bool>,
    pub ai_threshold: Option<f64>,
    pub trashed_only: Option<bool>,
    pub recent_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRange {
    pub from: i64,
    pub to: i64,
}

// ── View descriptor & selection contract (T18 / T14.5) ───────────────────────
// ── 视图描述符与选择契约（T18 / T14.5）───────────────────────────────────────
//
// 面向 >100 万项库：选择不枚举 id，而是描述「哪个视图的全集，减去哪些排除项」，由后端按
// filter 在 SQL 层流式解析 —— 既不把百万 id 灌进前端内存，也不经 IPC 整包传 id。

/// 排序规格：决定 ORDER BY，与 layout 分组同源（对齐 uiStore.groupBy / sortWithinGroup）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortSpec {
    pub group_by: String,          // "date" | "folder" | "none"
    pub sort_within_group: String, // "datetime" | "filename" | "similarity" ...
    pub sort_order: String,        // "asc" | "desc"
}

impl Default for SortSpec {
    fn default() -> Self {
        // 与画廊默认一致：按拍摄时间倒序、日期分组。
        Self {
            group_by: "date".into(),
            sort_within_group: "datetime".into(),
            sort_order: "desc".into(),
        }
    }
}

/// 视图集合来源（决定 FROM/JOIN 与基础谓词）。与 `GalleryFilter` 分工：scope 定**来源**，
/// filter 在来源上**再筛**。这些字段从 `MediaFilter` 剥离至此（D1），避免「同一语义两处可填、互相打架」。
/// ⚠️ serde 细节（R1-2 契约定形）：enum 级 `rename_all` 只改**变体名**，不改 struct 变体内的
/// 字段名——各携带字段的变体须自带 `rename_all`，前端才能以 camelCase（directoryId 等）构造。
/// 此前该路径无前端消费者，形状错配从未暴露；wire 格式已由 queries.rs 的 S1 锁测试钉死。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ViewScope {
    /// 全库（is_deleted=0）。「智能相册/系统夹」= All + GalleryFilter(media_types/favorited)，不单列 scope。
    All,
    /// 某目录递归子树（复用 query_layout_items 的 WITH RECURSIVE dir_tree）。
    #[serde(rename_all = "camelCase")]
    Directory { directory_id: i64 },
    /// 用户收藏夹（album_items 成员）。
    #[serde(rename_all = "camelCase")]
    Collection { album_id: i64 },
    /// 人脸簇视图（某人物的照片）。model_name 隔离（D2）v1 单模型下为 no-op，多模型随 Part4 T6 接入。
    #[serde(rename_all = "camelCase")]
    Person { person_id: i64 },
    /// 回收站（is_deleted=1）。
    Trash,
    /// CLIP 语义搜索：有序、非纯 SQL（v1 由 ai_search 既有路径承载，`view_to_sql` 不直接支持）。
    #[serde(rename_all = "camelCase")]
    SemanticSearch { query_embedding_id: i64, top_k: u32 },
}

/// 附加筛选（在 scope 选定来源上再筛），决定 WHERE 增量。**不含 scope 字段**（D1：scope 字段归 ViewScope）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GalleryFilter {
    pub media_types: Option<Vec<String>>,
    pub live_photo_only: Option<bool>,
    pub favorited_only: Option<bool>,
    pub min_rating: Option<i64>,
    /// 颜色标签筛选：精确匹配某色档（1-7；0=未标）（T16）。
    pub color_label: Option<i64>,
    pub date_range: Option<DateRange>,
    pub search_query: Option<String>,
    pub search_scope: Option<String>,
    /// 「最近导入」智能相册（R1-2 补）：此前 GalleryFilter 无法表达 recent 视图，
    /// 该视图下的 SelectAll 描述符会静默丢失谓词、作用到错误集合。
    pub recent_only: Option<bool>,
}

/// 不可变视图描述符：唯一确定「当前画廊视图全集 + 序」，是 `view_to_sql` 的输入、全选解析的依据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewDescriptor {
    pub scope: ViewScope,
    pub filter: GalleryFilter,
    pub sort: SortSpec,
    /// 与 `LayoutCache.layout_version` 对齐；解析时不一致即拒（`AppError::ViewStale`）。
    pub layout_version: u64,
}

impl ViewDescriptor {
    /// 把 scope + filter **lower 成既有 `MediaFilter`**，复用 `query_layout_items` 同一套 SQL builder
    /// （D1：单一事实源，不另起双套 WHERE，杜绝视图定义漂移）。
    pub fn to_media_filter(&self) -> MediaFilter {
        let mut mf = MediaFilter {
            media_types: self.filter.media_types.clone(),
            live_photo_only: self.filter.live_photo_only,
            favorited_only: self.filter.favorited_only,
            min_rating: self.filter.min_rating,
            color_label: self.filter.color_label,
            date_range: self.filter.date_range.clone(),
            search_query: self.filter.search_query.clone(),
            search_scope: self.filter.search_scope.clone(),
            recent_only: self.filter.recent_only,
            ..Default::default()
        };
        // scope 决定 FROM/JOIN 与基础谓词，映射回 MediaFilter 的对应字段。
        match &self.scope {
            ViewScope::All => {}
            ViewScope::Directory { directory_id } => mf.directory_id = Some(*directory_id),
            ViewScope::Collection { album_id } => mf.album_id = Some(*album_id),
            ViewScope::Person { person_id } => mf.person_id = Some(*person_id),
            ViewScope::Trash => mf.trashed_only = Some(true),
            // SemanticSearch 在 view_to_sql 入口已被拦截，此分支仅为穷尽匹配。
            ViewScope::SemanticSearch { .. } => mf.ai_search = Some(true),
        }
        mf
    }
}

/// 选择 = 描述而非枚举（百万级不灌前端内存 / 不经 IPC 整包传 id）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SelectionDescriptor {
    /// 显式 id 列表（手选少量项）。上限校验见 `resolve_selection`。
    Explicit { ids: Vec<i64> },
    /// 全选某视图 − 排除集（Ctrl+A）。`excluded_ids` 通常远小于全集。
    /// `view` 经 `Box` 装箱：`ViewDescriptor` 远大于 `Explicit` 变体，避免枚举按最大变体撑大
    /// （clippy large_enum_variant）。serde 对 `Box<T>` 透明，前端 JSON 契约不变。
    /// 变体级 `rename_all` 使 `excluded_ids` 上线为 `excludedIds`（同 ViewScope 注意事项）。
    #[serde(rename_all = "camelCase")]
    SelectAll {
        view: Box<ViewDescriptor>,
        excluded_ids: Vec<i64>,
    },
}

// ── Thumbnail result ─────────────────────────────────────────────────────────
/// Thumbnail result returned after thumb generation.
/// 缩略图生成后返回的缩略图结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbResult {
    pub item_id: i64,
    pub thumb_status: i64,
    pub thumb_path: Option<String>,
    pub thumbhash: Option<Vec<u8>>,
}

/// A document (pdf/svg) awaiting a frontend-rendered thumbnail (§3.4 Lite path).
/// `abs_path` is wrapped with `convertFileSrc` by the renderer to load the source file.
/// 等待前端渲染缩略图的文档（pdf/svg，§3.4 Lite 路径）。`abs_path` 由渲染器经 `convertFileSrc`
/// 包装以加载源文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDocThumb {
    pub item_id: i64,
    pub abs_path: String,
    pub file_format: String,
}

/// A text replacement rule (§5.2) — display-layer find/replace for role-play / name mapping,
/// scoped to one item, a book group, or global. Never mutates the source file.
/// 文本替换规则（§5.2）—— 角色扮演/人名映射的展示层查找替换，作用于单项/丛书组/全局，不改源文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacementRule {
    pub id: i64,
    pub scope_kind: String, // 'item' | 'group' | 'global'
    pub scope_id: Option<i64>,
    pub find: String,
    pub replace: String,
    pub is_regex: bool,
    pub enabled: bool,
    pub sort_order: i64,
}

/// A document version snapshot (§5.3) — git-like immutable snapshot tree off the original.
/// 文档版本快照（§5.3）—— 以原始件为基线的类 git 不可变快照树。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentVersion {
    pub id: i64,
    pub item_id: i64,
    pub parent_id: Option<i64>,
    pub label: Option<String>,
    pub storage: String, // 'appdata' | 'external'
    pub abs_path: String,
    pub source: String, // 'user' | 'ai-local' | 'ai-remote'
    pub note: Option<String>,
    pub content_hash: Option<String>,
    pub is_current: bool,
    pub created_at: i64,
}

/// One line-level diff op between two document versions (§5.3). `tag`: equal/insert/delete.
/// 两个文档版本间的一条行级 diff（§5.3）。`tag`：equal/insert/delete。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffOp {
    pub tag: String, // "equal" | "insert" | "delete"
    pub value: String,
}

// ── AI ───────────────────────────────────────────────────────────────────────
// ── AI ───────────────────────────────────────────────────────────────────────

/// AI processing status codes stored in `media_items.ai_status`.
/// 存储在 `media_items.ai_status` 中的 AI 处理状态码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i64)]
pub enum AiStatus {
    /// Not yet analysed | 尚未分析
    Pending = 0,
    /// Currently being processed | 当前正在处理
    Processing = 1,
    /// Embedding stored | 嵌入向量已存储
    Done = 2,
    /// Analysis failed (image unreadable etc.) | 分析失败
    Error = 3,
}

impl AiStatus {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

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

/// Face detection status codes stored in `media_items.face_status`.
/// 存储在 `media_items.face_status` 中的人脸检测状态码（语义同 `AiStatus`，独立开关）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i64)]
pub enum FaceStatus {
    /// Not yet detected | 尚未检测
    Pending = 0,
    /// Currently being processed | 当前正在处理
    Processing = 1,
    /// Faces detected + embedded (incl. zero-face images) | 已检测并嵌入（含零脸图）
    Done = 2,
    /// Detection failed (image unreadable etc.) | 检测失败
    Error = 3,
}

impl FaceStatus {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(v: i64) -> Self {
        match v {
            0 => FaceStatus::Pending,
            1 => FaceStatus::Processing,
            2 => FaceStatus::Done,
            _ => FaceStatus::Error,
        }
    }
}

/// A single stored CLIP embedding row.
/// 单条存储的 CLIP 嵌入向量行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiEmbedding {
    pub item_id: i64,
    pub model_name: String,
    /// Raw f32 bytes (512 × 4 = 2048 bytes for ViT-B/16).
    /// 原始 f32 字节（ViT-B/16 为 512 × 4 = 2048 字节）。
    #[serde(skip)]
    pub embedding: Vec<u8>,
    pub version: i64,
    pub created_at: i64,
}

/// Semantic search result with similarity score.
/// 带相似度分数的语义搜索结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchResult {
    pub id: i64,
    pub file_name: String,
    pub media_type: String,
    pub width: i64,
    pub height: i64,
    pub thumb_path: Option<String>,
    pub thumbhash: Option<Vec<u8>>,
    pub thumb_status: i64,
    /// Cosine similarity in [0, 1] range.
    /// [0, 1] 范围内的余弦相似度。
    pub similarity: f32,
}

/// Derivation pipeline status summary returned to the frontend (video cover/keyframes,
/// doc thumbnail, audio cover/meta). Mirrors the AI 3-button status surface.
/// 返回给前端的派生流水线状态摘要（视频封面/关键帧、文档缩略图、音频封面/元数据）。
/// 与 AI 三按钮状态面板同构。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivationStatusSummary {
    pub pending: i64,
    pub processing: i64,
    pub done: i64,
    pub error: i64,
    /// True while the pipeline is actively running (token present).
    /// 流水线正在运行时为真（令牌存在）。
    pub is_running: bool,
    /// "Desired" flag persisted across runs/restarts (drives resume + auto-resume),
    /// mirroring `ai_analysis_active`.
    /// 跨运行/重启持久化的「期望运行」标志（驱动续传与自动续传），与 `ai_analysis_active` 同构。
    pub active: bool,
}

/// AI status summary returned to the frontend.
/// 返回给前端的 AI 状态摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatusSummary {
    pub provider: String,
    pub gpu_name: String,
    pub vram_gb: Option<i64>,
    pub batch_size: i64,
    /// Active image variant's fixed batch `k` (>1), or None for dynamic / single-batch. Drives the
    /// "configured batch must be ≥ k" minimum in the settings UI.
    /// 当前图像变体的固定 batch `k`（>1），动态/单批为 None。驱动设置页「batch 不得 < k」最小限制。
    pub active_fixed_batch: Option<i64>,
    pub clip_loaded: bool,
    pub total_items: i64,
    pub analyzed_items: i64,
    pub pending_items: i64,
    pub is_analyzing: bool,
    /// True when analysis is "desired" — running, or paused/interrupted with work left
    /// (drives the resume / 3-button UI and auto-resume on launch — 问题7).
    /// 分析处于「期望运行」状态——正在运行，或已暂停/中断且仍有剩余（驱动续传/三按钮 UI
    /// 与启动自动续传 —— 问题7）。
    pub analysis_active: bool,
}

/// Face-recognition status summary returned to the frontend (F5). Mirrors `AiStatusSummary` but
/// reports persons/faces counts instead of embeddings, and `processed_items` counts Done+Error
/// (`face_status IN (2,3)`) so the progress bar completes even with some failures.
/// 返回给前端的人脸识别状态摘要（F5）。仿 `AiStatusSummary`，但报告人物/人脸数而非嵌入向量数，
/// 且 `processed_items` 统计 完成+错误（`face_status IN (2,3)`），使部分失败时进度条仍能到 100%。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceStatusSummary {
    pub provider: String,
    pub gpu_name: String,
    /// Both face sessions (detector + embedder) loaded — mirrors `clip_loaded`.
    /// 人脸双 session（检测器 + 嵌入器）均已加载——对应 `clip_loaded`。
    pub face_loaded: bool,
    pub total_items: i64,
    /// Images whose face detection finished (Done OR Error), not "images with faces".
    /// 完成人脸检测的图像数（完成 或 错误），非"有脸的图像数"。
    pub processed_items: i64,
    pub pending_items: i64,
    /// Number of clustered persons (people-wall roster size).
    /// 已聚类人物数（人物墙名册规模）。
    pub person_count: i64,
    /// Total detected faces across all images for the active model.
    /// 当前模型下跨所有图像检测到的人脸总数。
    pub face_count: i64,
    pub is_analyzing: bool,
    pub analysis_active: bool,
}

/// One person cluster as a people-wall card (F6): identity + a cover thumbnail to crop the face
/// from. `cover_thumb_path`/`cover_thumb_status` follow the same convention as `SearchResult`
/// (status=3 → the path is the original abs path; else a tiered cache rel-path); the frontend
/// crops `cover_bbox` (normalized [x,y,w,h]) out of that thumbnail for the avatar.
/// 一个人物簇作为人物墙卡片（F6）：身份 + 一张用于裁剪人脸的封面缩略图。`cover_thumb_path`/
/// `cover_thumb_status` 沿用 `SearchResult` 的约定（status=3 → path 为原图绝对路径；否则为分档
/// 缓存相对路径）；前端从该缩略图裁出 `cover_bbox`（归一化 [x,y,w,h]）作头像。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonSummary {
    pub id: i64,
    pub name: Option<String>,
    pub face_count: i64,
    pub is_named: bool,
    pub is_hidden: bool,
    /// Cover face geometry + its source image's thumbnail (None when the cover is dangling).
    /// 封面脸几何 + 其源图缩略图（封面悬空时为 None）。
    pub cover_item_id: Option<i64>,
    pub cover_thumb_path: Option<String>,
    pub cover_thumb_status: Option<i64>,
    pub cover_bbox: Option<[f32; 4]>,
}

/// One face-model track for the read-only model registry (F7): the two built-in tracks
/// (yunet-sface commercial / scrfd-arcface non-commercial) + on-disk install status. No download
/// URLs — assets are empty pending verified direct links + human license confirmation, so this is
/// display-only (lets the UI show "installed / place files here").
/// 只读人脸模型库的一条模型轨（F7）：两条内置轨（yunet-sface 商用 / scrfd-arcface 非商用）+ 磁盘
/// 安装状态。无下载 URL——assets 待填已校验直链 + 人工确认许可，故仅供展示（UI 显示"已装 / 请放
/// 文件到此"）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceModelInfo {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub detector: String,
    pub embedder: String,
    pub embed_dim: i64,
    pub commercial_ok: bool,
    pub license: String,
    pub size_mb: i64,
    /// Both onnx files present on disk.
    /// 两个 onnx 文件均在磁盘上。
    pub installed: bool,
    /// This is the currently-active track (`face_model_active`).
    /// 这是当前激活轨（`face_model_active`）。
    pub active: bool,
    /// Has a verified download manifest (one-click download). False = manual import only (the
    /// SCRFD/ArcFace track: no verified checksums + non-commercial).
    /// 有已校验下载清单（可一键下载）。false=仅手动导入（SCRFD/ArcFace 轨：无校验值 + 非商用）。
    pub downloadable: bool,
}

/// One detected face overlaid on the image detail viewer (F6): box + which person it belongs to.
/// 叠加在图片详情查看器上的一张检测人脸（F6）：框 + 它归属的人物。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceBox {
    pub id: i64,
    pub person_id: Option<i64>,
    pub person_name: Option<String>,
    /// Normalized [x, y, w, h] in [0,1] against the image's own dimensions.
    /// 相对图像自身尺寸归一化的 [x, y, w, h]（[0,1]）。
    pub bbox: [f32; 4],
    pub det_score: f32,
}

/// One unconfirmed face in a likely-match group (Part4 §3.5.1 / Part5 T10 batch approval): a
/// thumbnail to crop the face from + how strongly it matches the candidate person. `thumb_path`/
/// `thumb_status` follow the same convention as `PersonSummary`/`SearchResult` (status=3 →
/// resolve absolute source path via JOIN; else stored thumb_path); `bbox` crops the face out.
/// likely-match 组里的一张未确认脸（Part4 §3.5.1 / Part5 T10 批量审批）：用于裁剪人脸的缩略图 +
/// 它与候选人物的匹配强度。`thumb_path`/`thumb_status` 约定同 `PersonSummary`/`SearchResult`
///（status=3 → 经 JOIN 解析绝对源路径，否则用 thumb_path）；`bbox` 裁出人脸。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceThumb {
    pub face_id: i64,
    pub item_id: i64,
    pub thumb_path: Option<String>,
    pub thumb_status: Option<i64>,
    pub bbox: [f32; 4],
    /// Cosine similarity of this face to its candidate person's centroid (per-face match strength).
    /// 此脸与其候选人物质心的余弦相似度（单脸匹配强度）。
    pub similarity: f32,
}

/// A group of unconfirmed faces tentatively assigned to ONE candidate person, for the
/// batch-approval UI (Part4 §3.5.1 / Part5 T10). The user confirms / reassigns / rejects the
/// whole group at once. `confidence` = mean per-face similarity (group match strength).
/// 一组暂归于同一候选人物的未确认脸，供批量审批 UI（Part4 §3.5.1 / Part5 T10）。用户对整组一次性
/// 确认/改派/拒绝。`confidence` = 单脸相似度均值（组匹配强度）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LikelyMatchGroup {
    pub person_id: i64,
    pub person_name: Option<String>,
    pub candidate_faces: Vec<FaceThumb>,
    pub confidence: f32,
}

// ── Volume（卷可用性模型，SCHEMA_V10）─────────────────────────────────────────
// ── 卷：移动盘/网络盘插拔感知，「离线≠删除」的稳定身份锚点 ──────────────────────

/// 卷类型。映射 `volumes.kind` TEXT 列（'local'|'removable'|'network'）。
/// 仿 `TokenizerKind` 的「未知即兜底」哲学：解析未知字符串归 `Local`（旧库 / 未来新增类型
/// 都不致解析 panic），但写出仍用精确字面量——读宽容、写严格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeKind {
    /// 本机固定盘（向后兼容：迁移占位卷默认归此）。
    Local,
    /// 可移动盘（U盘 / 移动硬盘 / SD）——插拔感知的核心对象。
    Removable,
    /// 网络盘（SMB / NFS / UNC）。
    Network,
}

impl VolumeKind {
    /// 转 SQL TEXT 值（写严格）。
    pub fn as_str(self) -> &'static str {
        match self {
            VolumeKind::Local => "local",
            VolumeKind::Removable => "removable",
            VolumeKind::Network => "network",
        }
    }

    /// 从 SQL TEXT 值解析（读宽容）：未知值归 `Local`，防御旧库 / 未来类型导致的解析中断。
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "removable" => VolumeKind::Removable,
            "network" => VolumeKind::Network,
            _ => VolumeKind::Local,
        }
    }
}

/// 一条卷登记行（`volumes` 表，SCHEMA_V10）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub id: i64,
    /// 稳定身份锚点：Win 卷GUID / mac 卷UUID / 规范化 UNC；迁移占位期为 `'pending:<scan_root_id>'`，
    /// 由 `probe_volumes`（Part2）首次运行覆写为真实卷 ID。
    pub stable_id: String,
    /// 卷标（展示用，用户可改名）。
    pub label: Option<String>,
    pub kind: VolumeKind,
    /// 最近挂载点 / 盘符（提示 + 运行期路径重组用，**非身份键**——盘符会变，stable_id 不变）。
    pub last_mount_path: Option<String>,
    /// 最近在线 unix 秒。
    pub last_seen: Option<i64>,
    pub is_online: bool,
    pub created_at: i64,
}

/// `upsert_volume` 入参（不含 `id` / `created_at`——由 DB 生成 / 保留）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewVolume {
    pub stable_id: String,
    pub label: Option<String>,
    pub kind: VolumeKind,
    pub last_mount_path: Option<String>,
    pub last_seen: Option<i64>,
    pub is_online: bool,
}

// ── Document meta（document_meta，Phase 2）────────────────────────────────────

/// 文档元数据行（`document_meta` 表）。PDF/epub 等的页数 + 子类型，文档 enrichment 完成后写入；
/// 消费在 Part3 文档派生（封面/进度条）与 Part5 阅读器（PDF 页 / epub 章节进度）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMeta {
    pub item_id: i64,
    /// 总页数（PDF 页数 / epub 章节数）；未读取为 None。
    pub page_count: Option<i64>,
    /// 文档子类型（pdf/svg/epub/office/text…，见 `utils::format::doc_subtype`）。
    pub doc_subtype: Option<String>,
}
