# Picasa Next — Implementation Plan v1.0

> **产品定位**：面向 15万+ 媒体文件的本地高性能资产浏览器及管理工具
> **核心能力**：统一管理**图片、视频、音频、文本/文档**四大类媒体内容
> **技术路线**：Rust (Tauri V2) + Vue 3 (Vite + TS)，极限性能，跨平台就绪
> **开发模式**：个人开发者适用，兼顾工程化基础与开发效率
> **开发平台**：Windows 11

---

## 一、四大媒体类型与格式支持

### 1.1 媒体类型总览

| 类型 | 代码标识 | 核心能力 | 缩略图策略 | 实现阶段 |
|------|---------|----------|------------|----------|
| **图片** | `image` | Justified Layout + 大图预览 + EXIF + 动态照片 | EXIF 快速路径 / 完整解码缩放 | **Phase 1** (核心) |
| **视频** | `video` | 关键帧缩略图 + 时长角标 + 内建播放器 | FFmpeg 帧提取 | Phase 2 |
| **音频** | `audio` | 封面提取 + 时长角标 + 内建播放器 + 标签元数据 | 嵌入封面提取 / 音符图标占位 | Phase 2 |
| **文本** | `document` | 首页渲染 (PDF/SVG) + 类型图标 + 预览/外部打开 | PDF 首页渲染 / SVG 光栅化 / 类型图标 | Phase 2 |

### 1.2 Phase 1 — 图片格式

| 格式 | 扩展名 | 引擎 |
|------|--------|------|
| JPEG | .jpg, .jpeg | ImageRsEngine |
| PNG | .png | ImageRsEngine |
| WebP | .webp | ImageRsEngine |
| BMP | .bmp | ImageRsEngine |
| GIF | .gif (取第一帧) | ImageRsEngine |
| TIFF | .tif, .tiff | ImageRsEngine |
| PSD | .psd (扁平合成层) | ImageRsEngine |

### 1.3 Phase 1 — 动态照片

| 类型 | 文件形式 | 检测方式 |
|------|----------|----------|
| Apple Live Photo | .jpg/.jpeg + .mov 配对 | 文件名茎匹配 + EXIF ContentIdentifier |
| Google Motion Photo | 单个 .jpg (嵌入 MP4) | XMP `GCamera:MotionPhoto=1` |
| Samsung Motion Photo | 单个 .jpg (嵌入 MP4) | 文件尾部标记 / XMP |

### 1.4 Phase 2 — 图片扩展 (HeicEngine + RawEngine)

| 格式 | 扩展名 | 引擎 |
|------|--------|------|
| HEIC | .heic, .heif | HeicEngine |
| AVIF | .avif | HeicEngine |
| Canon CR2/CR3 | .cr2, .cr3 | RawEngine |
| Nikon NEF | .nef | RawEngine |
| Sony ARW | .arw | RawEngine |
| Adobe DNG | .dng | RawEngine |
| Fuji RAF | .raf | RawEngine |
| Olympus ORF | .orf | RawEngine |
| Panasonic RW2 | .rw2 | RawEngine |
| Pentax PEF | .pef | RawEngine |
| Samsung SRW | .srw | RawEngine |

### 1.5 Phase 2 — 视频格式 (FFmpeg Sidecar)

| 格式 | 扩展名 |
|------|--------|
| MPEG-4 | .mp4, .m4v |
| QuickTime | .mov |
| AVI | .avi |
| Matroska | .mkv |
| WebM | .webm |
| Windows Media | .wmv |
| Flash Video | .flv |
| MPEG | .mpg, .mpeg |
| 3GPP | .3gp, .3g2 |
| Transport Stream | .ts, .mts, .m2ts |
| OGG Video | .ogv |
| ASF | .asf |

### 1.6 Phase 2 — 音频格式 (lofty 元数据 + HTML5 播放)

| 格式 | 扩展名 | 标签格式 |
|------|--------|----------|
| MP3 | .mp3 | ID3v1/v2 |
| FLAC | .flac | Vorbis Comments |
| WAV | .wav | RIFF INFO |
| AAC | .aac, .m4a | MP4/iTunes Atoms |
| OGG Vorbis | .ogg, .oga | Vorbis Comments |
| Opus | .opus | Vorbis Comments |
| WMA | .wma | ASF Metadata |
| AIFF | .aiff, .aif | AIFF Chunks / ID3 |
| APE | .ape | APEv2 |
| ALAC | .alac | MP4/iTunes Atoms |

### 1.7 Phase 2 — 文本/文档格式

| 类别 | 扩展名 | 缩略图方式 |
|------|--------|------------|
| PDF | .pdf | 首页渲染 (mupdf) |
| SVG | .svg | 矢量光栅化 (resvg) |
| Word | .doc, .docx | 类型图标占位 |
| Excel | .xls, .xlsx | 类型图标占位 |
| PowerPoint | .ppt, .pptx | 类型图标占位 |
| Apple iWork | .pages, .numbers, .keynote | 类型图标占位 |
| 纯文本 | .txt | 类型图标占位 |
| Markdown | .md | 类型图标占位 |
| 富文本 | .rtf | 类型图标占位 |
| Illustrator | .ai | 类型图标占位 |
| EPS | .eps | 类型图标占位 |
| EPUB | .epub | 类型图标占位 |

---

## 二、已确认的核心决策汇总

| 编号 | 决策项 | 最终方案 | 架构说明 |
|------|--------|----------|----------|
| Q1 | 哈希算法 | `xxHash3` (xxh3_64) | `cache_key` 字段，**INTEGER (u64)** 原生整数存储 |
| Q2 | 主题色分析 | Phase 2 实现 | 生成缩略图时顺带 MMCQ 提取，零额外磁盘 I/O，仅图片有效 |
| Q3 | 主题色 UI | 双模式并行 | 色相色块矩阵 + 色谱渐变双端滑动条 |
| Q4 | 缩略图格式 | WebP 优先 → JPEG 降级 | WebP 编码失败时自动降级 JPEG |
| Q5 | 极速扫描 | EXIF 缩略图提取 | Phase 1 即打通快速路径 |
| Q6 | 性能日志 | `tracing` 结构化日志 | 不设独立统计库 |
| Q7 | EXIF 库 | `kamadak-exif` | 纯 Rust、极速、安全 |
| Q8 | 目录树 | `parent_id` + `depth` + `name` | 完整递归树结构 |
| Q9 | HEIC 解码 | `libheif-rs`（Phase 2） | C 绑定，成熟稳定 |
| Q10 | RAW 解码 | `rawler`（Phase 2） | 纯 Rust，零 C 依赖 |
| Q11 | 数据库驱动 | 仅 `rusqlite` | 前后端统一，避免双驱动竞争 |
| Q12 | 主题系统 | CSS Variables + data-theme | Light/Dark/System 三态 |
| Q13 | 数据库连接模型 | 读写分离双连接 | WAL 模式下读写不互斥 |
| Q14 | 路径存储策略 | 相对路径 + 锚点架构 | 盘符变更只需修改锚点一次 |
| Q15 | 缓存键 | INTEGER (u64)，文件身份绑定 | `xxh3_64("{rel_path}/{file_name}|{file_mtime}")`，不含 `thumb_size` |
| Q16 | 虚拟滚动 | 自研行级虚拟化 | 适配 Justified Layout 不等高行 |
| Q17 | 布局数据策略 | 分离获取 | 轻量 layout 一次加载 → 按需加载详情 |
| Q18 | 图片预光栅化 | `Image.decode()` | 消除滚动时 GPU 纹理上传掉帧 |
| Q19 | 大图预览模式 | 模态覆盖层 | 非路由，CSS `transform` 缩放/拖拽 |
| Q20 | 核心数据表 | `media_items` | 统一管理图片/视频/音频/文本四大类 |
| Q21 | 媒体类型枚举 | 4 值 + 标志位 | `'image'\|'video'\|'audio'\|'document'` + `is_live_photo` 标志 |
| Q22 | 动态照片 | Phase 1 实现 | 配对检测 + 嵌入检测，本质归类为 `image` |
| Q23 | 视频缩略图 | FFmpeg Sidecar | Tauri 原生 sidecar 打包 |
| Q24 | 视频/音频播放 | HTML5 `<video>`/`<audio>` | `convertFileSrc()` 直读，WebView 原生解码 |
| Q25 | 文档缩略图 | mupdf + resvg | PDF 首页 + SVG 光栅化，其余图标占位 |
| Q26 | 音频元数据 | `lofty` crate | 纯 Rust，支持 ID3/Vorbis/MP4/APE 等全格式标签解析 + 封面提取 |

