# Picasa Next — 极限性能跨平台高清图片浏览器
> **目标**：面向 15万+ 图片（含 30MB+ 大尺寸 PNG/照片）的本地高性能照片管理器，对标 Google Photos 本地版体验
> **定位**：个人开发者适用，兼顾工程化基础与开发效率
> **开发平台**：Windows 11
> **目标用户**：普通用户（JPEG/PNG 为主）

## 一、已确认的核心决策汇总

| 编号 | 决策项 | 最终方案 | 架构说明 |
|------|--------|----------|----------|
| Q1 | 哈希算法 | `xxHash3` (xxh3_64) | 字段名 `cache_key`，16字符 HEX，短字符串哈希提速 30 倍 |
| Q2 | 主题色分析 | Phase 2 实现 | 生成缩略图时顺带 MMCQ 提取，零额外磁盘 I/O |
| Q3 | 主题色 UI | 双模式并行 | 色相色块矩阵 + 色谱渐变双端滑动条 |
| Q4 | 缩略图格式 | WebP 优先 → JPEG 降级 | WebP 编码失败时自动降级 JPEG |
| Q5 | 极速扫描 | EXIF 缩略图提取 | Phase 1 即打通快速路径 |
| Q6 | 性能日志 | 采样写入 + 独立 DB | 开发全量/生产 1% 采样，`engine_stats.db` 独立 |
| Q7 | EXIF 库 | `kamadak-exif` | 纯 Rust、极速、安全 |
| Q8 | 目录树 | `parent_id` + `depth` + `name` | 完整递归树结构 |
| Q9 | HEIC 解码 | `libheif-rs`（Phase 2） | C 绑定，成熟稳定 |
| Q10 | RAW 解码 | `rawler`（Phase 2） | 纯 Rust，零 C 依赖 |
| Q11 | 数据库驱动 | 仅 `rusqlite` | 前后端统一，避免双驱动竞争 |
| Q12 | 主题系统 | CSS Variables + data-theme | Light/Dark/System 三态 |

---

## 二、技术栈总览

### 2.1 后端 (Rust / Tauri V2)

| 类别 | Crate | 版本 | 用途 | 引入阶段 |
|------|-------|------|------|----------|
| **框架** | `tauri` | `^2` | 应用框架 (WebView + Rust) | Phase 1 |
| **数据库** | `rusqlite` | `^0.31` (feature: `bundled`) | SQLite 统一驱动 | Phase 1 |
| **EXIF** | `kamadak-exif` | latest | EXIF 元数据解析 | Phase 1 |
| **哈希** | `xxhash-rust` | `^0.8` (feature: `xxh3`) | 极速缓存键生成 | Phase 1 |
| **图像基础** | `image` | `^0.25` | 标准格式编解码 (JPG/PNG/WebP/BMP/GIF/TIFF) | Phase 1 |
| **缩放** | `fast_image_resize` | latest | SIMD 加速图像缩放 (AVX2/SSE4.1/NEON) | Phase 1 |
| **占位图** | `thumbhash` | latest | 极小占位图生成 (~28 bytes/张) | Phase 1 |
| **异步** | `tokio` | `^1` (feature: `full`) | 异步 I/O + 任务调度 | Phase 1 |
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
| **文件监听** | `notify` | `^7` | 文件系统变更监听 | Phase 3 |
| **内容指纹** | `blake3` | `^1.5` | 文件去重哈希 | Phase 4 |

### 2.2 前端 (Vue 3 + TypeScript)

| 类别 | 库 | 用途 |
|------|-----|------|
| **框架** | Vue 3 (Composition API) | 响应式 UI |
| **状态管理** | Pinia | 全局状态 |
| **路由** | Vue Router 4 | 视图切换 |
| **构建** | Vite | 开发/构建工具 |
| **虚拟滚动** | 自研 (IntersectionObserver) | 百万级列表渲染 |
| **布局** | 自研 Justified Layout | 等高不等宽瀑布流 |
| **样式** | Vanilla CSS + CSS Variables | 主题系统 |
| **类型** | TypeScript strict mode | 全量类型覆盖 |
| **IPC** | `@tauri-apps/api` | Rust ↔ JS 通信 |

### 2.3 Tauri 插件（精简集）

| 插件 | 用途 | 说明 |
|------|------|------|
| `tauri-plugin-dialog` | 目录选择对话框 | 扫描目录注册 |
| `tauri-plugin-fs` | 文件系统权限 | `convertFileSrc` 依赖 |
| `tauri-plugin-shell` | 外部程序调用 | 在资源管理器中显示文件 |

> [!IMPORTANT]
> **不使用** `tauri-plugin-sql`。所有数据库操作统一由 Rust 后端 `rusqlite` 完成，前端通过 IPC Command 调用。

---

## 三、项目目录结构

```
pxphoto-tauri-v2/
├── src-tauri/                          # Rust 后端
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json               # Tauri V2 权限声明
│   ├── icons/                          # 应用图标资源
│   └── src/
│       ├── main.rs                     # Desktop 入口 + Tauri Builder
│       ├── lib.rs                      # 模块声明（mobile_entry_point）
│       ├── error.rs                    # 统一错误类型 (thiserror)
│       ├── state.rs                    # AppState 定义（数据库连接 + 全局状态）
│       │
│       ├── db/                         # 数据库层
│       │   ├── mod.rs
│       │   ├── connection.rs           # 连接初始化 + PRAGMA 配置
│       │   ├── schema.rs              # 建表 SQL + 迁移
│       │   ├── models.rs             # 数据模型 (struct)
│       │   └── queries.rs            # SQL 查询函数集合
│       │
│       ├── scanner/                    # 文件扫描引擎
│       │   ├── mod.rs
│       │   ├── walker.rs             # 目录递归遍历 + 增量检测
│       │   ├── metadata.rs           # EXIF 解析 + 元数据提取
│       │   └── watcher.rs            # 文件变更监听（Phase 3）
│       │
│       ├── thumbnail/                  # 缩略图系统
│       │   ├── mod.rs
│       │   ├── generator.rs          # 缩略图生成主流程
│       │   ├── exif_thumb.rs         # EXIF 嵌入缩略图提取（快速路径）
│       │   ├── cache.rs             # 两级哈希目录缓存管理
│       │   └── thumbhash.rs         # ThumbHash 占位图生成
│       │
│       ├── engine/                     # 图像解码引擎
│       │   ├── mod.rs                 # EngineArena 入口 + 调度
│       │   ├── traits.rs             # ImageEngine trait
│       │   ├── image_rs.rs           # 标准格式引擎 (image crate)
│       │   ├── heic.rs              # HEIC 引擎 (Phase 2)
│       │   └── raw.rs               # RAW 引擎 (Phase 2)
│       │
│       ├── color/                      # 主题色提取 (Phase 2)
│       │   ├── mod.rs
│       │   └── extractor.rs          # MMCQ 提取 + 12 桶量化
│       │
│       ├── ipc/                        # IPC 通信层
│       │   ├── mod.rs
│       │   ├── scan_commands.rs      # 扫描相关命令
│       │   ├── photo_commands.rs     # 照片查询/管理命令
│       │   ├── thumbnail_commands.rs # 缩略图请求命令
│       │   ├── system_commands.rs    # 系统集成命令
│       │   └── config_commands.rs    # 配置读写命令
│       │
│       └── utils/
│           ├── mod.rs
│           ├── hash.rs               # xxHash3 工具函数
│           └── format.rs            # 文件格式检测 + 支持格式常量
│
├── src/                               # Vue 前端
│   ├── App.vue                        # 根组件
│   ├── main.ts                        # 应用入口
│   ├── env.d.ts                       # Vite 环境类型声明
│   │
│   ├── assets/
│   │   └── styles/
│   │       ├── index.css              # 全局样式入口（@import 汇总）
│   │       ├── variables.css          # CSS 变量（间距、圆角、字体、布局）
│   │       ├── theme-dark.css         # Dark 主题色彩令牌
│   │       ├── theme-light.css        # Light 主题色彩令牌
│   │       ├── reset.css              # CSS Reset
│   │       └── animations.css         # 动画 Keyframes 定义
│   │
│   ├── components/
│   │   ├── layout/
│   │   │   ├── AppShell.vue           # 全局壳布局（侧边栏 + 主区域）
│   │   │   ├── AppSidebar.vue         # 侧边栏（目录树 + 智能相册 + 颜色筛选）
│   │   │   ├── AppToolbar.vue         # 顶部工具栏（视图切换、排序、搜索）
│   │   │   └── AppStatusBar.vue       # 底部状态栏（扫描进度、照片总数、缓存状态）
│   │   │
│   │   ├── photo/
│   │   │   ├── PhotoGrid.vue          # 照片网格主容器（虚拟滚动 + Justified Layout）
│   │   │   ├── PhotoCard.vue          # 单张照片卡片（ThumbHash 占位 → 缩略图过渡）
│   │   │   ├── PhotoDetail.vue        # 大图预览（Phase 1）
│   │   │   └── DateSeparator.vue      # 日期分隔条
│   │   │
│   │   ├── sidebar/
│   │   │   ├── FolderTree.vue         # 目录树组件（虚拟化 + 懒加载）
│   │   │   ├── SmartAlbums.vue        # 智能相册列表
│   │   │   └── ColorFilter.vue        # 颜色筛选器（Phase 2）
│   │   │
│   │   └── common/
│   │       ├── ProgressBar.vue        # 进度条
│   │       ├── ThemeToggle.vue        # 三态主题切换按钮
│   │       ├── ContextMenu.vue        # 右键菜单（Phase 3）
│   │       └── Toast.vue              # 全局通知
│   │
│   ├── composables/
│   │   ├── useVirtualScroll.ts        # 虚拟滚动（行级虚拟化）
│   │   ├── useJustifiedLayout.ts      # Justified Layout 计算
│   │   ├── useRequestQueue.ts         # IPC 请求优先级队列 + 快速取消
│   │   ├── useThumbnail.ts            # 缩略图加载（ThumbHash → 真实图切换）
│   │   ├── useFolderTree.ts           # 目录树展开/折叠状态管理
│   │   ├── useTheme.ts                # 主题切换逻辑（三态 + 系统跟随 + 持久化）
│   │   ├── useSelection.ts            # 批量选择逻辑（Shift/Ctrl 多选）
│   │   └── useColorFilter.ts          # 颜色筛选逻辑（Phase 2）
│   │
│   ├── stores/
│   │   ├── photoStore.ts              # 照片列表 Pinia Store
│   │   ├── scanStore.ts               # 扫描状态 Store
│   │   ├── uiStore.ts                 # UI 状态 Store（侧边栏、视图模式、主题）
│   │   └── filterStore.ts            # 筛选条件 Store（目录 + 颜色 + 日期）
│   │
│   ├── constants/
│   │   ├── formats.ts                 # 支持的图片格式列表
│   │   ├── defaults.ts                # 默认配置值
│   │   └── ipc.ts                     # IPC 命令名/事件名常量
│   │
│   ├── types/
│   │   ├── photo.ts                   # Photo / Directory 类型定义
│   │   ├── ipc.ts                     # IPC 请求/响应类型
│   │   └── ui.ts                      # UI 相关类型
│   │
│   ├── router/
│   │   └── index.ts                   # 路由配置
│   │
│   └── utils/
│       ├── thumbhash.ts               # ThumbHash 解码为 DataURL
│       └── format.ts                  # 日期/文件大小格式化
│
├── index.html
├── vite.config.ts
├── tsconfig.json
├── package.json
└── README.md
```

