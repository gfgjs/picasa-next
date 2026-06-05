// src-tauri/src/db/schema.rs
// src-tauri/src/db/schema.rs
//! DDL: CREATE TABLE / CREATE INDEX statements.
//! DDL：CREATE TABLE / CREATE INDEX 语句。
//! Called once from `migration.rs` during schema bootstrapping.
//! 在模式引导期间从 `migration.rs` 调用一次。

/// All DDL for schema version 1.
/// 模式版本 1 的所有 DDL。
pub const SCHEMA_V1: &str = "
-- ── app_config ──────────────────────────────────────────────────────────────
-- ── 应用配置 ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS app_config (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

-- Seed defaults (INSERT OR IGNORE = safe to re-run)
-- 播种默认值（INSERT OR IGNORE = 安全重新运行）
INSERT OR IGNORE INTO app_config (key, value) VALUES
    ('schema_version',    '1'),
    ('thumb_size',        '300'),
    ('thumb_format',      'webp'),
    ('thumb_quality',     '80'),
    ('thumb_skip_max_kb', '200'),
    ('thumb_strategy',    'gpu'),
    ('gpu_engine',        'wic'),
    ('theme',             'system'),
    ('last_directory_id', ''),
    ('last_sort_by',      'sort_datetime'),
    ('last_sort_order',   'desc'),
    ('sidebar_width',     '260');

-- ── scan_roots ───────────────────────────────────────────────────────────────
-- ── 扫描根目录 ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS scan_roots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    path            TEXT    NOT NULL UNIQUE,
    alias           TEXT,
    scan_status     TEXT    DEFAULT 'idle',
    scan_progress   INTEGER DEFAULT 0,
    total_files     INTEGER DEFAULT 0,
    last_scan_at    INTEGER,
    is_active       INTEGER DEFAULT 1,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- ── directories ──────────────────────────────────────────────────────────────
-- ── 目录 ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS directories (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id         INTEGER NOT NULL REFERENCES scan_roots(id) ON DELETE CASCADE,
    parent_id       INTEGER REFERENCES directories(id) ON DELETE CASCADE,
    rel_path        TEXT    NOT NULL,
    name            TEXT    NOT NULL,
    depth           INTEGER NOT NULL DEFAULT 0,
    media_count     INTEGER NOT NULL DEFAULT 0,
    mtime           INTEGER,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(root_id, rel_path)
);
CREATE INDEX IF NOT EXISTS idx_dir_root   ON directories(root_id);
CREATE INDEX IF NOT EXISTS idx_dir_parent ON directories(parent_id);

-- ── media_items ───────────────────────────────────────────────────────────────
-- ── 媒体项 ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS media_items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    directory_id    INTEGER NOT NULL REFERENCES directories(id) ON DELETE CASCADE,

    file_name       TEXT    NOT NULL,
    file_size       INTEGER NOT NULL,
    file_mtime      INTEGER NOT NULL,
    file_format     TEXT    NOT NULL,

    media_type      TEXT    NOT NULL DEFAULT 'image',
    width           INTEGER NOT NULL DEFAULT 0,
    height          INTEGER NOT NULL DEFAULT 0,
    duration_ms     INTEGER,

    sort_datetime   INTEGER NOT NULL,
    cache_key       INTEGER NOT NULL,

    thumb_status    INTEGER NOT NULL DEFAULT 0,
    thumb_path      TEXT,
    thumbhash       BLOB,

    is_favorited    INTEGER NOT NULL DEFAULT 0,
    is_deleted      INTEGER NOT NULL DEFAULT 0,
    deleted_at      INTEGER,
    rating          INTEGER DEFAULT 0,

    is_live_photo       INTEGER DEFAULT 0,
    has_embedded_video  INTEGER DEFAULT 0,
    companion_of        INTEGER REFERENCES media_items(id) ON DELETE SET NULL,

    content_hash    TEXT,

    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),

    UNIQUE(directory_id, file_name)
);

CREATE INDEX IF NOT EXISTS idx_media_directory ON media_items(directory_id);
CREATE INDEX IF NOT EXISTS idx_media_sort      ON media_items(sort_datetime DESC)
                                               WHERE is_deleted = 0 AND companion_of IS NULL;
CREATE INDEX IF NOT EXISTS idx_media_cache_key ON media_items(cache_key);
CREATE INDEX IF NOT EXISTS idx_media_format    ON media_items(file_format);
CREATE INDEX IF NOT EXISTS idx_media_type      ON media_items(media_type)  WHERE is_deleted = 0;
CREATE INDEX IF NOT EXISTS idx_media_thumb     ON media_items(thumb_status) WHERE thumb_status != 1;
CREATE INDEX IF NOT EXISTS idx_media_fav       ON media_items(is_favorited)
                                               WHERE is_favorited = 1 AND is_deleted = 0;
CREATE INDEX IF NOT EXISTS idx_media_del       ON media_items(is_deleted) WHERE is_deleted = 1;
CREATE INDEX IF NOT EXISTS idx_media_rating    ON media_items(rating) WHERE is_deleted = 0 AND rating > 0;
CREATE INDEX IF NOT EXISTS idx_media_hash      ON media_items(content_hash) WHERE content_hash IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_media_companion ON media_items(companion_of) WHERE companion_of IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_media_live      ON media_items(is_live_photo) WHERE is_live_photo = 1;