---

## 三、技术栈总览

### 3.1 后端 (Rust / Tauri V2)

| 类别 | Crate / 工具 | 版本 | 用途 | 引入阶段 |
|------|-------------|------|------|----------|
| **框架** | `tauri` | `^2` | 应用框架 (WebView + Rust) | Phase 1 |
| **数据库** | `rusqlite` | `^0.31` (bundled) | SQLite 统一驱动 | Phase 1 |
| **EXIF** | `kamadak-exif` | latest | EXIF 元数据解析 | Phase 1 |
| **哈希** | `xxhash-rust` | `^0.8` (xxh3) | 极速缓存键生成 | Phase 1 |
| **图像基础** | `image` | `^0.25` | 标准图片编解码 | Phase 1 |
| **缩放** | `fast_image_resize` | latest | SIMD 加速图像缩放 | Phase 1 |
| **占位图** | `thumbhash` | latest | 极小占位图 (~28 bytes/张) | Phase 1 |
| **异步** | `tokio` | `^1` (full) | 异步 I/O + 任务调度 | Phase 1 |
| **目录遍历** | `walkdir` | `^2` | 递归目录扫描 | Phase 1 |
| **序列化** | `serde` + `serde_json` | `^1` | IPC JSON 序列化 | Phase 1 |
| **日志** | `tracing` + `tracing-subscriber` | latest | 结构化分级日志 | Phase 1 |
| **错误** | `thiserror` | latest | 声明式错误类型 | Phase 1 |
| **并发** | `rayon` | latest | CPU 密集型数据并行 | Phase 1 |
| **并发工具** | `tokio-util` | `^0.7` | CancellationToken | Phase 1 |
| **系统垃圾箱** | `trash` | `^5` | 原生回收站 | Phase 2 |
| **HEIC** | `libheif-rs` | `^1.0` | HEIC/HEIF 解码 | Phase 2 |
| **RAW** | `rawler` | latest | 纯 Rust RAW 解码 | Phase 2 |
| **主题色** | `color-thief` | latest | MMCQ 颜色量化提取 | Phase 2 |
| **视频处理** | FFmpeg (Sidecar) | latest stable | 帧提取 + 视频元数据 | Phase 2 |
| **音频元数据** | `lofty` | latest | 音频标签解析 + 封面提取 (ID3/Vorbis/MP4/APE/RIFF) | Phase 2 |
| **PDF 渲染** | `mupdf` (mupdf-rs) | latest | PDF 首页缩略图 | Phase 2 |
| **SVG 渲染** | `resvg` | latest | SVG 光栅化 | Phase 2 |
| **文件监听** | `notify` | `^7` | 文件系统变更监听 | Phase 3 |
| **内容指纹** | `blake3` | `^1.5` | 文件去重哈希 | Phase 4 |

### 3.2 前端 (Vue 3 + TypeScript)

| 类别 | 库 | 用途 |
|------|-----|------|
| **框架** | Vue 3 (Composition API) | 响应式 UI |
| **状态管理** | Pinia | 全局状态 |
| **路由** | Vue Router 4 | 视图切换 |
| **构建** | Vite | 开发/构建工具 |
| **虚拟滚动** | 自研 (行级虚拟化) | 百万级列表渲染 |
| **布局** | 自研 Justified Layout | 等高不等宽瀑布流 |
| **样式** | Vanilla CSS + CSS Variables | 主题系统 |
| **类型** | TypeScript strict mode | 全量类型覆盖 |
| **IPC** | `@tauri-apps/api` | Rust ↔ JS 通信 |

### 3.3 Tauri 插件

| 插件 | 用途 |
|------|------|
| `tauri-plugin-dialog` | 目录选择对话框 |
| `tauri-plugin-fs` | `convertFileSrc` 依赖 |
| `tauri-plugin-shell` | 资源管理器显示 + FFmpeg sidecar 调用 |

---

## 四、项目目录结构

```
picasa-next/
├── src-tauri/                          # Rust 后端
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json               # Tauri V2 权限声明
│   ├── icons/
│   ├── binaries/                       # Sidecar (Phase 2: ffmpeg)
│   └── src/
│       ├── main.rs                     # Desktop 入口
│       ├── lib.rs                      # 模块声明
│       ├── error.rs                    # 统一错误类型
│       ├── state.rs                    # AppState（读写双连接 + 全局状态）
│       │
│       ├── db/
│       │   ├── mod.rs
│       │   ├── connection.rs           # 读写双连接 + PRAGMA
│       │   ├── schema.rs              # 建表 SQL（四大媒体类型字段预留）
│       │   ├── models.rs             # 数据模型
│       │   └── queries.rs            # SQL 查询
│       │
│       ├── scanner/
│       │   ├── mod.rs
│       │   ├── walker.rs             # 递归遍历 + 格式分类
│       │   ├── metadata.rs           # EXIF / XMP 解析
│       │   ├── live_photo.rs         # 动态照片检测
│       │   └── watcher.rs            # 文件监听 (Phase 3)
│       │
│       ├── thumbnail/
│       │   ├── mod.rs
│       │   ├── generator.rs          # 统一生成入口（按 media_type 分发）
│       │   ├── exif_thumb.rs         # EXIF 嵌入缩略图
│       │   ├── cache.rs             # 两级哈希缓存
│       │   └── thumbhash.rs         # ThumbHash 占位图
│       │
│       ├── engine/                     # 图片解码引擎
│       │   ├── mod.rs
│       │   ├── traits.rs
│       │   ├── image_rs.rs
│       │   ├── heic.rs              # Phase 2
│       │   └── raw.rs               # Phase 2
│       │
│       ├── video/                      # Phase 2
│       │   ├── mod.rs
│       │   ├── ffmpeg.rs             # FFmpeg sidecar 封装
│       │   ├── frame_extractor.rs
│       │   └── metadata.rs
│       │
│       ├── audio/                      # Phase 2
│       │   ├── mod.rs
│       │   ├── metadata.rs           # lofty 标签解析
│       │   └── cover_art.rs          # 封面提取
│       │
│       ├── document/                   # Phase 2
│       │   ├── mod.rs
│       │   ├── pdf_thumb.rs
│       │   └── svg_render.rs
│       │
│       ├── color/                      # Phase 2
│       │   ├── mod.rs
│       │   └── extractor.rs
│       │
│       ├── ipc/
│       │   ├── mod.rs
│       │   ├── scan_commands.rs
│       │   ├── media_commands.rs
│       │   ├── thumbnail_commands.rs
│       │   ├── system_commands.rs
│       │   └── config_commands.rs
│       │
│       └── utils/
│           ├── mod.rs
│           ├── hash.rs
│           ├── path.rs
│           └── format.rs            # 格式检测 + 四大类型分类
│
├── src/                               # Vue 前端
│   ├── App.vue
│   ├── main.ts
│   ├── env.d.ts
│   │
│   ├── assets/styles/
│   │   ├── index.css
│   │   ├── variables.css
│   │   ├── theme-dark.css
│   │   ├── theme-light.css
│   │   ├── reset.css
│   │   └── animations.css
│   │
│   ├── components/
│   │   ├── layout/
│   │   │   ├── AppShell.vue
│   │   │   ├── AppSidebar.vue
│   │   │   ├── AppToolbar.vue         # 视图切换、排序、搜索、媒体类型筛选芯片
│   │   │   └── AppStatusBar.vue
│   │   │
│   │   ├── media/
│   │   │   ├── MediaGrid.vue         # 统一网格（四大类型）
│   │   │   ├── MediaCard.vue         # 统一卡片（按类型渲染角标）
│   │   │   ├── MediaDetail.vue       # 统一预览入口（分发至类型专用视图）
│   │   │   ├── ImageViewer.vue       # 图片预览（缩放/拖拽/EXIF）
│   │   │   ├── VideoPlayer.vue       # 视频播放器 (Phase 2)
│   │   │   ├── AudioPlayer.vue       # 音频播放器 (Phase 2)
│   │   │   ├── DocumentViewer.vue    # 文档预览 (Phase 2)
│   │   │   └── DateSeparator.vue
│   │   │
│   │   ├── sidebar/
│   │   │   ├── FolderTree.vue
│   │   │   ├── SmartAlbums.vue       # 智能相册（含四大类型筛选）
│   │   │   └── ColorFilter.vue       # Phase 2
│   │   │
│   │   └── common/
│   │       ├── ProgressBar.vue
│   │       ├── ThemeToggle.vue
│   │       ├── ContextMenu.vue       # Phase 3
│   │       └── Toast.vue
│   │
│   ├── composables/
│   │   ├── useVirtualScroll.ts
│   │   ├── useJustifiedLayout.ts
│   │   ├── useRequestQueue.ts
│   │   ├── useThumbnail.ts
│   │   ├── useMediaDetail.ts
│   │   ├── useFolderTree.ts
│   │   ├── useTheme.ts
│   │   ├── useSelection.ts
│   │   └── useColorFilter.ts         # Phase 2
│   │
│   ├── stores/
│   │   ├── mediaStore.ts
│   │   ├── scanStore.ts
│   │   ├── uiStore.ts
│   │   └── filterStore.ts            # 媒体类型 + 颜色 + 日期 + 评分筛选状态
│   │
│   ├── constants/
│   │   ├── formats.ts                 # 四大类型格式注册表
│   │   ├── defaults.ts
│   │   └── ipc.ts
│   │
│   ├── types/
│   │   ├── media.ts
│   │   ├── ipc.ts
│   │   └── ui.ts
│   │
│   ├── router/
│   │   └── index.ts
│   │
│   └── utils/
│       ├── thumbhash.ts
│       └── format.ts                  # 日期/文件大小/时长格式化
│
├── index.html
├── vite.config.ts
├── tsconfig.json
├── package.json
└── README.md
```