## 四、数据库设计

### 4.1 PRAGMA 配置

在 `connection.rs` 中数据库连接初始化时执行：

```sql
PRAGMA journal_mode = WAL;          -- 读写并发不阻塞
PRAGMA synchronous = NORMAL;        -- WAL 模式下足够安全
PRAGMA cache_size = -64000;         -- 64MB 页缓存
PRAGMA foreign_keys = ON;           -- 启用外键约束
PRAGMA busy_timeout = 5000;         -- 5 秒等待避免 SQLITE_BUSY
PRAGMA temp_store = MEMORY;         -- 临时表存内存
PRAGMA mmap_size = 268435456;       -- 256MB 内存映射加速
```

> [!CAUTION]
> 这些 PRAGMA 必须在 Rust 后端建立连接时动态执行，**不可写在静态迁移 SQL 文件中**！因为 PRAGMA 是连接级设置，迁移脚本只运行一次。

### 4.2 主数据库 (`pxphoto.db`)

#### 表：`app_config` — 全局应用配置

```sql
CREATE TABLE IF NOT EXISTS app_config (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

-- 预填默认配置
INSERT OR IGNORE INTO app_config VALUES ('schema_version', '1');
INSERT OR IGNORE INTO app_config VALUES ('thumb_size', '300');
INSERT OR IGNORE INTO app_config VALUES ('thumb_format', 'webp');
INSERT OR IGNORE INTO app_config VALUES ('thumb_quality', '80');
INSERT OR IGNORE INTO app_config VALUES ('theme', 'system');
```

#### 表：`scan_roots` — 扫描根目录注册表

```sql
CREATE TABLE IF NOT EXISTS scan_roots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    path            TEXT    NOT NULL UNIQUE,          -- 根目录绝对路径
    alias           TEXT,                             -- 用户自定义别名（如"我的相机"）
    scan_status     TEXT    DEFAULT 'idle',           -- 'idle'/'scanning'/'completed'/'error'
    scan_progress   INTEGER DEFAULT 0,               -- 已处理文件数
    total_files     INTEGER DEFAULT 0,               -- 文件总数
    last_scan_at    INTEGER,                         -- 上次完成扫描 Unix 时间戳
    is_active       INTEGER DEFAULT 1,               -- 是否启用
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
```

#### 表：`directories` — 目录索引（递归树结构）

```sql
CREATE TABLE IF NOT EXISTS directories (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id         INTEGER NOT NULL REFERENCES scan_roots(id) ON DELETE CASCADE,
    parent_id       INTEGER REFERENCES directories(id) ON DELETE CASCADE,
    path            TEXT    NOT NULL UNIQUE,          -- 目录绝对路径
    name            TEXT    NOT NULL,                 -- 目录名（basename，前端直用）
    depth           INTEGER NOT NULL DEFAULT 0,       -- 相对 root 的深度（便于树渲染缩进）
    photo_count     INTEGER NOT NULL DEFAULT 0,       -- 直接子照片数
    mtime           INTEGER,                         -- 目录修改时间（增量扫描用）
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX idx_directories_root   ON directories(root_id);
CREATE INDEX idx_directories_parent ON directories(parent_id);
CREATE INDEX idx_directories_path   ON directories(path);
```

#### 表：`photos` — 照片主表（核心）

