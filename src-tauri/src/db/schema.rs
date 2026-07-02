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
    -- 480 是有效档位（[120,240,480,960]），不会被 snap_to_tier 改变。选 480 的原因：
    -- AI 分析按短边裁到 image_size（B/16·L/14=224）。缩略图按长边等比缩放，480 长边时
    -- 3:2/4:3/16:9 的短边均 ≥270 ≥224，使「用缩略图喂 CLIP」近乎全覆盖、免去解原图（见 ai/pipeline.rs）。
    ('thumb_size',        '480'),
    ('thumb_format',      'webp'),
    ('thumb_quality',     '80'),
    ('thumb_skip_max_kb', '200'),
    ('thumb_strategy',    'gpu'),
    ('gpu_engine',        'wic'),
    -- AI 高清缓存（opt-in，默认关）：开启后后台静默为每张图生成短边≥336 的 WebP 缓存，
    -- 使 CLIP 分析解码该小缓存而非全分辨率原图（见 derive/image.rs、ai/pipeline.rs）。
    ('ai_hq_cache_enabled', 'false'),
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

    media_type      TEXT    NOT NULL DEFAULT 'image',  -- image/video/audio/document;4 类为当前划分,Part9 冷门格式时可扩(TEXT 不锁死)
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
    rating          INTEGER DEFAULT 0,    -- ⚠️ 评分制(5星 vs 10分)未定、无值域约束;临时产品决策,UI 明确后可改

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

/// DDL deltas for schema version 4 — Feature Expansion P0 foundations.
/// 模式版本 4 的 DDL 增量 — 功能扩展 P0 地基（见 plan-docs/feature_expansion_plan_v1.md §2.2/§3.2/§3.6/§4）。
///
/// 包含：派生任务状态机表、video_meta/audio_meta 扩列、阅读进度表。
/// Note: `ALTER TABLE ... ADD COLUMN` is non-idempotent, but the migration runner
/// guards this block with `if version < 4`, so it executes exactly once.
/// 注意：`ALTER TABLE ... ADD COLUMN` 非幂等，但迁移器用 `if version < 4` 守护本块，仅执行一次。
pub const SCHEMA_V4: &str = "
-- ── media_derivations：派生任务状态机（每个 (item, kind) 一行）──────────────────
-- ── 派生任务（视频封面/关键帧、文档缩略图、音频封面/元数据…）的可续传调度状态 ──
-- status: 0 待处理 / 1 处理中 / 2 完成 / 3 错误（复用 ai_status 语义，支持断点续传 + 孤儿恢复）
CREATE TABLE IF NOT EXISTS media_derivations (
    item_id      INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    kind         TEXT    NOT NULL,            -- 'video_cover'|'video_keyframes'|'doc_thumb'|'audio_cover'|'audio_meta'|...
    status       INTEGER NOT NULL DEFAULT 0,
    payload_path TEXT,                         -- 产物相对路径（sprite/封面等），可空
    error        TEXT,
    updated_at   INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    PRIMARY KEY (item_id, kind)
);
-- 部分索引只覆盖未完成任务（status<2），生产者扫描待处理项时走索引、命中极小。
CREATE INDEX IF NOT EXISTS idx_deriv_pending ON media_derivations(kind, status) WHERE status < 2;

-- ── video_meta 扩列（现仅 video_codec，远不够）────────────────────────────────
ALTER TABLE video_meta ADD COLUMN fps           REAL;
ALTER TABLE video_meta ADD COLUMN bitrate       INTEGER;
ALTER TABLE video_meta ADD COLUMN rotation      INTEGER DEFAULT 0;   -- 旋转元数据，与图片 EXIF orientation 同理交换宽高
ALTER TABLE video_meta ADD COLUMN has_audio     INTEGER DEFAULT 0;
ALTER TABLE video_meta ADD COLUMN cover_time_ms INTEGER;             -- 封面取自哪一帧