---

## 五、数据库设计

### 5.1 PRAGMA 配置

在 `connection.rs` 中**每条连接**初始化时执行：

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -64000;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
PRAGMA temp_store = MEMORY;
PRAGMA mmap_size = 268435456;
```

> [!CAUTION]
> PRAGMA 是连接级设置，读连接和写连接都需要分别执行，不可仅在迁移脚本中执行一次。

### 5.2 读写分离连接架构

```rust
pub struct AppState {
    /// 写连接（扫描入库、更新、删除）
    pub db_writer: Mutex<Connection>,
    /// 只读连接（前端查询，WAL 下与写不互斥）
    pub db_reader: Mutex<Connection>,
    /// 扫描取消令牌
    pub scan_tokens: Mutex<HashMap<i64, CancellationToken>>,
}
```

```rust
pub fn init_database(db_path: &Path) -> Result<(Connection, Connection)> {
    let writer = Connection::open(db_path)?;
    apply_pragmas(&writer)?;

    let reader = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    apply_pragmas(&reader)?;

    create_tables(&writer)?;
    Ok((writer, reader))
}
```

### 5.3 主数据库 (`picasa_next.db`)

#### 表：`app_config`

```sql
CREATE TABLE IF NOT EXISTS app_config (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

INSERT OR IGNORE INTO app_config VALUES ('schema_version', '1');
INSERT OR IGNORE INTO app_config VALUES ('thumb_size', '300');
INSERT OR IGNORE INTO app_config VALUES ('thumb_format', 'webp');
INSERT OR IGNORE INTO app_config VALUES ('thumb_quality', '80');
INSERT OR IGNORE INTO app_config VALUES ('theme', 'system');
```

#### 表：`scan_roots` — 路径锚点

```sql
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
```

#### 表：`directories` — 递归树（相对路径）

```sql
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
CREATE INDEX idx_dir_root   ON directories(root_id);
CREATE INDEX idx_dir_parent ON directories(parent_id);
```

#### 表：`media_items` — 核心媒体表（四大类型统一）

```sql
CREATE TABLE IF NOT EXISTS media_items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    directory_id    INTEGER NOT NULL REFERENCES directories(id) ON DELETE CASCADE,

    -- ═══════════════ 通用字段（所有类型） ═══════════════
    file_name       TEXT    NOT NULL,
    file_size       INTEGER NOT NULL,
    file_mtime      INTEGER NOT NULL,
    file_format     TEXT    NOT NULL,                 -- 小写扩展名

    media_type      TEXT    NOT NULL DEFAULT 'image', -- 'image'|'video'|'audio'|'document'

    width           INTEGER NOT NULL,                 -- 像素宽（音频/文档赋默认值）
    height          INTEGER NOT NULL,                 -- 像素高（音频/文档赋默认值）

    sort_datetime   INTEGER NOT NULL,                 -- COALESCE(exif_datetime, file_mtime)
    cache_key       INTEGER NOT NULL,                 -- xxh3_64 u64

    thumb_status    INTEGER NOT NULL DEFAULT 0,       -- 0=待生成 1=已生成 2=失败
    thumb_path      TEXT,
    thumbhash       BLOB,

    is_favorited    INTEGER NOT NULL DEFAULT 0,
    is_deleted      INTEGER NOT NULL DEFAULT 0,
    deleted_at      INTEGER,
    rating          INTEGER DEFAULT 0,

    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),

    -- ═══════════════ 图片专属 ═══════════════
    orientation     INTEGER DEFAULT 1,               -- EXIF Orientation (1-8)
    is_live_photo   INTEGER DEFAULT 0,               -- 1=动态照片（配对或嵌入）
    has_embedded_video INTEGER DEFAULT 0,             -- 1=嵌入式 Motion Photo
    companion_of    INTEGER REFERENCES media_items(id) ON DELETE SET NULL,
                                                     -- 配对动态照片：MOV 伴侣 → 主体

    exif_datetime   INTEGER,
    exif_make       TEXT,
    exif_model      TEXT,
    exif_lens       TEXT,
    exif_focal_length REAL,
    exif_aperture   REAL,
    exif_shutter    TEXT,
    exif_iso        INTEGER,
    exif_gps_lat    REAL,
    exif_gps_lng    REAL,

    dominant_hue    INTEGER,                         -- Phase 2 色相桶
    dominant_sat    INTEGER,
    dominant_lum    INTEGER,
    dominant_hex    TEXT,
    is_monochrome   INTEGER DEFAULT 0,

    -- ═══════════════ 视频/音频共享 ═══════════════
    duration_ms     INTEGER,                         -- 时长（毫秒）

    -- ═══════════════ 视频专属 ═══════════════
    video_codec     TEXT,                            -- 'h264','hevc','vp9'...

    -- ═══════════════ 音频专属 ═══════════════
    audio_codec     TEXT,                            -- 'mp3','flac','aac','vorbis'...
    artist          TEXT,                            -- 艺术家/创作者
    album_title     TEXT,                            -- 专辑名
    track_title     TEXT,                            -- 曲目标题

    -- ═══════════════ 文档专属 ═══════════════
    page_count      INTEGER,                         -- 文档页数

    -- ═══════════════ 未来扩展 ═══════════════
    content_hash    TEXT,                            -- Phase 4: BLAKE3 去重

    UNIQUE(directory_id, file_name)
);

-- ══════════════════════════ 索引 ══════════════════════════
CREATE INDEX idx_media_directory    ON media_items(directory_id);
CREATE INDEX idx_media_sort         ON media_items(sort_datetime DESC)
                                    WHERE is_deleted = 0 AND companion_of IS NULL;
CREATE INDEX idx_media_cache_key    ON media_items(cache_key);
CREATE INDEX idx_media_format       ON media_items(file_format);
CREATE INDEX idx_media_type         ON media_items(media_type) WHERE is_deleted = 0;
CREATE INDEX idx_media_thumb        ON media_items(thumb_status) WHERE thumb_status != 1;
CREATE INDEX idx_media_fav          ON media_items(is_favorited)
                                    WHERE is_favorited = 1 AND is_deleted = 0;