```sql
CREATE TABLE IF NOT EXISTS photos (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    directory_id    INTEGER NOT NULL REFERENCES directories(id) ON DELETE CASCADE,

    -- 文件基本信息
    file_name       TEXT    NOT NULL,                 -- 文件名（basename）
    file_path       TEXT    NOT NULL UNIQUE,          -- 文件绝对路径
    file_size       INTEGER NOT NULL,                 -- 文件大小（字节）
    file_mtime      INTEGER NOT NULL,                 -- 文件修改时间 Unix 时间戳
    file_format     TEXT    NOT NULL,                 -- 小写扩展名 ('jpg','png','heic','cr3'...)

    -- 图像尺寸（非 EXIF 来源——解码时获取或 EXIF 获取，优先取解码实际值）
    width           INTEGER,                         -- 像素宽度
    height          INTEGER,                         -- 像素高度
    orientation     INTEGER DEFAULT 1,               -- EXIF Orientation (1-8)

    -- EXIF 元数据（可 NULL）
    exif_datetime   INTEGER,                         -- EXIF 拍摄时间 Unix 时间戳
    exif_make       TEXT,                            -- 相机品牌
    exif_model      TEXT,                            -- 相机型号
    exif_lens       TEXT,                            -- 镜头型号
    exif_focal_length REAL,                          -- 焦距 (mm)
    exif_aperture   REAL,                            -- 光圈 (f/x.x)
    exif_shutter    TEXT,                            -- 快门速度 ("1/250")
    exif_iso        INTEGER,                         -- ISO 感光度
    exif_gps_lat    REAL,                            -- GPS 纬度
    exif_gps_lng    REAL,                            -- GPS 经度

    -- 排序与缓存
    sort_datetime   INTEGER NOT NULL,                -- = COALESCE(exif_datetime, file_mtime)，入库时预计算
    cache_key       TEXT    NOT NULL,                 -- xxh3_64("{file_path}|{file_mtime}|{thumb_size}") hex

    -- 缩略图状态
    thumb_status    INTEGER NOT NULL DEFAULT 0,      -- 0=待生成, 1=已生成, 2=失败
    thumb_path      TEXT,                            -- 缩略图相对路径 ("a3/a3f4b2c1d0e9.webp")
    thumbhash       BLOB,                            -- ThumbHash 二进制数据 (~28 bytes)

    -- 主题色（Phase 2 填充，Phase 1 建表留空）
    dominant_hue    INTEGER,                         -- 色相桶 (0-11, NULL=无彩色)
    dominant_sat    INTEGER,                         -- 饱和度 (0-100)
    dominant_lum    INTEGER,                         -- 明度 (0-100)
    dominant_hex    TEXT,                            -- 主色调 hex ("FF6B35")
    is_monochrome   INTEGER DEFAULT 0,               -- 1=黑白/灰度

    -- 后台闲时指纹（Phase 4 填充）
    content_hash    TEXT,                            -- BLAKE3 文件哈希（去重用）

    -- 管理字段
    is_favorited    INTEGER NOT NULL DEFAULT 0,      -- 1=已收藏
    is_deleted      INTEGER NOT NULL DEFAULT 0,      -- 1=软删除
    deleted_at      INTEGER,                         -- 软删除时间
    rating          INTEGER DEFAULT 0,               -- 评分 0-5

    -- 时间戳
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- ============================================================
-- 百万级高频查询索引
-- ============================================================
CREATE INDEX idx_photos_directory     ON photos(directory_id);
CREATE INDEX idx_photos_sort          ON photos(sort_datetime DESC) WHERE is_deleted = 0;
CREATE INDEX idx_photos_cache_key     ON photos(cache_key);
CREATE INDEX idx_photos_format        ON photos(file_format);
CREATE INDEX idx_photos_thumb_status  ON photos(thumb_status) WHERE thumb_status != 1;
CREATE INDEX idx_photos_favorited     ON photos(is_favorited) WHERE is_favorited = 1 AND is_deleted = 0;
CREATE INDEX idx_photos_deleted       ON photos(is_deleted) WHERE is_deleted = 1;
CREATE INDEX idx_photos_hue           ON photos(dominant_hue, is_monochrome) WHERE is_deleted = 0 AND dominant_hue IS NOT NULL;
CREATE INDEX idx_photos_rating        ON photos(rating) WHERE is_deleted = 0 AND rating > 0;
CREATE INDEX idx_photos_content_hash  ON photos(content_hash) WHERE content_hash IS NOT NULL;
```

#### 表：`albums` / `album_photos` — 相册系统（Phase 3）

```sql
CREATE TABLE IF NOT EXISTS albums (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL,
    description     TEXT,
    cover_photo_id  INTEGER REFERENCES photos(id) ON DELETE SET NULL,
    sort_order      INTEGER DEFAULT 0,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS album_photos (
    album_id        INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    photo_id        INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    sort_order      INTEGER DEFAULT 0,
    added_at        INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (album_id, photo_id)
);
CREATE INDEX idx_album_photos_album ON album_photos(album_id);
```

#### 表：`tags` / `photo_tags` — 标签系统（Phase 3）

```sql
CREATE TABLE IF NOT EXISTS tags (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL UNIQUE,
    color           TEXT,                            -- 标签视觉 Hex
    parent_id       INTEGER REFERENCES tags(id) ON DELETE CASCADE,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS photo_tags (
    photo_id        INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    tag_id          INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (photo_id, tag_id)
);
CREATE INDEX idx_photo_tags_tag ON photo_tags(tag_id);
```

### 4.3 独立数据库 (`engine_stats.db`)

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

