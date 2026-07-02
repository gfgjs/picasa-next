// src-tauri/src/derive/kind.rs
//! Derivation kind registry — the single source of truth for *what* derivations exist,
//! which media they apply to, and whether a runnable backend is compiled into THIS build.
//! 派生 kind 注册表 —— 「有哪些派生、各自适用于哪些媒体、本次构建是否编入可运行后端」
//! 的唯一事实来源。
//!
//! Adding a new derivation = add a variant here + its `run` impl in `video/doc/audio.rs`,
//! then flip `is_implemented`. The pipeline/resume/yield/orphan-recovery is all generic.
//! 新增派生 = 在此加一个变体 + 在 `video/doc/audio.rs` 实现其 `run`，再翻转 `is_implemented`。
//! 流水线/续传/让步/孤儿恢复全部通用，无需改动。

use std::path::PathBuf;

use crate::error::{AppError, Result};

/// One derivation job type. The string form (`as_str`) is what's stored in
/// `media_derivations.kind`.
/// 一种派生任务类型。其字符串形式（`as_str`）即存入 `media_derivations.kind` 的值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivationKind {
    /// Video poster frame → WebP, reuses thumbnail cache (§3.2). | 视频封面帧 → WebP，复用缩略图缓存。
    VideoCover,
    /// Video keyframe sprite for hover/scrub preview (§3.3). | 视频关键帧雪碧图，用于悬停/进度条 scrub。
    VideoKeyframes,
    /// Document first-page/cover thumbnail (§3.4). | 文档首页/封面缩略图。
    DocThumb,
    /// Audio embedded cover art (§3.6). | 音频内嵌封面图。
    AudioCover,
    /// Audio tags/lyrics metadata (§3.6). | 音频标签/歌词元数据。
    AudioMeta,
    /// AI-analysis cache: a short-edge≥336 WebP per image so CLIP analysis decodes a tiny cache
    /// instead of the full-resolution original (opt-in; gated by `ai_hq_cache_enabled`). NOT a
    /// display thumbnail — `produces_thumbnail()` stays false.
    /// AI 分析缓存：每张图一份短边≥336 的 WebP，使 CLIP 分析解码一份小缓存而非全分辨率原图
    /// （opt-in，由 `ai_hq_cache_enabled` 控制）。非显示缩略图 —— `produces_thumbnail()` 保持 false。
    AiThumb,
}

impl DerivationKind {
    /// All kinds, ordered high→low derivation priority (cover/meta before the heavier
    /// keyframe sprite). Backfill enqueues in this order so higher-value artefacts land first.
    /// 所有 kind，按派生优先级高→低排列（封面/元数据先于更重的关键帧雪碧图）。
    /// backfill 按此顺序入队，使高价值产物先落地。
    pub const ALL: [DerivationKind; 6] = [
        DerivationKind::AudioMeta,
        DerivationKind::AudioCover,
        DerivationKind::VideoCover,
        DerivationKind::DocThumb,
        DerivationKind::AiThumb,
        DerivationKind::VideoKeyframes,
    ];