CREATE INDEX idx_media_del          ON media_items(is_deleted) WHERE is_deleted = 1;
CREATE INDEX idx_media_hue          ON media_items(dominant_hue, is_monochrome)
                                    WHERE is_deleted = 0 AND dominant_hue IS NOT NULL;
CREATE INDEX idx_media_rating       ON media_items(rating) WHERE is_deleted = 0 AND rating > 0;
CREATE INDEX idx_media_hash         ON media_items(content_hash) WHERE content_hash IS NOT NULL;
CREATE INDEX idx_media_companion    ON media_items(companion_of) WHERE companion_of IS NOT NULL;
CREATE INDEX idx_media_live         ON media_items(is_live_photo) WHERE is_live_photo = 1;
CREATE INDEX idx_media_artist       ON media_items(artist) WHERE artist IS NOT NULL;
```

> [!IMPORTANT]
> **伴侣过滤**：所有面向用户的网格查询必须追加 `AND companion_of IS NULL` 隐藏配对动态照片的视频伴侣。

> [!NOTE]
> **默认宽高**：音频文件赋 400×400（方形封面），文档赋 595×842（A4 比例）或 400×400（非 PDF），确保 `width/height NOT NULL` 约束满足 Justified Layout。

#### 表：`albums` / `album_items`（Phase 3）

```sql
CREATE TABLE IF NOT EXISTS albums (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL,
    description     TEXT,
    cover_item_id   INTEGER REFERENCES media_items(id) ON DELETE SET NULL,
    sort_order      INTEGER DEFAULT 0,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS album_items (
    album_id  INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    item_id   INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    sort_order INTEGER DEFAULT 0,
    added_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (album_id, item_id)
);
CREATE INDEX idx_album_items_album ON album_items(album_id);
```

#### 表：`tags` / `item_tags`（Phase 3）

```sql
CREATE TABLE IF NOT EXISTS tags (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name      TEXT    NOT NULL UNIQUE,
    color     TEXT,
    parent_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS item_tags (
    item_id INTEGER NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (item_id, tag_id)
);
CREATE INDEX idx_item_tags_tag ON item_tags(tag_id);
```

### 5.4 路径工具 (`utils/path.rs`)

```rust
/// 数据库路径规范化：统一正斜杠
pub fn normalize_db_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// 运行时拼接
pub fn resolve_media_path(root: &str, rel: &str, name: &str) -> PathBuf {
    let mut p = PathBuf::from(root);
    if !rel.is_empty() { p.push(rel); }
    p.push(name);
    p
}

/// 按 ID 查询并拼接
pub fn resolve_media_path_by_id(conn: &Connection, id: i64) -> Result<PathBuf> {
    let (r, d, f): (String, String, String) = conn.query_row(
        "SELECT sr.path, d.rel_path, mi.file_name
         FROM media_items mi
         JOIN directories d ON mi.directory_id = d.id
         JOIN scan_roots sr ON d.root_id = sr.id
         WHERE mi.id = ?1", [id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    Ok(resolve_media_path(&r, &d, &f))
}
```

### 5.5 媒体类型分类 (`utils/format.rs`)

```rust
pub enum MediaType { Image, Video, Audio, Document }

pub fn classify_media_type(ext: &str) -> Option<MediaType> {
    match ext {
        "jpg"|"jpeg"|"png"|"webp"|"bmp"|"gif"|"tiff"|"tif"|"psd"
        |"heic"|"heif"|"avif"
        |"cr2"|"cr3"|"nef"|"arw"|"dng"|"raf"|"orf"|"rw2"|"pef"|"srw"
            => Some(MediaType::Image),

        "mp4"|"m4v"|"mov"|"avi"|"mkv"|"webm"|"wmv"|"flv"
        |"mpg"|"mpeg"|"3gp"|"3g2"|"ts"|"mts"|"m2ts"|"ogv"|"asf"
            => Some(MediaType::Video),

        "mp3"|"flac"|"wav"|"aac"|"m4a"|"ogg"|"oga"|"opus"
        |"wma"|"aiff"|"aif"|"ape"|"alac"
            => Some(MediaType::Audio),

        "pdf"|"svg"|"doc"|"docx"|"xls"|"xlsx"|"ppt"|"pptx"
        |"pages"|"numbers"|"keynote"|"txt"|"md"|"rtf"
        |"ai"|"eps"|"epub"
            => Some(MediaType::Document),

        _ => None,
    }
}
```

> [!NOTE]
> Phase 1 扫描仅处理 `Image` + 动态照片 MOV 伴侣。`Video`/`Audio`/`Document` 格式已注册，Phase 2 启用扫描开关后即可无缝支持。

---

## 六、IPC 架构设计

### 6.1 Tauri Command（请求-响应）

#### 扫描管理 (`scan_commands.rs`)

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `add_scan_root` | `{ path }` | `ScanRoot` | 添加扫描根目录 |
| `remove_scan_root` | `{ id }` | `void` | 移除 |
| `list_scan_roots` | `void` | `ScanRoot[]` | 列表 |
| `start_scan` | `{ root_id, on_progress: Channel }` | `void` | 异步扫描 |
| `stop_scan` | `{ root_id }` | `void` | 取消 |

#### 媒体查询与管理 (`media_commands.rs`)

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `get_media_layout` | `{ directory_id?, filters? }` | `LayoutItem[]` | 轻量布局：`{id, w, h, sort_datetime, media_type, is_live_photo, duration_ms?}` |
| `get_media_items` | `{ offset, limit, sort_by, order, filters? }` | `MediaPage` | 分页详情 |
| `get_media_detail` | `{ id }` | `MediaDetail` | 完整信息 + 绝对路径 |
| `get_companion_video_url` | `{ item_id }` | `string` | 动态照片视频 URL |
| `toggle_favorite` | `{ item_id }` | `void` | 收藏 |
| `set_rating` | `{ item_id, rating }` | `void` | 评分 |
| `soft_delete_items` | `{ item_ids }` | `void` | 软删除 |
| `restore_items` | `{ item_ids }` | `void` | 恢复 |
| `get_trash` | `{ offset, limit }` | `MediaPage` | 回收站 |
| `get_media_by_color` | `{ hue_buckets }` | `MediaPage` | Phase 2 颜色筛选 |
| `get_stats` | `void` | `AppStats` | 统计（含各类型计数） |
| `get_directory_tree` | `{ root_id }` | `DirNode[]` | 目录树 |
| `get_directory_children` | `{ parent_id }` | `DirNode[]` | 懒加载 |

#### `filters` 参数类型定义

`get_media_layout` 和 `get_media_items` 的 `filters` 参数结构：

```typescript
interface MediaFilter {
  mediaTypes?: ('image' | 'video' | 'audio' | 'document')[]  // 空 = 全部
  livePhotoOnly?: boolean
  favoritedOnly?: boolean
  minRating?: number
  dateRange?: { from: number; to: number }
  hueBuckets?: number[]  // Phase 2
}
```

后端 SQL 动态拼接（参数绑定，非字符串拼接）：
```sql
WHERE is_deleted = 0 AND companion_of IS NULL
  AND media_type IN ('image', 'video')   -- mediaTypes 非空时追加
  AND is_live_photo = 1                  -- livePhotoOnly 时追加
  AND is_favorited = 1                   -- favoritedOnly 时追加
  AND rating >= ?                        -- minRating > 0 时追加
  AND sort_datetime BETWEEN ? AND ?      -- dateRange 时追加
```

> [!TIP]
> `mediaTypes` 为空或 undefined 时不追加类型条件，等同于全部。

#### 缩略图 (`thumbnail_commands.rs`)

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `request_thumbnail` | `{ item_id }` | `ThumbResult` | 单项（按 media_type 分发） |
| `batch_request_thumbnails` | `{ item_ids }` | `ThumbResult[]` | 批量 |

#### 系统 (`system_commands.rs`) / 配置 (`config_commands.rs`)

| 命令名 | 参数 | 返回值 |
|--------|------|--------|
| `show_in_explorer` | `{ item_id }` | `void` |
| `move_to_trash` | `{ item_ids }` | `void` |
| `get_app_config` | `{ key }` | `string?` |
| `set_app_config` | `{ key, value }` | `void` |

### 6.2 Tauri Channel（流式推送）

```rust
#[tauri::command]
async fn start_scan(
    root_id: i64,
    on_progress: tauri::ipc::Channel<ScanProgressPayload>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> { /* ... */ }
```

| 消息类型 | Payload | 推送时机 |
|----------|---------|----------|
| `ScanProgressPayload` | `{ root_id, scanned, total, current_dir }` | 每 500 项或每秒 |
| `ScanCompletedPayload` | `{ root_id, total_items, elapsed_ms }` | 完成 |
| `ScanErrorPayload` | `{ root_id, error }` | 出错 |
| `ThumbBatchPayload` | `{ items: [{item_id, thumb_path, thumbhash}] }` | 每批次 |

低频 Event：`db:media_updated` → `{ action, item_ids }`

### 6.3 媒体获取协议

```typescript
import { convertFileSrc } from '@tauri-apps/api/core'
// 缩略图、原图、视频、音频全部通过 convertFileSrc 直读磁盘
const url = convertFileSrc(absolutePath)
```

---

## 七、媒体处理引擎架构

### 7.1 图片引擎 (ImageEngine trait)

```rust
pub trait ImageEngine: Send + Sync {
    fn name(&self) -> &str;
    fn supported_formats(&self) -> &[&str];
    fn can_handle(&self, format: &str) -> bool { self.supported_formats().contains(&format) }
    fn decode(&self, file_path: &Path) -> Result<DecodedImage, EngineError>;
    fn extract_embedded_thumb(&self, file_path: &Path) -> Result<Option<Vec<u8>>, EngineError> { Ok(None) }
}
```

| 引擎 | 支持格式 | 阶段 |
|------|----------|------|
| `ImageRsEngine` | jpg, jpeg, png, webp, bmp, gif, tiff, psd | Phase 1 |
| `HeicEngine` | heic, heif, avif | Phase 2 |
| `RawEngine` | cr2, cr3, nef, arw, dng, raf, orf, rw2, pef, srw | Phase 2 |

### 7.2 视频帧提取 (Phase 2)

通过 FFmpeg Sidecar 提取关键帧：
```
ffmpeg -ss 1 -i <path> -vframes 1 -f image2pipe -vcodec png -
ffprobe -v quiet -print_format json -show_format -show_streams <path>
```

### 7.3 音频封面提取 (Phase 2)

通过 `lofty` crate 读取嵌入封面：
```rust
use lofty::{Accessor, AudioFile, TaggedFileExt};
use lofty::file::TaggedFile;

pub fn extract_cover_art(path: &Path) -> Result<Option<Vec<u8>>> {
    let tagged = lofty::read_from_path(path)?;
    // 遍历所有标签查找嵌入图片
    for tag in tagged.tags() {
        for pic in tag.pictures() {
            return Ok(Some(pic.data().to_vec()));
        }
    }
    Ok(None)
}

pub fn extract_audio_metadata(path: &Path) -> Result<AudioMeta> {
    let tagged = lofty::read_from_path(path)?;
    let props = tagged.properties();
    let tag = tagged.primary_tag();
    Ok(AudioMeta {
        duration_ms: props.duration().as_millis() as i64,
        audio_codec: detect_codec_from_format(path),
        artist: tag.and_then(|t| t.artist().map(|s| s.to_string())),
        album_title: tag.and_then(|t| t.album().map(|s| s.to_string())),
        track_title: tag.and_then(|t| t.title().map(|s| s.to_string())),
    })
}
```

### 7.4 文档缩略图 (Phase 2)

- **PDF** → `mupdf` 渲染首页为位图
- **SVG** → `resvg` 光栅化
- **其他** → 类型专属图标占位（带扩展名文字标签）

### 7.5 EXIF Orientation 矫正

> [!WARNING]
> 图片缩放前**必须**先矫正 EXIF Orientation，否则缩略图和 ThumbHash 会横躺/颠倒。矫正后 `width/height` 也必须相应交换再存入数据库。

---

## 八、缩略图/封面生成流水线

### 8.1 统一生成入口

```
┌──────────────────────────────────────────────────────────────────┐
│           generator.rs — 按 media_type 分发                      │
│                                                                  │
│  Step 0: 路径拼接 (root + rel_path + file_name)                 │
│  Step 1: 缓存命中检测 → 命中直接返回 ✅                          │
│                                                                  │
│  Step 2: 按类型分发                                              │
│  ├── image  → 7.2.1 图片流水线                                   │
│  ├── video  → 7.2.2 视频流水线 (Phase 2)                        │
│  ├── audio  → 7.2.3 音频流水线 (Phase 2)                        │
│  └── document → 7.2.4 文档流水线 (Phase 2)                      │
│                                                                  │
│  Step 末: ThumbHash 生成 + DB 更新 (所有类型共用)                │
│  ├── 缩放后像素 → 100x100 以内 → thumbhash ~28 bytes            │
│  └── thumb_status=1, thumb_path=..., thumbhash=<blob>            │
└──────────────────────────────────────────────────────────────────┘
```

### 8.2 各类型流水线

**8.2.1 图片**（Phase 1）
```
EXIF 快速路径 → 或 Engine Arena 完整解码 → Orientation 矫正
→ fast_image_resize 300px → WebP(80) / JPEG(85) 降级 → 缓存
→ Phase 2 追加: 主题色 MMCQ 提取
```

**8.2.2 视频**（Phase 2）
```
FFmpeg -ss 1 帧提取 → RGBA 像素
→ fast_image_resize 300px → WebP → 缓存
→ ffprobe 元数据回填 (duration_ms, video_codec, width, height)
```

**8.2.3 音频**（Phase 2）
```
lofty 封面提取 → 有封面? → 解码为 RGBA → 标准缩放 → 缓存
                  无封面? → 音符图标占位渲染 → 缓存
→ lofty 元数据回填 (duration_ms, audio_codec, artist, album_title, track_title)
```

**8.2.4 文档**（Phase 2）
```
PDF → mupdf 首页渲染 → 标准缩放 → 缓存 (+ page_count 回填)
SVG → resvg 光栅化 → 标准缩放 → 缓存
其他 → 类型图标占位 → 缓存
```

### 8.3 缓存目录结构

```
{app_data_dir}/cache/
├── thumbnails/           # 所有类型的缩略图
│   ├── a3/
│   │   └── a3f4b2c1d0e9f7a1.webp
│   └── ...
└── motion_videos/        # 嵌入式动态照片提取的视频
    └── c2/
        └── c2a1b3f4e5d6c7a8.mp4
```

---

## 九、动态照片检测与播放（Phase 1）

### 9.1 检测流程 (`scanner/live_photo.rs`)

```
1. 配对检测 (Apple Live Photo)
   ├── 扫描完一个目录后，收集所有 .mov 文件
   ├── 文件名茎匹配 (IMG_1234.jpg + IMG_1234.mov)
   ├── 主体: media_type='image', is_live_photo=1
   └── 伴侣: media_type='video', companion_of=主体.id (网格中自动隐藏)

2. 嵌入检测 (Google/Samsung Motion Photo)
   ├── EXIF/XMP 解析阶段检查 GCamera:MotionPhoto / Samsung 标记
   ├── media_type='image', is_live_photo=1, has_embedded_video=1
   └── 嵌入视频按需提取到 cache/motion_videos/
```

### 9.2 前端交互

- **MediaCard**: 左上角 `LIVE` 角标；悬停 1 秒自动播放短视频（静音循环）
- **MediaDetail**: 默认静态图；点击/长按 LIVE 按钮 → `<video>` 覆盖播放
- `get_companion_video_url` 统一处理配对式和嵌入式，前端无需区分

---

## 十、前端渲染架构

### 10.1 布局数据分离获取

```
阶段 1: get_media_layout → [{id, w, h, sort_datetime, media_type, is_live_photo, duration_ms?}]
        15万项 ≈ 4MB，一次性加载 → Justified Layout 计算全局坐标

阶段 2: 滚动到某区域 → get_media_items 按需加载该范围的详情 + thumbhash
```

### 10.2 Justified Layout

```
输入: layoutItems[], containerWidth, targetRowHeight(200px), gap(4px)
输出: rows[{ items, computedHeight, y }]

1. aspect = width / height
2. 逐项累加行宽，≥ containerWidth 时封行
3. computedHeight = targetRowHeight × (containerWidth / 行总宽度)
4. absolute + translate3d(x,y,0) 硬件加速定位
5. 最后一行左对齐不拉伸
```

### 10.3 虚拟滚动（自研）

仅渲染可视区域 + 上下各 3 行缓冲。非可视区域用 `padding-top/bottom` 占位。

### 10.4 媒体类型筛选（AppToolbar）

```
┌──────────────────────────────────────────────────────────────────────┐
│ 📂 所有媒体 ▾ │ 🔍 搜索... │ [全部] [图片] [视频] [音频] [文档] │ ↕排序 │
└──────────────────────────────────────────────────────────────────────┘
                               ────── 类型筛选芯片 ──────
```

**交互规则：**

| 行为 | 效果 |
|------|------|
| 点击单个芯片 | 激活该类型，取消其他（单选） |
| Ctrl+点击 | 追加/移除该类型（多选） |
| 点击已激活的唯一芯片 / 点击"全部" | 回到全部类型 |

**视觉状态：**
- 未选中：`background: transparent; color: var(--color-text-secondary)`
- 选中：`background: var(--color-accent-subtle); color: var(--color-accent); font-weight: 600`
- 悬停：`background: var(--color-bg-elevated)`

**filterStore 状态：**

```typescript
// stores/filterStore.ts
export const useFilterStore = defineStore('filter', {
  state: () => ({
    mediaTypes: [] as ('image' | 'video' | 'audio' | 'document')[],
    livePhotoOnly: false,
    favoritedOnly: false,
    minRating: 0,
    dateRange: null as { from: number; to: number } | null,
    hueBuckets: [] as number[],
  }),
  getters: {
    hasTypeFilter: (s) => s.mediaTypes.length > 0,
    asMediaFilter: (s) => ({
      mediaTypes: s.mediaTypes.length ? s.mediaTypes : undefined,
      livePhotoOnly: s.livePhotoOnly || undefined,
      favoritedOnly: s.favoritedOnly || undefined,
      minRating: s.minRating > 0 ? s.minRating : undefined,
      dateRange: s.dateRange ?? undefined,
      hueBuckets: s.hueBuckets.length ? s.hueBuckets : undefined,
    }),
  },
  actions: {
    setMediaType(type: 'image' | 'video' | 'audio' | 'document') {
      this.mediaTypes = [type]
    },
    toggleMediaType(type: 'image' | 'video' | 'audio' | 'document') {
      const i = this.mediaTypes.indexOf(type)
      if (i >= 0) this.mediaTypes.splice(i, 1)
      else this.mediaTypes.push(type)
    },
    clearMediaTypes() { this.mediaTypes = [] },
    resetAll() { this.$reset() },
  },
})
```

**数据流：**
```
Toolbar 芯片点击 -> filterStore.setMediaType / toggleMediaType
  -> watch(filterStore.asMediaFilter)
  -> mediaStore invoke('get_media_layout', { filters })
  -> Justified Layout 重算
  -> 虚拟滚动重置到顶部
```

> [!NOTE]
> **智能相册 vs 类型筛选**：智能相册是"视图级"筛选（全部/收藏/回收站/目录），类型芯片是"叠加级"筛选。两者可叠加——例如"收藏"视图下只看视频。

### 10.4 MediaCard 角标系统

```
┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
│          │  │LIVE      │  │ ▶        │  │ ♪        │  │📄PDF     │
│  [照片]  │  │[动态照片]│  │ [视频帧] │  │[专辑封面]│  │[PDF首页] │
│          │  │          │  │     1:23 │  │     3:45 │  │      3p  │
└──────────┘  └──────────┘  └──────────┘  └──────────┘  └──────────┘
  image        image+live     video         audio        document
```

### 10.5 ThumbHash → Image.decode() → 缩略图

```
1. thumbhash (28B) → 32x32 DataURL → blur(20px) 占位
2. IntersectionObserver → 请求队列 → request_thumbnail
3. new Image(url) + img.decode() 预光栅化
4. decode() 完成 → 替换 src → opacity 0→1 (300ms)
快速滚动 → 批量取消离开视口的请求
```

### 10.6 目录树虚拟化

扁平数组 + `parent_id` + `depth` 缩进。展开的节点参与虚拟列表。

---

## 十一、前端路由与导航

### 11.1 路由表

| 路由 | 组件 | 说明 |
|------|------|------|
| `/` | `MediaGrid` | 全部媒体 |
| `/folder/:id` | `MediaGrid` | 目录筛选 |
| `/favorites` | `MediaGrid` | 收藏 |
| `/trash` | `MediaGrid` | 回收站 |

MediaDetail 为模态覆盖层（非路由），关闭后保持滚动位置。

### 11.2 智能相册 (SmartAlbums)

| 相册 | 过滤条件 | 阶段 |
|------|----------|------|
| 全部媒体 | `companion_of IS NULL AND is_deleted=0` | Phase 1 |
| 图片 | `media_type='image' AND ...` | Phase 1 |
| 动态照片 | `media_type='image' AND is_live_photo=1 AND ...` | Phase 1 |
| 最近导入 | `created_at > :threshold AND ...` | Phase 1 |
| 收藏 | `is_favorited=1 AND ...` | Phase 1 |
| 视频 | `media_type='video' AND ...` | Phase 2 |
| 音频 | `media_type='audio' AND ...` | Phase 2 |
| 文档 | `media_type='document' AND ...` | Phase 2 |

> `...` 代表通用条件 `AND companion_of IS NULL AND is_deleted=0`

---

## 十二、媒体详情预览架构 (MediaDetail)

### 12.1 统一入口

`MediaDetail.vue` 根据 `media_type` 分发到四个专用子组件：

| media_type | 组件 | 核心功能 |
|------------|------|----------|
| `image` | `ImageViewer.vue` | 缩放/拖拽/EXIF 面板/动态照片播放 |
| `video` | `VideoPlayer.vue` | HTML5 播放器 (Phase 2) |
| `audio` | `AudioPlayer.vue` | 封面+播放器+元数据 (Phase 2) |
| `document` | `DocumentViewer.vue` | PDF 内联/SVG 渲染/外部打开 (Phase 2) |

### 12.2 图片预览（Phase 1）

- `convertFileSrc()` 直读原图
- 渐进式加载：缩略图放大 → `img.decode()` 原图 → 平滑替换
- CSS `transform: scale() + translate()` 缩放/拖拽
- 双击切换 fit / 1x
- EXIF 信息面板（I 键切换）
- 动态照片：LIVE 按钮 → `<video>` 覆盖播放

### 12.3 视频播放器（Phase 2）

```html
<video :src="convertFileSrc(path)" controls>
```
- 原生播放控件（播放/暂停/进度/音量/全屏）
- 不支持的编码提示用户使用外部播放器
- 键盘：Space 播放/暂停，← → 快进/快退

### 12.4 音频播放器（Phase 2）

```
┌──────────────────────────────────────────┐
│                                          │
│          ┌────────────────┐              │
│          │                │              │
│          │   [专辑封面]    │              │
│          │   400 × 400    │              │
│          │                │              │
│          └────────────────┘              │
│                                          │
│          Track Title                     │
│          Artist — Album                  │
│                                          │
│    ◁◁    ▶ / ⏸    ▷▷                   │
│    ───●──────────── 3:45 / 5:12         │
│    🔊 ─────●───                         │
│                                          │
│    ┌─ 信息面板 ────────────────────┐     │
│    │ 格式: FLAC                    │     │
│    │ 比特率: 1411 kbps             │     │
│    │ 采样率: 44100 Hz              │     │
│    │ 声道: Stereo                  │     │
│    │ 文件大小: 32.4 MB             │     │
│    └───────────────────────────────┘     │
└──────────────────────────────────────────┘
```

- HTML5 `<audio>` + `convertFileSrc()` 播放
- 封面提取自 ID3/Vorbis 标签，无封面显示音符图标
- 键盘：Space 播放/暂停，← → 快进/快退
- 详细元数据通过 `get_media_detail` 按需获取

### 12.5 文档预览（Phase 2）

- PDF: `<iframe>` 或 PDF.js 内联预览
- SVG: `<img>` 直接渲染
- 其他: 信息面板 + "用外部程序打开" 按钮

### 12.6 通用键盘快捷键

| 按键 | 功能 |
|------|------|
| `←` / `→` | 上一个 / 下一个 |
| `Escape` | 退出预览 |
| `Space` | 播放/暂停（视频/音频/动态照片） |
| `+` / `-` / 滚轮 | 缩放（图片） |
| `0` | 重置缩放 |
| `I` | 信息面板 |
| `F` | 收藏 |
| `Delete` | 软删除 |

### 12.7 状态管理 (`useMediaDetail.ts`)

```typescript
interface MediaDetailState {
  isOpen: boolean
  currentItemId: number | null
  currentIndex: number
  mediaType: 'image' | 'video' | 'audio' | 'document'
  // 图片
  scale: number
  translateX: number
  translateY: number
  isOriginalLoaded: boolean
  isLiveVideoPlaying: boolean
  // 音视频
  isPlaying: boolean
  currentTime: number
  // 通用
  isInfoPanelOpen: boolean
}
```

---

## 十三、主题系统

### 13.1 切换机制

```html
<html data-theme="dark">
```
三态：🌙 Dark → ☀️ Light → 💻 System → 循环

### 13.2 Dark 主题

```css
[data-theme="dark"] {
    --color-bg-primary: #0f0f17;
    --color-bg-secondary: #1a1a2e;
    --color-bg-surface: #222240;
    --color-bg-elevated: #2a2a4a;
    --color-bg-overlay: rgba(0,0,0,0.6);
    --color-text-primary: #e6e6f0;
    --color-text-secondary: #9090a8;
    --color-text-tertiary: #606078;
    --color-accent: #e94560;
    --color-accent-hover: #ff6b81;
    --color-accent-subtle: rgba(233,69,96,0.15);
    --color-border: rgba(255,255,255,0.08);
    --color-border-strong: rgba(255,255,255,0.15);
    --shadow-sm: 0 1px 3px rgba(0,0,0,0.4);
    --shadow-md: 0 4px 12px rgba(0,0,0,0.5);
    --shadow-lg: 0 8px 24px rgba(0,0,0,0.6);
    --color-scrollbar-thumb: rgba(255,255,255,0.15);
    --color-success: #34c759;
    --color-warning: #ff9500;
    --color-error: #ff3b30;
    --color-info: #5ac8fa;
}
```

### 13.3 Light 主题

```css
[data-theme="light"] {
    --color-bg-primary: #f5f5f7;
    --color-bg-secondary: #ffffff;
    --color-bg-surface: #ffffff;
    --color-bg-elevated: #f0f0f5;
    --color-bg-overlay: rgba(0,0,0,0.3);
    --color-text-primary: #1d1d1f;
    --color-text-secondary: #6e6e80;
    --color-text-tertiary: #aeaeb2;
    --color-accent: #d63050;
    --color-accent-hover: #c02040;
    --color-accent-subtle: rgba(214,48,80,0.10);
    --color-border: rgba(0,0,0,0.08);
    --color-border-strong: rgba(0,0,0,0.15);
    --shadow-sm: 0 1px 3px rgba(0,0,0,0.08);
    --shadow-md: 0 4px 12px rgba(0,0,0,0.1);
    --shadow-lg: 0 8px 24px rgba(0,0,0,0.12);
    --color-scrollbar-thumb: rgba(0,0,0,0.2);
    --color-success: #28a745;
    --color-warning: #e68a00;
    --color-error: #d63031;
    --color-info: #0984e3;
}
```

### 13.4 共享设计令牌

```css
:root {
    --spacing-xs: 4px; --spacing-sm: 8px; --spacing-md: 16px;
    --spacing-lg: 24px; --spacing-xl: 32px;
    --radius-sm: 4px; --radius-md: 8px; --radius-lg: 12px;
    --font-sans: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
    --font-mono: 'JetBrains Mono', monospace;
    --transition-fast: 150ms ease; --transition-normal: 300ms ease;
    --sidebar-width: 260px; --toolbar-height: 48px; --statusbar-height: 28px;
}
*, *::before, *::after {
    transition: background-color var(--transition-normal),
                color var(--transition-normal),
                border-color var(--transition-normal);
}
```

---

## 十四、前端交互设计规范

### 14.1 微动效

| 元素 | 效果 | CSS |
|------|------|-----|
| 卡片悬停 | 微放大 + 阴影 | `scale(1.03); box-shadow: var(--shadow-md)` |
| 收藏点击 | 弹簧缩放 | 1.0→1.3→0.9→1.0 (300ms) |
| 侧边栏折叠 | 平滑展开 | `max-height + cubic-bezier` |
| 缩略图加载 | 淡入 | `opacity 0→1 (300ms)` |
| LIVE 播放 | 角标高亮 | `scale(1.1); background glow` |

### 14.2 乐观置灰 UI

```css
.media-card--unfavorited {
    filter: grayscale(1); opacity: 0.4;
    pointer-events: none; transition: 300ms;
}
```

### 14.3 主色调辉光 (Phase 2)

```css
.media-card:hover::after {
    box-shadow: 0 0 20px var(--dominant-color); opacity: 0.4;
}
```

### 14.4 主题色提取（Phase 2，仅图片）

12 色相桶 (0°-360° / 30°) + 1 黑白。双模式筛选：色块矩阵 + 色谱滑动条。

---

## 十五、分阶段开发计划

### Phase 1：核心骨架 + 动态照片 + 大图预览 (4-5 周)

> [!IMPORTANT]
> 跑通「添加目录 → 扫描 → 缩略图 → 双击预览」闭环。含动态照片。数据库预留全部四大类型字段。

#### P1-1 项目初始化
- [ ] Tauri V2 + Vue 3 + TS + Vite 脚手架
- [ ] Cargo.toml Phase 1 依赖
- [ ] CSS 设计系统
- [ ] 目录结构 + `.editorconfig`

#### P1-2 数据库层
- [ ] AppState 读写双连接
- [ ] PRAGMA + 建表（`media_items` 含四大类型全部字段预留）
- [ ] models / queries / path.rs / format.rs

#### P1-3 文件扫描器
- [ ] walker.rs：遍历 + 格式分类（Phase 1 仅启用图片）
- [ ] metadata.rs：EXIF/XMP 解析（含宽高 NOT NULL）
- [ ] live_photo.rs：配对检测 + 嵌入检测
- [ ] 增量扫描 + 批量事务 + Channel 进度 + CancellationToken

#### P1-4 缩略图引擎
- [ ] ImageEngine trait + ImageRsEngine
- [ ] EngineArena 调度
- [ ] 统一 generator.rs（Phase 1 仅 image 分支）
- [ ] EXIF 快速路径 + Orientation 矫正
- [ ] ThumbHash + 两级缓存 + WebP/JPEG 降级

#### P1-5 动态照片
- [ ] Motion Photo 嵌入视频提取
- [ ] get_companion_video_url IPC
- [ ] MediaCard LIVE 角标 + 悬停播放
- [ ] MediaDetail 动态播放

#### P1-6 IPC 层
- [ ] scan / media / thumbnail / config / system 全部命令
- [ ] Channel + Event 定义

#### P1-7 前端 UI
- [ ] AppShell / Sidebar / Toolbar / StatusBar
- [ ] FolderTree + SmartAlbums
- [ ] **AppToolbar 媒体类型筛选芯片**（全部/图片/视频/音频/文档，P1 做 UI 骨架，视频/音频/文档 P2 启用数据）
- [ ] MediaGrid + MediaCard + DateSeparator
- [ ] MediaDetail + ImageViewer
- [ ] ThemeToggle

#### P1-8 前端逻辑
- [ ] useVirtualScroll / useJustifiedLayout / useRequestQueue
- [ ] useThumbnail / useMediaDetail / useFolderTree / useTheme
- [ ] **filterStore**（含 mediaTypes 筛选 + asMediaFilter getter + 芯片交互逻辑）
- [ ] router + Pinia stores (mediaStore, scanStore, uiStore)

#### P1 验收标准
- ✅ 1000+ 照片扫描 + Channel 实时进度
- ✅ Justified Layout + ThumbHash → Image.decode() → 缩略图
- ✅ 快速滚动 10000+ FPS ≥ 55
- ✅ 大图预览（缩放/拖拽/EXIF）
- ✅ 30MB+ PNG 不 OOM
- ✅ Light/Dark 主题
- ✅ 动态照片检测 + LIVE 角标 + 悬停播放
- ✅ 读写分离 + 相对路径验证

---

### Phase 2：四大类型全面支持 + 功能完善 (5-6 周)

> 目标：视频/音频/文档完整支持 + HEIC/RAW + 主题色 + 管理功能

#### P2-1 视频支持
- [ ] FFmpeg sidecar 配置
- [ ] video/ffmpeg.rs + frame_extractor.rs + metadata.rs
- [ ] generator.rs 视频分支
- [ ] 扫描器启用视频格式
- [ ] MediaCard 视频角标 (▶ + 时长)
- [ ] VideoPlayer.vue (HTML5 `<video>`)
- [ ] SmartAlbums 增加"视频"

#### P2-2 音频支持
- [ ] audio/metadata.rs + cover_art.rs (lofty)
- [ ] generator.rs 音频分支（封面提取/图标占位）
- [ ] 扫描器启用音频格式
- [ ] MediaCard 音频角标 (♪ + 时长)
- [ ] AudioPlayer.vue（封面 + 播放器 + 元数据面板）
- [ ] SmartAlbums 增加"音频"

#### P2-3 文档支持
- [ ] document/pdf_thumb.rs (mupdf) + svg_render.rs (resvg)
- [ ] generator.rs 文档分支
- [ ] 扫描器启用文档格式
- [ ] MediaCard 文档角标 (类型 + 页数)
- [ ] DocumentViewer.vue
- [ ] SmartAlbums 增加"文档"

#### P2-4 多引擎接入
- [ ] HeicEngine + RawEngine + EngineArena 降级链
- [ ] HEIC Live Photo 配对完整支持

#### P2-5 主题色功能
- [ ] MMCQ 提取 + 12 桶量化（仅图片）
- [ ] ColorFilter.vue + useColorFilter.ts

#### P2-6 管理功能
- [ ] 收藏 / 评分 / 软删除 / 恢复 / 回收站
- [ ] 乐观置灰 UI + 系统回收站 (trash crate)
- [ ] 多排序 + 缩略图大小滑块

#### P2 验收标准
- ✅ 视频正确生成缩略图 + 播放器可播放
- ✅ 音频封面提取 + 播放器可播放 + 元数据显示
- ✅ PDF 首页缩略图 + SVG 渲染
- ✅ HEIC + RAW 缩略图正确
- ✅ 颜色筛选可用
- ✅ 收藏/评分/删除/回收站正常

---

### Phase 3：高级交互 (2-3 周)

- [ ] notify 文件监听 → 自动同步
- [ ] ContextMenu.vue 右键菜单
- [ ] useSelection.ts 批量选择 + 批量操作
- [ ] 自定义相册 + 标签系统
- [ ] 多根目录管理

---

### Phase 4：AI 与高级特性 (未来)

- [ ] BLAKE3 去重
- [ ] CLIP 语义搜索
- [ ] 人脸识别
- [ ] GPS 地图视图
- [ ] 时间轴视图
- [ ] 移动端 (iOS/Android)
- [ ] 音频波形可视化
- [ ] 视频时间线缩略图条
- [ ] Office 文档丰富预览
- [ ] 云端挂载 (WebDAV/S3)

---

## 十六、工程实践

### 16.1 编码安全

所有文件 UTF-8。`.editorconfig` 强制。Rust `std::fs::write` 默认 UTF-8。

### 16.2 SQL 安全

所有动态查询**参数绑定**，禁止字符串拼接。

### 16.3 批量事务

```rust
let tx = conn.transaction()?;
for item in batch { tx.execute("INSERT INTO media_items ...", params![...])?; }
tx.commit()?;
// 500-1000 条/事务，性能提升百倍
```

### 16.4 路径规范

数据库统一正斜杠 `/`，运行时 `PathBuf::join()` 自动适配 OS。

### 16.5 错误处理

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("IO: {0}")] Io(#[from] std::io::Error),
    #[error("DB: {0}")] Db(#[from] rusqlite::Error),
    #[error("EXIF: {0}")] Exif(String),
    #[error("Unsupported: {0}")] UnsupportedFormat(String),
    #[error("Engine: {0}")] Engine(String),
    #[error("Path: {0}")] PathResolution(String),
    #[error("FFmpeg: {0}")] FFmpeg(String),
    #[error("Audio: {0}")] AudioMetadata(String),
    #[error("Document: {0}")] DocumentRender(String),
}
impl From<AppError> for String { fn from(e: AppError) -> String { e.to_string() } }
```

### 16.6 日志

| 级别 | 用途 | 示例 |
|------|------|------|
| ERROR | 不可恢复 | 数据库损坏 |
| WARN | 可恢复 | 引擎降级、FFmpeg 缺失 |
| INFO | 关键事件 | 扫描完成、动态照片配对 |
| DEBUG | 详细流程 | 缓存命中、引擎耗时 |
| TRACE | 极细粒度 | SQL 参数 |

### 16.7 Tauri V2 注意事项

- `capabilities/default.json` 声明权限
- `convertFileSrc()` 需要 `tauri-plugin-fs` scope
- FFmpeg sidecar 配置 `tauri.conf.json` → `externalBin`
- `cache_key` u64 仅 Rust 内部使用，不传给前端（JS 精度限制）
- `thumbhash` BLOB 前端接收为 `number[]` → `Uint8Array`

---

## 十七、性能基准目标

| 场景 | 目标 |
|------|------|
| 首次扫描 10,000 图片 | ≤ 30s (含 EXIF + 动态照片检测) |
| 首次扫描 150,000 项 | ≤ 8min |
| 缩略图 JPEG EXIF 快速路径 | ≤ 5ms/张 |
| 缩略图 JPEG 标准路径 | ≤ 50ms/张 |
| 缩略图 30MB+ PNG | ≤ 500ms/张 |
| 缩略图 视频帧 | ≤ 1s/个 |
| 缩略图 音频封面 | ≤ 100ms/个 |
| 缩略图 PDF 首页 | ≤ 500ms/个 |
| get_media_layout 15万项 | ≤ 150ms |
| Justified Layout 15万项 | ≤ 80ms |
| 虚拟滚动 FPS | ≥ 55 |
| 内存 15万项 | ≤ 400MB |
| 大图预览 30MB+ PNG | ≤ 3s |
| 动态照片播放延迟 | ≤ 500ms |

---

## 十八、风险与应对

| 风险 | 应对 |
|------|------|
| libheif 编译失败 | EngineArena 降级 |
| rawler 不支持某相机 | 日志 + 后续 libraw-rs |
| WAL 网络文件系统 | 检测提醒 + DB 仅本地 |
| 15万首扫慢 | Channel 进度 + 批量事务 |
| Windows 长路径 | `\\?\` + std::path |
| WebP 崩溃 | catch_unwind + JPEG 降级 |
| 大文件 OOM | rayon 并发限制 ≤ 2 |
| 盘符变更 | 相对路径架构 |
| FFmpeg 缺失 | 启动检测 + 图标降级 |
| FFmpeg 80MB 包体 | 可选下载策略 |
| mupdf 复杂 PDF 崩溃 | catch_unwind + 图标降级 |
| Motion Photo 解析失败 | 降级为普通照片 |
| WebView 不支持编码 | 提示外部播放器 |
| lofty 不支持冷门标签 | 降级为音符图标 + 日志 |
| 音频无封面 | 音符图标占位 |