CREATE TABLE IF NOT EXISTS engine_stats (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    file_format     TEXT    NOT NULL,
    engine_name     TEXT    NOT NULL,
    decode_ms       REAL,
    resize_ms       REAL,
    encode_ms       REAL,
    total_ms        REAL,
    input_size      INTEGER,
    output_size     INTEGER,
    success         INTEGER NOT NULL DEFAULT 1,
    error_msg       TEXT,
    created_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX idx_stats_format ON engine_stats(file_format);
CREATE INDEX idx_stats_engine ON engine_stats(engine_name);
```

**写入策略**：
- 开发模式 (`cfg!(debug_assertions)`)：每次操作全量写入
- 生产模式：每 100 次操作采样写入 1 次

### 4.4 数据库架构设计要点

| 设计决策 | 原理 |
|---------|------|
| **`xxh3_64` 缓存键** | 替代 MD5，单次计算从 200ns 降至 8ns，百万级零开销 |
| **`sort_datetime` 预计算** | 入库时 `COALESCE(exif_datetime, file_mtime)` 存入独立字段+降序索引，百万级排序 ≤1ms |
| **`WHERE is_deleted = 0` 局部索引** | 大部分查询排除已删除记录，局部索引减少索引体积、加速查询 |
| **`thumb_status` 三态** | 区分"待生成/已生成/失败"，可精确重试失败项 |
| **`thumb_path` 相对路径** | 缓存目录可迁移，不硬编码绝对路径 |
| **`thumbhash` 存 BLOB** | 比 Base64 TEXT 节省 33% 空间，~28 bytes 原始二进制 |
| **`depth` + `name` 字段** | 前端目录树渲染直接使用，无需运行时解析路径 |
| **两级哈希缓存目录** | `cache/{key[0..2]}/{key}.webp`，256 子目录均匀分散，规避单目录百万文件性能退化 |
| **独立 engine_stats.db** | 高频写入不冲击主库 WAL，降低锁竞争 |

---

## 五、IPC 架构设计

### 5.1 Tauri Command（请求-响应模式）

前端通过 `invoke()` 调用，等待 Rust 返回：

#### 扫描管理 (`scan_commands.rs`)

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `add_scan_root` | `{ path: string }` | `ScanRoot` | 添加扫描根目录 |
| `remove_scan_root` | `{ id: number }` | `void` | 移除根目录及关联数据 |
| `list_scan_roots` | `void` | `ScanRoot[]` | 获取所有根目录 |
| `start_scan` | `{ root_id: number, on_progress: Channel }` | `void` | 触发异步扫描（进度通过 Channel 流式推送） |
| `stop_scan` | `{ root_id: number }` | `void` | 取消扫描 (CancellationToken) |

#### 照片查询与管理 (`photo_commands.rs`)

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `get_photos` | `{ directory_id?, offset, limit, sort_by, order, filters? }` | `PhotoPage` | 分页查询照片（支持组合筛选） |
| `get_photo_detail` | `{ id: number }` | `PhotoDetail` | 获取完整信息 + EXIF |
| `toggle_favorite` | `{ photo_id: number }` | `void` | 切换收藏状态 |
| `set_rating` | `{ photo_id: number, rating: number }` | `void` | 设置评分 |
| `soft_delete_photos` | `{ photo_ids: number[] }` | `void` | 软删除 |
| `restore_photos` | `{ photo_ids: number[] }` | `void` | 从回收站恢复 |
| `get_trash` | `{ offset, limit }` | `PhotoPage` | 查询回收站 |
| `get_photos_by_color` | `{ hue_buckets: number[] }` | `PhotoPage` | Phase 2：按颜色桶筛选 |
| `get_stats` | `void` | `AppStats` | 获取统计数据 |

#### 目录树 (`photo_commands.rs`)

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `get_directory_tree` | `{ root_id: number }` | `DirNode[]` | 获取目录树（扁平数组+parent_id） |
| `get_directory_children` | `{ parent_id: number }` | `DirNode[]` | 懒加载子目录 |

#### 缩略图 (`thumbnail_commands.rs`)

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `request_thumbnail` | `{ photo_id: number }` | `ThumbResult` | 请求单张缩略图 |
| `batch_request_thumbnails` | `{ photo_ids: number[] }` | `ThumbResult[]` | 批量请求 |

#### 系统集成 (`system_commands.rs`)

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `show_in_explorer` | `{ path: string }` | `void` | 在资源管理器中显示文件 |
| `move_to_trash` | `{ photo_ids: number[] }` | `void` | 移至系统回收站（Phase 2） |

#### 配置 (`config_commands.rs`)

| 命令名 | 参数 | 返回值 | 说明 |
|--------|------|--------|------|
| `get_app_config` | `{ key: string }` | `string \| null` | 读取配置 |
| `set_app_config` | `{ key: string, value: string }` | `void` | 写入配置 |

### 5.2 Tauri Channel（高性能流式推送）

扫描和缩略图生成的进度通过 **Tauri Channel** 流式推送到前端，避免 Event 的广播开销和序列化瓶颈：

```rust
// Rust 端：扫描命令接收 Channel 参数
#[tauri::command]
async fn start_scan(
    root_id: i64,
    on_progress: tauri::ipc::Channel<ScanProgressPayload>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // 扫描循环中直接推送进度
    on_progress.send(ScanProgressPayload {
        root_id,
        scanned: current_count,
        total: total_count,
        current_dir: dir_name.to_string(),
    }).map_err(|e| e.to_string())?;
    Ok(())
}
```

```typescript
// 前端：调用时传入回调
import { invoke, Channel } from '@tauri-apps/api/core'

const onProgress = new Channel<ScanProgress>()
onProgress.onmessage = (progress) => {
  scanStore.updateProgress(progress)
}
await invoke('start_scan', { rootId, onProgress })
```

#### Channel 消息类型

| 消息类型 | Payload | 推送时机 | 说明 |
|----------|---------|----------|------|
| `ScanProgressPayload` | `{ root_id, scanned, total, current_dir }` | 每 500 文件或每秒 | 扫描进度（通过 start_scan Channel） |
| `ScanCompletedPayload` | `{ root_id, total_photos, elapsed_ms }` | 扫描完成时 | 同一 Channel 的终止消息 |
| `ScanErrorPayload` | `{ root_id, error }` | 出错时 | 同一 Channel 的错误消息 |
| `ThumbBatchPayload` | `{ items: [{photo_id, thumb_path, thumbhash}] }` | 每批次完成时 | 缩略图就绪（通过独立 Channel） |

#### 补充 Event（低频通知）

以下低频事件仍使用 Tauri Event（不需要 Channel 的高吞吐量）：

| 事件名 | Payload | 说明 |
|--------|---------|------|
| `db:photos_updated` | `{ action, photo_ids }` | 数据变更通知前端刷新 |

> [!TIP]
> **Channel vs Event 的选择原则**：高频流式数据（扫描进度、缩略图批次）用 Channel；低频通知（数据库变更）用 Event。Channel 避免了 Event 的全局广播序列化开销，在 15万+ 照片扫描场景下性能显著更优。

### 5.3 缩略图获取协议

使用 Tauri 内置 `convertFileSrc()` 将磁盘路径转为 `asset://` 协议 URL：

```typescript
import { convertFileSrc } from '@tauri-apps/api/core'

// 前端获取缩略图 URL
const thumbUrl = convertFileSrc(absoluteThumbPath)
// → "asset://localhost/C:/Users/.../cache/a3/a3f4b2.webp"
```

```html
<img :src="thumbUrl" loading="lazy" />
```

> [!WARNING]
> **严禁**通过自定义 HTTP 服务器或 Base64 over IPC 传输缩略图！`convertFileSrc` 让 WebView 直读磁盘，性能提升 5 倍以上。

---

## 六、图像引擎架构 (Engine Arena)

### 6.1 Engine Trait 定义

```rust
/// 所有图像引擎必须实现此 trait
pub trait ImageEngine: Send + Sync {
    /// 引擎名称（日志和统计用）
    fn name(&self) -> &str;

    /// 支持的文件格式列表
    fn supported_formats(&self) -> &[&str];

    /// 检查能否处理指定格式
    fn can_handle(&self, format: &str) -> bool {
        self.supported_formats().contains(&format)
    }

    /// 解码图片为 RGBA 像素数据
    fn decode(&self, file_path: &Path) -> Result<DecodedImage, EngineError>;

    /// 尝试提取 EXIF 嵌入缩略图（可选）
    fn extract_embedded_thumb(&self, file_path: &Path) -> Result<Option<Vec<u8>>, EngineError> {
        Ok(None)
    }
}
```

### 6.2 引擎实例与调度

| 引擎 | 实现 | 支持格式 | 引入阶段 |
|------|------|----------|----------|
| `ImageRsEngine` | `image` crate | jpg, jpeg, png, webp, bmp, gif, tiff, ico | Phase 1 |
| `HeicEngine` | `libheif-rs` | heic, heif, avif | Phase 2 |
| `RawEngine` | `rawler` | cr2, cr3, nef, arw, dng, raf, orf, rw2, pef, srw | Phase 2 |

**调度逻辑**：按文件格式匹配第一个可用引擎 → 失败自动尝试下一个 → 全部失败返回 `UnsupportedFormat`。

> [!TIP]
> **V1.0 简化策略**：砍掉 WIC/wgpu/SIMD 独立引擎。`fast_image_resize` 自带 SIMD 加速，在 Phase 1 就用于所有引擎的缩放步骤。GPU 加速推迟到 Phase 4+（如有性能瓶颈再考虑）。

### 6.3 EXIF Orientation 矫正（关键陷阱）

> [!WARNING]
> 图片进入任何引擎缩放之前，**必须**前置解析 EXIF Orientation 标签。若值为 3/6/8 等旋转态，**必须先在内存中将像素矩阵旋转至正向**，再执行 `fast_image_resize` 缩放。否则缩略图和 ThumbHash 都会出现横躺或颠倒。

---

## 七、缩略图生成流水线

### 7.1 生成流程

```
┌──────────────────────────────────────────────────────────────────────┐
│                       缩略图生成流水线                                │
│                                                                      │
│  输入: Photo { file_path, file_mtime, cache_key }                   │
│                                                                      │
│  Step 1: 缓存命中检测                                                │
│  ├── 查找: cache/{cache_key[0..2]}/{cache_key}.webp                 │
│  └── 命中 → 直接返回路径 ✅                                          │
│                                                                      │
│  Step 2: EXIF 嵌入缩略图提取（快速路径 ⚡）                          │
│  ├── kamadak-exif 读取文件头（仅前 64KB）                            │
│  ├── 提取 EXIF thumbnail → 检查尺寸 ≥ 目标                         │
│  └── 满足 → 解码 + Orientation 矫正 + 缩放 → 完成 ✅                │
│                                                                      │
│  Step 3: Engine Arena 完整解码（标准路径）                           │
│  ├── 根据 file_format 选择引擎                                       │
│  ├── 解码为 RGBA 像素                                                │
│  ├── EXIF Orientation 像素矩阵旋转矫正                               │
│  ├── fast_image_resize 缩放至 300px 长边                             │
│  ├── WebP 编码 (quality=80)                                          │
│  │   └── 失败时降级 JPEG (quality=85)                                │
│  └── 写入缓存文件 ✅                                                 │
│                                                                      │
│  Step 4: ThumbHash 生成                                              │
│  ├── 将缩放后像素再缩小到 100x100 以内                               │
│  ├── thumbhash::rgba_to_thumb_hash() → ~28 bytes                    │
│  └── 写入 photos.thumbhash 字段 (BLOB) ✅                            │
│                                                                      │
│  Step 5: 主题色提取（Phase 2 追加）                                  │
│  ├── color_thief::get_palette(pixels, quality=10, max_colors=3)      │
│  ├── Top1 → RGB→HSL → 量化到 12 桶                                  │
│  └── 写入 dominant_hue/sat/lum/hex ✅                                │
│                                                                      │
│  Step 6: 更新数据库                                                  │
│  ├── thumb_status = 1, thumb_path = "a3/a3f4b2.webp"                │
│  └── thumbhash = <blob>                                              │
└──────────────────────────────────────────────────────────────────────┘
```

### 7.2 缓存目录结构（两级哈希）

```
{app_data_dir}/cache/thumbnails/
├── a3/
│   ├── a3f4b2c1d0e9f7a1.webp
│   └── a38812340abc9def.webp
├── b7/
│   ├── b7001234abcdef01.webp
│   └── ...
└── ff/
    └── ...
```

- 取 `cache_key` 前 2 个 hex 字符作为子目录名
- 256 个子目录，百万级文件每目录约 4000 个文件
- 规避 NTFS/APFS 单目录过多文件的性能退化

---

## 八、前端渲染架构

### 8.1 Justified Layout（完美对齐瀑布流）

```
输入: photos[], containerWidth, targetRowHeight(200px), gap(4px)
输出: rows[{ photos: [...], computedHeight }]

算法：
1. 遍历照片，计算纵横比 aspect = width / height
2. 逐张放入当前行
3. 行总宽度 = Σ(aspect_i × targetRowHeight) + gap × (n-1)
4. 行总宽度 ≥ containerWidth 时：
   → computedHeight = targetRowHeight × (containerWidth / 行总宽度)
   → 每张渲染宽 = aspect_i × computedHeight
   → 开启下一行
5. 最后一行不强行拉伸，保持 targetRowHeight 左对齐
6. 每张图使用 absolute + transform: translate3d(x,y,0) 硬件加速定位
```

### 8.2 虚拟滚动（行级虚拟化）

```
┌────────────────────────────────────────────────────┐
│ Total Content Height (padding-top/bottom 占位)      │
│  ┌──────────── Rendered Zone ──────────────┐       │
│  │  overscan 上方 3 行（预加载）            │       │
│  ├──────────────────────────────────────────┤       │
│  │  可视区域 (Viewport)                     │       │
│  │  Row 1: [img] [img] [img] [img]          │       │
│  │  Row 2: [img] [img] [img]                │       │
│  │  Row 3: [img] [img] [img] [img] [img]    │       │
│  ├──────────────────────────────────────────┤       │
│  │  overscan 下方 3 行（预加载）            │       │
│  └──────────────────────────────────────────┘       │
└────────────────────────────────────────────────────┘
```

- 仅渲染可视区域 + 上下各 3 行缓冲
- 非可视区域用 `padding-top/padding-bottom` 占位
- Justified Layout 预计算所有行高 → 精确总高度 → 滚动条不抖

### 8.3 双重加载策略：ThumbHash → 缩略图

```
┌────────────────────────── PhotoCard 渲染流程 ─────────────────────┐
│                                                                    │
│  阶段 1: 即时占位（0ms）                                          │
│  ├── 从 photos 表获取 thumbhash (28 bytes BLOB)                   │
│  ├── 前端解码为 ~32x32 DataURL                                    │
│  ├── 渲染为 <img> + CSS filter: blur(20px)                        │
│  └── 用户看到模糊但有色彩的占位图                                  │
│                                                                    │
│  阶段 2: 真实缩略图（IntersectionObserver 触发）                   │
│  ├── 进入视口 → 加入请求队列 (useRequestQueue)                    │
│  ├── 优先级调度：可见区域 > 预加载区域                             │
│  ├── IPC request_thumbnail → 获得磁盘路径 → convertFileSrc        │
│  ├── new Image() 预加载 → onload 后替换 src                       │
│  └── CSS transition: opacity 0→1 (300ms) 平滑过渡                 │
│                                                                    │
│  取消优化：                                                        │
│  ├── 快速滚动时 → 从队列移除未发送的请求                           │
│  └── 避免浪费 IPC 带宽和 CPU 资源                                  │
└────────────────────────────────────────────────────────────────────┘
```

### 8.4 请求队列 (`useRequestQueue.ts`)

```typescript
interface QueueItem {
  photoId: number
  priority: 'high' | 'normal' | 'low'  // high=视口内, normal=overscan, low=预取
}

// 核心设计：
// - 最大并发数: 6
// - high 优先级优先发送
// - 快速滚动时批量取消 pending 请求
// - 离开视口的请求立即从队列移除
```

### 8.5 日期分隔条

```
┌──────────────────────────────────────────┐
│  📅 2024年6月15日 · 周六 · 36张           │  ← DateSeparator
├──────────────────────────────────────────┤
│  🖼 🖼 🖼 🖼 🖼 🖼 🖼 🖼                   │  ← Photo Row
├──────────────────────────────────────────┤
│  📅 2024年6月14日 · 周五 · 22张           │
├──────────────────────────────────────────┤
│  🖼 🖼 🖼 🖼 🖼 🖼                         │
└──────────────────────────────────────────┘
```

在照片列表预处理阶段，按 `sort_datetime` 分组，每组开头插入 `{ type: 'date-separator', date, count }` 虚拟元素，与照片行统一参与虚拟滚动。

### 8.6 目录树虚拟化

> [!WARNING]
> Vue 对大规模嵌套对象的 Proxy 响应式代理 + 海量 DOM 节点会导致浏览器僵死。**设计天条**：侧边栏目录树必须引入虚拟滚动 (`useVirtualTree`)，仅渲染视口内的节点。

- 使用扁平数组 + `parent_id` 表示树结构（非递归嵌套对象）
- 展开/折叠状态在 `useFolderTree.ts` 中管理
- 只有展开的节点才参与虚拟列表渲染
- 缩进量由 `depth` 字段决定

---

## 九、主题系统 (Light / Dark)

### 9.1 切换机制

```html
<html data-theme="dark"> <!-- 或 "light" -->
```

三态循环切换：🌙 Dark → ☀️ Light → 💻 System → 🌙 Dark → ...

### 9.2 主题偏好持久化

```typescript
// useTheme.ts 核心逻辑
type ThemeMode = 'light' | 'dark' | 'system'

// 读取：await invoke('get_app_config', { key: 'theme' })
// 写入：await invoke('set_app_config', { key: 'theme', value: mode })
// 系统跟随：window.matchMedia('(prefers-color-scheme: dark)') 监听
// 应用：document.documentElement.setAttribute('data-theme', resolved)
```

### 9.3 Dark 主题色彩令牌 (默认)

```css
[data-theme="dark"] {
    --color-bg-primary: #0f0f17;
    --color-bg-secondary: #1a1a2e;
    --color-bg-surface: #222240;
    --color-bg-elevated: #2a2a4a;
    --color-bg-overlay: rgba(0, 0, 0, 0.6);

    --color-text-primary: #e6e6f0;
    --color-text-secondary: #9090a8;
    --color-text-tertiary: #606078;

    --color-accent: #e94560;
    --color-accent-hover: #ff6b81;
    --color-accent-subtle: rgba(233, 69, 96, 0.15);

    --color-border: rgba(255, 255, 255, 0.08);
    --color-border-strong: rgba(255, 255, 255, 0.15);

    --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.4);
    --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.5);
    --shadow-lg: 0 8px 24px rgba(0, 0, 0, 0.6);

    --color-scrollbar-thumb: rgba(255, 255, 255, 0.15);
    --color-success: #34c759;
    --color-warning: #ff9500;
    --color-error: #ff3b30;
    --color-info: #5ac8fa;
}
```

### 9.4 Light 主题色彩令牌

```css
[data-theme="light"] {
    --color-bg-primary: #f5f5f7;
    --color-bg-secondary: #ffffff;
    --color-bg-surface: #ffffff;
    --color-bg-elevated: #f0f0f5;
    --color-bg-overlay: rgba(0, 0, 0, 0.3);

    --color-text-primary: #1d1d1f;
    --color-text-secondary: #6e6e80;
    --color-text-tertiary: #aeaeb2;

    --color-accent: #d63050;
    --color-accent-hover: #c02040;
    --color-accent-subtle: rgba(214, 48, 80, 0.10);

    --color-border: rgba(0, 0, 0, 0.08);
    --color-border-strong: rgba(0, 0, 0, 0.15);

    --shadow-sm: 0 1px 3px rgba(0, 0, 0, 0.08);
    --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.1);
    --shadow-lg: 0 8px 24px rgba(0, 0, 0, 0.12);

    --color-scrollbar-thumb: rgba(0, 0, 0, 0.2);
    --color-success: #28a745;
    --color-warning: #e68a00;
    --color-error: #d63031;
    --color-info: #0984e3;
}
```

### 9.5 共享设计令牌 (`variables.css`)

```css
:root {
    --spacing-xs: 4px;
    --spacing-sm: 8px;
    --spacing-md: 16px;
    --spacing-lg: 24px;
    --spacing-xl: 32px;

    --radius-sm: 4px;
    --radius-md: 8px;
    --radius-lg: 12px;

    --font-sans: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
    --font-mono: 'JetBrains Mono', monospace;

    --transition-fast: 150ms ease;
    --transition-normal: 300ms ease;

    --sidebar-width: 260px;
    --toolbar-height: 48px;
    --statusbar-height: 28px;
}

/* 主题切换时颜色平滑过渡 */
*, *::before, *::after {
    transition: background-color var(--transition-normal),
                color var(--transition-normal),
                border-color var(--transition-normal);
}
```

---

## 十、前端交互设计规范

### 10.1 微动效规范

| 元素 | 效果 | CSS |
|------|------|-----|
| 照片卡片悬停 | 微放大 + 阴影加深 | `transform: scale(1.03); box-shadow: var(--shadow-md)` |
| 收藏心标点击 | 弹簧缩放 | Keyframe: 1.0→1.3→0.9→1.0 (300ms) |
| 侧边栏折叠 | 平滑展开 | `max-height + cubic-bezier(0.4, 0, 0.2, 1)` |
| 主题切换 | 颜色渐变 | CSS Variables transition 300ms |
| 缩略图加载 | 淡入 | `opacity: 0→1 (300ms)` |

### 10.2 乐观置灰 UI (Optimistic UI)

在"我的收藏"视图下取消收藏时：

```css
.photo-card--unfavorited {
    filter: grayscale(1);
    opacity: 0.4;
    pointer-events: none;
    transition: filter 300ms, opacity 300ms;
}
```

- **不**立即从列表移除
- 视觉置灰 + 禁止交互
- 3 秒无操作或离开视图后，静默重排
- 给用户"反悔重选"空间

### 10.3 侧边栏弹性布局

```
┌──────────── Sidebar ────────────┐
│  智能相册 (flex: 0 0 auto)       │ ← 按需撑高，不挤压
│  ┌──────────────────────────┐   │
│  │ 全部照片  最近导入  收藏  │   │
│  └──────────────────────────┘   │
│  颜色筛选 (flex: 0 0 auto)       │ ← Phase 2
│  ┌──────────────────────────┐   │
│  │ 12色块 + 黑白色块         │   │
│  └──────────────────────────┘   │
├─────────────────────────────────┤
│  目录树 (flex: 1 1 0%)          │ ← 吞噬剩余空间
│  ┌──────────────────────────┐   │
│  │ overflow-y: auto          │   │
│  │ 树内部独立滚动             │   │
│  └──────────────────────────┘   │
└─────────────────────────────────┘
```

### 10.4 右键上下文菜单

- 自研绝对定位 `ContextMenu.vue`，弃用浏览器原生右键
- 边界溢出检测：`clientX + menuWidth > window.innerWidth` 时左偏移
- 通过 Rust `Command` 实现系统集成：
  - Windows: `explorer.exe /select, <path>`
  - macOS: `open -R <path>`

### 10.5 主色调辉光 (Phase 2)

```css
/* PhotoCard 悬停时的主色调弥散辉光 */
.photo-card:hover::after {
    content: '';
    position: absolute;
    inset: -4px;
    border-radius: var(--radius-md);
    box-shadow: 0 0 20px var(--dominant-color);
    opacity: 0.4;
    transition: opacity var(--transition-normal);
}
```

---

## 十一、主题色提取与颜色筛选 (Phase 2)

### 11.1 12 色相桶映射

| 桶 ID | 色相范围 | 代表色名 | 代表 Hex |
|-------|---------|---------|---------|
| 0 | 0°-29° | 红色 | #FF3B30 |
| 1 | 30°-59° | 橙色 | #FF9500 |
| 2 | 60°-89° | 黄色 | #FFCC00 |
| 3 | 90°-119° | 黄绿 | #8CC63F |
| 4 | 120°-149° | 绿色 | #34C759 |
| 5 | 150°-179° | 青绿 | #30B0C7 |
| 6 | 180°-209° | 青色 | #5AC8FA |
| 7 | 210°-239° | 蓝色 | #007AFF |
| 8 | 240°-269° | 靛蓝 | #5856D6 |
| 9 | 270°-299° | 紫色 | #AF52DE |
| 10 | 300°-329° | 品红 | #FF2D55 |
| 11 | 330°-359° | 玫红 | #FF375F |
| — | 无彩色 | 黑白 | #808080 |

### 11.2 颜色筛选 UI（双模式并行）

**模式 A：色相色块矩阵**
- 12 色相圆圈 + 1 灰度色块
- 每个色块显示照片数
- Ctrl 多选 → OR 逻辑过滤
- 选中状态：放大 + 弹簧微动效

**模式 B：色谱双端滑动条**
- 彩虹渐变色谱条
- 双端滑块选择色相范围
- 底部显示匹配照片数
- 适合精确范围筛选

---

## 十二、分阶段开发计划

### Phase 1：核心骨架 + 大图预览 (预计 3-4 周)

> [!IMPORTANT]
> **Phase 1 目标**：跑通「添加目录 → 扫描 → 看到照片缩略图 → 双击查看大图」的完整闭环。大图预览是核心功能，前置到 Phase 1。

#### P1-1 项目初始化
- [ ] Tauri V2 + Vue 3 + TypeScript + Vite 脚手架
- [ ] Cargo.toml 添加 Phase 1 核心依赖
- [ ] CSS 设计系统搭建 (variables.css, reset.css, theme-dark.css, theme-light.css, animations.css)
- [ ] 项目目录结构创建
- [ ] `.editorconfig` 设置 `charset = utf-8`（Windows 11 编码安全）

#### P1-2 数据库层
- [ ] `state.rs`：AppState（Mutex<Connection> + 全局配置）
- [ ] `connection.rs`：连接初始化 + PRAGMA（7 项优化参数）
- [ ] `schema.rs`：建表 SQL (app_config, scan_roots, directories, photos)
- [ ] `models.rs`：Rust 结构体 + Serialize/Deserialize
- [ ] `queries.rs`：基础 CRUD 查询（含 15万级分页优化）

#### P1-3 文件扫描器
- [ ] `walker.rs`：walkdir 递归遍历 + 格式过滤
- [ ] `metadata.rs`：kamadak-exif 元数据提取
- [ ] 增量扫描：file_mtime 对比跳过已有文件
- [ ] 目录表自动维护 (parent_id, depth, photo_count)
- [ ] sort_datetime 预计算
- [ ] 扫描进度 **Channel** 流式推送 (ScanProgressPayload / ScanCompletedPayload)
- [ ] CancellationToken 支持取消
- [ ] 大文件处理：30MB+ PNG/照片的 EXIF 读取不应阻塞（仅读文件头 64KB）

#### P1-4 缩略图引擎
- [ ] `traits.rs`：ImageEngine trait
- [ ] `image_rs.rs`：标准格式引擎 (JPEG/PNG/WebP/BMP/GIF/TIFF)
- [ ] `engine/mod.rs`：EngineArena 调度（Phase 1 仅 ImageRsEngine）
- [ ] `exif_thumb.rs`：EXIF 嵌入缩略图提取（快速路径）
- [ ] `generator.rs`：缩略图生成主流程（30MB+ PNG 需内存控制：流式解码或限制并发数）
- [ ] `cache.rs`：两级哈希目录管理
- [ ] `thumbhash.rs`：ThumbHash 占位图
- [ ] `hash.rs`：xxHash3 cache_key
- [ ] WebP → JPEG 降级

#### P1-5 IPC 层
- [ ] `scan_commands.rs`：add/remove/list_scan_roots, start_scan(Channel)/stop_scan
- [ ] `photo_commands.rs`：get_photos (分页+排序), get_photo_detail, get_directory_tree/children
- [ ] `thumbnail_commands.rs`：request_thumbnail, batch_request_thumbnails
- [ ] `config_commands.rs`：get/set_app_config
- [ ] Channel 类型定义 + 低频 Event 定义

#### P1-6 前端 UI
- [ ] `AppShell.vue`：全局壳布局
- [ ] `AppSidebar.vue`：侧边栏骨架
- [ ] `AppToolbar.vue`：顶部工具栏
- [ ] `AppStatusBar.vue`：底部状态栏 + 进度条
- [ ] `FolderTree.vue`：目录树（虚拟化 + 懒加载）
- [ ] `SmartAlbums.vue`：智能相册（全部照片、最近导入）
- [ ] `PhotoGrid.vue`：照片网格容器
- [ ] `PhotoCard.vue`：照片卡片 (ThumbHash → 缩略图)
- [ ] `DateSeparator.vue`：日期分隔条
- [ ] `ThemeToggle.vue`：三态主题切换
- [ ] **`PhotoDetail.vue`：大图预览**（核心功能，Phase 1 完成）

#### P1-7 前端逻辑
- [ ] `useVirtualScroll.ts`：行级虚拟滚动
- [ ] `useJustifiedLayout.ts`：Justified Layout 算法
- [ ] `useRequestQueue.ts`：请求队列 + 优先级 + 快速取消
- [ ] `useThumbnail.ts`：ThumbHash → 缩略图切换
- [ ] `useFolderTree.ts`：目录树状态
- [ ] `useTheme.ts`：主题切换 + 持久化
- [ ] Pinia Stores：photoStore, scanStore, uiStore

#### P1-8 大图预览
- [ ] `PhotoDetail.vue`：全屏/半屏大图预览组件
- [ ] 渐进式加载：先显示缩略图放大 → 原图加载完毕后替换
- [ ] 键盘导航：← → 切换上一张/下一张，ESC 退出
- [ ] 鼠标滚轮缩放 + 拖拽平移
- [ ] EXIF 信息面板（右侧抽屉，显示相机型号/焦距/光圈/ISO/拍摄时间等）
- [ ] 30MB+ 大图加载优化：先加载缩略图 → 异步解码原图 → 平滑替换
- [ ] `get_photo_detail` IPC 命令 → 返回完整 EXIF + 文件信息

#### P1 验收标准
- ✅ 添加 1000+ 照片目录 → 底部状态栏 Channel 实时进度
- ✅ 扫描完成 → Justified Layout 展示
- ✅ ThumbHash → 缩略图平滑过渡无闪烁
- ✅ 目录树可展开/折叠，点击筛选
- ✅ 快速滚动 10000+ 照片 FPS ≥ 55
- ✅ 日期分隔条正确
- ✅ Light/Dark 主题可切换，偏好持久化
- ✅ **双击照片进入大图预览，键盘导航流畅**
- ✅ **大图可缩放/拖拽，EXIF 信息面板可展开**
- ✅ **30MB+ PNG 大文件不导致卡死或 OOM**

---

### Phase 2：功能完善 (预计 2-3 周)

> 目标：多引擎 + 主题色 + 照片管理

#### P2-1 多引擎接入
- [ ] `heic.rs`：HeicEngine (libheif-rs)
- [ ] `raw.rs`：RawEngine (rawler)
- [ ] EngineArena 注册全部引擎 + 降级链
- [ ] 引擎初始化失败优雅跳过

#### P2-2 主题色功能
- [ ] `color/extractor.rs`：MMCQ 提取 + 12 桶量化
- [ ] 缩略图流水线 Step 5 集成
- [ ] `ColorFilter.vue`：双模式颜色筛选 UI
- [ ] `useColorFilter.ts` + `filterStore.ts`
- [ ] `get_photos_by_color` IPC 命令
- [ ] 已有照片批量回填主题色

#### P2-3 照片管理
- [ ] 收藏 / 评分 UI + IPC 命令
- [ ] 软删除 / 恢复 / 回收站视图
- [ ] 乐观置灰 UI + 失败回滚
- [ ] 系统回收站集成 (`trash` crate)
- [ ] 在资源管理器中显示文件

#### P2-4 排序与视图
- [ ] 多排序：按日期 / 文件名 / 大小 / 评分
- [ ] 缩略图大小调节滑块
- [ ] 照片悬停 Tooltip

#### P2 验收标准
- ✅ HEIC + 主流 RAW 正确显示缩略图
- ✅ 颜色筛选双模式可用
- ✅ 收藏/评分/软删除正常
- ✅ 回收站可浏览/恢复
- ✅ 排序切换流畅

---

### Phase 3：高级交互 (预计 2-3 周)

> 目标：文件监听 + 右键菜单 + 批量操作 + 多根目录
> （大图预览已前移至 Phase 1）

#### P3-1 文件监听
- [ ] `watcher.rs`：notify crate 文件系统监听
- [ ] 新增/删除/修改 → 自动更新数据库
- [ ] `db:photos_updated` Event 通知前端

#### P3-2 右键菜单
- [ ] `ContextMenu.vue` + 边界溢出检测
- [ ] 照片右键：显示文件、复制路径、收藏、评分、删除
- [ ] 目录右键：刷新扫描、复制路径

#### P3-3 批量操作与相册
- [ ] `useSelection.ts`：Shift/Ctrl 多选
- [ ] 批量收藏/删除/评分
- [ ] 自定义相册创建 + 拖拽添加
- [ ] 标签系统

#### P3-4 多根目录
- [ ] 添加/移除根目录 UI
- [ ] 独立扫描状态管理
- [ ] 目录树多根平级展示

#### P3 验收标准
- ✅ 文件变更自动同步
- ✅ 右键菜单功能完整
- ✅ 批量选择与操作流畅
- ✅ 多根目录管理正常

---

### Phase 4：AI 与高级特性 (未来规划)

- [ ] BLAKE3 文件指纹 → 相似图片检测/去重
- [ ] CLIP 模型 → 语义搜图
- [ ] 人脸识别/聚类
- [ ] 全文搜索 (EXIF + 文件名 + AI 标签)
- [ ] GPS 地图聚合视图 (Leaflet/OSM)
- [ ] 时间轴视图
- [ ] 移动端适配 (iOS / Android)
- [ ] 云端挂载 (WebDAV / S3)

---

## 十三、工程实践与踩坑防御

### 13.1 编码安全

> [!CAUTION]
> **所有文件写入必须 UTF-8**。Windows 下 PowerShell 默认编码可能导致中文路径乱码，引发 `U+FFFD` 解析崩溃。Rust 端 `std::fs::write` 默认 UTF-8 无需额外处理。前端脚本工具确保 `.editorconfig` 设置 `charset = utf-8`。

### 13.2 SQL 安全

> [!WARNING]
> 所有涉及 `Path`、`Name`、`Tags` 的动态查询，**必须使用参数绑定 (`?` / `:param`)**，绝对禁止字符串拼接。用户文件夹名可能包含 `'`（如 `O'Connor`）导致 SQL 语法异常。

### 13.3 数据库事务策略

```rust
// 批量写入：500 条/事务
let tx = conn.transaction()?;
for photo in batch {
    tx.execute("INSERT INTO photos ...", params![...])?;
}
tx.commit()?;
```

### 13.4 错误处理模式

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("EXIF parse error: {0}")]
    Exif(String),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("Engine error: {0}")]
    Engine(String),
}