    /// Stable string stored in the DB `kind` column.
    /// 存入 DB `kind` 列的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            DerivationKind::VideoCover => "video_cover",
            DerivationKind::VideoKeyframes => "video_keyframes",
            DerivationKind::DocThumb => "doc_thumb",
            DerivationKind::AudioCover => "audio_cover",
            DerivationKind::AudioMeta => "audio_meta",
            DerivationKind::AiThumb => "ai_thumb",
        }
    }

    /// Parse from the DB `kind` column.
    /// 从 DB `kind` 列解析。
    // 固有 from_str：返回 Option<Self>（非 std FromStr 的 Result），语义不同；保留固有方法。
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "video_cover" => DerivationKind::VideoCover,
            "video_keyframes" => DerivationKind::VideoKeyframes,
            "doc_thumb" => DerivationKind::DocThumb,
            "audio_cover" => DerivationKind::AudioCover,
            "audio_meta" => DerivationKind::AudioMeta,
            "ai_thumb" => DerivationKind::AiThumb,
            _ => return None,
        })
    }

    /// Whether a runnable backend for this kind is compiled into the current binary.
    /// P0: every kind is scaffolding-only — the real backends (MF video, pdf.js/pdfium
    /// doc, lofty audio) land in P2/P3/P4. Until a kind returns `true` here, `backfill`
    /// never enqueues it, so nothing is processed (or spuriously marked error).
    /// 当前二进制是否编入了该 kind 的可运行后端。
    /// P0：所有 kind 仅为脚手架 —— 真实后端（MF 视频、pdf.js/pdfium 文档、lofty 音频）
    /// 在 P2/P3/P4 落地。在某 kind 于此返回 `true` 之前，`backfill` 不会入队它，
    /// 因此不会被处理（也不会被误标为错误）。
    pub fn is_implemented(&self) -> bool {
        match self {
            // 后端落地时改为 true（并实现对应 run），框架其余部分无需改动。
            // 视频派生由 Media Foundation 后端驱动（仅 Windows）；非 Windows 平台暂无后端 → 保持 false
            // 不入队，待 AVFoundation/FFmpeg 后补。
            DerivationKind::VideoCover => cfg!(windows), // P2 — derive/video.rs（MF）
            DerivationKind::VideoKeyframes => cfg!(windows), // P2 — derive/video.rs（MF）
            // P4：epub 封面由后端 zip 处理（跨平台）；pdf/svg 由前端离屏渲染（见 get_pending_derivations
            // 排除 + store_doc_thumbnail）。返回 true 使 backfill 为 pdf/epub/svg 全部建行 —— 后端只跑 epub，
            // 前端经 list_pending_doc_thumbs 领取 pdf/svg。
            DerivationKind::DocThumb => true, // P4 — derive/doc.rs（epub）+ 前端（pdf/svg）
            // P3：音频内嵌封面由 lofty 提取（跨平台，纯 Rust）→ 缩略图缓存（derive/audio.rs）。
            DerivationKind::AudioCover => true, // P3 — derive/audio.rs（lofty 内嵌封面）
            // 音频元数据/歌词由 enricher 在补全阶段回填 audio_meta（与视频元数据同处理，非派生），
            // 故此 kind 不入队（保持 false）；run_meta 仅为框架占位。详见 derive/audio.rs 模块文档。
            DerivationKind::AudioMeta => false, // P3 — 由 enricher 处理（见 derive/audio.rs）
            // AI 缓存后端跨平台（WIC + image crate 回退）；实际是否入队仍由 `ai_hq_cache_enabled`
            // 开关在 pipeline 的 disabled_kinds 中 gate（opt-in，默认关）。
            DerivationKind::AiThumb => true, // derive/image.rs
        }
    }

    /// Whether this kind produces a **cover thumbnail** that should be mirrored onto
    /// `media_items.thumb_status/thumb_path/thumbhash` (so `MediaThumb` shows it with zero
    /// frontend changes, §3.2). Keyframe sprites are NOT covers — they're a separate scrub asset.
    /// 该 kind 是否产出应回填到 `media_items.thumb_status/thumb_path/thumbhash` 的**封面缩略图**
    /// （使 `MediaThumb` 零改动即可显示，§3.2）。关键帧雪碧图不是封面 —— 是独立的 scrub 资源。
    pub fn produces_thumbnail(&self) -> bool {
        matches!(
            self,
            DerivationKind::VideoCover | DerivationKind::AudioCover | DerivationKind::DocThumb
        )
    }

    /// Document subtypes that get a rasterised thumbnail. Plain text (txt/md) is rendered
    /// as a CSS "text card" on the frontend instead (§3.4), so it is intentionally excluded.
    /// 会生成栅格化缩略图的文档子类型。纯文本（txt/md）由前端用 CSS「文本卡」渲染（§3.4），
    /// 故有意排除。
    pub const DOC_THUMB_FORMATS: [&'static str; 3] = ["pdf", "epub", "svg"];

    /// Which kinds apply to a given media item. Used by `backfill` to enqueue rows.
    /// 某媒体项适用哪些 kind。供 `backfill` 入队行使用。
    pub fn for_media(media_type: &str, file_format: &str) -> Vec<DerivationKind> {
        match media_type {
            "video" => vec![DerivationKind::VideoCover, DerivationKind::VideoKeyframes],
            "audio" => vec![DerivationKind::AudioMeta, DerivationKind::AudioCover],
            "document" => {
                if DerivationKind::DOC_THUMB_FORMATS.contains(&file_format) {
                    vec![DerivationKind::DocThumb]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }
}

/// Everything a kind's `run` needs to produce its artefact, resolved once by the consumer.
/// 某 kind 的 `run` 产出其产物所需的一切，由消费者一次性解析好。
pub struct DerivationContext {
    pub item_id: i64,
    pub kind: DerivationKind,
    /// Absolute path to the source media file.
    /// 源媒体文件的绝对路径。
    pub abs_path: PathBuf,
    pub file_format: String,
    pub media_type: String,
    /// `cache_key` of the source item — covers reuse the exact thumbnail cache path/key so
    /// `MediaThumb` loads them like any image thumbnail (invariant §1.3.3).
    /// 源项的 `cache_key` —— 封面复用缩略图缓存的路径/键，使 `MediaThumb` 像普通图片缩略图一样加载
    /// （不变量 §1.3.3）。
    pub cache_key: i64,
    /// Thumbnail cache root — covers/sprites are written under here (reusing `cache_key`).
    /// 缩略图缓存根目录 —— 封面/雪碧图写入此处（复用 `cache_key`）。
    pub cache_dir: PathBuf,
    /// Target thumbnail edge size (px), for cover scaling.
    /// 目标缩略图边长（像素），用于封面缩放。
    pub thumb_size: u32,
}

/// Output of a successful derivation.
/// 派生成功的产物。
pub struct DerivationOutput {
    /// Relative payload path (sprite for keyframes; cover thumb db-path for covers). Stored in
    /// `media_derivations.payload_path`.
    /// 相对产物路径（关键帧的雪碧图；封面的缩略图 db 路径）。存入 `media_derivations.payload_path`。
    pub payload_path: Option<String>,
    /// For cover kinds: the thumbhash to mirror onto `media_items` (placeholder blur color).
    /// `None` for non-cover kinds.
    /// 封面类 kind：回填到 `media_items` 的 thumbhash（占位模糊色）。非封面 kind 为 `None`。
    pub thumbhash: Option<Vec<u8>>,
    /// For document kinds (epub doc_thumb): page count to upsert into `document_meta` (§3.8.2 / T10).
    /// `None` for all other kinds (the pipeline writer only upserts `document_meta` when `Some`).
    /// 文档类 kind（epub doc_thumb）：upsert 进 `document_meta` 的页数（§3.8.2 / T10）。
    /// 其它 kind 一律 `None`（流水线写入器仅在 `Some` 时 upsert `document_meta`）。
    pub page_count: Option<i64>,
}

/// Dispatch one derivation task to its kind-specific `run`. P0 routes every kind to a
/// `NotImplemented` error; since `is_implemented` gates enqueuing, this is never reached
/// in normal operation until a backend lands.
/// 将单个派生任务分发到对应 kind 的 `run`。P0 下每个 kind 都路由到 `NotImplemented` 错误；
/// 由于 `is_implemented` 门控了入队，正常运行下在后端落地前永不触达此处。
pub fn run(ctx: &DerivationContext) -> Result<DerivationOutput> {
    match ctx.kind {
        DerivationKind::VideoCover => super::video::run_cover(ctx),
        DerivationKind::VideoKeyframes => super::video::run_keyframes(ctx),
        DerivationKind::DocThumb => super::doc::run_thumb(ctx),
        DerivationKind::AudioCover => super::audio::run_cover(ctx),
        DerivationKind::AudioMeta => super::audio::run_meta(ctx),
        DerivationKind::AiThumb => super::image::run_ai_thumb(ctx),
    }
}

/// Shared "not yet implemented in this build" error for stub kinds.
/// 各桩 kind 共用的「本次构建尚未实现」错误。
pub(crate) fn not_implemented(kind: DerivationKind) -> AppError {
    AppError::Internal(format!(
        "derivation kind '{}' has no backend in this build | 该派生 kind 在本次构建中无后端",
        kind.as_str()
    ))
}