-- ── image_meta ────────────────────────────────────────────────────────────────
-- ── 图像元数据 ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS image_meta (
    item_id           INTEGER PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    orientation       INTEGER DEFAULT 1,

    exif_datetime     INTEGER,
    exif_make         TEXT,
    exif_model        TEXT,
    exif_lens         TEXT,
    exif_focal_length REAL,
    exif_aperture     REAL,
    exif_shutter      TEXT,
    exif_iso          INTEGER,
    exif_gps_lat      REAL,
    exif_gps_lng      REAL,

    dominant_hue      INTEGER,
    dominant_sat      INTEGER,
    dominant_lum      INTEGER,
    dominant_hex      TEXT,
    is_monochrome     INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_img_hue ON image_meta(dominant_hue, is_monochrome)
                                       WHERE dominant_hue IS NOT NULL;

-- ── video_meta (Phase 2 — table created now, populated later) ────────────────
-- ── 视频元数据（阶段 2 — 现在创建表，稍后填充） ────────────────
CREATE TABLE IF NOT EXISTS video_meta (
    item_id      INTEGER PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    video_codec  TEXT
);

-- ── audio_meta (Phase 2) ──────────────────────────────────────────────────────
-- ── 音频元数据（阶段 2） ──────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS audio_meta (
    item_id      INTEGER PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    audio_codec  TEXT,
    artist       TEXT,
    album_title  TEXT,
    track_title  TEXT
);
CREATE INDEX IF NOT EXISTS idx_audio_artist ON audio_meta(artist) WHERE artist IS NOT NULL;

-- ── document_meta (Phase 2) ───────────────────────────────────────────────────
-- ── 文档元数据（阶段 2） ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS document_meta (
    item_id      INTEGER PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    page_count   INTEGER,
    doc_subtype  TEXT
);

-- ── albums / album_items (Phase 3) ────────────────────────────────────────────
-- ── 相册 / 相册项（阶段 3） ────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS albums (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    description     TEXT,
    cover_item_id   INTEGER REFERENCES media_items(id) ON DELETE SET NULL,
    sort_order      INTEGER DEFAULT 0,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS album_items (
    album_id   INTEGER NOT NULL REFERENCES albums(id)      ON DELETE CASCADE,
    item_id    INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    sort_order INTEGER DEFAULT 0,
    added_at   INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (album_id, item_id)
);

-- ── tags / item_tags (Phase 3) ────────────────────────────────────────────────
-- ── 标签 / 项目标签（阶段 3） ────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS tags (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL UNIQUE,
    color      TEXT,
    parent_id  INTEGER REFERENCES tags(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS item_tags (
    item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tags(id)        ON DELETE CASCADE,
    PRIMARY KEY (item_id, tag_id)
);
";

/// DDL deltas for schema version 2 — AI embeddings.
/// 模式版本 2 的 DDL 增量 — AI 嵌入向量。
///
/// Note: `ALTER TABLE ... ADD COLUMN` with `DEFAULT 0` is safe in SQLite.
/// 注意：带 `DEFAULT 0` 的 `ALTER TABLE ... ADD COLUMN` 在 SQLite 中是安全的。
pub const SCHEMA_V2: &str = "
-- ── AI embeddings ─────────────────────────────────────────────────────────────
-- ── AI 嵌入向量 ─────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS ai_embeddings (
    item_id      INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    model_name   TEXT    NOT NULL,
    embedding    BLOB    NOT NULL,
    version      INTEGER NOT NULL DEFAULT 1,
    created_at   INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (item_id, model_name)
);
CREATE INDEX IF NOT EXISTS idx_embed_model ON ai_embeddings(model_name);

-- ── ai_status on media_items ──────────────────────────────────────────────────
-- ── media_items 上的 ai_status 字段 ──────────────────────────────────────────
-- ai_status: 0=pending, 1=processing, 2=done, 3=error
-- ai_status: 0=待处理, 1=处理中, 2=已完成, 3=错误
ALTER TABLE media_items ADD COLUMN ai_status INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_media_ai ON media_items(ai_status) WHERE ai_status < 3;

-- ── AI config defaults ────────────────────────────────────────────────────────
-- ── AI 配置默认值 ─────────────────────────────────────────────────────────────
INSERT OR IGNORE INTO app_config (key, value) VALUES
    ('ai_provider',     ''),
    ('ai_gpu_name',     ''),
    ('ai_enabled',      '1'),
    ('ai_auto_analyze', '1'),
    ('clip_model',      'cn-clip-vit-b16');
";

/// DDL deltas for schema version 3 — AI search results.
/// 模式版本 3 的 DDL 增量 — AI 搜索结果。
pub const SCHEMA_V3: &str = "
-- ── ai_search_results ─────────────────────────────────────────────────────────
-- ── AI 搜索结果临时表（持久化存储会话数据） ──────────────────────────────────
CREATE TABLE IF NOT EXISTS ai_search_results (
    file_id    INTEGER PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    similarity REAL NOT NULL
);
";