// Tauri Command 统一返回 Result<T, String>
impl From<AppError> for String {
    fn from(e: AppError) -> String { e.to_string() }
}
```

### 13.5 日志规范

| 级别 | 用途 | 示例 |
|------|------|------|
| ERROR | 不可恢复 | 数据库损坏、权限不足 |
| WARN | 可恢复 | 引擎降级、格式不支持 |
| INFO | 关键事件 | 扫描开始/完成、根目录添加 |
| DEBUG | 详细流程 | 缓存命中/未命中 |
| TRACE | 极细粒度 | SQL 参数、像素大小 |

### 13.6 Tauri V2 注意事项

- **capabilities/default.json** 必须声明所有需要的权限
- `invoke()` 参数和返回值必须 `Serialize + Deserialize`
- `thumbhash` BLOB 传输：前端接收为 `number[]`，用 `new Uint8Array()` 转换
- `convertFileSrc()` 需要 `tauri-plugin-fs` 的 scope 权限

---

## 十四、性能基准目标

> 基于用户实际场景：15万+ 图片，含 30MB+ 大尺寸 PNG/照片，Windows 11 平台。

| 场景 | 目标 | 说明 |
|------|------|------|
| 首次扫描 10,000 张 JPEG | ≤ 30 秒 | 含 EXIF + 入库（不含缩略图） |
| 首次扫描 150,000 张混合格式 | ≤ 8 分钟 | 含 EXIF + 入库（不含缩略图），Channel 实时进度 |
| 缩略图生成 (JPEG, EXIF 快速路径) | ≤ 5ms/张 | 直接提取 EXIF 嵌入缩略图 |
| 缩略图生成 (JPEG, 标准路径) | ≤ 50ms/张 | 完整解码 + 缩放 + 编码 |
| 缩略图生成 (30MB+ PNG) | ≤ 500ms/张 | 大尺寸 PNG 完整解码较慢，需控制并发防 OOM |
| 缩略图生成 (HEIC) | ≤ 200ms/张 | C 库解码 |
| 缩略图生成 (RAW) | ≤ 500ms/张 | 大文件解码 |
| 分页查询 1000 张 | ≤ 20ms | SQLite 索引覆盖 |
| 虚拟滚动 FPS | ≥ 55 | 快速滚动 15万 照片无掉帧 |
| 内存占用 (15 万张已缓存) | ≤ 400MB | 前端 + WebView + Rust（大文件场景适当放宽） |
| ThumbHash 解码 | ≤ 1ms/张 | 前端 JS 解码 |
| 大图预览加载 (30MB+ PNG) | ≤ 3 秒 | 先显示缩略图放大 → 原图异步加载替换 |

---

## 十五、风险与应对

| 风险 | 影响 | 应对策略 |
|------|------|----------|
| libheif C 库编译失败 | HEIC 不可用 | EngineArena 优雅降级 + 提示用户安装 libheif |
| rawler 不支持某些相机 | 部分 RAW 无缩略图 | 日志记录 + 用户反馈 + 后续考虑 libraw-rs |
| SQLite WAL 在网络文件系统失败 | 数据库损坏 | 检测到网络路径时提醒 + 数据库仅存本地 |
| 15万+ 首次扫描较慢 | 用户等待数分钟 | Channel 实时进度 + 分批 commit + EXIF 快速路径 |
| Windows 长路径 (>260 字符) | 扫描遗漏 | 使用 `\\?\` 前缀 / Rust std::path 自动处理 |
| WebP 编码器崩溃 | 缩略图失败 | catch_unwind + 降级 JPEG + thumb_status=2 标记 |
| 30MB+ 大文件 OOM | 并行解码内存爆炸 | rayon 并发数限制（大文件 ≤ 2 并发）+ 内存预估检查 |
| 大图预览加载慢 | 用户等待空白 | 先放大缩略图展示 → 原图异步解码 → 平滑替换（渐进式加载） |

---

## 十六、支持格式列表

### Phase 1 (ImageRsEngine)

| 格式 | 扩展名 |
|------|--------|
| JPEG | .jpg, .jpeg |
| PNG | .png |
| WebP | .webp |
| BMP | .bmp |
| GIF | .gif (取第一帧) |
| TIFF | .tif, .tiff |
| ICO | .ico |

### Phase 2 (HeicEngine + RawEngine)

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

## 用户审核结果 ✅

> [!NOTE]
| 决策 | 用户反馈 | 最终状态 |
|------|---------|----------|
| D16 进度推送方式 | ⚡ **改为 Channel**（用户要求高性能） | 已更新为 Channel |
| D18 移动端推迟到 Phase 4+ | ✅ 可以 | 已确认 |
| 大图预览优先级 | ⚡ **前移至 Phase 1**（用户认为是重要功能） | 已更新 |

## 开放问题（已解决）

| 问题 | 用户回答 | 影响 |
|------|---------|------|
| 数据量预期 | **15万+ 张图片，含 30MB+ 大型 PNG/照片** | 性能基准已调整；需增加大文件 OOM 防护；并发解码控制 

## 需要ai专家研究
Picasa Next 架构深度优化核心建议汇总
1. 读写彻底分离，终结并发阻塞 (Read/Write Connection Split)
    痛点：SQLite 是单文件数据库，在海量扫描期间如果前后端共用一条同步连接，会导致 UI 请求数据时发生严重卡顿，甚至引发 SQLITE_BUSY 报错。
    方案：借助 SQLite 的 WAL 模式，引入 r2d2 建立只读连接池（专门响应前端极其高频的缩略图和列表拉取）；同时维护唯一一条受互斥锁（Mutex）保护的写连接（专职后台扫描）。确保前台极速丝滑，后台疯狂吞吐。

2. 极致便携，根除“绝对路径的脆弱性” (Relative Path Storage)
    痛点：原方案在 photos 表中硬编码绝对路径。一旦用户更换移动硬盘盘符（如 D:\ 变 E:\）或重命名上级文件夹，15万张照片的数据关联将全部报废。
    方案：采用物理锚点架构。scan_roots 存绝对路径锚点，directories 仅存相对路径，photos 仅存文件名。硬盘搬家时，只需在 UI 修改一次 scan_roots 的根路径，十五万照片零 IO 秒级“复活”。

3. “批量事务”榨干写入极限 (Batch Transactions)
    痛点：逐条 INSERT 哪怕在 WAL 模式下也会触发频繁的同步锁，极大拖慢 15 万文件的首次扫描速度。
    方案：强制扫描引擎使用批处理模式，在内存中积累 500-1000 条数据后，开启 BEGIN TRANSACTION 一次性落盘。这将使磁盘写入性能成百上千倍提升。

4. 索引降维打击：Cache Key 整数化 (Cache Key as u64)
    痛点：将 xxh3_64 生成的缓存键存储为 16位 HEX 字符串，会浪费一倍的存储空间并拖慢千万级索引树的检索速度。
    方案：xxh3_64 本质是 64 位无符号整数，直接作为 INTEGER 类型存入 SQLite。这不仅让索引体积大幅缩水，查询速度也达到了硬件极致。

5. 布局计算与数据加载的双轨制 (Layout vs Content Fetching)
    痛点：瀑布流 (Justified Layout) 必须预知所有图片的宽高比才能算出完美的高度。如果只按页请求数据，向下滚动时必然出现“滚动条乱跳”的噩梦。
    方案：数据分离获取。提供一个极轻量的 IPC 命令 get_photo_layout，15万张仅返回 [{id, width, height, sort_datetime}]（约2MB），瞬间撑开全局虚拟列表框架。当内容即将进入视口时，再精准按需请求该批次的详情和缩略图。

6. 主线程保卫战：Web Worker 剥离 (Worker Offloading)
    痛点：15万个浮点坐标框的 Justified Layout 计算非常沉重，足以让主线程在打开软件瞬间发生数秒的“假死”。
    方案：将整个瀑布流的矩阵计算丢进单独的 Web Worker 后台线程，计算完毕后直接回传一维坐标系。确保主框架 UI 响应的绝对平滑。

7. 原生级图像防微卡顿 (Image.decode Pre-rasterization)
    痛点：当用户猛烈滚动屏幕，数十张新缩略图同时被插入 DOM 时，浏览器底层将图片纹理上传至 GPU 会引发主线程掉帧。
    方案：在图片正式替换给 <img src> 之前，先利用原生的 Image.decode() API 在浏览器后台线程强制完成“光栅化（Rasterize）”，彻底抹平视觉割裂感。

8. 工业级虚拟滚动底座 (TanStack Virtual)
    痛点：面对动态高度、多端缩放等复杂场景，手写 IntersectionObserver 极易出现边界计算错误和白屏。
    方案：拥抱开源工业级标杆 @tanstack/vue-virtual 进行行级渲染，我们只负责喂给它精准的坐标和尺寸，它负责把 DOM 节点数量死死限制在 50 个以内。

9. 图像处理引擎的“按需动态升降级” (Dynamic Engine Arena)
    方案：不要局限于单一的解码方式。在设置面板引入“处理引擎”切换选项。
    默认使用高兼容性的纯 CPU 引擎 (image + SIMD)。
    未来可一键切入系统底层的 WIC (Windows原生) 甚至 WGPU 引擎。
    给予极客用户掌控力，也为后续开发留下了明确的性能天花板突破口。

10. 暴露深水区调优参数 (Advanced Settings Tuning)
    方案：每个用户的硬件天差地别（机械硬盘 vs NVMe，双核 vs 32核）。在高级设置中直接暴露 “最大扫描并发线程数” 和 “入库缓存批次大小”。让应用在不同设备上都能跑到它的极限。