-- ── audio_meta 扩列（现有 artist/album_title/track_title）─────────────────────
ALTER TABLE audio_meta ADD COLUMN track_no      INTEGER;
ALTER TABLE audio_meta ADD COLUMN year          INTEGER;
ALTER TABLE audio_meta ADD COLUMN genre         TEXT;
ALTER TABLE audio_meta ADD COLUMN lyrics_source TEXT;   -- 'embedded'|'lrc'|'none'
ALTER TABLE audio_meta ADD COLUMN lyrics_path   TEXT;   -- 外部 .lrc 路径

-- ── reading_progress：文档/EPUB 阅读进度（页码 / CFI / 滚动比例）──────────────
CREATE TABLE IF NOT EXISTS reading_progress (
    item_id    INTEGER PRIMARY KEY REFERENCES media_items(id) ON DELETE CASCADE,
    position   TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
";

/// DDL deltas for schema version 5 — Collections / Favorites (需求7, §3.7).
/// 模式版本 5 的 DDL 增量 — 收藏夹（需求7, §3.7）。
///
/// 不另造机制：复用既有 `is_favorited`（快速收藏标志，红心/索引/缓存快路径全不动）
/// 与 `albums`/`album_items`（通用多对多）。本迁移为 `albums` 扩 3 列并播种 4 个系统收藏夹。
/// 系统夹是「虚拟」的：成员 = 该类型 + is_favorited（走 idx_media_fav 快路径，无需写 album_items）；
/// 用户夹是「实体」的：成员存 album_items。详见 list_collections 注释。
pub const SCHEMA_V5: &str = "
-- ── albums 扩列：区分系统/用户夹 + 系统夹的类型过滤 + 图标 ──────────────────────
ALTER TABLE albums ADD COLUMN kind              TEXT DEFAULT 'user';   -- 'system' | 'user'
ALTER TABLE albums ADD COLUMN media_type_filter TEXT;                  -- 系统夹：image/video/audio/document
ALTER TABLE albums ADD COLUMN icon              TEXT;                  -- lucide 图标名（前端映射组件）

-- ── 播种 4 个系统收藏夹（图/视/音/文档），幂等：仅当不存在时插入 ──────────────────
INSERT INTO albums (name, kind, media_type_filter, icon, sort_order)
SELECT '图片收藏', 'system', 'image', 'Image', 1
WHERE NOT EXISTS (SELECT 1 FROM albums WHERE kind='system' AND media_type_filter='image');
INSERT INTO albums (name, kind, media_type_filter, icon, sort_order)
SELECT '视频收藏', 'system', 'video', 'Video', 2
WHERE NOT EXISTS (SELECT 1 FROM albums WHERE kind='system' AND media_type_filter='video');
INSERT INTO albums (name, kind, media_type_filter, icon, sort_order)
SELECT '音频收藏', 'system', 'audio', 'Music', 3
WHERE NOT EXISTS (SELECT 1 FROM albums WHERE kind='system' AND media_type_filter='audio');
INSERT INTO albums (name, kind, media_type_filter, icon, sort_order)
SELECT '文档收藏', 'system', 'document', 'FileText', 4
WHERE NOT EXISTS (SELECT 1 FROM albums WHERE kind='system' AND media_type_filter='document');
";

/// DDL deltas for schema version 6 — 文档浏览器/编辑（需求5.2/5.3, §3.5/§4）。
/// 模式版本 6：替换规则（角色扮演/人名替换）+ 文档版本管理（类 git 快照树）。
///
/// 注：`replace` 是 SQLite 函数名，作列名时在所有查询中加引号 `"replace"` 以消歧。
pub const SCHEMA_V6: &str = "
-- ── doc_replacements：替换规则（§5.2）──────────────────────────────────────────
-- 纯展示层替换（不改源文件）：可绑 item / group（同系列丛书） / global。
CREATE TABLE IF NOT EXISTS doc_replacements (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    scope_kind TEXT NOT NULL,        -- 'item' | 'group' | 'global'
    scope_id   INTEGER,              -- item_id 或书籍系列 id；global 为 NULL
    find       TEXT NOT NULL,
    replace    TEXT NOT NULL,
    is_regex   INTEGER DEFAULT 0,
    enabled    INTEGER DEFAULT 1,
    sort_order INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX IF NOT EXISTS idx_repl_scope ON doc_replacements(scope_kind, scope_id) WHERE enabled = 1;

-- ── document_versions：文档版本（§5.3，类 git 全量快照 + 按需 diff）──────────────
-- 源文件不可变为基线；版本独立成文件 + 元数据成树（parent_id）。版本不进画廊。
CREATE TABLE IF NOT EXISTS document_versions (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id      INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE, -- 原始件
    parent_id    INTEGER REFERENCES document_versions(id) ON DELETE SET NULL,   -- 父版本（成树）
    label        TEXT,                 -- 'AI校对稿' / '我的修订'
    storage      TEXT NOT NULL,        -- 'appdata' | 'external'
    abs_path     TEXT NOT NULL,
    source       TEXT NOT NULL,        -- 'user' | 'ai-local' | 'ai-remote'
    note         TEXT,
    content_hash TEXT,
    is_current   INTEGER DEFAULT 0,
    created_at   INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX IF NOT EXISTS idx_docver_item ON document_versions(item_id);
";

/// DDL deltas for schema version 7 — 网络盘（需求8 8B, §3.8/§4）。
/// 模式版本 7：存储后端抽象（`storage_backends`）+ `scan_roots.backend_id`（NULL=本地）。
///
/// 8A（OS 挂载盘/UNC）不依赖本表 —— `backend_id IS NULL` 即走本地 `LocalFs`。8B 原生 VFS
/// （WebDAV，feature `netfs`）的连接信息存此表；密码不落库，仅存 keyring 引用（`cred_ref`）。
pub const SCHEMA_V7: &str = "
-- ── storage_backends：存储后端连接（§3.8 8B）──────────────────────────────────
-- 一行 = 一个已配置的存储后端（local / smb / webdav）。密码绝不落库，仅存 keyring 引用。
CREATE TABLE IF NOT EXISTS storage_backends (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT NOT NULL,        -- 'local'|'smb'|'webdav'
    name       TEXT NOT NULL,
    host       TEXT,                 -- 或 base_url（webdav）
    base_path  TEXT,
    username   TEXT,
    cred_ref   TEXT,                 -- keyring 引用，密码不落库
    options    TEXT,                 -- JSON（扩展项，如 TLS 校验开关）
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

-- ── scan_roots.backend_id：扫描根归属的存储后端（NULL=本地/OS 挂载，即 8A）──────────
ALTER TABLE scan_roots ADD COLUMN backend_id INTEGER REFERENCES storage_backends(id);
";

/// DDL deltas for schema version 8 — 人脸识别（Face Recognition F1 地基）。
/// 模式版本 8：persons（人物簇）+ faces（人脸实例，一图多脸）+ media_items.face_status。
///
/// # 设计要点
/// - 人脸破 `ai_embeddings` 的 `(item_id, model_name)` 单主键范式（一图多脸）→ `faces` 每脸自增 id。
/// - `faces.model_name` = 嵌入模型身份 = 向量空间；换模型则该空间向量失效须重算（同 CLIP 不变量）。
/// - `face_status` 独立于 `ai_status`，使人脸分析与 CLIP 语义分析可分别开关、互不阻塞。
/// - `embedding`/`centroid` 为 BLOB（f32 小端）；维度由 `FaceProfile` 决定（SFace=128 / ArcFace=512）。
pub const SCHEMA_V8: &str = "
-- ── persons：人物簇（聚类结果）────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS persons (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT,                              -- NULL=未命名
    cover_face_id INTEGER,                           -- 代表脸（挑 quality 最高），关联 faces.id
    centroid      BLOB,                              -- 簇质心向量（增量归类用，f32 LE）
    face_count    INTEGER NOT NULL DEFAULT 0,
    is_named      INTEGER NOT NULL DEFAULT 0,
    is_hidden     INTEGER NOT NULL DEFAULT 0,        -- 用户隐藏（不入人物墙）
    is_ignored    INTEGER NOT NULL DEFAULT 0,        -- 误检/非人脸 归类桶
    created_at    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at    INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

-- ── faces：人脸实例（一图多脸；person_id 可空=未归类）──────────────────────────
CREATE TABLE IF NOT EXISTS faces (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id      INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    person_id    INTEGER REFERENCES persons(id) ON DELETE SET NULL,
    model_name   TEXT    NOT NULL,                    -- 嵌入模型身份 = 向量空间
    bbox_x       REAL NOT NULL, bbox_y REAL NOT NULL,
    bbox_w       REAL NOT NULL, bbox_h REAL NOT NULL, -- 归一化 [0,1]，与显示分辨率解耦
    landmarks    BLOB,                                -- 5 关键点（对齐+展示，f32 LE 5×2）
    det_score    REAL NOT NULL,
    quality      REAL NOT NULL DEFAULT 0,             -- 综合质量分（挑 cover / 滤低质聚类）
    embedding    BLOB NOT NULL,                       -- 嵌入向量（f32 LE，维度由 FaceProfile 定）
    is_confirmed INTEGER NOT NULL DEFAULT 0,          -- 用户确认/手动指派（重聚类不打散）
    created_at   INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
CREATE INDEX IF NOT EXISTS idx_faces_item   ON faces(item_id);
CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);
CREATE INDEX IF NOT EXISTS idx_faces_model  ON faces(model_name);

-- ── media_items.face_status：人脸检测状态机（仿 ai_status，独立开关）────────────
-- 0=待处理 / 1=处理中 / 2=完成 / 3=错误（支持断点续传 + 孤儿恢复）
ALTER TABLE media_items ADD COLUMN face_status INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_media_face ON media_items(face_status) WHERE face_status < 3;

-- ── 人脸配置默认值 ────────────────────────────────────────────────────────────
-- face_auto_analyze 默认 0：人脸分析较重，不随扫描自动触发，由用户主动开启。
-- face_model_active 默认 'yunet-sface'：商用友好（YuNet MIT + SFace Apache-2.0）。
INSERT OR IGNORE INTO app_config (key, value) VALUES
    ('face_enabled',      '1'),
    ('face_auto_analyze', '0'),
    ('face_model_active', 'yunet-sface');
";

/// All DDL for schema version 9 — exotic（冷门格式插件）子系统 Part1。
/// 模式版本 9 的所有 DDL —— 冷门格式插件子系统 Part1（v3 总纲 §5.3 / Part1 §1.3）。
///
/// 三份真相分表，互不推导（v3 §5.1）：
///   - exotic_catalog_formats：能力真相（某格式有无产品/属哪类/提供哪些能力/哪些平台）
///   - exotic_plugins        ：安装真相（磁盘装了什么版本、各文件应是什么 hash）
///   - exotic_tasks          ：处理真相（能力级任务，独立重试/失效；非 media_items 上的状态列）
///
/// 关键设计：
///   - 不新增 `media_items.exotic_status`（会把普通媒体全标待处理；见 v3 §2.2）。
///   - exotic_tasks 含 `claimed_at` + `lease_owner`（R2 必选；项目无单实例插件，不得以单实例兜底）。
///   - 禁新增 `exotic_dev_mode` 配置（Release 不得有授权旁路；D8 / Part2 §3.5）。
pub const SCHEMA_V9: &str = "
-- ── exotic_catalog_formats：能力真相（内置 Catalog + 远程签名 Catalog 的本地投影）──────
-- 主键 = 规范化扩展名（小写、无点）。format → offering / media_kind / capabilities。
CREATE TABLE IF NOT EXISTS exotic_catalog_formats (
    format            TEXT PRIMARY KEY,              -- 小写扩展名，[a-z0-9]{1,16}
    plugin_id         TEXT NOT NULL,
    display_name      TEXT NOT NULL,
    media_kind        TEXT NOT NULL,                 -- image / video / audio / document
    capabilities_json TEXT NOT NULL,                 -- JSON 数组，如 [\"thumbnail\"]
    license_tier      TEXT NOT NULL,                 -- free / paid
    platforms_json    TEXT NOT NULL,                 -- JSON 数组，rust target triple
    min_host_version  TEXT NOT NULL,
    store_url         TEXT,
    catalog_sequence  INTEGER NOT NULL,              -- 防目录回滚（R11；安全单调）
    source            TEXT NOT NULL                  -- builtin / remote
);

-- ── exotic_plugins：安装真相（已验证的 Package Manifest 落地）─────────────────────────
CREATE TABLE IF NOT EXISTS exotic_plugins (
    plugin_id          TEXT PRIMARY KEY,
    version            TEXT NOT NULL,                -- 展示用版本字符串
    manifest_hash      TEXT NOT NULL,
    package_sequence   INTEGER NOT NULL,             -- 防包回滚（R11；安全单调），升级只许更高
    install_state      TEXT NOT NULL,               -- installed / disabled / broken ...
    installed_at       INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL
);

-- ── exotic_tasks：处理真相（能力级任务表）────────────────────────────────────────────
-- status：0=pending / 1=processing / 2=done / 3=retryable_error / 4=terminal_error
-- 未安装/未授权/禁用不写任务状态；Scheduler 领取时经 FormatResolution 门控（v3 §5.3）。
CREATE TABLE IF NOT EXISTS exotic_tasks (
    id                 INTEGER PRIMARY KEY,
    item_id            INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    plugin_id          TEXT NOT NULL,
    capability         TEXT NOT NULL,
    status             INTEGER NOT NULL DEFAULT 0,
    input_fingerprint  TEXT,                          -- SHA-256(规范化结构)，源/版本/参数变化即失效
    attempts           INTEGER NOT NULL DEFAULT 0,
    next_retry_at      INTEGER,
    claimed_at         INTEGER,                        -- 租约时间戳（R2）
    lease_owner        TEXT,                           -- 进程级 instance_id（仅内存生成，落库防跨实例覆盖，R2）
    last_error_code    TEXT,
    last_error_message TEXT,
    output_path        TEXT,
    worker_version     TEXT,
    created_at         INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    updated_at         INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    UNIQUE(item_id, plugin_id, capability)
);

-- 领取索引：按 (plugin_id, capability, status, next_retry_at) 取就绪任务（百万库覆盖索引）。
CREATE INDEX IF NOT EXISTS idx_exotic_tasks_ready
ON exotic_tasks(plugin_id, capability, status, next_retry_at);
-- 跨流水线门控索引：按 item 查某 capability 是否仍未完成（CLIP/face/derive 的 NOT EXISTS）。
CREATE INDEX IF NOT EXISTS idx_exotic_tasks_item
ON exotic_tasks(item_id, capability, status);

-- ── 配置默认值（Part1 §1.3；禁 exotic_dev_mode）──────────────────────────────────────
INSERT OR IGNORE INTO app_config (key, value) VALUES
    ('exotic_enabled',      'true'),
    ('exotic_auto_process', 'true'),
    ('exotic_paused',       'false'),
    ('exotic_max_workers',  '0');     -- 0 = 由 Host 自动决定并发上限
";

/// All DDL for schema version 10 — 卷可用性模型（Part0 §6 / Part1 §3.2）。
/// 模式版本 10 的所有 DDL —— 移动盘/网络盘插拔感知，「离线 ≠ 删除」的数据基础。
///
/// **纯加表 + 加列 + 加索引，零破坏**（既有行经 DEFAULT 自动在线/向后兼容）：
///   - `volumes`            ：卷登记表（稳定身份锚点 = Win 卷GUID / mac 卷UUID / 网络 UNC）
///   - `scan_roots` 扩列    ：`volume_id` / `volume_subpath`
///   - `media_items` 扩列：`volume_id`（冗余免JOIN）/ `volume_relative_path` / `availability` 三态、
///     `color_label`（Part5 T16 硬前置）、`content_identifier`（Live Photo/HEIC，Part2 硬前置）
///   - `persons.model_name` ：人脸模型轨隔离（Part4 T6 硬前置；旧 persons 经 DEFAULT 归 default 轨）
///   - `face_rejections`    ：人脸「不是这个人」负样本（Part4 §3.5.1 / §8.4 硬前置）
///
/// 关键设计：
///   - `availability` 与 `is_deleted` **正交**：前者扫描/卷驱动（online/offline/missing，可自动复原），
///     后者用户驱动（回收站，仅用户可逆）——扫描路径永不触碰 is_deleted（Part2 §3.2.4）。
///   - 多个后续 Part 的零散加列在 terminal review 时**合并进 V10**，防「跨 Part 落空」+ 免去仅为
///     一列而起 V11/V12（迁移单向，V10 落库后无法回补）。
///   - 回填 DML 与 DDL **同事务**（migrate_step 的 unchecked_transaction）：失败整块回滚、版本不前进。
pub const SCHEMA_V10: &str = "
-- ── SCHEMA_V10：卷可用性（移动盘/网络盘插拔感知，Part0 §6）──────────────

-- 卷登记表：稳定身份锚点（Win 卷GUID / mac 卷UUID / 网络 UNC）
CREATE TABLE IF NOT EXISTS volumes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    stable_id       TEXT    NOT NULL UNIQUE,            -- Win '{GUID}' / mac UUID / 规范化UNC '//host/share'
    label           TEXT,                               -- 卷标(展示用，可重命名)
    kind            TEXT    NOT NULL DEFAULT 'local',    -- 'local'|'removable'|'network'
    last_mount_path TEXT,                               -- 最近挂载点/盘符(提示+运行期路径重组，非身份键)
    last_seen       INTEGER,                            -- 最近在线 unix 秒
    is_online       INTEGER NOT NULL DEFAULT 0,         -- 运行期状态(启动 probe_volumes 刷新)
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

-- scan_roots：关联卷 + 卷内子路径(path 列语义降级为'最后已知绝对路径')
-- ALTER ADD COLUMN 带 REFERENCES 时列必须可空(SQLite 约束)；NULL=本地固定路径(向后兼容)。
-- ON DELETE SET NULL 与 media_items.volume_id 对称、与'删卷后根 volume_id 置空'语义一致。
ALTER TABLE scan_roots ADD COLUMN volume_id      INTEGER REFERENCES volumes(id) ON DELETE SET NULL;
ALTER TABLE scan_roots ADD COLUMN volume_subpath TEXT;  -- 卷内子路径，与 volumes.last_mount_path 拼合得绝对路径

-- media_items：冗余 volume_id(免三表JOIN批量切换整盘) + 卷内相对路径 + 可用性三态
ALTER TABLE media_items ADD COLUMN volume_id            INTEGER REFERENCES volumes(id) ON DELETE SET NULL;
ALTER TABLE media_items ADD COLUMN volume_relative_path TEXT;                     -- 卷根起完整相对路径(正斜杠)，重挂载重链接键
ALTER TABLE media_items ADD COLUMN availability         TEXT NOT NULL DEFAULT 'online';  -- 'online'|'offline'|'missing'

-- persons：人脸模型轨隔离(切轨维度,与 faces.model_name 对称；Part4 T6 硬前置)
-- 旧 persons 经 DEFAULT 自动归 'yunet-sface' 默认轨(回填随本事务)。
ALTER TABLE persons ADD COLUMN model_name TEXT NOT NULL DEFAULT 'yunet-sface';

-- media_items 颜色标签(Part5 星级颜色标签 T16 硬依赖)
-- ⚠️ 色数(7)与值域未经产品/用户调研,属临时产品决策——Part5 T16 接前端时可改。
--    暂不加 CHECK 约束(SQLite 给已有列加 CHECK 需重建表,代价高且当前无脏数据来源)。
ALTER TABLE media_items ADD COLUMN color_label INTEGER NOT NULL DEFAULT 0;  -- 0=无 / 1-7 色档

-- media_items HEIC ContentIdentifier(Part2 §3.5.2 Live Photo 匹配/HEIC 元数据硬依赖)
-- 可空，enricher 读 HEIC EXIF/QuickTime 元数据回填；NULL=非 Live Photo/未读取。
ALTER TABLE media_items ADD COLUMN content_identifier TEXT;
CREATE INDEX IF NOT EXISTS idx_media_content_id ON media_items(content_identifier) WHERE content_identifier IS NOT NULL;

-- face_rejections：人脸'不是这个人'负样本(Part4 §3.5.1 reject_face_candidate；recluster 跳过已拒绝对，防质心相近反复误聚)
CREATE TABLE IF NOT EXISTS face_rejections (
    face_id    INTEGER NOT NULL REFERENCES faces(id)   ON DELETE CASCADE,
    person_id  INTEGER NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
    PRIMARY KEY (face_id, person_id)
);

-- 部分索引：仅离线/异常项与按卷过滤建索引(99% 在线项不占索引)
CREATE INDEX IF NOT EXISTS idx_media_avail  ON media_items(availability) WHERE availability != 'online';
CREATE INDEX IF NOT EXISTS idx_media_volume ON media_items(volume_id)    WHERE volume_id IS NOT NULL;

-- ── 卷回填(DML 与 DDL 同事务)──────────────────────────────────────────────────
-- 为每个现有 scan_root 生成一条 volumes(临时 stable_id='pending:<id>' 占位，probe 后覆写真实 GUID/UUID)。
INSERT OR IGNORE INTO volumes (stable_id, label, kind, last_mount_path, is_online)
SELECT 'pending:' || sr.id, sr.alias, 'local', sr.path, 0 FROM scan_roots sr;
-- scan_roots.volume_id 回填(按 path 关联刚建的 volumes)；volume_subpath 留空。
UPDATE scan_roots SET volume_id = (
    SELECT v.id FROM volumes v WHERE v.last_mount_path = scan_roots.path
), volume_subpath = '' WHERE volume_id IS NULL;
-- media_items.volume_id 回填(经 directory→scan_root→volume 链)。
-- volume_relative_path 留空(百万行 UPDATE 较重)，由 Part2 扫描时填。
UPDATE media_items SET volume_id = (
    SELECT sr.volume_id FROM directories d JOIN scan_roots sr ON sr.id = d.root_id
    WHERE d.id = media_items.directory_id
) WHERE volume_id IS NULL;
";

/// All DDL for schema version 11 — keyset 分页支撑（Part1 §3.5 / T10）。
/// 模式版本 11：复合排序索引 + 回收站 keyset seek 索引。
///
/// **纯索引重建，零数据变更**：
///   - `idx_media_sort` 单列 `(sort_datetime DESC)` → 复合 `(sort_datetime DESC, id DESC)`：
///     给 `query_layout_items` 的 `ORDER BY sort_datetime` 一个**确定性 tiebreaker**（同秒时间戳
///     稳定序、消除布局抖动），并让默认画廊排序**吃满索引**。⚠️ 索引≠tiebreaker：`query_layout_items`
///     的 `ORDER BY` 须**同时**追加 `, m.id {dir}` 次键（已在 queries.rs 统一追加），缺一不可。
///   - 新增 `idx_media_trash (deleted_at DESC, id DESC) WHERE is_deleted=1`：支撑回收站 keyset seek
///     翻页（`get_trash_keyset`，取代 OFFSET，百万行恒定 <5ms）。
pub const SCHEMA_V11: &str = "
-- idx_media_sort 单列 → 复合键。DROP+CREATE：旧索引无次键、ALTER 不能改索引列。
DROP INDEX IF EXISTS idx_media_sort;
CREATE INDEX IF NOT EXISTS idx_media_sort ON media_items(sort_datetime DESC, id DESC)
    WHERE is_deleted = 0 AND companion_of IS NULL;

-- 回收站 keyset seek 复合索引（行值比较 (deleted_at,id)<(?,?) 走此索引）。
CREATE INDEX IF NOT EXISTS idx_media_trash ON media_items(deleted_at DESC, id DESC)
    WHERE is_deleted = 1;
";
