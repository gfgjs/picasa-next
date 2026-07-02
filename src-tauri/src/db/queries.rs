// src-tauri/src/db/queries.rs
// src-tauri/src/db/queries.rs
//! Reusable parameterised SQL query functions.
//! 可重用的参数化 SQL 查询函数。
//! All SQL uses parameter binding — never string concatenation.
//! 所有 SQL 均使用参数绑定 — 绝不使用字符串拼接。

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::db::models::{
    AppStats, AudioMeta, Collection, DirFile, DirNode, Directory, DocumentMeta, ImageMeta,
    LayoutItem, MediaDetail, MediaFilter, MediaItem, MediaMeta, NewVolume, ScanRoot, SearchResult,
    SelectionDescriptor, StorageBackendInfo, ThumbResult, ViewDescriptor, ViewScope, Volume,
    VolumeKind,
};
use crate::error::{AppError, Result};
use crate::exotic::task::{ExoticTaskRow, ExoticTaskStatus};
use crate::utils::path::resolve_media_path;

// ── Row mappers ──────────────────────────────────────────────────────────────
// ── 行映射器 ──────────────────────────────────────────────────────────────

fn map_scan_root(row: &Row<'_>) -> rusqlite::Result<ScanRoot> {
    Ok(ScanRoot {
        id: row.get(0)?,
        path: row.get(1)?,
        alias: row.get(2)?,
        scan_status: row.get(3)?,
        scan_progress: row.get(4)?,
        total_files: row.get(5)?,
        last_scan_at: row.get(6)?,
        is_active: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        backend_id: row.get(10)?,
    })
}

fn map_volume(row: &Row<'_>) -> rusqlite::Result<Volume> {
    Ok(Volume {
        id: row.get(0)?,
        stable_id: row.get(1)?,
        label: row.get(2)?,
        // kind 读宽容：未知字符串归 Local（防御旧库 / 未来类型）。
        kind: VolumeKind::from_str_lossy(&row.get::<_, String>(3)?),
        last_mount_path: row.get(4)?,
        last_seen: row.get(5)?,
        is_online: row.get::<_, i64>(6)? != 0,
        created_at: row.get(7)?,
    })
}

fn map_media_item(row: &Row<'_>) -> rusqlite::Result<MediaItem> {
    Ok(MediaItem {
        id: row.get(0)?,
        directory_id: row.get(1)?,
        file_name: row.get(2)?,
        file_size: row.get(3)?,
        file_mtime: row.get(4)?,
        file_format: row.get(5)?,
        media_type: row.get(6)?,
        width: row.get(7)?,
        height: row.get(8)?,
        duration_ms: row.get(9)?,
        sort_datetime: row.get(10)?,
        cache_key: row.get(11)?,
        thumb_status: row.get(12)?,
        thumb_path: row.get(13)?,
        thumbhash: row.get(14)?,
        is_favorited: row.get::<_, i64>(15)? != 0,
        is_deleted: row.get::<_, i64>(16)? != 0,
        deleted_at: row.get(17)?,
        rating: row.get(18)?,
        is_live_photo: row.get::<_, i64>(19)? != 0,
        has_embedded_video: row.get::<_, i64>(20)? != 0,
        companion_of: row.get(21)?,
        content_hash: row.get(22)?,
        created_at: row.get(23)?,
        updated_at: row.get(24)?,
        // color_label 追加在末列（索引 25）而非插中间——保既有列位全不动，仅新增一位，
        // 避免移动 rating 之后所有列引发静默串列。喂入的三处 SELECT 同样把它追加到末尾。
        color_label: row.get(25)?,
    })
}

fn map_layout_item(row: &Row<'_>) -> rusqlite::Result<LayoutItem> {
    Ok(LayoutItem {
        id: row.get(0)?,
        width: row.get(1)?,
        height: row.get(2)?,
        file_size: row.get(3)?,
        sort_datetime: row.get(4)?,
        file_format: row.get(5)?,
        media_type: row.get(6)?,
        is_live_photo: row.get::<_, i64>(7)? != 0,
        duration_ms: row.get(8)?,
        thumb_status: row.get(9)?,
        thumb_path: row.get(10)?,
        thumbhash: row.get(11)?,
        is_favorited: row.get::<_, i64>(12)? != 0,
        dir_path: row.get(13)?,
        dir_name: row.get(14)?,
        dir_id: row.get(15)?,
        availability: row.get(16)?,
        // rating 紧随 availability（SELECT 第 17 列），similarity 顺移到第 18 列——
        // 列位与 query_layout_items 的 SELECT 顺序严格对齐，错位会静默串列。
        rating: row.get(17)?,
        // color_label 紧随 rating（SELECT 第 18 列），similarity 顺移到第 19 列。
        color_label: row.get(18)?,
        similarity: row.get(19)?,
    })
}

fn map_dir_node(row: &Row<'_>) -> rusqlite::Result<DirNode> {
    Ok(DirNode {
        id: row.get(0)?,
        root_id: row.get(1)?,
        parent_id: row.get(2)?,
        name: row.get(3)?,
        rel_path: row.get(4)?,
        depth: row.get(5)?,
        media_count: row.get(6)?,
        has_children: row.get::<_, i64>(7)? != 0,
    })
}

// ── Scan roots ───────────────────────────────────────────────────────────────
// ── 扫描根目录 ───────────────────────────────────────────────────────────────

pub fn insert_scan_root(
    conn: &Connection,
    path: &str,
    alias: Option<&str>,
    backend_id: Option<i64>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO scan_roots (path, alias, backend_id) VALUES (?1, ?2, ?3)",
        params![path, alias, backend_id],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 设置 / 清除扫描根的存储后端归属（`None`=本地/OS 挂载）。供 Part5 网络盘绑定 UI 调用。
pub fn set_scan_root_backend(
    conn: &Connection,
    root_id: i64,
    backend_id: Option<i64>,
) -> Result<()> {
    conn.execute(
        "UPDATE scan_roots SET backend_id = ?2, updated_at = strftime('%s','now') WHERE id = ?1",
        params![root_id, backend_id],
    )?;
    Ok(())
}

/// 绑定 scan_root 所属卷。新根添加时调用，使其媒体可继承 volume_id 参与缺失检测守门1
/// （未绑卷的新根 → media volume_id 恒 NULL → 缺失检测休眠，见 C5 Piece1）。
pub fn set_scan_root_volume(conn: &Connection, root_id: i64, volume_id: Option<i64>) -> Result<()> {
    conn.execute(
        "UPDATE scan_roots SET volume_id = ?2, updated_at = strftime('%s','now') WHERE id = ?1",
        params![root_id, volume_id],
    )?;
    Ok(())
}

pub fn delete_scan_root(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM scan_roots WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_scan_roots(conn: &Connection) -> Result<Vec<ScanRoot>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, alias, scan_status, scan_progress, total_files,
                last_scan_at, is_active, created_at, updated_at, backend_id
         FROM scan_roots ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], map_scan_root)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn get_scan_root(conn: &Connection, id: i64) -> Result<ScanRoot> {
    conn.query_row(
        "SELECT id, path, alias, scan_status, scan_progress, total_files,
                last_scan_at, is_active, created_at, updated_at, backend_id
         FROM scan_roots WHERE id = ?1",
        params![id],
        map_scan_root,
    )
    .map_err(|_| AppError::ScanRootNotFound(id))
}

pub fn update_scan_root_status(
    conn: &Connection,
    id: i64,
    status: &str,
    progress: i64,
    total: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE scan_roots SET scan_status=?1, scan_progress=?2, total_files=?3,
                 updated_at=strftime('%s','now')
         WHERE id=?4",
        params![status, progress, total, id],
    )?;
    Ok(())
}

pub fn finish_scan_root(conn: &Connection, id: i64, total: i64) -> Result<()> {
    conn.execute(
        "UPDATE scan_roots SET scan_status='idle', scan_progress=?1, total_files=?1,
                 last_scan_at=strftime('%s','now'), updated_at=strftime('%s','now')
         WHERE id=?2",
        params![total, id],
    )?;
    Ok(())
}

// ── Directories ──────────────────────────────────────────────────────────────
// ── 目录 ──────────────────────────────────────────────────────────────

/// Upsert a directory. Returns the row id.
/// 插入或更新目录。返回行 ID。
pub fn upsert_directory(
    conn: &Connection,
    root_id: i64,
    parent_id: Option<i64>,
    rel_path: &str,
    name: &str,
    depth: i64,
    mtime: Option<i64>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO directories (root_id, parent_id, rel_path, name, depth, mtime)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(root_id, rel_path) DO UPDATE SET
             name=excluded.name, mtime=excluded.mtime",
        params![root_id, parent_id, rel_path, name, depth, mtime],
    )?;
    // After upsert, get the id (may have existed before)
    // 插入或更新后，获取 id（可能之前就已存在）
    let id: i64 = conn.query_row(
        "SELECT id FROM directories WHERE root_id=?1 AND rel_path=?2",
        params![root_id, rel_path],
        |row| row.get(0),
    )?;
    Ok(id)
}

pub fn get_directory_tree(conn: &Connection, root_id: i64) -> Result<Vec<DirNode>> {
    // R2-6 去相关化:原实现对每个输出行重跑一个相关递归 CTE 统计子树(每行一次子树遍历)
    // + 每行一个 COUNT(*)>0 子查询(数完全部子行才比较)。改为一次带 top 标签的共享递归
    // CTE + 单次 GROUP BY,has_children 改 EXISTS 短路。口径不变:is_deleted=0 AND
    // companion_of IS NULL(与 list_directory_files/角标一致);列序 8 列不变,map_dir_node 复用。
    let mut stmt = conn.prepare(
        "WITH RECURSIVE sub(id, top) AS (
             SELECT id, id FROM directories WHERE root_id = ?1 AND parent_id IS NULL
             UNION ALL
             SELECT c.id, s.top FROM directories c JOIN sub s ON c.parent_id = s.id
         ),
         agg(top, cnt) AS (
             SELECT s.top, COUNT(*)
             FROM sub s JOIN media_items m ON m.directory_id = s.id
             WHERE m.is_deleted = 0 AND m.companion_of IS NULL
             GROUP BY s.top
         )
         SELECT d.id, d.root_id, d.parent_id, d.name, d.rel_path, d.depth,
                COALESCE(a.cnt, 0) AS media_count,
                EXISTS(SELECT 1 FROM directories c WHERE c.parent_id = d.id) AS has_children
         FROM directories d
         LEFT JOIN agg a ON a.top = d.id
         WHERE d.root_id = ?1 AND d.parent_id IS NULL
         ORDER BY d.name ASC",
    )?;
    let rows = stmt.query_map(params![root_id], map_dir_node)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn get_directory_children(conn: &Connection, parent_id: i64) -> Result<Vec<DirNode>> {
    // 与 get_directory_tree 同形改写(R2-6),仅 CTE 种子与外层过滤不同;懒加载契约保持
    // (有意不做「全根一次查询+内存建树」——那会让巨库上每次展开小叶子都全根扫描)。
    let mut stmt = conn.prepare(
        "WITH RECURSIVE sub(id, top) AS (
             SELECT id, id FROM directories WHERE parent_id = ?1
             UNION ALL
             SELECT c.id, s.top FROM directories c JOIN sub s ON c.parent_id = s.id
         ),
         agg(top, cnt) AS (
             SELECT s.top, COUNT(*)
             FROM sub s JOIN media_items m ON m.directory_id = s.id
             WHERE m.is_deleted = 0 AND m.companion_of IS NULL
             GROUP BY s.top
         )
         SELECT d.id, d.root_id, d.parent_id, d.name, d.rel_path, d.depth,
                COALESCE(a.cnt, 0) AS media_count,
                EXISTS(SELECT 1 FROM directories c WHERE c.parent_id = d.id) AS has_children
         FROM directories d
         LEFT JOIN agg a ON a.top = d.id
         WHERE d.parent_id = ?1
         ORDER BY d.name ASC",
    )?;
    let rows = stmt.query_map(params![parent_id], map_dir_node)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Direct media files of a single directory (NOT its subtree) for the sidebar tree's
/// expandable file list. Subfolders show up as their own nodes, so we list only files
/// physically in this directory. Excludes soft-deleted items and Live-Photo companion
/// videos — the same filter used for a directory's `media_count` — so the leaf count
/// matches the badge. Sorted case-insensitively by name for a tidy file-manager list.
/// 单个目录（非其子树）的直接媒体文件，供侧边栏树的可展开文件列表使用。子文件夹各自
/// 作为节点出现，故此处仅列出物理位于本目录的文件。排除软删除项与 Live Photo 伴随视频
///（与目录 `media_count` 同口径），使叶子数量与角标一致。按名称不区分大小写排序，呈现
/// 整洁的文件管理器式列表。
pub fn list_directory_files(conn: &Connection, directory_id: i64) -> Result<Vec<DirFile>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_name, media_type, is_favorited
         FROM media_items
         WHERE directory_id = ?1 AND is_deleted = 0 AND companion_of IS NULL
         ORDER BY file_name COLLATE NOCASE ASC",
    )?;
    let rows = stmt.query_map(params![directory_id], |row| {
        Ok(DirFile {
            id: row.get(0)?,
            file_name: row.get(1)?,
            media_type: row.get(2)?,
            is_favorited: row.get(3)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// 写回某目录的「直接（非递归）媒体计数」基线（T17a 增量剪枝判据之一）。
///
/// ⚠️ **列语义已复用**：`directories.media_count` 此前是 DEFAULT 0 的死列（仅 mock_data 写）。
/// T17a 起由 fast_scan 写入**直接子项**媒体数（仅本目录、不含子目录），与目录 `mtime` 一起
/// 构成「目录未变 → 整子树可跳过」的判据。**目录树 UI 的角标计数仍走 `get_directory_tree`/
/// `get_directory_children` 的递归子查询别名（子树聚合），不读此列**——二者口径不同、互不影响。
pub fn set_directory_media_count(conn: &Connection, dir_id: i64, count: i64) -> Result<()> {
    conn.execute(
        "UPDATE directories SET media_count = ?1 WHERE id = ?2",
        params![count, dir_id],
    )?;
    Ok(())
}

pub fn get_directory_ancestors(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE ancestors(id, parent_id) AS (
            SELECT id, parent_id FROM directories WHERE id = ?1
            UNION ALL
            SELECT d.id, d.parent_id FROM directories d
            JOIN ancestors a ON a.parent_id = d.id
         )
         SELECT id FROM ancestors;",
    )?;

    let mut rows = stmt.query(params![id])?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next()? {
        ids.push(row.get::<_, i64>(0)?);
    }

    ids.reverse();
    Ok(ids)
}

/// Resolve a directory's absolute filesystem path (root.path joined with rel_path).
/// 解析目录的绝对文件系统路径（root.path 拼接 rel_path）。
pub fn get_directory_abs_path(conn: &Connection, dir_id: i64) -> Result<String> {
    conn.query_row(
        "SELECT r.path, d.rel_path
         FROM directories d JOIN scan_roots r ON r.id = d.root_id
         WHERE d.id = ?1",
        params![dir_id],
        |row| {
            let root: String = row.get(0)?;
            let rel: String = row.get(1)?;
            Ok(if rel.is_empty() {
                root
            } else {
                format!("{}/{}", root, rel)
            })
        },
    )
    .map_err(|_| AppError::Internal(format!("directory {} not found", dir_id)))
}

/// All descendant directory ids of `dir_id`, INCLUDING `dir_id` itself. Used to resolve
/// where to scroll when a folder with no direct media is clicked — we jump to its first
/// descendant subfolder that does have media (问题1).
/// `dir_id` 的所有后代目录 id（含自身）。用于点击「无直接媒体」的文件夹时确定滚动目标——
/// 跳到其首个「有媒体」的后代子文件夹（问题1）。
pub fn get_directory_descendant_ids(conn: &Connection, dir_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM directories WHERE id = ?1
            UNION ALL
            SELECT d.id FROM directories d JOIN subtree s ON d.parent_id = s.id
         )
         SELECT id FROM subtree",
    )?;
    let rows = stmt.query_map(params![dir_id], |r| r.get::<_, i64>(0))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Duplicate a media_items row into `target_dir_id` (new auto id), reusing the source's
/// cache_key / thumbnail / dimensions so the copy shows instantly. Returns the new id.
/// Used by drag-copy; the precise new id makes the copy cleanly undoable (问题2).
/// 将一条 media_items 行复制到 `target_dir_id`（新自增 id），复用源的 cache_key / 缩略图 /
/// 尺寸，使副本即时显示。返回新 id。用于拖拽复制；精确的新 id 使复制可干净撤销（问题2）。
pub fn duplicate_media_item_into_dir(
    conn: &Connection,
    src_id: i64,
    target_dir_id: i64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO media_items
            (directory_id, file_name, file_size, file_mtime, file_format, media_type,
             width, height, duration_ms, sort_datetime, cache_key, thumb_status, thumb_path,
             thumbhash, is_favorited, is_deleted, deleted_at, rating, is_live_photo,
             has_embedded_video, companion_of, content_hash, created_at, updated_at)
         SELECT ?1, file_name, file_size, file_mtime, file_format, media_type,
             width, height, duration_ms, sort_datetime, cache_key, thumb_status, thumb_path,
             thumbhash, is_favorited, is_deleted, deleted_at, rating, is_live_photo,
             has_embedded_video, companion_of, content_hash,
             strftime('%s','now'), strftime('%s','now')
         FROM media_items WHERE id = ?2",
        params![target_dir_id, src_id],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Fetch a single directory row by id.
/// 按 id 获取单个目录行。
pub fn get_directory(conn: &Connection, id: i64) -> Result<Directory> {
    conn.query_row(
        "SELECT id, root_id, parent_id, rel_path, name, depth, media_count, mtime, created_at
         FROM directories WHERE id = ?1",
        params![id],
        |row| {
            Ok(Directory {
                id: row.get(0)?,
                root_id: row.get(1)?,
                parent_id: row.get(2)?,
                rel_path: row.get(3)?,
                name: row.get(4)?,
                depth: row.get(5)?,
                media_count: row.get(6)?,
                mtime: row.get(7)?,
                created_at: row.get(8)?,
            })
        },
    )
    .optional()?
    .ok_or(AppError::Internal(format!(
        "directory not found: id={id} | 未找到目录"
    )))
}

/// Whether a directory already has a direct child folder with the given name.
/// 给定父目录下是否已存在同名的直接子文件夹。
pub fn dir_has_child_named(conn: &Connection, parent_id: i64, name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM directories WHERE parent_id = ?1 AND name = ?2",
        params![parent_id, name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// One directory in a moved subtree (id + its current rel_path + depth).
/// 被移动子树中的一个目录（id + 当前 rel_path + depth）。
pub struct SubtreeDirRow {
    pub id: i64,
    pub rel_path: String,
    pub depth: i64,
}

/// All directories in the subtree rooted at `root_dir_id`, including itself.
/// 以 `root_dir_id` 为根的子树中的所有目录（含自身）。
pub fn get_directory_subtree(conn: &Connection, root_dir_id: i64) -> Result<Vec<SubtreeDirRow>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE subtree(id, rel_path, depth) AS (
            SELECT id, rel_path, depth FROM directories WHERE id = ?1
            UNION ALL
            SELECT d.id, d.rel_path, d.depth FROM directories d
            JOIN subtree s ON d.parent_id = s.id
         )
         SELECT id, rel_path, depth FROM subtree",
    )?;
    let rows = stmt.query_map(params![root_dir_id], |row| {
        Ok(SubtreeDirRow {
            id: row.get(0)?,
            rel_path: row.get(1)?,
            depth: row.get(2)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Minimal media-item info needed to recompute cache_key / rename thumbnails on move.
/// 移动时重算 cache_key / 重命名缩略图所需的最小媒体项信息。
pub struct SubtreeMediaRow {
    pub id: i64,
    pub directory_id: i64,
    pub file_name: String,
    pub file_mtime: i64,
    pub cache_key: i64,
    pub thumb_status: i64,
    pub thumb_path: Option<String>,
}

/// All media items (including soft-deleted + companions) within a directory subtree.
/// 目录子树内的所有媒体项（含软删除项与伴随项）。
pub fn get_media_in_subtree(conn: &Connection, root_dir_id: i64) -> Result<Vec<SubtreeMediaRow>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM directories WHERE id = ?1
            UNION ALL
            SELECT d.id FROM directories d JOIN subtree s ON d.parent_id = s.id
         )
         SELECT m.id, m.directory_id, m.file_name, m.file_mtime, m.cache_key, m.thumb_status, m.thumb_path
         FROM media_items m
         WHERE m.directory_id IN (SELECT id FROM subtree)",
    )?;
    let rows = stmt.query_map(params![root_dir_id], |row| {
        Ok(SubtreeMediaRow {
            id: row.get(0)?,
            directory_id: row.get(1)?,
            file_name: row.get(2)?,
            file_mtime: row.get(3)?,
            cache_key: row.get(4)?,
            thumb_status: row.get(5)?,
            thumb_path: row.get(6)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Delete a directory row (CASCADE removes descendant directories + their media).
/// Returns the number of directory rows directly matched (0 or 1).
/// 删除目录行（CASCADE 级联删除后代目录及其媒体）。返回直接匹配的目录行数（0 或 1）。
pub fn delete_directory_by_id(conn: &Connection, id: i64) -> Result<usize> {
    let n = conn.execute("DELETE FROM directories WHERE id = ?1", params![id])?;
    Ok(n)
}

/// Find a directory id by (root_id, rel_path). Used by copy-undo to locate ingested rows.
/// 按 (root_id, rel_path) 查找目录 id。供复制撤销定位已登记的行。
pub fn find_directory_id(conn: &Connection, root_id: i64, rel_path: &str) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM directories WHERE root_id = ?1 AND rel_path = ?2",
        params![root_id, rel_path],
        |row| row.get(0),
    )
    .optional()
    .map_err(AppError::from)
}

// ── Media items ───────────────────────────────────────────────────────────────
// ── 媒体项 ───────────────────────────────────────────────────────────────

/// Batch-upsert helper data for fast scan.
/// 快速扫描的批量插入/更新辅助数据。
pub struct FastScanItem {
    pub directory_id: i64,
    pub file_name: String,
    pub file_size: i64,
    pub file_mtime: i64,
    pub file_format: String,
    pub media_type: String,
    pub width: i64,
    pub height: i64,
    pub sort_datetime: i64,
    pub cache_key: i64,
}

/// Outcome of a fast-scan upsert (Part1 §1.5).
/// 快速扫描 upsert 的结果（Part1 §1.5）。
///
/// `SourceChanged` 必须触发失效：主缩略图已在 SQL 内重置；调用方还须把该 item 的
/// exotic 任务退回 pending、清旧产物（见 `scanner::fast_scan`）。不能只依赖列默认值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    /// mtime 未变 → 跳过。
    Unchanged(i64),
    /// 全新插入。
    Inserted(i64),
    /// 已存在但源文件变化（mtime 不同）→ 缓存与任务须失效重做。
    SourceChanged(i64),
}

impl UpsertOutcome {
    pub fn id(&self) -> i64 {
        match self {
            UpsertOutcome::Unchanged(id)
            | UpsertOutcome::Inserted(id)
            | UpsertOutcome::SourceChanged(id) => *id,
        }
    }
}

/// 失效一个媒体项的全部派生元数据（Part2 §3.3 + Part3 §3.4，SourceChanged 统一入口）。
/// 文件被替换/编辑（mtime 变）时调用：删三类 meta，使 enricher 的 `image_meta.item_id IS NULL`
/// 过滤重新命中、重算 EXIF/时长/编码（否则旧 EXIF/时长/编码永久停滞——这是修复前的真 bug）；
/// 并把 `media_derivations`（视频封面/关键帧、文档缩略图、音频封面…）退回 pending，使 Producer
/// 重新派生（Part3 Q5：源变后派生停留旧版的修复）。
///
/// **必须在 upsert 的同一事务内调用**（避免半失效）。范围：image/video/audio_meta + media_derivations。
/// - 主缩略图状态（`thumb_status/thumb_path/thumbhash`）由 upsert 的 UPDATE 同事务复位，不在此重复。
/// - exotic 任务失效已在 `scanner::fast_scan` 单独接（`invalidate_exotic_tasks_for_item`），此处不重复。
/// - 旧磁盘派生产物（sprites/封面）靠 `cache_key`（含 mtime）天然换 key 成孤儿，交缓存 GC（§3.3.2）
///   兜底回收——本函数只管 DB 状态，不碰文件系统。
/// - 🟢 AI 向量（`ai_status`）/ 人脸（`face_status`）失效已接（Part4 T4 / §3.12）：换内容后旧 CLIP
///   向量/人脸框是旧图的，复位状态重分析 + 删旧向量/faces 行 + 受影响 person 派生重算。
pub fn invalidate_derived_for_item(conn: &Connection, item_id: i64) -> Result<()> {
    conn.execute("DELETE FROM image_meta WHERE item_id=?1", params![item_id])?;
    conn.execute("DELETE FROM video_meta WHERE item_id=?1", params![item_id])?;
    conn.execute("DELETE FROM audio_meta WHERE item_id=?1", params![item_id])?;
    // Part3 Q5：派生任务退回 pending（status=0），清空旧产物路径与错误，刷新时间戳。
    // Producer 的 `get_pending_derivations`（status=0）据此重新领取重派；`updated_at` 走部分索引
    // `idx_deriv_pending`（status<2）天然纳入。
    conn.execute(
        "UPDATE media_derivations
            SET status=0, payload_path=NULL, error=NULL, updated_at=strftime('%s','now')
          WHERE item_id=?1",
        params![item_id],
    )?;

    // 🟢 Part4 T4（§3.12）：AI/人脸失效。换内容（mtime+size 变）后旧 CLIP 向量/人脸框属旧图，
    // 不失效则语义搜索/人脸命中错图。复位 ai_status/face_status=0（重分析），删旧向量/人脸行。
    // 🔑 删脸前先收集受影响 person（删后查不到）——删脸后这些 person 的质心/封面/计数陈旧，须
    // 连带重算（同事务），范本与 §3.5.1 审批一致（recompute_person_aggregates 含删空簇策略）。
    // ai_embeddings 有 ON DELETE CASCADE，但此处是“源变”非“删 item”，须显式删。
    let affected_persons: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT person_id FROM faces WHERE item_id=?1 AND person_id IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![item_id], |row| row.get::<_, i64>(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    conn.execute(
        "UPDATE media_items SET ai_status=0, face_status=0, updated_at=strftime('%s','now') WHERE id=?1",
        params![item_id],
    )?;
    conn.execute(
        "DELETE FROM ai_embeddings WHERE item_id=?1",
        params![item_id],
    )?;
    conn.execute("DELETE FROM faces WHERE item_id=?1", params![item_id])?;
    for pid in affected_persons {
        recompute_person_aggregates(conn, pid)?;
    }
    Ok(())
}

/// Insert or update a media item from the fast scan phase.
/// 插入或更新来自快速扫描阶段的媒体项。
/// Returns the [`UpsertOutcome`] so the caller can seed/invalidate exotic tasks.
/// 返回 [`UpsertOutcome`]，供调用方据此播种/失效 exotic 任务。
/// 快速入库 upsert。`volume_id` 为本 scan_root 所属卷（扫描上下文常量，非 per-file 数据）——
/// 新项据此入库、历史 NULL 项经 `COALESCE` 顺带治愈，使其能参与缺失检测守门1（在线卷集）。
/// `None`（未识别卷/孤儿根）→ 新项 volume_id 留 NULL（守门把它天然排除，宁可不删）。
pub fn upsert_fast_scan_item(
    conn: &Connection,
    item: &FastScanItem,
    volume_id: Option<i64>,
) -> Result<UpsertOutcome> {
    // Check if exists with same mtime (no change needed)
    // 检查是否存在具有相同 mtime 的项（无需更改）；同时取 availability + volume_id 以支撑
    // missing→online 自动恢复 与 历史 NULL volume_id 的定向治愈。
    let existing: Option<(i64, i64, String, Option<i64>)> = conn
        .query_row(
            "SELECT id, file_mtime, availability, volume_id FROM media_items WHERE directory_id=?1 AND file_name=?2",
            params![item.directory_id, item.file_name],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .ok();

    if let Some((id, mtime, availability, existing_vol)) = existing {
        if mtime == item.file_mtime {
            // Unchanged — skip（但若曾被标 missing 的文件原样重现，需自动恢复 online；
            // 或历史插入遗留 volume_id=NULL，需补绑本根卷使其可参与缺失检测）。
            // 🔴 重现自动恢复（Part2 §3.2.4）+ 卷补绑：Unchanged 路径本不写库，仅当确有需要
            // （missing 复位 或 卷 NULL 待治愈）时做一次定向写 —— 普通未变更文件仍零写。
            // **不碰 is_deleted/offline**。COALESCE 保留既有卷、仅填 NULL。
            let needs_avail_fix = availability == "missing";
            let needs_vol_heal = existing_vol.is_none() && volume_id.is_some();
            if needs_avail_fix || needs_vol_heal {
                conn.execute(
                    "UPDATE media_items SET
                         availability = CASE WHEN availability='missing' THEN 'online' ELSE availability END,
                         volume_id    = COALESCE(volume_id, ?2),
                         updated_at   = strftime('%s','now')
                     WHERE id=?1",
                    params![id, volume_id],
                )?;
            }
            return Ok(UpsertOutcome::Unchanged(id));
        }
        // Changed — update（同时重置主缩略图状态；exotic 任务失效由调用方处理）
        // 已更改 — 更新。availability 经 CASE 顺带恢复（文件变 = 必在场，曾 missing 则复位 online）；
        // volume_id 经 COALESCE 顺带治愈历史 NULL（既有卷不变）。
        conn.execute(
            "UPDATE media_items SET file_size=?1, file_mtime=?2, file_format=?3,
                      media_type=?4, width=?5, height=?6, sort_datetime=?7,
                      cache_key=?8, thumb_status=0, thumb_path=NULL, thumbhash=NULL,
                      volume_id=COALESCE(volume_id, ?9),
                      availability=CASE WHEN availability='missing' THEN 'online' ELSE availability END,
                      updated_at=strftime('%s','now')
             WHERE id=?10",
            params![
                item.file_size,
                item.file_mtime,
                item.file_format,
                item.media_type,
                item.width,
                item.height,
                item.sort_datetime,
                item.cache_key,
                volume_id,
                id
            ],
        )?;
        // 🔴 SourceChanged 全失效（Part2 §3.3）：源文件变了，旧 EXIF/时长/编码必须作废，
        // 否则 enricher 不再重选该项、元数据永久停滞。同一事务内，避免半失效。
        invalidate_derived_for_item(conn, id)?;
        return Ok(UpsertOutcome::SourceChanged(id));
    }

    // New item — 据扫描上下文卷入库（修复:此前新项 volume_id 恒 NULL → 缺失检测对新数据休眠）。
    // 新项
    conn.execute(
        "INSERT INTO media_items
             (directory_id, file_name, file_size, file_mtime, file_format,
              media_type, width, height, sort_datetime, cache_key, volume_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            item.directory_id,
            item.file_name,
            item.file_size,
            item.file_mtime,
            item.file_format,
            item.media_type,
            item.width,
            item.height,
            item.sort_datetime,
            item.cache_key,
            volume_id
        ],
    )?;
    Ok(UpsertOutcome::Inserted(conn.last_insert_rowid()))
}

pub fn get_media_item(conn: &Connection, id: i64) -> Result<MediaItem> {
    conn.query_row(
        "SELECT id, directory_id, file_name, file_size, file_mtime, file_format,
                media_type, width, height, duration_ms, sort_datetime, cache_key,
                thumb_status, thumb_path, thumbhash, is_favorited, is_deleted,
                deleted_at, rating, is_live_photo, has_embedded_video, companion_of,
                content_hash, created_at, updated_at, color_label
         FROM media_items WHERE id=?1",
        params![id],
        map_media_item,
    )
    .map_err(|_| AppError::MediaNotFound(id))
}

pub fn get_media_detail(conn: &Connection, id: i64) -> Result<MediaDetail> {
    let item = get_media_item(conn, id)?;

    // Resolve absolute path via joined directories + scan_roots
    // 通过关联目录和扫描根目录解析绝对路径
    let (rel_path, root_path): (String, String) = conn
        .query_row(
            "SELECT d.rel_path, r.path
             FROM directories d JOIN scan_roots r ON d.root_id = r.id
             WHERE d.id=?1",
            params![item.directory_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| AppError::PathResolution(e.to_string()))?;

    let abs_path = resolve_media_path(&root_path, &rel_path, &item.file_name);

    // 系统可用态（缺失检测 Part2 §3.2）：供查看器对「卷离线/文件缺失」明确提示。
    let availability: String = conn
        .query_row(
            "SELECT availability FROM media_items WHERE id=?1",
            params![id],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "online".to_string());

    // Image meta (optional — may not exist yet)
    // 图像元数据（可选 — 可能尚不存在）
    let image_meta = conn
        .query_row(
            "SELECT item_id, orientation, exif_datetime, exif_make, exif_model, exif_lens,
                    exif_focal_length, exif_aperture, exif_shutter, exif_iso,
                    exif_gps_lat, exif_gps_lng,
                    dominant_hue, dominant_sat, dominant_lum, dominant_hex, is_monochrome
             FROM image_meta WHERE item_id=?1",
            params![id],
            |row| {
                Ok(ImageMeta {
                    item_id: row.get(0)?,
                    orientation: row.get(1)?,
                    exif_datetime: row.get(2)?,
                    exif_make: row.get(3)?,
                    exif_model: row.get(4)?,
                    exif_lens: row.get(5)?,
                    exif_focal_length: row.get(6)?,
                    exif_aperture: row.get(7)?,
                    exif_shutter: row.get(8)?,
                    exif_iso: row.get(9)?,
                    exif_gps_lat: row.get(10)?,
                    exif_gps_lng: row.get(11)?,
                    dominant_hue: row.get(12)?,
                    dominant_sat: row.get(13)?,
                    dominant_lum: row.get(14)?,
                    dominant_hex: row.get(15)?,
                    is_monochrome: row.get::<_, i64>(16)? != 0,
                })
            },
        )
        .ok();

    Ok(MediaDetail {
        item,
        abs_path,
        image_meta,
        availability,
    })
}

/// Query all layout items matching the given filter.
/// 查询与给定过滤器匹配的所有布局项。
/// Used by `compute_layout`.
/// 被 `compute_layout` 使用。
pub fn query_layout_items(
    conn: &Connection,
    filter: &MediaFilter,
    group_by: Option<&str>,
    sort_within: Option<&str>,
    sort_order: Option<&str>,
    _include_meta: bool, // retained for call-site compatibility; EXIF is no longer selected here
) -> Result<Vec<LayoutItem>> {
    let mut sql = String::from(
        "SELECT m.id, m.width, m.height, m.file_size, m.sort_datetime, m.file_format, m.media_type, m.is_live_photo,
                m.duration_ms, m.thumb_status, m.thumb_path, m.thumbhash, m.is_favorited,
                CASE WHEN d.rel_path = '' THEN r.path ELSE r.path || '/' || d.rel_path END as dir_path, d.name as dir_name, m.directory_id as dir_id, m.availability, m.rating, m.color_label, "
    );

    // similarity is the final SELECT column — heavy EXIF/GPS/file_name columns are
    // no longer selected here; they are fetched lazily via get_meta_for_viewport.
    // similarity 是最后一个 SELECT 列 — 重型 EXIF/GPS/文件名列不再在此查询，
    // 改为经 get_meta_for_viewport 按需拉取。
    if filter.ai_search == Some(true) {
        sql.push_str("ai.similarity\n");
    } else {
        sql.push_str("NULL as similarity\n");
    }

    let mut extras: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    // FROM/JOIN/WHERE/ORDER 主体抽到 push_query_body，与 view_to_sql（只取 id）共用同一构造，
    // 杜绝双套视图定义漂移（T18 §3.10.2 单一事实源）。
    push_query_body(
        &mut sql,
        &mut extras,
        filter,
        group_by,
        sort_within,
        sort_order,
    );

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = extras.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), map_layout_item)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// 构造画廊查询的「FROM/JOIN/WHERE/ORDER BY」主体（SELECT 列由调用方前置）。
///
/// `query_layout_items`（取完整 `LayoutItem`）与 `view_to_sql`（只取 `m.id`）共用本函数 —— 二者
/// 共享同一套 FROM/JOIN/WHERE/ORDER 构造，是视图定义的单一事实源（T18 §4）。
/// 全部谓词参数绑定（符合「SQL 必参数绑定」铁律）；`extras` 由调用方传入空 Vec、本函数追加。
fn push_query_body(
    sql: &mut String,
    extras: &mut Vec<Box<dyn rusqlite::ToSql>>,
    filter: &MediaFilter,
    group_by: Option<&str>,
    sort_within: Option<&str>,
    sort_order: Option<&str>,
) {
    sql.push_str("         FROM media_items m\n         JOIN directories d ON m.directory_id = d.id\n         JOIN scan_roots r ON d.root_id = r.id");

    // image_meta is only needed when a search scope filters on EXIF/GPS columns.
    // image_meta 仅在按 EXIF/GPS 列过滤的搜索范围下才需要连接。
    let mut needs_meta_join = false;
    if let Some(ref q) = filter.search_query {
        if !q.trim().is_empty() {
            let scope = filter.search_scope.as_deref().unwrap_or("filename");
            if scope == "device" || scope == "location" || scope == "global" {
                needs_meta_join = true;
            }
        }
    }

    if needs_meta_join {
        sql.push_str("\n         LEFT JOIN image_meta im ON m.id = im.item_id");
    }

    if filter.ai_search == Some(true) {
        sql.push_str("\n         JOIN ai_search_results ai ON m.id = ai.file_id");
    }

    if filter.trashed_only == Some(true) {
        sql.push_str("\n         WHERE m.is_deleted=1 AND m.companion_of IS NULL");
    } else {
        sql.push_str("\n         WHERE m.is_deleted=0 AND m.companion_of IS NULL");
    }

    let mut param_idx = 0usize;

    if filter.ai_search == Some(true) {
        if let Some(threshold) = filter.ai_threshold {
            param_idx += 1;
            // Match the frontend's visual rounding (e.g. Math.round(similarity * 100))
            sql.push_str(&format!(
                " AND ROUND(ai.similarity * 100.0) >= ?{param_idx}"
            ));
            extras.push(Box::new((threshold * 100.0).round()));
        }
    }

    if let Some(dir_id) = filter.directory_id {
        param_idx += 1;
        sql.push_str(&format!(
            " AND directory_id IN (
            WITH RECURSIVE dir_tree(id) AS (
                SELECT ?{param_idx}
                UNION ALL
                SELECT d.id FROM directories d
                JOIN dir_tree t ON d.parent_id = t.id
            )
            SELECT id FROM dir_tree
        )"
        ));
        extras.push(Box::new(dir_id));
    }

    if let Some(ref types) = filter.media_types {
        if !types.is_empty() {
            let placeholders: Vec<String> = types
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", param_idx + i + 1))
                .collect();
            sql.push_str(&format!(" AND media_type IN ({})", placeholders.join(",")));
            for t in types {
                extras.push(Box::new(t.clone()));
            }
            param_idx += types.len();
        }
    }

    if filter.favorited_only == Some(true) {
        sql.push_str(" AND is_favorited=1");
    }

    // User collection: restrict to its album_items membership. (System collections use
    // media_types + favorited_only above and never set album_id.)
    // 用户收藏夹：限定为其 album_items 成员。（系统夹用上面的 media_types + favorited_only，不设 album_id。）
    if let Some(album_id) = filter.album_id {
        param_idx += 1;
        sql.push_str(&format!(
            " AND m.id IN (SELECT item_id FROM album_items WHERE album_id = ?{param_idx})"
        ));
        extras.push(Box::new(album_id));
    }

    // Person (F6 people wall → person's photos): images containing a face in this cluster.
    // 人物（F6 人物墙 → 某人物的照片）：包含此簇人脸的图像。
    if let Some(person_id) = filter.person_id {
        param_idx += 1;
        sql.push_str(&format!(
            " AND m.id IN (SELECT item_id FROM faces WHERE person_id = ?{param_idx})"
        ));
        extras.push(Box::new(person_id));
    }

    if let Some(min_r) = filter.min_rating {
        param_idx += 1;
        sql.push_str(&format!(" AND rating >= ?{param_idx}"));
        extras.push(Box::new(min_r));
    }

    // 颜色标签：精确匹配某色档（min_rating 是 >=，颜色无序故取等值 =）。
    if let Some(cl) = filter.color_label {
        param_idx += 1;
        sql.push_str(&format!(" AND color_label = ?{param_idx}"));
        extras.push(Box::new(cl));
    }

    if let Some(ref dr) = filter.date_range {
        param_idx += 1;
        sql.push_str(&format!(" AND sort_datetime >= ?{param_idx}"));
        extras.push(Box::new(dr.from));
        param_idx += 1;
        sql.push_str(&format!(" AND sort_datetime <= ?{param_idx}"));
        extras.push(Box::new(dr.to));
    }

    if filter.live_photo_only == Some(true) {
        sql.push_str(" AND m.is_live_photo=1");
    }

    if filter.recent_only == Some(true) {
        sql.push_str(" AND m.created_at >= strftime('%s', 'now', '-30 days')");
    }

    if let Some(ref q) = filter.search_query {
        if !q.trim().is_empty() {
            let scope = filter.search_scope.as_deref().unwrap_or("filename");
            let pattern = format!("%{}%", q.trim());
            match scope {
                "folder" => {
                    param_idx += 1;
                    let p1 = format!("?{}", param_idx);
                    param_idx += 1;
                    let p2 = format!("?{}", param_idx);
                    sql.push_str(&format!(
                        " AND (d.rel_path LIKE {} OR d.name LIKE {})",
                        p1, p2
                    ));
                    extras.push(Box::new(pattern.clone()));
                    extras.push(Box::new(pattern));
                }
                "date" => {
                    param_idx += 1;
                    sql.push_str(&format!(" AND strftime('%Y-%m-%d %H:%M:%S', m.sort_datetime, 'unixepoch', 'localtime') LIKE ?{}", param_idx));
                    extras.push(Box::new(pattern));
                }
                "device" => {
                    param_idx += 1;
                    let p1 = format!("?{}", param_idx);
                    param_idx += 1;
                    let p2 = format!("?{}", param_idx);
                    param_idx += 1;
                    let p3 = format!("?{}", param_idx);
                    sql.push_str(&format!(" AND (im.exif_make LIKE {} OR im.exif_model LIKE {} OR im.exif_lens LIKE {})", p1, p2, p3));
                    extras.push(Box::new(pattern.clone()));
                    extras.push(Box::new(pattern.clone()));
                    extras.push(Box::new(pattern));
                }
                "location" => {
                    param_idx += 1;
                    let p1 = format!("?{}", param_idx);
                    param_idx += 1;
                    let p2 = format!("?{}", param_idx);
                    // 预留经纬度字符串或未来 city 字段匹配
                    sql.push_str(&format!(" AND (CAST(im.exif_gps_lat AS TEXT) LIKE {} OR CAST(im.exif_gps_lng AS TEXT) LIKE {})", p1, p2));
                    extras.push(Box::new(pattern.clone()));
                    extras.push(Box::new(pattern));
                }
                "global" => {
                    let mut p = vec![];
                    for _ in 0..9 {
                        param_idx += 1;
                        p.push(format!("?{}", param_idx));
                        extras.push(Box::new(pattern.clone()));
                    }
                    sql.push_str(&format!(
                        " AND (m.file_name LIKE {} OR d.rel_path LIKE {} OR d.name LIKE {} OR strftime('%Y-%m-%d %H:%M:%S', m.sort_datetime, 'unixepoch', 'localtime') LIKE {} OR im.exif_make LIKE {} OR im.exif_model LIKE {} OR im.exif_lens LIKE {} OR CAST(im.exif_gps_lat AS TEXT) LIKE {} OR CAST(im.exif_gps_lng AS TEXT) LIKE {})",
                        p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7], p[8]
                    ));
                }
                _ => {
                    // "filename"
                    param_idx += 1;
                    sql.push_str(&format!(" AND m.file_name LIKE ?{}", param_idx));
                    extras.push(Box::new(pattern));
                }
            }
        }
    }

    let order_dir = match sort_order {
        Some("asc") => "ASC",
        _ => "DESC",
    };

    if group_by == Some("folder") {
        if sort_within == Some("similarity") && filter.ai_search == Some(true) {
            sql.push_str(&format!(
                " ORDER BY d.rel_path ASC, ai.similarity {}",
                order_dir
            ));
        } else if sort_within == Some("filename") {
            sql.push_str(&format!(
                " ORDER BY d.rel_path ASC, m.file_name COLLATE NATURAL_CMP {}",
                order_dir
            ));
        } else {
            sql.push_str(&format!(
                " ORDER BY d.rel_path ASC, m.sort_datetime {}",
                order_dir
            ));
        }
    } else if group_by == Some("date") {
        let date_expr = "date(m.sort_datetime, 'unixepoch', 'localtime')";
        if sort_within == Some("similarity") && filter.ai_search == Some(true) {
            sql.push_str(&format!(
                " ORDER BY {} {}, ai.similarity {}",
                date_expr, order_dir, order_dir
            ));
        } else if sort_within == Some("filename") {
            sql.push_str(&format!(
                " ORDER BY {} {}, m.file_name COLLATE NATURAL_CMP {}",
                date_expr, order_dir, order_dir
            ));
        } else {
            sql.push_str(&format!(" ORDER BY m.sort_datetime {}", order_dir));
        }
    } else {
        if sort_within == Some("similarity") && filter.ai_search == Some(true) {
            sql.push_str(&format!(" ORDER BY ai.similarity {}", order_dir));
        } else if sort_within == Some("filename") {
            sql.push_str(&format!(
                " ORDER BY m.file_name COLLATE NATURAL_CMP {}",
                order_dir
            ));
        } else {
            sql.push_str(&format!(" ORDER BY m.sort_datetime {}", order_dir));
        }
    }

    // 确定性 tiebreaker：上面所有分支的末键都按 order_dir 排，统一追加 m.id 同向次键——
    // 同秒/同名/同相似度时稳定序（消除布局抖动），并让默认 sort_datetime DESC 分支吃满
    // 复合索引 idx_media_sort(sort_datetime DESC, id DESC)。索引≠tiebreaker，两处都改（§3.5）。
    sql.push_str(&format!(", m.id {}", order_dir));
}

/// 把 `ViewDescriptor` 编译为「只取 `m.id`」的 SQL + 绑定参数，与 `query_layout_items` 共用
/// `push_query_body` 主体构造（单一事实源）。供 `resolve_selection` / `count_selection` /
/// `get_view_ids` 按当前 filter 在 SQL 层取全集 id，**不把百万 id 灌前端 / 不经 IPC 整包传**。
///
/// `SemanticSearch` scope 是有序、非纯 SQL 的例外（v1 由 ai_search 既有路径承载），此处显式不支持。
pub fn view_to_sql(view: &ViewDescriptor) -> Result<(String, Vec<Box<dyn rusqlite::ToSql>>)> {
    if let ViewScope::SemanticSearch { .. } = view.scope {
        return Err(AppError::Internal(
            "view_to_sql: SemanticSearch scope 不支持纯 SQL 解析（v1 走 ai_search 路径）".into(),
        ));
    }
    let filter = view.to_media_filter();
    let mut sql = String::from("SELECT m.id ");
    let mut extras: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    push_query_body(
        &mut sql,
        &mut extras,
        &filter,
        Some(view.sort.group_by.as_str()),
        Some(view.sort.sort_within_group.as_str()),
        Some(view.sort.sort_order.as_str()),
    );
    Ok((sql, extras))
}

/// 选择规模上限（T18 D4 初值，可调）。
const SELECTION_EXPLICIT_MAX: usize = 100_000;
/// 批量写分块大小（S3 消费 + count 交集分块）：平衡单事务大小与往返次数。
pub const SELECTION_BATCH_CHUNK: usize = 5_000;

/// 把 `SelectionDescriptor` 展开为实际 id 集合（按视图布局序）。
///
/// - `Explicit{ids}`：上限校验后原样返回。
/// - `SelectAll{view, excluded}`：先以 `current_layout_version` 守门（不一致 → `ViewStale`），
///   再经 `view_to_sql` 流式取全集 id，扣除 `excluded_ids`（HashSet 过滤）。
///
/// `current_layout_version` 由调用方（IPC 命令）从 `AppState` 的 LayoutCache 读出传入 —— 本 DB 层
/// 不依赖全局状态，保持纯函数可测。
pub fn resolve_selection(
    conn: &Connection,
    sel: &SelectionDescriptor,
    current_layout_version: u64,
) -> Result<Vec<i64>> {
    match sel {
        SelectionDescriptor::Explicit { ids } => {
            if ids.len() > SELECTION_EXPLICIT_MAX {
                return Err(AppError::Internal(format!(
                    "显式选择 {} 项超过上限 {SELECTION_EXPLICIT_MAX}，请改用全选（SelectAll）",
                    ids.len()
                )));
            }
            Ok(ids.clone())
        }
        SelectionDescriptor::SelectAll { view, excluded_ids } => {
            if view.layout_version != current_layout_version {
                return Err(AppError::ViewStale);
            }
            let (sql, params) = view_to_sql(view)?;
            let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(refs.as_slice(), |row| row.get::<_, i64>(0))?;

            if excluded_ids.is_empty() {
                return rows.map(|r| r.map_err(AppError::from)).collect();
            }
            // excluded 通常很小，HashSet 过滤即可（无需把 NOT IN 灌进 SQL）。
            let excluded: std::collections::HashSet<i64> = excluded_ids.iter().copied().collect();
            let mut out = Vec::new();
            for r in rows {
                let id = r?;
                if !excluded.contains(&id) {
                    out.push(id);
                }
            }
            Ok(out)
        }
    }
}

/// 仅计数 `SelectionDescriptor`（UI「将操作 N 项」），SelectAll 走 `COUNT(*)` 不取全 id。
///
/// 精确计数（T18 D3）：`COUNT(*) − (excluded ∩ view)`。不近似为 `total − excluded.len()`，因
/// `excluded` 可能含已不在视图的 id；分块统计其与视图的真实交集，保「已选 N 项」与实际一致。
pub fn count_selection(
    conn: &Connection,
    sel: &SelectionDescriptor,
    current_layout_version: u64,
) -> Result<u64> {
    match sel {
        SelectionDescriptor::Explicit { ids } => {
            if ids.len() > SELECTION_EXPLICIT_MAX {
                return Err(AppError::Internal(format!(
                    "显式选择 {} 项超过上限 {SELECTION_EXPLICIT_MAX}，请改用全选（SelectAll）",
                    ids.len()
                )));
            }
            Ok(ids.len() as u64)
        }
        SelectionDescriptor::SelectAll { view, excluded_ids } => {
            if view.layout_version != current_layout_version {
                return Err(AppError::ViewStale);
            }
            let (view_sql, params) = view_to_sql(view)?;
            // 包成 COUNT(*) 子查询：内层 ?1..?k 仍按位绑定 params。子查询 ORDER BY 对 COUNT 无意义
            // 但无害（v1 容忍微小浪费）。
            let count_sql = format!("SELECT COUNT(*) FROM ({view_sql})");
            let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
            let total: i64 = conn.query_row(&count_sql, refs.as_slice(), |row| row.get(0))?;

            if excluded_ids.is_empty() {
                return Ok(total as u64);
            }
            // 交集（excluded ∩ view）：分块 IN 统计（占位从 params.len()+1 起，接续内层绑定）。
            let mut intersect: i64 = 0;
            for chunk in excluded_ids.chunks(SELECTION_BATCH_CHUNK) {
                let placeholders: Vec<String> = (0..chunk.len())
                    .map(|i| format!("?{}", params.len() + i + 1))
                    .collect();
                let in_sql = format!(
                    "SELECT COUNT(*) FROM ({view_sql}) WHERE id IN ({})",
                    placeholders.join(",")
                );
                let mut all_refs: Vec<&dyn rusqlite::ToSql> =
                    params.iter().map(|b| b.as_ref()).collect();
                for v in chunk {
                    all_refs.push(v as &dyn rusqlite::ToSql);
                }
                let c: i64 = conn.query_row(&in_sql, all_refs.as_slice(), |row| row.get(0))?;
                intersect += c;
            }
            Ok((total - intersect).max(0) as u64)
        }
    }
}

/// R1-2（S4 消费）：对 id 集分块执行「单值 SET」批量 UPDATE，整体包在一个事务内。
///
/// 分块（[`SELECTION_BATCH_CHUNK`]）的原因：SelectAll 解析出的 id 可达百万级，拼进单条
/// IN 会超 SQLite 绑定变量上限；分块后事务仍保证整体原子。`set_expr` 为 SET 片段
/// （其值绑定为 ?1，id 占位从 ?2 起逐块生成）——仅拼接**编译期常量片段与占位符**，
/// 值一律参数绑定（项目 SQL 红线）。返回受影响总行数。
fn batch_update_set_value(
    conn: &Connection,
    set_expr: &str,
    value: rusqlite::types::Value,
    ids: &[i64],
) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    let mut affected: u64 = 0;
    for chunk in ids.chunks(SELECTION_BATCH_CHUNK) {
        let placeholders: Vec<String> = (0..chunk.len()).map(|i| format!("?{}", i + 2)).collect();
        let sql = format!(
            "UPDATE media_items SET {set_expr} WHERE id IN ({}) AND is_deleted = 0",
            placeholders.join(",")
        );
        let mut params: Vec<rusqlite::types::Value> = vec![value.clone()];
        for id in chunk {
            params.push(rusqlite::types::Value::Integer(*id));
        }
        affected += tx.execute(&sql, rusqlite::params_from_iter(params.iter()))? as u64;
    }
    tx.commit()?;
    Ok(affected)
}

/// 批量设收藏（R1-2：IPC 命令解析 SelectionDescriptor 后落到此，SQL 收拢回 db 层）。
pub fn batch_set_favorite(conn: &Connection, ids: &[i64], value: bool) -> Result<u64> {
    batch_update_set_value(
        conn,
        "is_favorited = ?1",
        rusqlite::types::Value::Integer(i64::from(value)),
        ids,
    )
}

/// 批量设评分（0-5，越界钳制）。镜像 [`batch_set_favorite`]。
pub fn batch_set_rating(conn: &Connection, ids: &[i64], rating: i64) -> Result<u64> {
    batch_update_set_value(
        conn,
        "rating = ?1, updated_at = strftime('%s','now')",
        rusqlite::types::Value::Integer(rating.clamp(0, 5)),
        ids,
    )
}

/// 批量设颜色标签（0=清除 / 1-7 色档，越界钳制）。镜像 [`batch_set_favorite`]。
pub fn batch_set_color_label(conn: &Connection, ids: &[i64], color_label: i64) -> Result<u64> {
    batch_update_set_value(
        conn,
        "color_label = ?1, updated_at = strftime('%s','now')",
        rusqlite::types::Value::Integer(color_label.clamp(0, 7)),
        ids,
    )
}

/// Fetch heavy per-item metadata (file name, dir path, EXIF, GPS) for a set of ids.
/// Backs `get_meta_for_viewport`, which lazily populates only the visible window.
///
/// 为一组 id 批量获取重型逐项元数据（文件名、目录路径、EXIF、GPS）。
/// 支撑 `get_meta_for_viewport` —— 仅懒填充可视窗口。
pub fn get_media_meta_batch(conn: &Connection, ids: &[i64]) -> Result<Vec<MediaMeta>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    // ids are i64 — safe to inline; avoids SQLite's bound-parameter limit on large windows.
    // id 为 i64 — 内联安全；规避大窗口下 SQLite 的绑定参数数量上限。
    let in_clause = ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT m.id,
                m.file_name,
                CASE WHEN d.rel_path = '' THEN r.path ELSE r.path || '/' || d.rel_path END AS dir_path,
                im.exif_gps_lat, im.exif_gps_lng,
                im.exif_make, im.exif_model, im.exif_lens,
                im.exif_focal_length, im.exif_aperture, im.exif_shutter, im.exif_iso
         FROM media_items m
         JOIN directories d ON m.directory_id = d.id
         JOIN scan_roots r ON d.root_id = r.id
         LEFT JOIN image_meta im ON m.id = im.item_id
         WHERE m.id IN ({in_clause})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(MediaMeta {
            id: row.get(0)?,
            file_name: row.get(1)?,
            dir_path: row.get(2)?,
            gps_lat: row.get(3)?,
            gps_lng: row.get(4)?,
            exif_make: row.get(5)?,
            exif_model: row.get(6)?,
            exif_lens: row.get(7)?,
            exif_focal_length: row.get(8)?,
            exif_aperture: row.get(9)?,
            exif_shutter: row.get(10)?,
            exif_iso: row.get(11)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

// 主缩略图 pending 查询统一排除「有未完成 exotic thumbnail 任务」的项（v3 §6.2 / Part1 §2.2）。
// 与 §2.4 给 CLIP/face 的门控同一模式：exotic 项由 Worker 流水线出图，绝不进主 generator
// （主解码引擎无法解码 PSD 等冷门格式，否则会 UnsupportedFormat / 写 thumb_status=2）。
const NOT_BLOCKED_BY_EXOTIC: &str = "AND NOT EXISTS (
        SELECT 1 FROM exotic_tasks et
        WHERE et.item_id = media_items.id AND et.capability='thumbnail' AND et.status<>2)";

/// 同上，但用于以 `m` 为 media_items 别名的查询（CLIP/face/derive 生产者，§2.4）。
/// exotic 已认领 thumbnail 的 item 在其完成前，不进 CLIP/人脸/主派生；完成后这些流水线
/// 优先用生成的 thumb_path（WebP），避免再尝试解码原始 PSD。
const NOT_BLOCKED_BY_EXOTIC_M: &str = "AND NOT EXISTS (
        SELECT 1 FROM exotic_tasks et
        WHERE et.item_id = m.id AND et.capability='thumbnail' AND et.status<>2)";

pub fn get_pending_thumb_items(conn: &Connection, limit: i64) -> Result<Vec<(i64, i64)>> {
    let sql = format!(
        "SELECT id, cache_key FROM media_items
         WHERE thumb_status=0 AND is_deleted=0 {NOT_BLOCKED_BY_EXOTIC}
         ORDER BY created_at DESC
         LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn get_all_pending_thumb_ids(conn: &Connection) -> Result<Vec<i64>> {
    let sql = format!(
        "SELECT id FROM media_items
         WHERE thumb_status=0 AND is_deleted=0 {NOT_BLOCKED_BY_EXOTIC}
         ORDER BY created_at DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn count_pending_thumb_items(conn: &Connection) -> Result<i64> {
    let sql = format!(
        "SELECT COUNT(*) FROM media_items WHERE thumb_status=0 AND is_deleted=0 {NOT_BLOCKED_BY_EXOTIC}"
    );
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(AppError::from)
}

/// 批量取一组 item 的 thumbnail exotic 任务状态（缩略图 Router 用，避免逐项查询 N+1，R7）。
/// 返回 item_id → 状态；无任务的 item 不在表中。
pub fn exotic_thumbnail_task_status_for_items(
    conn: &Connection,
    item_ids: &[i64],
) -> Result<std::collections::HashMap<i64, ExoticTaskStatus>> {
    let mut map = std::collections::HashMap::new();
    if item_ids.is_empty() {
        return Ok(map);
    }
    let placeholders = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT item_id, status FROM exotic_tasks
         WHERE capability='thumbnail' AND item_id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(item_ids), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    for r in rows.flatten() {
        if let Some(st) = ExoticTaskStatus::from_i64(r.1) {
            map.insert(r.0, st);
        }
    }
    Ok(map)
}

/// thumbnail 任务的路由信息（status + done 任务的指纹/worker 版本）。缩略图入口用它重算期望指纹、
/// 失效「指纹已变的 done」（R5/R6，问题4）——尤其用户改缩略图档位后，旧档 done 不再匹配新请求指纹。
pub struct ExoticThumbRouteInfo {
    pub status: ExoticTaskStatus,
    pub input_fingerprint: Option<String>,
    pub worker_version: Option<String>,
}

/// 批量取一组 item 的 thumbnail 任务路由信息（避免 N+1，R7）。无任务的 item 不在表中。
/// 比 `exotic_thumbnail_task_status_for_items` 多取指纹/worker 版本，供 Router 做指纹有效性判定。
pub fn exotic_thumbnail_route_info_for_items(
    conn: &Connection,
    item_ids: &[i64],
) -> Result<std::collections::HashMap<i64, ExoticThumbRouteInfo>> {
    let mut map = std::collections::HashMap::new();
    if item_ids.is_empty() {
        return Ok(map);
    }
    let placeholders = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT item_id, status, input_fingerprint, worker_version FROM exotic_tasks
         WHERE capability='thumbnail' AND item_id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(item_ids), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    for (item_id, status, fp, wv) in rows.flatten() {
        if let Some(st) = ExoticTaskStatus::from_i64(status) {
            map.insert(
                item_id,
                ExoticThumbRouteInfo {
                    status: st,
                    input_fingerprint: fp,
                    worker_version: wv,
                },
            );
        }
    }
    Ok(map)
}

pub fn update_thumb_result(
    conn: &Connection,
    item_id: i64,
    status: i64,
    path: Option<&str>,
    thumbhash: Option<&[u8]>,
) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET thumb_status=?1, thumb_path=?2, thumbhash=?3,
                 updated_at=strftime('%s','now')
         WHERE id=?4",
        params![status, path, thumbhash, item_id],
    )?;
    Ok(())
}

pub fn get_thumb_by_item_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<ThumbResult>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT m.id,
                CASE
                    WHEN m.thumb_path IS NULL THEN 3
                    ELSE m.thumb_status
                END,
                CASE
                    WHEN m.thumb_status = 3 OR m.thumb_path IS NULL THEN
                        CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                             ELSE r.path || '/' || d.rel_path || '/' || m.file_name
                        END
                    ELSE m.thumb_path
                END,
                m.thumbhash
         FROM media_items m
         JOIN directories d ON m.directory_id = d.id
         JOIN scan_roots r ON d.root_id = r.id
         WHERE m.id IN ({})",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(ThumbResult {
            item_id: row.get(0)?,
            thumb_status: row.get(1)?,
            thumb_path: row.get(2)?,
            thumbhash: row.get(3)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

// ── Image meta upsert ────────────────────────────────────────────────────────
// ── 图像元数据更新插入 ────────────────────────────────────────────────────────

pub fn upsert_image_meta(conn: &Connection, meta: &ImageMeta) -> Result<()> {
    conn.execute(
        "INSERT INTO image_meta
             (item_id, orientation, exif_datetime, exif_make, exif_model, exif_lens,
              exif_focal_length, exif_aperture, exif_shutter, exif_iso,
              exif_gps_lat, exif_gps_lng)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
         ON CONFLICT(item_id) DO UPDATE SET
             orientation=excluded.orientation,
             exif_datetime=excluded.exif_datetime,
             exif_make=excluded.exif_make,
             exif_model=excluded.exif_model,
             exif_lens=excluded.exif_lens,
             exif_focal_length=excluded.exif_focal_length,
             exif_aperture=excluded.exif_aperture,
             exif_shutter=excluded.exif_shutter,
             exif_iso=excluded.exif_iso,
             exif_gps_lat=excluded.exif_gps_lat,
             exif_gps_lng=excluded.exif_gps_lng",
        params![
            meta.item_id,
            meta.orientation,
            meta.exif_datetime,
            meta.exif_make,
            meta.exif_model,
            meta.exif_lens,
            meta.exif_focal_length,
            meta.exif_aperture,
            meta.exif_shutter,
            meta.exif_iso,
            meta.exif_gps_lat,
            meta.exif_gps_lng
        ],
    )?;
    Ok(())
}

pub fn update_sort_datetime(conn: &Connection, item_id: i64, dt: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET sort_datetime=?1, updated_at=strftime('%s','now') WHERE id=?2",
        params![dt, item_id],
    )?;
    Ok(())
}

/// Backfill real pixel dimensions for an item that was inserted with a 0×0
/// placeholder during the fast scan. Guarded by `width=0 OR height=0` so it
/// never re-touches (and never double-flips) items that already have real,
/// orientation-corrected dimensions from the eager fast-scan path.
/// 为快速扫描时以 0×0 占位插入的项补全真实像素尺寸。以 `width=0 OR height=0`
/// 作为条件守卫，绝不重写（也绝不双重翻转）已在即时路径得到真实、方向校正尺寸的项。
pub fn update_media_dimensions(
    conn: &Connection,
    item_id: i64,
    width: i64,
    height: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET width=?1, height=?2, updated_at=strftime('%s','now')
         WHERE id=?3 AND (width=0 OR height=0)",
        params![width, height, item_id],
    )?;
    Ok(())
}

// ── Video meta (§2.1 / §3.2) ──────────────────────────────────────────────────
// ── 视频元数据（§2.1 / §3.2）──────────────────────────────────────────────────

/// Overwrite a video's placeholder (16:9) size with the MF-probed **display** dims (rotation
/// applied) + duration. Unlike `update_media_dimensions` there is NO `0×0` guard: videos are
/// inserted with a 16:9 placeholder (not 0×0), so we always backfill the true aspect once
/// probed — layout strongly depends on this (§3.2).
/// 用 MF 探测得到的**显示**尺寸（已应用旋转）+ 时长覆盖视频的占位（16:9）尺寸。与
/// `update_media_dimensions` 不同，这里无 `0×0` 守卫：视频以 16:9 占位（非 0×0）入库，
/// 故探测后总是回填真实比例 —— 布局强依赖之（§3.2）。
pub fn update_video_dimensions(
    conn: &Connection,
    item_id: i64,
    width: i64,
    height: i64,
    duration_ms: Option<i64>,
) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET width=?1, height=?2, duration_ms=?3, updated_at=strftime('%s','now')
         WHERE id=?4 AND media_type='video'",
        params![width, height, duration_ms, item_id],
    )?;
    Ok(())
}

/// Upsert a `video_meta` row (codec/fps/bitrate/rotation/has_audio). `cover_time_ms` is left
/// untouched (set later by the cover derivation path if needed).
/// 更新插入一行 `video_meta`（编解码/帧率/比特率/旋转/是否含音频）。`cover_time_ms` 不动
/// （如需由封面派生路径后续设置）。
pub fn upsert_video_meta(
    conn: &Connection,
    item_id: i64,
    codec: Option<&str>,
    fps: Option<f64>,
    bitrate: Option<i64>,
    rotation: i64,
    has_audio: bool,
) -> Result<()> {
    conn.execute(
        "INSERT INTO video_meta (item_id, video_codec, fps, bitrate, rotation, has_audio)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(item_id) DO UPDATE SET
            video_codec = excluded.video_codec,
            fps         = excluded.fps,
            bitrate     = excluded.bitrate,
            rotation    = excluded.rotation,
            has_audio   = excluded.has_audio",
        params![item_id, codec, fps, bitrate, rotation, has_audio as i64],
    )?;
    Ok(())
}

/// Videos in `root_id` that still lack a `video_meta` row — the enrichment work queue for the
/// MF probe pass (dimensions/rotation/duration). Returns `(id, abs_path, file_format)`.
/// `root_id` 下仍缺 `video_meta` 行的视频 —— MF 探测补全（宽高/旋转/时长）的工作队列。
/// 返回 `(id, abs_path, file_format)`。
pub fn get_videos_needing_meta(
    conn: &Connection,
    root_id: i64,
    limit: i64,
) -> Result<Vec<(i64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT m.id,
                CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                     ELSE r.path || '/' || d.rel_path || '/' || m.file_name
                END,
                m.file_format
         FROM media_items m
         JOIN directories d ON d.id = m.directory_id
         JOIN scan_roots r ON r.id = d.root_id
         LEFT JOIN video_meta vm ON vm.item_id = m.id
         WHERE d.root_id = ?1 AND m.is_deleted = 0 AND m.media_type = 'video' AND vm.item_id IS NULL
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![root_id, limit], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

// ── Audio meta (§3.6) ──────────────────────────────────────────────────────────
// ── 音频元数据（§3.6）──────────────────────────────────────────────────────────

/// Audio items in `root_id` that still lack an `audio_meta` row — the enrichment work queue
/// for the lofty tag/lyrics pass. Returns `(id, abs_path, file_format)`.
/// `root_id` 下仍缺 `audio_meta` 行的音频 —— lofty 标签/歌词补全的工作队列。
/// 返回 `(id, abs_path, file_format)`。
pub fn get_audios_needing_meta(
    conn: &Connection,
    root_id: i64,
    limit: i64,
) -> Result<Vec<(i64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT m.id,
                CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                     ELSE r.path || '/' || d.rel_path || '/' || m.file_name
                END,
                m.file_format
         FROM media_items m
         JOIN directories d ON d.id = m.directory_id
         JOIN scan_roots r ON r.id = d.root_id
         LEFT JOIN audio_meta am ON am.item_id = m.id
         WHERE d.root_id = ?1 AND m.is_deleted = 0 AND m.media_type = 'audio' AND am.item_id IS NULL
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![root_id, limit], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Upsert an `audio_meta` row (codec/artist/album/title/track/year/genre + lyrics provenance).
/// 更新插入一行 `audio_meta`（编解码/艺术家/专辑/标题/音轨/年份/流派 + 歌词来源）。
#[allow(clippy::too_many_arguments)]
pub fn upsert_audio_meta(
    conn: &Connection,
    item_id: i64,
    codec: Option<&str>,
    artist: Option<&str>,
    album: Option<&str>,
    title: Option<&str>,
    track_no: Option<i64>,
    year: Option<i64>,
    genre: Option<&str>,
    lyrics_source: Option<&str>,
    lyrics_path: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO audio_meta
             (item_id, audio_codec, artist, album_title, track_title,
              track_no, year, genre, lyrics_source, lyrics_path)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(item_id) DO UPDATE SET
             audio_codec   = excluded.audio_codec,
             artist        = excluded.artist,
             album_title   = excluded.album_title,
             track_title   = excluded.track_title,
             track_no      = excluded.track_no,
             year          = excluded.year,
             genre         = excluded.genre,
             lyrics_source = excluded.lyrics_source,
             lyrics_path   = excluded.lyrics_path",
        params![
            item_id,
            codec,
            artist,
            album,
            title,
            track_no,
            year,
            genre,
            lyrics_source,
            lyrics_path
        ],
    )?;
    Ok(())
}

/// Read an item's `audio_meta` row (the persisted tag subset), if present (§3.6).
/// 读取某项的 `audio_meta` 行（持久化的标签子集），若存在（§3.6）。
pub fn get_audio_meta(conn: &Connection, item_id: i64) -> Result<Option<AudioMeta>> {
    conn.query_row(
        "SELECT item_id, audio_codec, artist, album_title, track_title,
                track_no, year, genre, lyrics_source, lyrics_path
         FROM audio_meta WHERE item_id = ?1",
        params![item_id],
        |row| {
            Ok(AudioMeta {
                item_id: row.get(0)?,
                audio_codec: row.get(1)?,
                artist: row.get(2)?,
                album_title: row.get(3)?,
                track_title: row.get(4)?,
                track_no: row.get(5)?,
                year: row.get(6)?,
                genre: row.get(7)?,
                lyrics_source: row.get(8)?,
                lyrics_path: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(AppError::from)
}

// ── Storage backends (network drives, §3.8 8B) ─────────────────────────────────
// ── 存储后端（网络盘，§3.8 8B）─────────────────────────────────────────────────

fn map_storage_backend(row: &Row<'_>) -> rusqlite::Result<StorageBackendInfo> {
    let cred_ref: Option<String> = row.get(6)?;
    Ok(StorageBackendInfo {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        host: row.get(3)?,
        base_path: row.get(4)?,
        username: row.get(5)?,
        has_password: cred_ref.is_some(),
        created_at: row.get(7)?,
    })
}

/// List all configured storage backends (§3.8). Passwords are never returned (only `has_password`).
/// 列出所有已配置的存储后端（§3.8）。密码绝不返回（仅 `has_password`）。
pub fn list_storage_backends(conn: &Connection) -> Result<Vec<StorageBackendInfo>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, name, host, base_path, username, cred_ref, created_at
         FROM storage_backends ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], map_storage_backend)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Read one backend's full config (incl. `cred_ref`) for building a `StorageBackend` (§3.8).
/// 读取某后端的完整配置（含 `cred_ref`）以构建 `StorageBackend`（§3.8）。
/// 返回 `(kind, host, base_path, username, cred_ref)`。
// 返回元组已在上方 doc 注明各字段语义，构建 StorageBackend 一次性消费，抽别名收益有限。
#[allow(clippy::type_complexity)]
pub fn get_storage_backend_config(
    conn: &Connection,
    id: i64,
) -> Result<
    Option<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )>,
> {
    conn.query_row(
        "SELECT kind, host, base_path, username, cred_ref FROM storage_backends WHERE id = ?1",
        params![id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )
    .optional()
    .map_err(AppError::from)
}

/// Insert a storage backend (password stored separately in keyring; only `cred_ref` here). §3.8.
/// 插入一个存储后端（密码另存 keyring；此处仅 `cred_ref`）。§3.8。
#[allow(clippy::too_many_arguments)]
pub fn insert_storage_backend(
    conn: &Connection,
    kind: &str,
    name: &str,
    host: Option<&str>,
    base_path: Option<&str>,
    username: Option<&str>,
    cred_ref: Option<&str>,
    options: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO storage_backends (kind, name, host, base_path, username, cred_ref, options)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![kind, name, host, base_path, username, cred_ref, options],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Delete a storage backend by id; returns its `cred_ref` so the caller can purge the keyring. §3.8.
/// 按 id 删除存储后端；返回其 `cred_ref` 以便调用方清理 keyring。§3.8。
pub fn delete_storage_backend(conn: &Connection, id: i64) -> Result<Option<String>> {
    let cred_ref: Option<String> = conn
        .query_row(
            "SELECT cred_ref FROM storage_backends WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    conn.execute("DELETE FROM storage_backends WHERE id = ?1", params![id])?;
    Ok(cred_ref)
}

/// Absolute path + extension for an item that STILL has placeholder (0×0)
/// dimensions; `None` if it's missing or already measured. Backs the
/// viewport-priority dimension extraction.
/// 仍为占位(0×0)尺寸的项的绝对路径+扩展名；若不存在或已测量则为 `None`。
/// 支撑可视窗口优先取尺寸。
pub fn get_placeholder_item_path(conn: &Connection, id: i64) -> Result<Option<(String, String)>> {
    conn.query_row(
        "SELECT r.path, d.rel_path, m.file_name, m.file_format
         FROM media_items m
         JOIN directories d ON d.id = m.directory_id
         JOIN scan_roots  r ON r.id = d.root_id
         WHERE m.id = ?1 AND (m.width = 0 OR m.height = 0)",
        params![id],
        |row| {
            let root: String = row.get(0)?;
            let rel: String = row.get(1)?;
            let name: String = row.get(2)?;
            let ext: String = row.get(3)?;
            Ok((resolve_media_path(&root, &rel, &name), ext))
        },
    )
    .optional()
    .map_err(AppError::from)
}

pub fn update_live_photo_flags(
    conn: &Connection,
    item_id: i64,
    is_live: bool,
    has_embedded: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET is_live_photo=?1, has_embedded_video=?2,
                 updated_at=strftime('%s','now')
         WHERE id=?3",
        params![is_live as i64, has_embedded as i64, item_id],
    )?;
    Ok(())
}

pub fn set_companion_of(conn: &Connection, companion_id: i64, main_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET companion_of=?1, updated_at=strftime('%s','now') WHERE id=?2",
        params![main_id, companion_id],
    )?;
    Ok(())
}

// ── Favourites / Rating / Soft-delete ────────────────────────────────────────
// ── 收藏 / 评分 / 软删除 ────────────────────────────────────────

pub fn toggle_favorite(conn: &Connection, item_id: i64) -> Result<bool> {
    conn.execute(
        "UPDATE media_items SET is_favorited = NOT is_favorited,
                 updated_at=strftime('%s','now')
         WHERE id=?1",
        params![item_id],
    )?;
    let new_val: i64 = conn.query_row(
        "SELECT is_favorited FROM media_items WHERE id=?1",
        params![item_id],
        |row| row.get(0),
    )?;
    Ok(new_val != 0)
}

pub fn set_rating(conn: &Connection, item_id: i64, rating: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET rating=?1, updated_at=strftime('%s','now') WHERE id=?2",
        params![rating, item_id],
    )?;
    Ok(())
}

/// 设置颜色标签（0=无，1-7 色档；调用方负责 clamp）。镜像 `set_rating`（T16）。
pub fn set_color_label(conn: &Connection, item_id: i64, color_label: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET color_label=?1, updated_at=strftime('%s','now') WHERE id=?2",
        params![color_label, item_id],
    )?;
    Ok(())
}

/// 把一组 id 扩展为「自身 ∪ 其 Live Photo companion」（T18 §6.1，结构性写操作——删/移/恢复——专用）。
///
/// Live Photo 的 mov/mp4 伴随项以 `companion_of` 指向静图、在画廊**不独立显示**；删/移/恢复静图时
/// 必须连带处理其伴随项，否则伴随文件成孤儿（D5 取证：`soft_delete_items` 此前不展开 → 现存 bug）。
/// 评分/收藏等元数据操作**不**调用本函数（companion 不单独评分）。分块查 companion 避免单条巨 IN。
pub fn expand_companions(conn: &Connection, ids: &[i64]) -> Result<Vec<i64>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let mut out: Vec<i64> = ids.to_vec();
    let mut seen: std::collections::HashSet<i64> = ids.iter().copied().collect();
    for chunk in ids.chunks(SELECTION_BATCH_CHUNK) {
        let placeholders: Vec<String> = (0..chunk.len()).map(|i| format!("?{}", i + 1)).collect();
        let sql = format!(
            "SELECT id FROM media_items WHERE companion_of IN ({})",
            placeholders.join(",")
        );
        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> =
            chunk.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(refs.as_slice(), |r| r.get::<_, i64>(0))?;
        for r in rows {
            let cid = r?;
            // seen 去重：companion 不会与输入 id 重复，但多个静图可能指向同一伴随项（防御）。
            if seen.insert(cid) {
                out.push(cid);
            }
        }
    }
    Ok(out)
}

pub fn soft_delete_items(conn: &Connection, item_ids: &[i64]) -> Result<()> {
    if item_ids.is_empty() {
        return Ok(());
    }
    // D5：连带其 Live Photo companion，避免删静图留下孤儿伴随视频。
    let ids = expand_companions(conn, item_ids)?;
    let tx = conn.unchecked_transaction()?;
    for &id in &ids {
        tx.execute(
            "UPDATE media_items SET is_deleted=1, deleted_at=strftime('%s','now'),
                     updated_at=strftime('%s','now')
             WHERE id=?1",
            params![id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn restore_items(conn: &Connection, item_ids: &[i64]) -> Result<()> {
    if item_ids.is_empty() {
        return Ok(());
    }
    // D5：恢复静图时对称地连带恢复其 companion。
    let ids = expand_companions(conn, item_ids)?;
    let tx = conn.unchecked_transaction()?;
    for &id in &ids {
        tx.execute(
            "UPDATE media_items SET is_deleted=0, deleted_at=NULL,
                     updated_at=strftime('%s','now')
             WHERE id=?1",
            params![id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn get_trash(conn: &Connection, offset: i64, limit: i64) -> Result<Vec<MediaItem>> {
    let mut stmt = conn.prepare(
        "SELECT id, directory_id, file_name, file_size, file_mtime, file_format,
                media_type, width, height, duration_ms, sort_datetime, cache_key,
                thumb_status, thumb_path, thumbhash, is_favorited, is_deleted,
                deleted_at, rating, is_live_photo, has_embedded_video, companion_of,
                content_hash, created_at, updated_at, color_label
         FROM media_items WHERE is_deleted=1
         ORDER BY deleted_at DESC, id DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], map_media_item)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// 回收站 keyset seek 翻页（取代 OFFSET：百万行 `OFFSET 1e6` 要扫过百万行，keyset 恒定 <5ms）。
/// `cursor` = 上一页**最后一项**的 `(deleted_at, id)`，首页传 `None`；复合序 `(deleted_at DESC, id DESC)`，
/// 走 `idx_media_trash`。SQLite 行值比较 `(a,b) < (c,d)` 原生支持。
/// 注：回收站项的 `deleted_at` 在软删时即写入（非空），故 keyset 比较不需 COALESCE（保索引可用）。
pub fn get_trash_keyset(
    conn: &Connection,
    cursor: Option<(i64, i64)>,
    limit: i64,
) -> Result<Vec<MediaItem>> {
    // has_cursor=0 时 OR 短路放行全部（首页）；=1 时按行值游标 seek 下一页。
    let (cur_da, cur_id, has_cursor): (i64, i64, i64) = match cursor {
        Some((da, id)) => (da, id, 1),
        None => (0, 0, 0),
    };
    let mut stmt = conn.prepare(
        "SELECT id, directory_id, file_name, file_size, file_mtime, file_format,
                media_type, width, height, duration_ms, sort_datetime, cache_key,
                thumb_status, thumb_path, thumbhash, is_favorited, is_deleted,
                deleted_at, rating, is_live_photo, has_embedded_video, companion_of,
                content_hash, created_at, updated_at, color_label
         FROM media_items
         WHERE is_deleted = 1
           AND (?1 = 0 OR (deleted_at, id) < (?2, ?3))
         ORDER BY deleted_at DESC, id DESC
         LIMIT ?4",
    )?;
    let rows = stmt.query_map(params![has_cursor, cur_da, cur_id, limit], map_media_item)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

// ── Stats ─────────────────────────────────────────────────────────────────────
// ── 统计 ─────────────────────────────────────────────────────────────────────

pub fn get_app_stats(conn: &Connection) -> Result<AppStats> {
    // R2-6 合一:原 8 条独立 COUNT(8 次扫描,且并发写下互相可能不一致)→ 单次全表
    // 扫描 + FILTER 聚合(库内 list_volumes_with_item_counts 已有先例)。三处口径差异
    // 是既有语义,逐字保留:favorited 不排 companion、deleted 不加任何过滤、其余 6 项
    // 排 companion+软删。
    Ok(conn.query_row(
        "SELECT
            COUNT(*) FILTER (WHERE is_deleted=0 AND companion_of IS NULL),
            COUNT(*) FILTER (WHERE is_deleted=0 AND companion_of IS NULL AND media_type='image'),
            COUNT(*) FILTER (WHERE is_deleted=0 AND companion_of IS NULL AND media_type='video'),
            COUNT(*) FILTER (WHERE is_deleted=0 AND companion_of IS NULL AND media_type='audio'),
            COUNT(*) FILTER (WHERE is_deleted=0 AND companion_of IS NULL AND media_type='document'),
            COUNT(*) FILTER (WHERE is_favorited=1 AND is_deleted=0),
            COUNT(*) FILTER (WHERE is_deleted=1),
            COUNT(*) FILTER (WHERE is_live_photo=1 AND is_deleted=0 AND companion_of IS NULL)
         FROM media_items",
        [],
        |r| {
            Ok(AppStats {
                total_items: r.get(0)?,
                total_images: r.get(1)?,
                total_videos: r.get(2)?,
                total_audios: r.get(3)?,
                total_documents: r.get(4)?,
                total_favorited: r.get(5)?,
                total_deleted: r.get(6)?,
                total_live_photos: r.get(7)?,
            })
        },
    )?)
}

// ── Search ────────────────────────────────────────────────────────────────────
// ── 搜索 ────────────────────────────────────────────────────────────────────

pub fn search_media(
    conn: &Connection,
    query: &str,
    filter: &MediaFilter,
    limit: i64,
) -> Result<Vec<SearchResult>> {
    let pattern = format!("%{query}%");
    let mut sql = String::from(
        "SELECT id, file_name, media_type, width, height, thumb_path, thumbhash, thumb_status
         FROM media_items
         WHERE is_deleted=0 AND companion_of IS NULL AND file_name LIKE ?1",
    );

    let mut extras: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(pattern)];
    let mut param_idx = 1usize;

    if let Some(dir_id) = filter.directory_id {
        param_idx += 1;
        sql.push_str(&format!(
            " AND directory_id IN (
            WITH RECURSIVE dir_tree(id) AS (
                SELECT ?{param_idx}
                UNION ALL
                SELECT d.id FROM directories d
                JOIN dir_tree t ON d.parent_id = t.id
            )
            SELECT id FROM dir_tree
        )"
        ));
        extras.push(Box::new(dir_id));
    }

    if let Some(ref types) = filter.media_types {
        if !types.is_empty() {
            let placeholders: Vec<String> = types
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", param_idx + i + 1))
                .collect();
            sql.push_str(&format!(" AND media_type IN ({})", placeholders.join(",")));
            for t in types {
                extras.push(Box::new(t.clone()));
            }
            param_idx += types.len();
        }
    }

    param_idx += 1;
    sql.push_str(&format!(" ORDER BY sort_datetime DESC LIMIT ?{param_idx}"));
    extras.push(Box::new(limit));

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = extras.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(SearchResult {
            id: row.get(0)?,
            file_name: row.get(1)?,
            media_type: row.get(2)?,
            width: row.get(3)?,
            height: row.get(4)?,
            thumb_path: row.get(5)?,
            thumbhash: row.get(6)?,
            thumb_status: row.get(7)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

// ── App config ────────────────────────────────────────────────────────────────
// ── 应用配置 ────────────────────────────────────────────────────────────────

pub fn get_config(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM app_config WHERE key=?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(AppError::from)
}

pub fn set_config(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

// ── Enrichment helpers ────────────────────────────────────────────────────────
// ── 丰富化辅助函数 ────────────────────────────────────────────────────────

/// Items needing enrichment: those without an `image_meta` row and media_type='image'.
/// 需要丰富化的项：没有 `image_meta` 行且 media_type='image' 的项。
pub fn get_unenriched_image_ids(conn: &Connection, limit: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT m.id FROM media_items m
         LEFT JOIN image_meta im ON im.item_id = m.id
         WHERE m.is_deleted=0 AND m.media_type='image' AND im.item_id IS NULL
         ORDER BY m.created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| row.get(0))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Get full path info for a media item.
/// 获取媒体项的完整路径信息。
pub fn get_item_path_info(conn: &Connection, item_id: i64) -> Result<(String, String, String)> {
    conn.query_row(
        "SELECT r.path, d.rel_path, m.file_name
         FROM media_items m
         JOIN directories d ON d.id = m.directory_id
         JOIN scan_roots r ON r.id = d.root_id
         WHERE m.id=?1",
        params![item_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .map_err(|_| AppError::MediaNotFound(item_id))
}

/// Get companion video URL for a live photo (Apple style: by file stem).
/// 获取实况照片的伴随视频 URL（Apple 风格：按文件主干）。
pub fn get_companion_item_id(conn: &Connection, item_id: i64) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM media_items WHERE companion_of=?1 LIMIT 1",
        params![item_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(AppError::from)
}

// ── AI embeddings ─────────────────────────────────────────────────────────────
// ── AI 嵌入向量 ─────────────────────────────────────────────────────────────

/// Upsert an AI embedding for a media item.
/// 插入或更新媒体项的 AI 嵌入向量。
pub fn upsert_ai_embedding(
    conn: &Connection,
    item_id: i64,
    model_name: &str,
    embedding: &[u8],
    version: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO ai_embeddings (item_id, model_name, embedding, version)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(item_id, model_name) DO UPDATE SET
             embedding=excluded.embedding,
             version=excluded.version,
             created_at=strftime('%s','now')",
        params![item_id, model_name, embedding, version],
    )?;
    Ok(())
}

/// Batch-upsert embeddings within a single transaction.
/// 在单个事务中批量插入或更新嵌入向量。
pub fn batch_upsert_ai_embeddings(
    conn: &Connection,
    rows: &[(i64, String, Vec<u8>, i64)], // (item_id, model_name, embedding, version)
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for (item_id, model_name, embedding, version) in rows {
        tx.execute(
            "INSERT INTO ai_embeddings (item_id, model_name, embedding, version)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(item_id, model_name) DO UPDATE SET
                 embedding=excluded.embedding,
                 version=excluded.version,
                 created_at=strftime('%s','now')",
            params![item_id, model_name, embedding, version],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Fetch all embeddings for a given model (used for in-memory cosine search).
/// 获取给定模型的所有嵌入向量（用于内存余弦搜索）。
pub fn get_all_embeddings(conn: &Connection, model_name: &str) -> Result<Vec<(i64, Vec<u8>)>> {
    let mut stmt =
        conn.prepare("SELECT item_id, embedding FROM ai_embeddings WHERE model_name=?1")?;
    let rows = stmt.query_map(params![model_name], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Count stored embeddings for a model.
/// 统计某个模型已经写入的向量数量；这是语义搜索真实可用的覆盖数。
pub fn count_embeddings_for_model(conn: &Connection, model_name: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM ai_embeddings WHERE model_name=?1",
        params![model_name],
        |row| row.get(0),
    )
    .map_err(AppError::from)
}

/// Update `ai_status` for a single media item.
/// 更新单个媒体项的 `ai_status`。
pub fn update_ai_status(conn: &Connection, item_id: i64, status: i64) -> Result<()> {
    conn.execute(
        "UPDATE media_items SET ai_status=?1, updated_at=strftime('%s','now') WHERE id=?2",
        params![status, item_id],
    )?;
    Ok(())
}

/// Batch update `ai_status` for multiple items.
/// 批量更新多个媒体项的 `ai_status`。
pub fn batch_update_ai_status(conn: &Connection, item_ids: &[i64], status: i64) -> Result<()> {
    if item_ids.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for &id in item_ids {
        tx.execute(
            "UPDATE media_items SET ai_status=?1, updated_at=strftime('%s','now') WHERE id=?2",
            params![status, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Get items with `ai_status=0` (pending) ordered by most recent, up to `limit`.
/// 获取 `ai_status=0`（待处理）的项，按最近顺序，最多 `limit` 条。
///
/// One pending image awaiting CLIP analysis, with everything the pipeline needs to pick the
/// **cheapest sufficient** decode source (AI cache → regular thumbnail → original).
/// 一个待 CLIP 分析的图像项，携带流水线选择**最廉价且足够**解码源（AI 缓存 → 常规缩略图 → 原图）
/// 所需的全部信息。
pub struct PendingAiItem {
    pub id: i64,
    /// Absolute path to the original source file.
    /// 源文件绝对路径。
    pub abs_path: String,
    pub file_format: String,
    /// `cache_key` — locates the AI-analysis cache file on disk via `ai_cache_path(cache_dir,
    /// cache_key)`. When that file exists it is the highest-priority, cheapest decode source.
    /// Discovery is by FILE existence (not a `media_derivations` row), so a cache produced as a
    /// byproduct of thumbnail generation (one decode, two outputs) is found too.
    /// `cache_key` —— 经 `ai_cache_path(cache_dir, cache_key)` 定位磁盘上的 AI 分析缓存文件。
    /// 该文件存在时即最高优先级、最廉价的解码源。按**文件存在性**发现（而非 `media_derivations` 行），
    /// 故缩略图生成时顺带产出的缓存（一次解码两份产物）同样能被发现。
    pub cache_key: i64,
    /// 1 = a generated thumbnail exists (`thumb_path` is a tiered cache rel-path);
    /// 3 = small-file direct display (`thumb_path` is the original abs path).
    /// 1 = 已生成缩略图（`thumb_path` 为分档缓存相对路径）；3 = 小文件直显（`thumb_path` 即原图绝对路径）。
    pub thumb_status: i64,
    pub thumb_path: Option<String>,
    /// Original pixel dimensions (0 when unknown) — used to predict the thumbnail's short edge
    /// WITHOUT touching disk, deciding whether a thumbnail is large enough to feed CLIP.
    /// 原图像素尺寸（未知为 0）—— 用于**不读盘**预测缩略图短边，判断其是否够大以喂入 CLIP。
    pub width: i64,
    pub height: i64,
}

/// Fetch a batch of images pending CLIP analysis, with thumbnail/dimension hints so the
/// pipeline can avoid decoding the full-resolution original when a sufficiently large
/// thumbnail (or AI cache) already exists.
///
/// 取一批待 CLIP 分析的图像，附带缩略图/尺寸提示，使流水线在已有足够大的缩略图（或 AI 缓存）时
/// 免去解码全分辨率原图。
pub fn get_pending_ai_items(conn: &Connection, limit: i64) -> Result<Vec<PendingAiItem>> {
    let sql = format!(
        "SELECT m.id,
                CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                     ELSE r.path || '/' || d.rel_path || '/' || m.file_name
                END,
                m.file_format, m.cache_key, m.thumb_status, m.thumb_path, m.width, m.height
         FROM media_items m
         JOIN directories d ON m.directory_id = d.id
         JOIN scan_roots r ON d.root_id = r.id
         WHERE m.ai_status=0 AND m.is_deleted=0 AND m.media_type='image' {NOT_BLOCKED_BY_EXOTIC_M}
         ORDER BY m.created_at DESC
         LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(PendingAiItem {
            id: row.get(0)?,
            abs_path: row.get(1)?,
            file_format: row.get(2)?,
            cache_key: row.get(3)?,
            thumb_status: row.get(4)?,
            thumb_path: row.get(5)?,
            width: row.get(6)?,
            height: row.get(7)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Count pending AI items.
/// 统计待处理的 AI 项数量。
pub fn count_pending_ai_items(conn: &Connection) -> Result<i64> {
    let sql = format!(
        "SELECT COUNT(*) FROM media_items
         WHERE ai_status=0 AND is_deleted=0 AND media_type='image' {NOT_BLOCKED_BY_EXOTIC}"
    );
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(AppError::from)
}

/// Count analysed AI items (status=2 or 3).
/// 统计已分析的 AI 项数量（status=2 或 3）。
pub fn count_analyzed_ai_items(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE ai_status IN (2, 3) AND is_deleted=0 AND media_type='image'",
        [],
        |row| row.get(0),
    )
    .map_err(AppError::from)
}

/// Count total AI items.
/// 统计所有的 AI 项数量。
pub fn count_total_ai_items(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE is_deleted=0 AND media_type='image'",
        [],
        |row| row.get(0),
    )
    .map_err(AppError::from)
}

/// Release items the AI pipeline claimed (ai_status=Processing) but never finished —
/// e.g. after a crash, forced exit, pause, or stop. Sets them back to Pending so a later
/// run resumes them instead of stranding them forever. Returns how many were recovered.
/// 释放 AI 流水线已领取（ai_status=Processing）但未完成的项——例如崩溃、强退、暂停或停止后。
/// 将其设回 Pending，使后续运行能续传而非永久搁置（问题7）。返回恢复的数量。
pub fn reset_processing_ai_items(conn: &Connection) -> Result<usize> {
    conn.execute(
        "UPDATE media_items SET ai_status=0 WHERE ai_status=1 AND media_type='image'",
        [],
    )
    .map_err(AppError::from)
}

/// Reset all AI embeddings — set ai_status back to 0 and delete embeddings.
/// 重置所有 AI 嵌入向量 — 将 ai_status 设回 0 并删除嵌入向量。
///
/// R2-6 分批化:每批独立语句/事务、**批间释放 `db_writer`**——竞争在 Rust Mutex 层
/// (交互写如收藏/评分排队等的正是这把锁,WAL 只保护读),百万行全表 UPDATE 单事务会
/// 秒级独占写锁,故签名改收 `&Mutex<Connection>`。放弃单事务原子性是安全的:调用方
/// 已先取消流水线,且下次 start_ai_analysis 的 sync_ai_status_for_model 会按真实向量
/// 覆盖重新对账(自愈);中间态最多令个别项多重跑,不产生错数据。
pub fn reset_ai_embeddings(db: &std::sync::Mutex<Connection>, model_name: &str) -> Result<()> {
    reset_ai_embeddings_batched(db, model_name, 10_000)
}

fn reset_ai_embeddings_batched(
    db: &std::sync::Mutex<Connection>,
    model_name: &str,
    batch: i64,
) -> Result<()> {
    // 先删该模型向量(2KB 级 BLOB 大表,同样分批;单语句自成事务)。
    loop {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        let n = conn.execute(
            "DELETE FROM ai_embeddings WHERE rowid IN
               (SELECT rowid FROM ai_embeddings WHERE model_name=?1 LIMIT ?2)",
            params![model_name, batch],
        )?;
        if (n as i64) < batch {
            break;
        }
    }
    // 再分批清状态;`ai_status<>0` 谓词既跳过本就 Pending 的行(免白改 updated_at 与
    // 索引翻搅),也是循环的终止条件。
    loop {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        let n = conn.execute(
            "UPDATE media_items SET ai_status=0, updated_at=strftime('%s','now')
             WHERE rowid IN (SELECT rowid FROM media_items
                             WHERE media_type='image' AND ai_status<>0 LIMIT ?1)",
            params![batch],
        )?;
        if (n as i64) < batch {
            break;
        }
    }
    Ok(())
}

/// Re-sync `ai_status` to a model's embedding coverage: items that already have an embedding
/// under `model_name` → Done(2); the rest → Pending(0). Used when SWITCHING the active model.
/// `ai_status` is a single global column (not per-model) and the pipeline only queries status=0,
/// so after a switch we must reset it to the NEW model's coverage — already-embedded items are
/// skipped, missing ones get (re)analysed. Embeddings of other models are kept (DB keyed by
/// `(item_id, model_name)`), so switching BACK is free.
/// 将 `ai_status` 按某模型的向量覆盖重新同步：已有该 `model_name` 向量的项 → Done(2)，其余 →
/// Pending(0)。用于**切换激活模型**。ai_status 是单列全局状态（非按模型）且流水线只查 status=0，
/// 故切换后须据新模型覆盖重置 —— 已嵌入项跳过、缺失项重新分析。其它模型的向量保留（DB 以
/// `(item_id, model_name)` 为键），故切回某模型零成本。
///
/// R2-6:只改「现状 ≠ 目标」的行并分批(签名改 Mutex 的理由同 reset_ai_embeddings)。
/// 语义与原全表 CASE UPDATE 严格等价(任何现状≠目标的行都会被改),但常态「已同步」
/// 时零写——原实现在**每次** start_ai_analysis 与模型切换时把全表每行重写一遍
/// (白改 updated_at + 索引翻搅)。
pub fn sync_ai_status_for_model(db: &std::sync::Mutex<Connection>, model_name: &str) -> Result<()> {
    sync_ai_status_batched(db, model_name, 10_000)
}

fn sync_ai_status_batched(
    db: &std::sync::Mutex<Connection>,
    model_name: &str,
    batch: i64,
) -> Result<()> {
    loop {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        let n = conn.execute(
            "UPDATE media_items SET
                ai_status = CASE
                    WHEN id IN (SELECT item_id FROM ai_embeddings WHERE model_name=?1) THEN 2
                    ELSE 0 END,
                updated_at = strftime('%s','now')
             WHERE rowid IN (
                 SELECT rowid FROM media_items
                 WHERE media_type='image' AND is_deleted=0
                   AND ai_status <> (CASE WHEN id IN
                        (SELECT item_id FROM ai_embeddings WHERE model_name=?1) THEN 2 ELSE 0 END)
                 LIMIT ?2)",
            params![model_name, batch],
        )?;
        if (n as i64) < batch {
            break;
        }
    }
    Ok(())
}

// ── 人脸识别（Face Recognition，F3）─────────────────────────────────────────

/// One pending image awaiting face detection — same decode-source hints as `PendingAiItem`
/// (no `cache_key`: the face pipeline never uses the AI-analysis cache, see
/// `ai::face_pipeline::resolve_face_decode_source` for why).
/// 一个待人脸检测的图像项 —— 解码源提示与 `PendingAiItem` 相同（无 `cache_key`：人脸流水线从不
/// 使用 AI 分析缓存，原因见 `ai::face_pipeline::resolve_face_decode_source`）。
pub struct PendingFaceItem {
    pub id: i64,
    pub abs_path: String,
    pub file_format: String,
    pub thumb_status: i64,
    pub thumb_path: Option<String>,
    pub width: i64,
    pub height: i64,
}

/// Fetch a batch of images pending face detection (`face_status=0`).
/// 取一批待人脸检测的图像（`face_status=0`）。
pub fn get_pending_face_items(conn: &Connection, limit: i64) -> Result<Vec<PendingFaceItem>> {
    let sql = format!(
        "SELECT m.id,
                CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                     ELSE r.path || '/' || d.rel_path || '/' || m.file_name
                END,
                m.file_format, m.thumb_status, m.thumb_path, m.width, m.height
         FROM media_items m
         JOIN directories d ON m.directory_id = d.id
         JOIN scan_roots r ON d.root_id = r.id
         WHERE m.face_status=0 AND m.is_deleted=0 AND m.media_type='image' {NOT_BLOCKED_BY_EXOTIC_M}
         ORDER BY m.created_at DESC
         LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(PendingFaceItem {
            id: row.get(0)?,
            abs_path: row.get(1)?,
            file_format: row.get(2)?,
            thumb_status: row.get(3)?,
            thumb_path: row.get(4)?,
            width: row.get(5)?,
            height: row.get(6)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Count pending face-detection items.
/// 统计待人脸检测的项数量。
pub fn count_pending_face_items(conn: &Connection) -> Result<i64> {
    let sql = format!(
        "SELECT COUNT(*) FROM media_items
         WHERE face_status=0 AND is_deleted=0 AND media_type='image' {NOT_BLOCKED_BY_EXOTIC}"
    );
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(AppError::from)
}

/// Batch update `face_status` for multiple items.
/// 批量更新多个媒体项的 `face_status`。
pub fn batch_update_face_status(conn: &Connection, item_ids: &[i64], status: i64) -> Result<()> {
    if item_ids.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for &id in item_ids {
        tx.execute(
            "UPDATE media_items SET face_status=?1, updated_at=strftime('%s','now') WHERE id=?2",
            params![status, id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Release items the face pipeline claimed (`face_status`=Processing) but never finished —
/// e.g. after a crash, forced exit, pause, or stop. Mirrors `reset_processing_ai_items`（问题7）.
/// 释放人脸流水线已领取（`face_status`=Processing）但未完成的项——镜像 `reset_processing_ai_items`（问题7）。
pub fn reset_processing_face_items(conn: &Connection) -> Result<usize> {
    conn.execute(
        "UPDATE media_items SET face_status=0 WHERE face_status=1 AND media_type='image'",
        [],
    )
    .map_err(AppError::from)
}

/// Count face items that finished processing — `face_status IN (2,3)`, i.e. Done OR Error.
/// Intentionally counts errored items as "processed" (unlike CLIP's `count_embeddings`-based
/// progress) so the progress bar can reach 100% even when some images failed to decode.
/// 统计已处理完成的人脸项——`face_status IN (2,3)`，即 完成 或 错误。刻意把错误项也算作
/// "已处理"（不同于 CLIP 基于 `count_embeddings` 的进度），使部分图解码失败时进度条仍能到 100%。
pub fn count_processed_face_items(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE face_status IN (2,3) AND is_deleted=0 AND media_type='image'",
        [],
        |row| row.get(0),
    )
    .map_err(AppError::from)
}

/// Count clustered persons (the people wall's roster size).
/// 统计已聚类的人物数（人物墙的名册规模）。
pub fn count_persons(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM persons", [], |row| row.get(0))
        .map_err(AppError::from)
}

/// Count stored faces for a model (across all images).
/// 统计某模型下已存的人脸数（跨所有图像）。
pub fn count_faces_for_model(conn: &Connection, model_name: &str) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM faces WHERE model_name=?1",
        params![model_name],
        |row| row.get(0),
    )
    .map_err(AppError::from)
}

/// Wipe ALL face data for a fresh restart: delete this model's faces, delete every person
/// (the `persons` table has no model_name column — single-model assumption, see
/// `ai::face_cluster`), and reset `face_status` to Pending so the pipeline reprocesses
/// everything. WARNING destroys user labor: named persons and `is_confirmed` assignments are
/// gone (callers must warn the user; preserving labels across restart is deferred).
/// 全量重来时清空所有人脸数据：删除该模型的 faces、删除所有 persons（`persons` 表无 model_name
/// 列——单模型假设，见 `ai::face_cluster`），并把 `face_status` 重置为待处理，使流水线全量重跑。
/// 警告会销毁用户劳动：已命名人物与 `is_confirmed` 指派将丢失（调用方须提示用户；跨重启保留标签
/// 留待后续）。
///
/// R2-6 分批化(签名改 Mutex 的理由同 reset_ai_embeddings):faces→persons 两个 DELETE
/// 保持单事务(FK 顺序;persons 行数小,不构成长事务),仅 face_status 全表 UPDATE 分批。
pub fn reset_face_data(db: &std::sync::Mutex<Connection>, model_name: &str) -> Result<()> {
    reset_face_data_batched(db, model_name, 10_000)
}

fn reset_face_data_batched(
    db: &std::sync::Mutex<Connection>,
    model_name: &str,
    batch: i64,
) -> Result<()> {
    {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM faces WHERE model_name=?1", params![model_name])?;
        tx.execute("DELETE FROM persons", [])?;
        tx.commit()?;
    }
    loop {
        let conn = db.lock().unwrap_or_else(|e| e.into_inner());
        let n = conn.execute(
            "UPDATE media_items SET face_status=0, updated_at=strftime('%s','now')
             WHERE rowid IN (SELECT rowid FROM media_items
                             WHERE media_type='image' AND face_status<>0 LIMIT ?1)",
            params![batch],
        )?;
        if (n as i64) < batch {
            break;
        }
    }
    Ok(())
}

// ── 人物墙 / 详情画框（F6）─────────────────────────────────────────────────────

/// List person clusters for the people wall (F6). Joins each person's cover face → its image's
/// thumbnail (same status=3-vs-tiered convention as `get_search_results_by_ids`). Excludes the
/// "ignored" bucket (误检/非人脸); named persons sort first, then by face_count.
/// 列出人物墙的人物簇（F6）。把每个人物的封面脸 → 其图像缩略图（status=3 vs 分档的约定同
/// `get_search_results_by_ids`）。排除"忽略"桶（误检/非人脸）；已命名优先，再按 face_count。
pub fn list_persons(conn: &Connection) -> Result<Vec<crate::db::models::PersonSummary>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.face_count, p.is_named, p.is_hidden,
                f.item_id,
                CASE
                    WHEN m.thumb_status = 3 OR m.thumb_path IS NULL THEN
                        CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                             ELSE r.path || '/' || d.rel_path || '/' || m.file_name END
                    ELSE m.thumb_path
                END AS cover_thumb_path,
                CASE WHEN m.thumb_path IS NULL THEN 3 ELSE m.thumb_status END AS cover_thumb_status,
                f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h
         FROM persons p
         LEFT JOIN faces f ON f.id = p.cover_face_id
         LEFT JOIN media_items m ON m.id = f.item_id AND m.is_deleted = 0
         LEFT JOIN directories d ON m.directory_id = d.id
         LEFT JOIN scan_roots r ON d.root_id = r.id
         WHERE p.is_ignored = 0
         ORDER BY p.is_named DESC, p.face_count DESC, p.id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        let bx: Option<f64> = row.get(8)?;
        let by: Option<f64> = row.get(9)?;
        let bw: Option<f64> = row.get(10)?;
        let bh: Option<f64> = row.get(11)?;
        let cover_bbox = match (bx, by, bw, bh) {
            (Some(x), Some(y), Some(w), Some(h)) => Some([x as f32, y as f32, w as f32, h as f32]),
            _ => None,
        };
        Ok(crate::db::models::PersonSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            face_count: row.get(2)?,
            is_named: row.get::<_, i64>(3)? != 0,
            is_hidden: row.get::<_, i64>(4)? != 0,
            cover_item_id: row.get(5)?,
            cover_thumb_path: row.get(6)?,
            cover_thumb_status: row.get(7)?,
            cover_bbox,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// All faces detected in one image, for the detail-viewer overlay (F6).
/// 一张图中检测到的所有人脸，用于详情查看器叠加框（F6）。
pub fn get_faces_for_item(
    conn: &Connection,
    item_id: i64,
) -> Result<Vec<crate::db::models::FaceBox>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.person_id, p.name, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h, f.det_score
         FROM faces f
         LEFT JOIN persons p ON p.id = f.person_id
         WHERE f.item_id = ?1
         ORDER BY f.det_score DESC",
    )?;
    let rows = stmt.query_map(params![item_id], |row| {
        Ok(crate::db::models::FaceBox {
            id: row.get(0)?,
            person_id: row.get(1)?,
            person_name: row.get(2)?,
            bbox: [
                row.get::<_, f64>(3)? as f32,
                row.get::<_, f64>(4)? as f32,
                row.get::<_, f64>(5)? as f32,
                row.get::<_, f64>(6)? as f32,
            ],
            det_score: row.get::<_, f64>(7)? as f32,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Name a person (sets `is_named=1`). An empty/whitespace name clears it back to unnamed.
/// 给人物命名（置 `is_named=1`）。空白名字则清回未命名。
pub fn rename_person(conn: &Connection, person_id: i64, name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        conn.execute(
            "UPDATE persons SET name=NULL, is_named=0, updated_at=strftime('%s','now') WHERE id=?1",
            params![person_id],
        )?;
    } else {
        conn.execute(
            "UPDATE persons SET name=?2, is_named=1, updated_at=strftime('%s','now') WHERE id=?1",
            params![person_id, trimmed],
        )?;
    }
    Ok(())
}

/// Show/hide a person on the wall (`is_hidden`).
/// 在人物墙上显示/隐藏某人物（`is_hidden`）。
pub fn set_person_hidden(conn: &Connection, person_id: i64, hidden: bool) -> Result<()> {
    conn.execute(
        "UPDATE persons SET is_hidden=?2, updated_at=strftime('%s','now') WHERE id=?1",
        params![person_id, hidden as i64],
    )?;
    Ok(())
}

/// Merge `src_ids` person clusters INTO `dst_id` in one transaction: reassign their faces to
/// `dst`, recompute `dst`'s centroid as a face_count-weighted average of all merged centroids
/// (re-normalized to unit length), bump face_count, then delete the now-empty src persons.
///
/// The weighted-average centroid is an APPROXIMATION (the true centroid is the mean of all member
/// embeddings) — deliberately consistent with F4's incremental running-average, and cheap (reads
/// only `persons`, not every face embedding). `dst`'s cover face is kept (the user merged others
/// INTO dst, so dst's identity/cover is authoritative). Named/`is_confirmed` faces aren't split.
///
/// 在单个事务中把 `src_ids` 人物簇并入 `dst_id`：把它们的脸改派给 `dst`，将 `dst` 质心重算为所有
/// 被并簇质心的 face_count 加权平均（重归一化为单位长度），累加 face_count，再删除已空的 src。
///
/// 加权平均质心是**近似**（真质心是所有成员嵌入的均值）——刻意与 F4 增量滑动平均一致，且廉价
///（只读 `persons`，不读每张脸的嵌入）。保留 `dst` 的封面脸（用户把别人并入 dst，dst 身份/封面
/// 权威）。已命名/`is_confirmed` 的脸不被打散。
pub fn merge_persons(conn: &Connection, src_ids: &[i64], dst_id: i64) -> Result<()> {
    let src: Vec<i64> = src_ids.iter().copied().filter(|&id| id != dst_id).collect();
    if src.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;

    // Gather dst + src centroids/counts for the weighted average. f32 LE, len = embed_dim.
    // 收集 dst + src 的质心/计数做加权平均。f32 小端，长度 = embed_dim。
    let mut all_ids = vec![dst_id];
    all_ids.extend_from_slice(&src);
    let placeholders: Vec<String> = (1..=all_ids.len()).map(|i| format!("?{i}")).collect();
    let sel = format!(
        "SELECT centroid, face_count FROM persons WHERE id IN ({})",
        placeholders.join(",")
    );
    let mut acc: Vec<f64> = Vec::new();
    let mut total_count: i64 = 0;
    {
        let mut stmt = tx.prepare(&sel)?;
        let refs: Vec<&dyn rusqlite::ToSql> = all_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let mut rows = stmt.query(refs.as_slice())?;
        while let Some(row) = rows.next()? {
            let blob: Option<Vec<u8>> = row.get(0)?;
            let count: i64 = row.get(1)?;
            if let Some(bytes) = blob {
                let centroid: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                if acc.is_empty() {
                    acc = vec![0.0; centroid.len()];
                }
                // 加权累加（按 face_count）。
                let w = count.max(1) as f64;
                for (a, &c) in acc.iter_mut().zip(centroid.iter()) {
                    *a += c as f64 * w;
                }
            }
            total_count += count;
        }
    }

    // Reassign src faces → dst.
    // 把 src 的脸改派给 dst。
    let src_ph: Vec<String> = (1..=src.len()).map(|i| format!("?{}", i + 1)).collect();
    let upd = format!(
        "UPDATE faces SET person_id=?1 WHERE person_id IN ({})",
        src_ph.join(",")
    );
    let mut upd_refs: Vec<&dyn rusqlite::ToSql> = vec![&dst_id];
    for id in &src {
        upd_refs.push(id as &dyn rusqlite::ToSql);
    }
    tx.execute(&upd, upd_refs.as_slice())?;

    // Re-normalize the weighted centroid and write it back with the new face_count.
    // 重归一化加权质心并连同新 face_count 写回。
    if !acc.is_empty() {
        let norm = acc.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-12);
        let centroid_bytes: Vec<u8> = acc
            .iter()
            .flat_map(|&x| ((x / norm) as f32).to_le_bytes())
            .collect();
        tx.execute(
            "UPDATE persons SET centroid=?2, face_count=?3, updated_at=strftime('%s','now') WHERE id=?1",
            params![dst_id, centroid_bytes, total_count],
        )?;
    } else {
        tx.execute(
            "UPDATE persons SET face_count=?2, updated_at=strftime('%s','now') WHERE id=?1",
            params![dst_id, total_count],
        )?;
    }

    // Delete the now-empty src persons.
    // 删除已空的 src 人物。
    let del_ph: Vec<String> = (1..=src.len()).map(|i| format!("?{i}")).collect();
    let del_sql = format!("DELETE FROM persons WHERE id IN ({})", del_ph.join(","));
    let del_refs: Vec<&dyn rusqlite::ToSql> =
        src.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
    tx.execute(&del_sql, del_refs.as_slice())?;

    tx.commit()?;
    Ok(())
}

// ── 人脸批量审批（Part4 T3 / §3.5.1）───────────────────────────────────────────
// ── Face batch approval (Part4 T3 / §3.5.1) ──────────────────────────────────
//
// 这些命令让用户校正聚类结果（确认/改派/移出/拒绝/建人），并写入 `faces.is_confirmed`
// （recluster 锁定不打散）与 `face_rejections`（负样本）。每个改动 person 归属的命令都在同一
// 事务内连带重算受影响 person 的派生字段（质心/封面/计数），避免计数与质心陈旧（§3.5.1a）。
// These commands let the user correct clustering (confirm/reassign/unassign/reject/create) by
// writing `faces.is_confirmed` (pinned across recluster) and `face_rejections` (negative samples).
// Every command that changes a person's membership recomputes the affected persons' derived
// fields (centroid/cover/count) in the SAME transaction, so counts/centroids never go stale.

/// Recompute one person's derived fields from its CURRENT member faces, within an open
/// transaction (`conn` may be a `&Transaction`). Centroid = L2-normalized TRUE MEAN of member
/// embeddings (more accurate than the incremental running-average; affordable here because batch
/// approval is rare and a person holds at most a few hundred faces). Cover = the max-quality
/// member. If the person ends up with ZERO faces, mirror `rebuild_person_clusters`' cleanup
/// policy: DELETE it when unnamed & non-ignored (fragment), else keep it as an empty roster slot
/// (centroid/cover cleared) to preserve a named/ignored person the user cares about.
///
/// 在一个已开启的事务内，按 person **当前**成员脸重算其派生字段（`conn` 可为 `&Transaction`）。
/// 质心 = 成员嵌入的 L2 归一化**真均值**（比增量滑动均值更准；批量审批罕见且单 person 至多数百脸，
/// 开销可接受）。封面 = 质量最高的成员。若归零，复刻 `rebuild_person_clusters` 的清理策略：未命名
/// 且非忽略者删除（碎片），否则保留为空槽（清空质心/封面）以护命名/忽略人物。
fn recompute_person_aggregates(conn: &Connection, person_id: i64) -> Result<()> {
    let mut stmt = conn.prepare("SELECT id, embedding, quality FROM faces WHERE person_id=?1")?;
    let mut embeddings: Vec<Vec<f32>> = Vec::new();
    let mut best_cover: Option<(i64, f32)> = None;
    {
        let rows = stmt.query_map(params![person_id], |row| {
            let id: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let quality: f64 = row.get(2)?;
            Ok((id, blob, quality as f32))
        })?;
        for r in rows {
            let (id, blob, quality) = r?;
            let emb: Vec<f32> = blob
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            embeddings.push(emb);
            // 取质量最高者为封面（与聚类/重聚类 cover 升级规则一致）。
            if best_cover.is_none_or(|(_, q)| quality > q) {
                best_cover = Some((id, quality));
            }
        }
    }

    if embeddings.is_empty() {
        // 归零：删未命名非忽略碎片；命名/忽略者留空槽。两条语句互斥（删了就更新不到行）。
        conn.execute(
            "DELETE FROM persons WHERE id=?1 AND is_named=0 AND is_ignored=0",
            params![person_id],
        )?;
        conn.execute(
            "UPDATE persons SET centroid=NULL, cover_face_id=NULL, face_count=0,
                                updated_at=strftime('%s','now')
             WHERE id=?1",
            params![person_id],
        )?;
        return Ok(());
    }

    // 真均值质心，L2 归一化（f64 累加减小数值误差，写回 f32 LE）。
    let dim = embeddings[0].len();
    let mut acc = vec![0f64; dim];
    for emb in &embeddings {
        for (a, &v) in acc.iter_mut().zip(emb.iter()) {
            *a += v as f64;
        }
    }
    let n = embeddings.len() as f64;
    for a in acc.iter_mut() {
        *a /= n;
    }
    let norm = acc.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-12);
    let centroid_bytes: Vec<u8> = acc
        .iter()
        .flat_map(|&x| ((x / norm) as f32).to_le_bytes())
        .collect();
    let cover_id = best_cover.map(|(id, _)| id).unwrap_or(0);
    conn.execute(
        "UPDATE persons SET centroid=?2, cover_face_id=?3, face_count=?4,
                            updated_at=strftime('%s','now')
         WHERE id=?1",
        params![person_id, centroid_bytes, cover_id, embeddings.len() as i64],
    )?;
    Ok(())
}

/// Collect the DISTINCT non-null `person_id`s currently held by `face_ids` — the set of persons
/// whose aggregates must be recomputed AFTER those faces move/leave.
/// 收集 `face_ids` 当前所属的去重非空 `person_id` —— 这些脸移动/离开后须重算其派生字段的 person 集。
fn persons_of_faces(conn: &Connection, face_ids: &[i64]) -> Result<Vec<i64>> {
    if face_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ph: Vec<String> = (1..=face_ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT DISTINCT person_id FROM faces WHERE id IN ({}) AND person_id IS NOT NULL",
        ph.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = face_ids
        .iter()
        .map(|id| id as &dyn rusqlite::ToSql)
        .collect();
    let rows = stmt.query_map(refs.as_slice(), |row| row.get::<_, i64>(0))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Build `"?1,?2,…?k"` placeholders + the matching `ToSql` refs for an `IN (…)` over `face_ids`.
/// 为 `face_ids` 的 `IN (…)` 构造 `"?1,?2,…?k"` 占位符与对应 `ToSql` 引用。
fn in_clause(face_ids: &[i64]) -> (String, Vec<&dyn rusqlite::ToSql>) {
    let ph: Vec<String> = (1..=face_ids.len()).map(|i| format!("?{i}")).collect();
    let refs: Vec<&dyn rusqlite::ToSql> = face_ids
        .iter()
        .map(|id| id as &dyn rusqlite::ToSql)
        .collect();
    (ph.join(","), refs)
}

/// Confirm (pin) the given faces' current assignment: `is_confirmed=1`. Recluster will no longer
/// move them (the `is_pinned` closure in `face_cluster`). Pure lock — does NOT change membership,
/// so no centroid recompute is needed.
/// 确认（锁定）这些脸的当前归属：`is_confirmed=1`。重聚类不再移动它们（见 `face_cluster` 的
/// `is_pinned`）。纯锁定——不改归属，故无须重算质心。
pub fn confirm_face_assignment(conn: &Connection, face_ids: &[i64]) -> Result<()> {
    if face_ids.is_empty() {
        return Ok(());
    }
    let (ph, refs) = in_clause(face_ids);
    let sql = format!("UPDATE faces SET is_confirmed=1 WHERE id IN ({ph})");
    conn.execute(&sql, refs.as_slice())?;
    Ok(())
}

/// Manually reassign `face_ids` to `person_id` and pin them (`is_confirmed=1`) — the user
/// correcting a clustering mistake. 🔴 Same-model guard (§3.5.1a): every reassigned face's
/// `model_name` MUST equal the target person's `model_name`, else a 128-dim face would land in a
/// 512-dim person and the centroid recompute would mix dimensions (panic / garbage) — rejected.
/// Recomputes BOTH the target and every source person (faces left them) in one transaction.
///
/// 手动把 `face_ids` 改派给 `person_id` 并锁定（`is_confirmed=1`）——用户纠正聚类错误。🔴 同模型
/// 守卫（§3.5.1a）：每张被改派脸的 `model_name` 必须等于目标 person 的 `model_name`，否则会把 128
/// 维脸塞进 512 维 person、质心重算混维（panic/算错）——拒绝。在同一事务内重算目标 + 所有源 person。
pub fn reassign_face_to_person(conn: &Connection, face_ids: &[i64], person_id: i64) -> Result<()> {
    if face_ids.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;

    // 目标 person 的 model_name（不存在 → QueryReturnedNoRows 经 ? 透出 Db 错误）。
    let target_model: String = tx.query_row(
        "SELECT model_name FROM persons WHERE id=?1",
        params![person_id],
        |row| row.get(0),
    )?;

    // 同模型守卫：任一被改派脸的 model_name 与目标不符即拒绝（防混维质心）。
    let (ph, mut refs) = in_clause(face_ids);
    let mismatch_sql = format!(
        "SELECT COUNT(*) FROM faces WHERE id IN ({ph}) AND model_name <> ?{}",
        face_ids.len() + 1
    );
    refs.push(&target_model);
    let mismatch: i64 = tx.query_row(&mismatch_sql, refs.as_slice(), |row| row.get(0))?;
    if mismatch > 0 {
        return Err(AppError::Internal(
            "跨模型改派被拒：人脸与目标人物的模型不一致 | cross-model reassign rejected".into(),
        ));
    }

    // 改派前先取源 person 集（改派后这些脸已不在源下）。
    let mut affected = persons_of_faces(&tx, face_ids)?;

    // 🔴 占位符顺序：IN 子句先占 ?1..?k，绑定的 person_id 放末位 ?{k+1}——否则 `person_id=?1 …
    // IN (?1)` 会与 IN 占位冲突（rusqlite 按最大索引计数，给 2 个值却只认 1 个 → InvalidParameterCount）。
    let (ph2, refs2) = in_clause(face_ids);
    let upd_sql = format!(
        "UPDATE faces SET person_id=?{}, is_confirmed=1 WHERE id IN ({ph2})",
        face_ids.len() + 1
    );
    let mut upd_refs = refs2;
    upd_refs.push(&person_id);
    tx.execute(&upd_sql, upd_refs.as_slice())?;

    // 重算目标 + 所有源 person（去重，含目标）。
    if !affected.contains(&person_id) {
        affected.push(person_id);
    }
    for pid in affected {
        recompute_person_aggregates(&tx, pid)?;
    }

    tx.commit()?;
    Ok(())
}

/// Unassign `face_ids` (误检/非人脸或归错): `person_id=NULL` AND `is_confirmed=0`. 🔴 Clearing
/// `is_confirmed` is mandatory (§3.5.1a): a leftover `is_confirmed=1 + person_id=NULL` would make
/// the next recluster treat the face as a free-but-pinned anchor and re-attract it, silently
/// undoing the unassign. Recomputes every source person in one transaction.
/// 移出 `face_ids`（误检/非人脸或归错）：`person_id=NULL` 且 `is_confirmed=0`。🔴 必须清
/// `is_confirmed`（§3.5.1a）：残留 `is_confirmed=1 + person_id=NULL` 会让下次重聚类把它当 free
/// 锚脸重新吸附，等于悄悄撤销 unassign。在同一事务内重算所有源 person。
pub fn unassign_face(conn: &Connection, face_ids: &[i64]) -> Result<()> {
    if face_ids.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    let affected = persons_of_faces(&tx, face_ids)?;
    let (ph, refs) = in_clause(face_ids);
    let sql = format!("UPDATE faces SET person_id=NULL, is_confirmed=0 WHERE id IN ({ph})");
    tx.execute(&sql, refs.as_slice())?;
    for pid in affected {
        recompute_person_aggregates(&tx, pid)?;
    }
    tx.commit()?;
    Ok(())
}

/// Record "these faces are NOT `person_id`" negative samples in `face_rejections` AND remove the
/// faces from that person right now (`person_id=NULL`, `is_confirmed=0` if they were assigned to
/// it). The rejection rows are what a later full-recluster consults to skip already-rejected
/// (face, person) pairs (Stage B; prevents a near centroid from re-attracting them). Recomputes
/// the rejected person's aggregates. `INSERT OR IGNORE` makes repeat rejections idempotent.
///
/// 在 `face_rejections` 记录「这些脸不是 `person_id`」负样本，并立即把（当前归在该 person 的）脸
/// 移出（`person_id=NULL`、`is_confirmed=0`）。负样本行供后续全量重聚类查阅以跳过已拒绝
///（face, person）对（Stage B；防相近质心反复吸附）。重算该 person 的派生字段。`INSERT OR IGNORE`
/// 使重复拒绝幂等。
pub fn reject_face_candidate(conn: &Connection, face_ids: &[i64], person_id: i64) -> Result<()> {
    if face_ids.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;

    // 1. 记负样本（幂等）。
    for &fid in face_ids {
        tx.execute(
            "INSERT OR IGNORE INTO face_rejections (face_id, person_id) VALUES (?1, ?2)",
            params![fid, person_id],
        )?;
    }

    // 2. 当前归在该 person 的被拒脸立即移出（仅限确实归在它的，避免误清其它 person 的脸）。
    let (ph, refs) = in_clause(face_ids);
    let detach_sql = format!(
        "UPDATE faces SET person_id=NULL, is_confirmed=0 WHERE id IN ({ph}) AND person_id=?{}",
        face_ids.len() + 1
    );
    let mut detach_refs = refs;
    detach_refs.push(&person_id);
    tx.execute(&detach_sql, detach_refs.as_slice())?;

    // 3. 重算被拒 person（可能减员）。
    recompute_person_aggregates(&tx, person_id)?;

    tx.commit()?;
    Ok(())
}

/// Create a brand-new person from `face_ids` (one-tap "make a person" from a likely-match group),
/// binding them with `is_confirmed=1`. Optional `name` sets `is_named=1`. The new person's
/// aggregates are computed from the bound faces; every source person the faces left is recomputed.
/// 🔴 All faces must share ONE `model_name` (the new person's vector space); mismatch is rejected.
/// Returns the new `person_id`.
///
/// 从 `face_ids` 新建一个 person（从 likely-match 组一键「建人」），以 `is_confirmed=1` 绑定。可选
/// `name` 置 `is_named=1`。新 person 派生字段由绑定脸算出；脸离开的每个源 person 都重算。🔴 所有脸
/// 必须同一 `model_name`（新 person 的向量空间）；不一致则拒绝。返回新 `person_id`。
pub fn create_person_from_faces(
    conn: &Connection,
    face_ids: &[i64],
    name: Option<&str>,
) -> Result<i64> {
    if face_ids.is_empty() {
        return Err(AppError::Internal(
            "建人需至少一张人脸 | create_person needs at least one face".into(),
        ));
    }
    let tx = conn.unchecked_transaction()?;

    // 取这批脸的 model_name 去重——必须唯一（新 person 的向量空间身份）。
    let (ph, refs) = in_clause(face_ids);
    let model_sql = format!("SELECT DISTINCT model_name FROM faces WHERE id IN ({ph})");
    let mut models: Vec<String> = {
        let mut stmt = tx.prepare(&model_sql)?;
        let rows = stmt.query_map(refs.as_slice(), |row| row.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    if models.len() != 1 {
        return Err(AppError::Internal(
            "建人失败：所选人脸跨多个模型 | faces span multiple models".into(),
        ));
    }
    let model_name = models.pop().unwrap();

    // 源 person 集（绑定后这些脸已离开）。
    let affected = persons_of_faces(&tx, face_ids)?;

    let trimmed = name.map(str::trim).filter(|s| !s.is_empty());
    tx.execute(
        "INSERT INTO persons (name, is_named, model_name, face_count) VALUES (?1, ?2, ?3, 0)",
        params![trimmed, trimmed.is_some() as i64, model_name],
    )?;
    let new_id = tx.last_insert_rowid();

    // 占位符顺序同 reassign：IN 先占 ?1..?k，new_id 末位 ?{k+1}（避免与 IN 占位冲突）。
    let (ph2, refs2) = in_clause(face_ids);
    let bind_sql = format!(
        "UPDATE faces SET person_id=?{}, is_confirmed=1 WHERE id IN ({ph2})",
        face_ids.len() + 1
    );
    let mut bind_refs = refs2;
    bind_refs.push(&new_id);
    tx.execute(&bind_sql, bind_refs.as_slice())?;

    // 新 person + 所有源 person 重算。
    recompute_person_aggregates(&tx, new_id)?;
    for pid in affected {
        recompute_person_aggregates(&tx, pid)?;
    }

    tx.commit()?;
    Ok(new_id)
}

/// List "likely match" groups for the batch-approval UI (Part4 §3.5.1 / Part5 T10): unconfirmed
/// faces (`is_confirmed=0`) tentatively clustered under a (non-ignored) person, grouped by that
/// candidate person. Each face carries its source-image thumbnail (same status=3-vs-tiered path
/// convention as `list_persons`) + bbox to crop it, and its cosine similarity to the person
/// centroid; the group `confidence` is the mean of those similarities. Optional `person_id`
/// narrows to one person; optional `limit` caps the number of groups (strongest confidence first).
///
/// 列出批量审批 UI 的「likely match」组（Part4 §3.5.1 / Part5 T10）：未确认脸（`is_confirmed=0`）
/// 暂归于某（非忽略）person，按候选 person 分组。每张脸带源图缩略图（status=3 vs 分档路径约定同
/// `list_persons`）+ bbox 裁剪框，及其与 person 质心的余弦相似度；组 `confidence` 为这些相似度的
/// 均值。可选 `person_id` 限定单人；可选 `limit` 限组数（confidence 高者优先）。
pub fn list_likely_matches(
    conn: &Connection,
    person_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<crate::db::models::LikelyMatchGroup>> {
    use crate::db::models::{FaceThumb, LikelyMatchGroup};

    // person_id 过滤是可选的；用 (?1 IS NULL OR f.person_id=?1) 避免动态拼 SQL。
    let mut stmt = conn.prepare(
        "SELECT f.id, f.item_id, f.person_id, p.name, p.centroid, f.embedding,
                f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                CASE
                    WHEN m.thumb_status = 3 OR m.thumb_path IS NULL THEN
                        CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                             ELSE r.path || '/' || d.rel_path || '/' || m.file_name END
                    ELSE m.thumb_path
                END AS thumb_path,
                CASE WHEN m.thumb_path IS NULL THEN 3 ELSE m.thumb_status END AS thumb_status
         FROM faces f
         JOIN persons p ON p.id = f.person_id
         LEFT JOIN media_items m ON m.id = f.item_id AND m.is_deleted = 0
         LEFT JOIN directories d ON m.directory_id = d.id
         LEFT JOIN scan_roots r ON d.root_id = r.id
         WHERE f.is_confirmed = 0 AND f.person_id IS NOT NULL AND p.is_ignored = 0
           AND (?1 IS NULL OR f.person_id = ?1)
         ORDER BY f.person_id, f.det_score DESC",
    )?;

    struct Row {
        face_id: i64,
        item_id: i64,
        person_id: i64,
        person_name: Option<String>,
        centroid: Option<Vec<u8>>,
        embedding: Vec<u8>,
        bbox: [f32; 4],
        thumb_path: Option<String>,
        thumb_status: Option<i64>,
    }
    let rows = stmt.query_map(params![person_id], |row| {
        Ok(Row {
            face_id: row.get(0)?,
            item_id: row.get(1)?,
            person_id: row.get(2)?,
            person_name: row.get(3)?,
            centroid: row.get(4)?,
            embedding: row.get(5)?,
            bbox: [
                row.get::<_, f64>(6)? as f32,
                row.get::<_, f64>(7)? as f32,
                row.get::<_, f64>(8)? as f32,
                row.get::<_, f64>(9)? as f32,
            ],
            thumb_path: row.get(10)?,
            thumb_status: row.get(11)?,
        })
    })?;

    // 按 person 顺序分组（SQL 已 ORDER BY person_id）。centroid/embedding 解码后算余弦（两者均
    // L2 归一化 → 余弦=点积）；centroid 为 NULL 时相似度记 0。
    let mut groups: Vec<LikelyMatchGroup> = Vec::new();
    for r in rows {
        let r = r?;
        let sim = match &r.centroid {
            Some(c) => cosine_from_le_bytes(c, &r.embedding),
            None => 0.0,
        };
        let thumb = FaceThumb {
            face_id: r.face_id,
            item_id: r.item_id,
            thumb_path: r.thumb_path,
            thumb_status: r.thumb_status,
            bbox: r.bbox,
            similarity: sim,
        };
        match groups.last_mut() {
            Some(g) if g.person_id == r.person_id => g.candidate_faces.push(thumb),
            _ => groups.push(LikelyMatchGroup {
                person_id: r.person_id,
                person_name: r.person_name,
                candidate_faces: vec![thumb],
                confidence: 0.0,
            }),
        }
    }

    // 组 confidence = 成员相似度均值；按 confidence 降序，limit 限组数。
    for g in groups.iter_mut() {
        let n = g.candidate_faces.len().max(1) as f32;
        g.confidence = g.candidate_faces.iter().map(|f| f.similarity).sum::<f32>() / n;
    }
    groups.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(lim) = limit {
        if lim >= 0 {
            groups.truncate(lim as usize);
        }
    }
    Ok(groups)
}

/// Cosine similarity of two f32-LE-encoded embeddings. Both are unit-normalized in the pipeline,
/// so this is effectively their dot product; we still divide by the norms defensively in case a
/// stored centroid drifted off the unit sphere. Mismatched lengths → 0 (cross-model safety).
/// 两个 f32 小端编码嵌入的余弦相似度。两者在流水线中均已单位归一化，故实为点积；仍除以范数以
/// 防库存质心漂移出单位球。长度不一致 → 0（跨模型安全）。
fn cosine_from_le_bytes(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f64;
    let mut na = 0f64;
    let mut nb = 0f64;
    for (ca, cb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let va = f32::from_le_bytes([ca[0], ca[1], ca[2], ca[3]]) as f64;
        let vb = f32::from_le_bytes([cb[0], cb[1], cb[2], cb[3]]) as f64;
        dot += va * vb;
        na += va * va;
        nb += vb * vb;
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-12);
    (dot / denom) as f32
}

/// One face row ready to be written. `bbox`/`landmarks` must already be normalized against the
/// **decoded image's own** width/height (not the original file's DB-stored dimensions — the
/// decode source may be a smaller thumbnail), see `ai::face_pipeline`.
/// 一条待写入的人脸行。`bbox`/`landmarks` 须已按**解码图自身**宽高归一化（非数据库存的原图尺寸——
/// 解码源可能是更小的缩略图），见 `ai::face_pipeline`。
pub struct NewFace {
    pub item_id: i64,
    pub bbox_x: f32,
    pub bbox_y: f32,
    pub bbox_w: f32,
    pub bbox_h: f32,
    /// 5 landmarks flattened to 10 f32, little-endian (`ai::clip::embedding_to_bytes`).
    /// 5 关键点拍平为 10 个 f32，小端（复用 `ai::clip::embedding_to_bytes`）。
    pub landmarks: Vec<u8>,
    pub det_score: f32,
    pub quality: f32,
    pub embedding: Vec<u8>,
}

/// Replace all `faces` rows for the given items under `model_name`: delete then re-insert in one
/// transaction. `item_ids` must include EVERY successfully-processed item in this flush
/// (including zero-face images), so stale faces from a previous run are cleared even when the
/// new result has no rows to insert for that item.
/// 在单个事务中替换给定 `model_name` 下这批项的所有 `faces` 行：先删后插。`item_ids` 须包含本批
/// **所有**处理成功的项（含零脸图），使上次运行的陈旧人脸被清除——即便新结果对该项无行可插。
pub fn batch_replace_faces(
    conn: &Connection,
    item_ids: &[i64],
    model_name: &str,
    rows: &[NewFace],
) -> Result<()> {
    if item_ids.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for &id in item_ids {
        tx.execute(
            "DELETE FROM faces WHERE item_id=?1 AND model_name=?2",
            params![id, model_name],
        )?;
    }
    for r in rows {
        tx.execute(
            "INSERT INTO faces
                (item_id, person_id, model_name, bbox_x, bbox_y, bbox_w, bbox_h, landmarks, det_score, quality, embedding, is_confirmed)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
            params![
                r.item_id,
                model_name,
                r.bbox_x,
                r.bbox_y,
                r.bbox_w,
                r.bbox_h,
                r.landmarks,
                r.det_score,
                r.quality,
                r.embedding
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

// ── 人脸聚类（Face Clustering，F4，仅增量）─────────────────────────────────────

/// One existing person, decoded enough for incremental nearest-centroid matching.
/// `cover_quality` comes from a LEFT JOIN (defaults to 0.0 if `cover_face_id` is dangling —
/// `persons.cover_face_id` has no FK constraint, see schema).
/// 一个既有人物，已解码出增量最近质心匹配所需的字段。`cover_quality` 来自 LEFT JOIN
/// （`cover_face_id` 悬空时默认 0.0——`persons.cover_face_id` 无 FK 约束，见 schema）。
pub struct PersonRow {
    pub id: i64,
    pub centroid: Vec<u8>,
    pub face_count: i64,
    pub cover_face_id: i64,
    pub cover_quality: f32,
}

/// Load every person with a centroid (i.e. every person that already has ≥1 face), for
/// in-memory nearest-centroid matching against newly-written faces.
/// 加载所有已有质心的人物（即已有 ≥1 张脸的人物），供与新写入人脸做内存中最近质心匹配。
pub fn get_all_persons_for_clustering(conn: &Connection) -> Result<Vec<PersonRow>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.centroid, p.face_count, p.cover_face_id, COALESCE(f.quality, 0.0)
         FROM persons p
         LEFT JOIN faces f ON f.id = p.cover_face_id
         WHERE p.centroid IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(PersonRow {
            id: row.get(0)?,
            centroid: row.get(1)?,
            face_count: row.get(2)?,
            cover_face_id: row.get(3)?,
            cover_quality: row.get(4)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// One just-written face, ready for incremental clustering.
/// 一张刚写入的人脸，已就绪可供增量聚类。
pub struct ClusterableFaceRow {
    pub id: i64,
    pub embedding: Vec<u8>,
    pub quality: f32,
}

/// Re-fetch the faces just written for `item_ids` under `model_name` — `batch_replace_faces`
/// discards the auto-assigned row ids, so clustering re-queries them by the same key instead of
/// threading ids back through the write path.
/// 重新按 `item_ids` + `model_name` 取回刚写入的人脸——`batch_replace_faces` 不返回自增 id，
/// 聚类按同样的键重新查询，而不是把 id 一路串回写入路径。
pub fn get_clusterable_faces(
    conn: &Connection,
    item_ids: &[i64],
    model_name: &str,
) -> Result<Vec<ClusterableFaceRow>> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: Vec<String> = (1..=item_ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT id, embedding, quality FROM faces WHERE item_id IN ({}) AND model_name=?{}",
        placeholders.join(","),
        item_ids.len() + 1
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut refs: Vec<&dyn rusqlite::ToSql> = item_ids
        .iter()
        .map(|id| id as &dyn rusqlite::ToSql)
        .collect();
    refs.push(&model_name);
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(ClusterableFaceRow {
            id: row.get(0)?,
            embedding: row.get(1)?,
            quality: row.get(2)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// One person touched by this flush's clustering pass: `id < 0` is an unsaved placeholder
/// (insert and resolve the real id); `id > 0` is an existing person to update in place.
/// `face_ids` lists every face in this flush assigned to it — applied as `faces.person_id`
/// once the person row is ready (so a brand-new person's id is known).
/// 本次刷新聚类涉及的一个人物：`id < 0` 是未落库的占位（插入后解析真实 id）；`id > 0` 是要原地
/// 更新的既有人物。`face_ids` 列出本批归入它的所有脸——人物行就位（新人物 id 已知）后用于设置
/// `faces.person_id`。
pub struct PersonClusterUpdate {
    pub id: i64,
    pub centroid: Vec<u8>,
    pub face_count: i64,
    pub cover_face_id: i64,
    pub face_ids: Vec<i64>,
}

/// Apply incremental clustering decisions in one transaction: insert new persons (resolving
/// their real id via `last_insert_rowid`), update existing persons' centroid/face_count/cover,
/// then set `faces.person_id` for every face in each update.
/// 在单个事务中应用增量聚类决策：插入新人物（经 `last_insert_rowid` 解析真实 id）、更新既有
/// 人物的质心/计数/封面，然后为每条更新里的所有脸设置 `faces.person_id`。
pub fn apply_face_clusters(conn: &Connection, updates: &[PersonClusterUpdate]) -> Result<()> {
    if updates.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for u in updates {
        let person_id = if u.id < 0 {
            tx.execute(
                "INSERT INTO persons (cover_face_id, centroid, face_count) VALUES (?1, ?2, ?3)",
                params![u.cover_face_id, u.centroid, u.face_count],
            )?;
            tx.last_insert_rowid()
        } else {
            tx.execute(
                "UPDATE persons SET centroid=?1, face_count=?2, cover_face_id=?3, updated_at=strftime('%s','now')
                 WHERE id=?4",
                params![u.centroid, u.face_count, u.cover_face_id, u.id],
            )?;
            u.id
        };
        for &face_id in &u.face_ids {
            tx.execute(
                "UPDATE faces SET person_id=?1 WHERE id=?2",
                params![person_id, face_id],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

// ── 全量重新聚类（显式命令，非增量）────────────────────────────────────────────
// ── Full re-clustering (explicit command, not the incremental flush) ─────────

/// One existing person, decoded for the full re-cluster rebuild. Carries `is_named`/`is_ignored`
/// so the rebuild can keep "anchored" persons and seed a named person's stored `centroid` (so it
/// still attracts its members even when it has no confirmed/pinned face). `centroid` is empty when
/// the column is NULL.
/// 一个既有人物，为全量重聚类解码。带 `is_named`/`is_ignored` 以便重建保留"锚定"人物，并用已命名
/// 人物的库存 `centroid` 作种子（即便它没有已确认/锁定脸也仍能吸附成员）。`centroid` 为空表示列为 NULL。
pub struct PersonReclusterRow {
    pub id: i64,
    pub is_named: bool,
    pub is_ignored: bool,
    pub centroid: Vec<u8>,
}

/// Load every person (with flags + stored centroid) for the full re-cluster rebuild.
/// 加载全部人物（含标志位 + 库存质心），供全量重聚类重建。
pub fn get_persons_for_recluster(conn: &Connection) -> Result<Vec<PersonReclusterRow>> {
    let mut stmt = conn.prepare("SELECT id, is_named, is_ignored, centroid FROM persons")?;
    let rows = stmt.query_map([], |row| {
        let centroid: Option<Vec<u8>> = row.get(3)?;
        Ok(PersonReclusterRow {
            id: row.get(0)?,
            is_named: row.get::<_, i64>(1)? != 0,
            is_ignored: row.get::<_, i64>(2)? != 0,
            centroid: centroid.unwrap_or_default(),
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// One face for the full re-cluster pass: includes its CURRENT `person_id` + `is_confirmed` so the
/// rebuild can PIN confirmed faces and faces sitting in the "ignored" bucket (never move them).
/// 全量重聚类用的一张脸：带当前 `person_id` + `is_confirmed`，使重建能锁定已确认脸与落在"忽略"
/// 桶里的脸（永不移动它们）。
pub struct ReclusterFaceRow {
    pub id: i64,
    pub person_id: Option<i64>,
    pub embedding: Vec<u8>,
    pub quality: f32,
    pub is_confirmed: bool,
}

/// Load all faces of `model_name` for the full re-cluster rebuild.
/// 加载 `model_name` 下的全部人脸，供全量重聚类重建。
pub fn get_all_faces_for_recluster(
    conn: &Connection,
    model_name: &str,
) -> Result<Vec<ReclusterFaceRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, person_id, embedding, quality, is_confirmed FROM faces WHERE model_name=?1",
    )?;
    let rows = stmt.query_map(params![model_name], |row| {
        Ok(ReclusterFaceRow {
            id: row.get(0)?,
            person_id: row.get(1)?,
            embedding: row.get(2)?,
            quality: row.get::<_, f64>(3)? as f32,
            is_confirmed: row.get::<_, i64>(4)? != 0,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Load the `(face_id, person_id)` negative samples for `model_name`'s faces — the pairs the user
/// rejected via `reject_face_candidate`. The full re-cluster consults these to never re-attract a
/// rejected face back into the person it was rejected from (Part4 T3 StageB / §3.5.1). JOINed to
/// `faces` so only this model's rejections are returned (face ids are per-model).
/// 加载 `model_name` 下人脸的 `(face_id, person_id)` 负样本——用户经 `reject_face_candidate` 拒绝的对。
/// 全量重聚类查阅它，确保被拒脸绝不被重新吸回它被拒绝的那个 person（Part4 T3 StageB / §3.5.1）。
/// JOIN `faces` 以仅返回该模型的拒绝对（face id 按模型唯一）。
pub fn get_face_rejections(conn: &Connection, model_name: &str) -> Result<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT fr.face_id, fr.person_id
         FROM face_rejections fr
         JOIN faces f ON f.id = fr.face_id
         WHERE f.model_name = ?1",
    )?;
    let rows = stmt.query_map(params![model_name], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Apply a full re-clustering rebuild in ONE transaction:
/// 1. detach every face of this model (`person_id=NULL`) — clean slate;
/// 2. re-apply the computed `updates` (id<0 INSERT new UNNAMED person; id>0 UPDATE centroid/
///    face_count/cover **but NEVER touch name/is_named/is_hidden/is_ignored**) + set each listed
///    face's `person_id` (this re-attaches pinned faces too — they MUST be in some update);
/// 3. delete UNNAMED, non-ignored persons that ended up referenced by no face (fragmentation
///    cleanup). Named and ignored persons survive even if empty (preserve user labor).
///
/// 在单事务中应用全量重聚类：①把该模型所有脸解绑（`person_id=NULL`）做白板；②重放算出的 `updates`
///（id<0 插入新**未命名**人物；id>0 更新质心/计数/封面，**绝不动 name/is_named/is_hidden/is_ignored**）
/// 并回填每张脸的 `person_id`（锁定脸也借此重新挂回——故它们必须出现在某条 update 里）；③删除最终无任
/// 何脸指向的**未命名**非忽略人物（清碎片化遗留）。已命名/忽略人物即便空也保留（护用户劳动）。
pub fn rebuild_person_clusters(
    conn: &Connection,
    model_name: &str,
    updates: &[PersonClusterUpdate],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE faces SET person_id=NULL WHERE model_name=?1",
        params![model_name],
    )?;
    for u in updates {
        let person_id = if u.id < 0 {
            tx.execute(
                "INSERT INTO persons (cover_face_id, centroid, face_count) VALUES (?1, ?2, ?3)",
                params![u.cover_face_id, u.centroid, u.face_count],
            )?;
            tx.last_insert_rowid()
        } else {
            tx.execute(
                "UPDATE persons SET centroid=?1, face_count=?2, cover_face_id=?3, updated_at=strftime('%s','now')
                 WHERE id=?4",
                params![u.centroid, u.face_count, u.cover_face_id, u.id],
            )?;
            u.id
        };
        for &face_id in &u.face_ids {
            tx.execute(
                "UPDATE faces SET person_id=?1 WHERE id=?2",
                params![person_id, face_id],
            )?;
        }
    }
    // 清碎片：不再被任何脸引用的"未命名非忽略"人物。子查询已滤 NULL，故 NOT IN 安全。
    tx.execute(
        "DELETE FROM persons WHERE is_named=0 AND is_ignored=0
         AND id NOT IN (SELECT person_id FROM faces WHERE person_id IS NOT NULL)",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

/// Fetch media item thumbnail info for a list of IDs (for semantic search results).
/// 获取一批 ID 的媒体项缩略图信息（用于语义搜索结果）。
///
/// For `thumb_status=3` (small-file direct display), the `thumb_path` column is NULL
/// but we resolve the absolute path via JOIN, exactly like `get_thumb_by_item_ids` does.
/// 对于 `thumb_status=3`（小文件直接显示），`thumb_path` 列为 NULL，
/// 但通过 JOIN 解析绝对路径（与 `get_thumb_by_item_ids` 的处理方式完全一致）。
pub fn get_search_results_by_ids(
    conn: &Connection,
    ids: &[i64],
) -> Result<Vec<crate::db::models::SearchResult>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT m.id, m.file_name, m.media_type, m.width, m.height,
                CASE
                    WHEN m.thumb_status = 3 OR m.thumb_path IS NULL THEN
                        CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                             ELSE r.path || '/' || d.rel_path || '/' || m.file_name
                        END
                    ELSE m.thumb_path
                END AS thumb_path,
                m.thumbhash,
                CASE
                    WHEN m.thumb_path IS NULL THEN 3
                    ELSE m.thumb_status
                END AS thumb_status
         FROM media_items m
         JOIN directories d ON m.directory_id = d.id
         JOIN scan_roots r ON d.root_id = r.id
         WHERE m.id IN ({})",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok(crate::db::models::SearchResult {
            id: row.get(0)?,
            file_name: row.get(1)?,
            media_type: row.get(2)?,
            width: row.get(3)?,
            height: row.get(4)?,
            thumb_path: row.get(5)?,
            thumbhash: row.get(6)?,
            thumb_status: row.get(7)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

// ── Derivations (media_derivations) ───────────────────────────────────────────
// ── 派生任务（media_derivations） ─────────────────────────────────────────────
//
// 状态机与 AI 完全同构：0 待处理 / 1 处理中 / 2 完成 / 3 错误，支持断点续传 + 孤儿恢复。
// 与 AI（ai_status 列）不同，派生任务是独立表，每个 (item, kind) 一行，需显式入队（backfill）。
// 见 plan-docs/feature_expansion_plan_v1.md §2.2。

/// A pending derivation task resolved for the consumer: the absolute source path is
/// JOIN-resolved here (mirroring `get_pending_ai_items`) so each kind's `run` can read
/// the source file directly.
/// 解析给消费者的待处理派生任务：绝对源路径在此通过 JOIN 解析（仿 `get_pending_ai_items`），
/// 使每种 kind 的 `run` 可直接读取源文件。
///
/// `(item_id, kind, abs_path, file_format, media_type, cache_key)`
pub type DerivationTask = (i64, String, String, String, String, i64);

/// Get pending derivation tasks (status=0), optionally filtered to a single `kind` and/or
/// excluding a set of kinds. `exclude_kinds` lets the pipeline honour the user's "extract video
/// cover / keyframes" toggles: when a kind is switched off, its already-enqueued pending rows
/// are simply skipped here (non-destructive — they resume if the toggle is turned back on).
/// 获取待处理派生任务（status=0），可选按单个 `kind` 过滤，并可排除一组 kind。`exclude_kinds`
/// 使流水线尊重用户的「提取视频封面 / 关键帧」开关：某 kind 被关闭时，其已入队的待处理行在此
/// 直接跳过（非破坏性 —— 开关重新打开后即续传）。
pub fn get_pending_derivations(
    conn: &Connection,
    limit: i64,
    kind_filter: Option<&str>,
    exclude_kinds: &[&str],
) -> Result<Vec<DerivationTask>> {
    // 真正的跨 kind 优先级由「入队哪些 kind」（backfill 顺序）在上游保证，
    // 这里按 (kind, item_id) 排序即可保证确定性批处理。
    // pdf/svg 文档缩略图是「前端驱动」（Lite 无 native 栅格化器）：后端无法生成，
    // 故在生产者查询里排除，使其保持待处理（status=0）留给前端 list_pending_doc_thumbs 领取。
    // epub 文档缩略图仍由后端处理（derive/doc.rs 取 OPF 封面）。详见 §3.4。
    let base = "
        SELECT dv.item_id, dv.kind,
               CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                    ELSE r.path || '/' || d.rel_path || '/' || m.file_name
               END,
               m.file_format, m.media_type, m.cache_key
        FROM media_derivations dv
        JOIN media_items m ON dv.item_id = m.id
        JOIN directories d ON m.directory_id = d.id
        JOIN scan_roots r ON d.root_id = r.id
        WHERE dv.status = 0 AND m.is_deleted = 0
          AND NOT (dv.kind = 'doc_thumb' AND m.file_format IN ('pdf','svg'))";

    let map_row = |row: &Row<'_>| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ))
    };

    // Build the WHERE/ORDER/LIMIT tail dynamically so the optional `kind` filter and the
    // variable-length `exclude_kinds` set are all passed as bound parameters (never concatenated).
    // 动态拼接 WHERE/ORDER/LIMIT 尾部，使可选 `kind` 过滤与变长 `exclude_kinds` 集合全部以
    // 绑定参数传入（绝不拼接值）。
    let mut sql = String::from(base);
    let mut sql_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
    let mut idx = 1;

    // 借用 `&kind_filter` 取得指向函数参数的引用，使压入 `sql_params` 的指针在整个函数内有效
    // （直接 `&kind` 会指向 if-let 的局部绑定而悬垂）。
    if let Some(kind) = &kind_filter {
        sql.push_str(&format!(" AND dv.kind = ?{idx}"));
        sql_params.push(kind as &dyn rusqlite::ToSql);
        idx += 1;
    }

    if !exclude_kinds.is_empty() {
        let placeholders: Vec<String> = (idx..idx + exclude_kinds.len())
            .map(|i| format!("?{i}"))
            .collect();
        sql.push_str(&format!(" AND dv.kind NOT IN ({})", placeholders.join(",")));
        for k in exclude_kinds {
            sql_params.push(k as &dyn rusqlite::ToSql);
        }
        idx += exclude_kinds.len();
    }

    sql.push_str(&format!(" ORDER BY dv.kind, dv.item_id LIMIT ?{idx}"));
    sql_params.push(&limit as &dyn rusqlite::ToSql);

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(sql_params.as_slice(), map_row)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Mark a batch of derivation tasks as processing (status=1) to avoid re-queuing.
/// 将一批派生任务标记为处理中（status=1），避免重复排队。
pub fn mark_derivations_processing(conn: &Connection, tasks: &[(i64, String)]) -> Result<()> {
    if tasks.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for (item_id, kind) in tasks {
        tx.execute(
            "UPDATE media_derivations SET status=1, updated_at=strftime('%s','now')
             WHERE item_id=?1 AND kind=?2",
            params![item_id, kind],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Result of one derivation: `(item_id, kind, status, payload_path, error, thumbhash, page_count)`.
/// `thumbhash` is `Some` only for cover-producing kinds (mirrored onto `media_items` by the
/// pipeline writer). `page_count` is `Some` only for the epub `doc_thumb` kind (upserted into
/// `document_meta` by the writer). Neither is stored in `media_derivations`.
/// 单个派生的结果：`(item_id, kind, status, payload_path, error, thumbhash, page_count)`。
/// `thumbhash` 仅封面类 kind 为 `Some`（写入器回填 `media_items`）；`page_count` 仅 epub `doc_thumb`
/// 为 `Some`（写入器 upsert 进 `document_meta`，§3.8.2 / T10）。两者均不存入 `media_derivations`。
pub type DerivationResultRow = (
    i64,
    String,
    i64,
    Option<String>,
    Option<String>,
    Option<Vec<u8>>,
    Option<i64>,
);

/// Batch-write derivation results (status + payload_path + error) in one transaction.
/// `thumbhash` (6th tuple element) is intentionally ignored here — covers mirror it to
/// `media_items` separately via `update_thumb_result`.
/// 在单个事务中批量写入派生结果（状态 + 产物路径 + 错误）。
/// `thumbhash`（元组第 6 项）此处有意忽略 —— 封面另经 `update_thumb_result` 回填到 `media_items`。
pub fn batch_finish_derivations(conn: &Connection, results: &[DerivationResultRow]) -> Result<()> {
    if results.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for (item_id, kind, status, payload_path, error, _thumbhash, _page_count) in results {
        tx.execute(
            "UPDATE media_derivations
             SET status=?3, payload_path=?4, error=?5, updated_at=strftime('%s','now')
             WHERE item_id=?1 AND kind=?2",
            params![item_id, kind, status, payload_path, error],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Recover orphaned derivation tasks (status=1 left behind by crash/pause/stop) back to
/// pending (status=0) so the next run resumes them. Returns how many were recovered.
/// 将孤儿派生任务（崩溃/暂停/停止遗留的 status=1）恢复为待处理（status=0），使下次运行续传。
/// 返回恢复的数量。
pub fn reset_processing_derivations(conn: &Connection) -> Result<usize> {
    conn.execute("UPDATE media_derivations SET status=0 WHERE status=1", [])
        .map_err(AppError::from)
}

/// Insert pending derivation rows (`INSERT OR IGNORE`) for every non-deleted item of
/// `media_type` (optionally restricted to `formats`) that lacks a `(item, kind)` row.
/// Returns the number of rows inserted. This is the explicit "enqueue" step that the
/// separate derivations table needs (AI gets it for free via the `ai_status` column).
/// 为 `media_type`（可选限定 `formats`）下所有缺少 `(item, kind)` 行的未删除项插入待处理派生行
/// （`INSERT OR IGNORE`）。返回插入的行数。这是独立派生表所需的显式「入队」步骤
/// （AI 通过 `ai_status` 列天然免费获得）。
pub fn backfill_derivations(
    conn: &Connection,
    kind: &str,
    media_type: &str,
    formats: Option<&[&str]>,
) -> Result<usize> {
    let inserted = if let Some(fmts) = formats {
        if fmts.is_empty() {
            return Ok(0);
        }
        // Bind formats as parameters (?3, ?4, …) — never string-concatenate values.
        // 将格式作为参数绑定（?3、?4…）—— 绝不拼接值。
        let placeholders: Vec<String> = (3..3 + fmts.len()).map(|i| format!("?{i}")).collect();
        // §6.3：exotic 已认领 thumbnail 的媒体不建主派生封面任务（NOT_BLOCKED_BY_EXOTIC）。
        let sql = format!(
            "INSERT OR IGNORE INTO media_derivations (item_id, kind, status)
             SELECT id, ?1, 0 FROM media_items
             WHERE media_type = ?2 AND is_deleted = 0 AND file_format IN ({}) {NOT_BLOCKED_BY_EXOTIC}",
            placeholders.join(",")
        );
        let mut sql_params: Vec<&dyn rusqlite::ToSql> = vec![
            &kind as &dyn rusqlite::ToSql,
            &media_type as &dyn rusqlite::ToSql,
        ];
        for f in fmts {
            sql_params.push(f as &dyn rusqlite::ToSql);
        }
        conn.execute(&sql, sql_params.as_slice())?
    } else {
        let sql = format!(
            "INSERT OR IGNORE INTO media_derivations (item_id, kind, status)
             SELECT id, ?1, 0 FROM media_items
             WHERE media_type = ?2 AND is_deleted = 0 {NOT_BLOCKED_BY_EXOTIC}"
        );
        conn.execute(&sql, params![kind, media_type])?
    };
    Ok(inserted)
}

/// The keyframe sprite payload path (relative to `cache_dir`) for a video, if it has been
/// generated (status=2). Used by the hover scrub fallback (§3.1 / §3.3).
/// 某视频的关键帧雪碧图产物路径（相对 `cache_dir`），若已生成（status=2）。用于悬停 scrub 降级（§3.1 / §3.3）。
pub fn get_keyframe_sprite_payload(conn: &Connection, item_id: i64) -> Result<Option<String>> {
    conn.query_row(
        "SELECT payload_path FROM media_derivations
         WHERE item_id = ?1 AND kind = 'video_keyframes' AND status = 2",
        params![item_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|o| o.flatten())
    .map_err(AppError::from)
}

/// List documents (pdf/svg) awaiting a **frontend-rendered** thumbnail (§3.4 Lite path):
/// pending `doc_thumb` rows whose format the backend can't rasterise. Returns
/// `(item_id, abs_path, file_format)`; the frontend renders each offscreen and posts the
/// bytes back via `store_doc_thumbnail`. epub is intentionally excluded (handled in-backend).
/// 列出等待「前端渲染」缩略图的文档（pdf/svg，§3.4 Lite 路径）：后端无法栅格化的待处理
/// `doc_thumb` 行。返回 `(item_id, 绝对路径, 格式)`；前端逐个离屏渲染后经 `store_doc_thumbnail`
/// 回传字节。epub 有意排除（由后端处理）。
pub fn list_pending_doc_thumbs(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<(i64, String, String)>> {
    let sql = "
        SELECT dv.item_id,
               CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                    ELSE r.path || '/' || d.rel_path || '/' || m.file_name
               END,
               m.file_format
        FROM media_derivations dv
        JOIN media_items m ON dv.item_id = m.id
        JOIN directories d ON m.directory_id = d.id
        JOIN scan_roots r ON d.root_id = r.id
        WHERE dv.kind = 'doc_thumb' AND dv.status = 0 AND m.is_deleted = 0
          AND m.file_format IN ('pdf','svg')
        ORDER BY dv.item_id LIMIT ?1";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

// ── Document replacements (§5.2) ──────────────────────────────────────────────
// 注：`replace` 是 SQLite 函数名，作列名一律加引号 `"replace"`。

fn map_replacement(row: &Row<'_>) -> rusqlite::Result<crate::db::models::ReplacementRule> {
    Ok(crate::db::models::ReplacementRule {
        id: row.get(0)?,
        scope_kind: row.get(1)?,
        scope_id: row.get(2)?,
        find: row.get(3)?,
        replace: row.get(4)?,
        is_regex: row.get::<_, i64>(5)? != 0,
        enabled: row.get::<_, i64>(6)? != 0,
        sort_order: row.get(7)?,
    })
}

/// List replacement rules for a specific scope (for the rule editor). `scope_id = None`
/// targets global rules (`scope_id IS NULL`).
/// 列出某具体作用域的替换规则（供规则编辑器）。`scope_id = None` → 全局（`scope_id IS NULL`）。
pub fn list_replacements(
    conn: &Connection,
    scope_kind: &str,
    scope_id: Option<i64>,
) -> Result<Vec<crate::db::models::ReplacementRule>> {
    let mut stmt = conn.prepare(
        "SELECT id, scope_kind, scope_id, find, \"replace\", is_regex, enabled, sort_order
         FROM doc_replacements
         WHERE scope_kind = ?1 AND scope_id IS ?2
         ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map(params![scope_kind, scope_id], map_replacement)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Effective rules to APPLY for an item = enabled global + item-scoped (group deferred), ordered.
/// 对某项实际生效的规则 = 启用的 global + item 作用域（group 暂缓），按序。
pub fn get_effective_replacements(
    conn: &Connection,
    item_id: i64,
) -> Result<Vec<crate::db::models::ReplacementRule>> {
    let mut stmt = conn.prepare(
        "SELECT id, scope_kind, scope_id, find, \"replace\", is_regex, enabled, sort_order
         FROM doc_replacements
         WHERE enabled = 1
           AND (scope_kind = 'global' OR (scope_kind = 'item' AND scope_id = ?1))
         ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map(params![item_id], map_replacement)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Insert (id=None) or update (id=Some) a replacement rule. Returns the row id.
/// 插入（id=None）或更新（id=Some）一条替换规则。返回行 id。
#[allow(clippy::too_many_arguments)]
pub fn upsert_replacement(
    conn: &Connection,
    id: Option<i64>,
    scope_kind: &str,
    scope_id: Option<i64>,
    find: &str,
    replace: &str,
    is_regex: bool,
    enabled: bool,
    sort_order: i64,
) -> Result<i64> {
    if let Some(id) = id {
        conn.execute(
            "UPDATE doc_replacements
             SET scope_kind=?2, scope_id=?3, find=?4, \"replace\"=?5, is_regex=?6, enabled=?7, sort_order=?8
             WHERE id=?1",
            params![id, scope_kind, scope_id, find, replace, is_regex as i64, enabled as i64, sort_order],
        )?;
        Ok(id)
    } else {
        conn.execute(
            "INSERT INTO doc_replacements (scope_kind, scope_id, find, \"replace\", is_regex, enabled, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![scope_kind, scope_id, find, replace, is_regex as i64, enabled as i64, sort_order],
        )?;
        Ok(conn.last_insert_rowid())
    }
}

/// Delete a replacement rule by id.
/// 按 id 删除一条替换规则。
pub fn delete_replacement(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM doc_replacements WHERE id = ?1", params![id])?;
    Ok(())
}

// ── Document versions (§5.3) ──────────────────────────────────────────────────

fn map_version(row: &Row<'_>) -> rusqlite::Result<crate::db::models::DocumentVersion> {
    Ok(crate::db::models::DocumentVersion {
        id: row.get(0)?,
        item_id: row.get(1)?,
        parent_id: row.get(2)?,
        label: row.get(3)?,
        storage: row.get(4)?,
        abs_path: row.get(5)?,
        source: row.get(6)?,
        note: row.get(7)?,
        content_hash: row.get(8)?,
        is_current: row.get::<_, i64>(9)? != 0,
        created_at: row.get(10)?,
    })
}

const VERSION_COLS: &str =
    "id, item_id, parent_id, label, storage, abs_path, source, note, content_hash, is_current, created_at";

/// All versions of a document, oldest first (§5.3).
/// 某文档的所有版本，最旧在前（§5.3）。
pub fn list_versions(
    conn: &Connection,
    item_id: i64,
) -> Result<Vec<crate::db::models::DocumentVersion>> {
    let sql = format!(
        "SELECT {VERSION_COLS} FROM document_versions WHERE item_id = ?1 ORDER BY created_at, id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![item_id], map_version)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// One version by id (§5.3).
/// 按 id 取单个版本（§5.3）。
pub fn get_version(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::db::models::DocumentVersion>> {
    let sql = format!("SELECT {VERSION_COLS} FROM document_versions WHERE id = ?1");
    conn.query_row(&sql, params![id], map_version)
        .optional()
        .map_err(AppError::from)
}

/// The current version of a document, if one is marked (§5.3). `None` → source baseline is current.
/// 文档当前版本（若有标记，§5.3）。`None` → 以源文件为当前。
pub fn get_current_version(
    conn: &Connection,
    item_id: i64,
) -> Result<Option<crate::db::models::DocumentVersion>> {
    let sql = format!(
        "SELECT {VERSION_COLS} FROM document_versions WHERE item_id = ?1 AND is_current = 1 LIMIT 1"
    );
    conn.query_row(&sql, params![item_id], map_version)
        .optional()
        .map_err(AppError::from)
}

/// Insert a version row (abs_path filled in afterwards once the id-derived path is known).
/// 插入一行版本（abs_path 在拿到 id 推导路径后再回填）。
#[allow(clippy::too_many_arguments)]
pub fn insert_version(
    conn: &Connection,
    item_id: i64,
    parent_id: Option<i64>,
    label: Option<&str>,
    storage: &str,
    abs_path: &str,
    source: &str,
    content_hash: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO document_versions (item_id, parent_id, label, storage, abs_path, source, content_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![item_id, parent_id, label, storage, abs_path, source, content_hash],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Fill in a version's file path after writing it (two-step: insert → write → set path).
/// 写盘后回填版本文件路径（两步：插入 → 写文件 → 置路径）。
pub fn update_version_path(conn: &Connection, id: i64, abs_path: &str) -> Result<()> {
    conn.execute(
        "UPDATE document_versions SET abs_path = ?2 WHERE id = ?1",
        params![id, abs_path],
    )?;
    Ok(())
}

/// Mark a version as current (or clear all → source baseline current). Exactly one current.
/// 将某版本标为当前（或全清 → 以源文件为当前）。当前版本至多一个。
pub fn set_current_version(conn: &Connection, item_id: i64, version_id: Option<i64>) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE document_versions SET is_current = 0 WHERE item_id = ?1",
        params![item_id],
    )?;
    if let Some(vid) = version_id {
        tx.execute(
            "UPDATE document_versions SET is_current = 1 WHERE id = ?1 AND item_id = ?2",
            params![vid, item_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Delete a version row, returning its file path so the caller can remove the file.
/// 删除一行版本，返回其文件路径以便调用者删除文件。
pub fn delete_version(conn: &Connection, id: i64) -> Result<Option<String>> {
    let path = conn
        .query_row(
            "SELECT abs_path FROM document_versions WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    conn.execute("DELETE FROM document_versions WHERE id = ?1", params![id])?;
    Ok(path)
}

/// Read a document's saved reading position (page / CFI / scroll ratio), if any (§5.1).
/// 读取文档已保存的阅读位置（页码 / CFI / 滚动比例），若有（§5.1）。
pub fn get_reading_progress(conn: &Connection, item_id: i64) -> Result<Option<String>> {
    conn.query_row(
        "SELECT position FROM reading_progress WHERE item_id = ?1",
        params![item_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(AppError::from)
}

/// Upsert a document's reading position (§5.1). Opaque string: the renderer decides the format
/// (page number for pdf, CFI for epub, scroll ratio for text).
/// 写入/更新文档阅读位置（§5.1）。不透明字符串：由渲染器决定格式（pdf 页码、epub CFI、文本滚动比例）。
pub fn set_reading_progress(conn: &Connection, item_id: i64, position: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO reading_progress (item_id, position, updated_at)
         VALUES (?1, ?2, strftime('%s','now'))
         ON CONFLICT(item_id) DO UPDATE SET position=excluded.position, updated_at=excluded.updated_at",
        params![item_id, position],
    )?;
    Ok(())
}

/// The `cache_key` of a non-deleted item, for cover/thumbnail encoding by id.
/// 某未删除项的 `cache_key`，供按 id 编码封面/缩略图使用。
pub fn get_item_cache_key(conn: &Connection, item_id: i64) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT cache_key FROM media_items WHERE id = ?1 AND is_deleted = 0",
        params![item_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()
    .map_err(AppError::from)
}

/// Aggregate derivation counts by status: `(pending, processing, done, error)`.
/// 按状态聚合派生计数：`(待处理, 处理中, 完成, 错误)`。
pub fn count_derivations_by_status(conn: &Connection) -> Result<(i64, i64, i64, i64)> {
    conn.query_row(
        "SELECT
            COALESCE(SUM(status=0),0),
            COALESCE(SUM(status=1),0),
            COALESCE(SUM(status=2),0),
            COALESCE(SUM(status=3),0)
         FROM media_derivations",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )
    .map_err(AppError::from)
}

// ── Collections (favorites, §3.7) ─────────────────────────────────────────────
// ── 收藏夹（需求7, §3.7） ──────────────────────────────────────────────────────
//
// 复用 albums/album_items，不另造机制。系统夹（kind='system'）成员虚拟：
//   该类型 + is_favorited（走 idx_media_fav 快路径，红心即收藏，无需写 album_items）。
// 用户夹（kind='user'）成员实体：存 album_items，可跨类型混装。
// list_collections 用 CASE 分别计算两类的 cover/count。

fn map_collection(row: &Row<'_>) -> rusqlite::Result<Collection> {
    Ok(Collection {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        media_type_filter: row.get(3)?,
        icon: row.get(4)?,
        cover_item_id: row.get(5)?,
        item_count: row.get(6)?,
        sort_order: row.get(7)?,
    })
}

/// List all collections (system folders first, then user), each with a computed cover
/// item and member count.
/// 列出所有收藏夹（系统夹在前、用户夹在后），各带计算出的封面项与成员数。
pub fn list_collections(conn: &Connection) -> Result<Vec<Collection>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.name, a.kind, a.media_type_filter, a.icon,
            CASE WHEN a.kind='system'
                 THEN (SELECT m.id FROM media_items m
                       WHERE m.media_type=a.media_type_filter AND m.is_favorited=1 AND m.is_deleted=0
                       ORDER BY m.sort_datetime DESC LIMIT 1)
                 ELSE COALESCE(a.cover_item_id,
                       (SELECT ai.item_id FROM album_items ai
                        JOIN media_items m ON ai.item_id=m.id
                        WHERE ai.album_id=a.id AND m.is_deleted=0
                        ORDER BY ai.added_at DESC LIMIT 1))
            END AS cover_item_id,
            CASE WHEN a.kind='system'
                 THEN (SELECT COUNT(*) FROM media_items m
                       WHERE m.media_type=a.media_type_filter AND m.is_favorited=1 AND m.is_deleted=0)
                 ELSE (SELECT COUNT(*) FROM album_items ai
                       JOIN media_items m ON ai.item_id=m.id
                       WHERE ai.album_id=a.id AND m.is_deleted=0)
            END AS item_count,
            a.sort_order
         FROM albums a
         ORDER BY (a.kind='user'), a.sort_order, a.id",
    )?;
    let rows = stmt.query_map([], map_collection)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// User collections ordered by most-recently-used (latest member added), for the
/// "加入收藏夹" toast chips. Limited to `limit`.
/// 用户收藏夹按最近使用（最新加入成员）排序，用于「加入收藏夹」toast 快捷 chips，限 `limit` 个。
pub fn recent_collections(conn: &Connection, limit: i64) -> Result<Vec<Collection>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.name, a.kind, a.media_type_filter, a.icon, a.cover_item_id,
            (SELECT COUNT(*) FROM album_items ai JOIN media_items m ON ai.item_id=m.id
             WHERE ai.album_id=a.id AND m.is_deleted=0) AS item_count,
            a.sort_order
         FROM albums a
         WHERE a.kind='user'
         ORDER BY COALESCE((SELECT MAX(added_at) FROM album_items WHERE album_id=a.id), a.created_at) DESC,
                  a.id DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], map_collection)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Create a new user collection. Returns the new album id.
/// 新建一个用户收藏夹。返回新 album id。
pub fn create_collection(conn: &Connection, name: &str, icon: Option<&str>) -> Result<i64> {
    conn.execute(
        "INSERT INTO albums (name, kind, icon) VALUES (?1, 'user', ?2)",
        params![name, icon],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Delete a USER collection (system folders are protected). `album_items` cascade-delete.
/// 删除一个用户收藏夹（系统夹受保护）。`album_items` 级联删除。
pub fn delete_collection(conn: &Connection, album_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM albums WHERE id=?1 AND kind='user'",
        params![album_id],
    )?;
    Ok(())
}

/// Rename a USER collection (system folders are protected by the `kind='user'` guard).
/// 重命名一个用户收藏夹（`kind='user'` 守卫保护系统夹不被改名）。非用户夹为空操作。
pub fn rename_collection(conn: &Connection, album_id: i64, name: &str) -> Result<()> {
    conn.execute(
        "UPDATE albums SET name=?2 WHERE id=?1 AND kind='user'",
        params![album_id, name],
    )?;
    Ok(())
}

/// Add items to a USER collection (`INSERT OR IGNORE`). No-op for system/missing albums
/// (system membership is virtual — driven by `is_favorited`). Returns rows inserted.
/// 向用户收藏夹添加项（`INSERT OR IGNORE`）。系统夹/不存在的夹为空操作（系统成员虚拟，由
/// `is_favorited` 驱动）。返回插入行数。
pub fn add_to_collection(conn: &Connection, album_id: i64, item_ids: &[i64]) -> Result<usize> {
    if item_ids.is_empty() {
        return Ok(0);
    }
    let kind: Option<String> = conn
        .query_row(
            "SELECT kind FROM albums WHERE id=?1",
            params![album_id],
            |r| r.get(0),
        )
        .optional()?;
    if kind.as_deref() != Some("user") {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    let mut inserted = 0usize;
    for &id in item_ids {
        inserted += tx.execute(
            "INSERT OR IGNORE INTO album_items (album_id, item_id) VALUES (?1, ?2)",
            params![album_id, id],
        )?;
    }
    tx.commit()?;
    Ok(inserted)
}

/// Remove items from a collection. Returns rows deleted.
/// 从收藏夹移除项。返回删除行数。
pub fn remove_from_collection(conn: &Connection, album_id: i64, item_ids: &[i64]) -> Result<usize> {
    if item_ids.is_empty() {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    let mut deleted = 0usize;
    for &id in item_ids {
        deleted += tx.execute(
            "DELETE FROM album_items WHERE album_id=?1 AND item_id=?2",
            params![album_id, id],
        )?;
    }
    tx.commit()?;
    Ok(deleted)
}

// ════════════════════════════════════════════════════════════════════════════
// 冷门格式插件 · 任务 DAO（v3 Part1 §1.6 / 勘误 R2）
// ════════════════════════════════════════════════════════════════════════════
//
// 状态码（与 ExoticTaskStatus 同步）：0=pending 1=processing 2=done 3=retryable 4=terminal。
// SQL 内只能用整数字面量；变更须与 src/exotic/task.rs 同步。
//
// 原子领取与租约（R2）：
//   - 领取用单条 `UPDATE ... WHERE id IN (SELECT ... LIMIT) RETURNING`，一句完成 = 原子；
//     条件 `status IN (0,3)` 防两实例重复领取（输的一方 UPDATE 命中 0 行）。
//   - `lease_owner`(进程级 instance_id) + ttl：防活实例任务被误恢复、隔离过期 Writer。
//   - finish/fail 的最终更新均带 `status=1 AND lease_owner=?`，失去租约的旧结果只能丢弃。

/// `exotic_tasks` 全列（顺序与 `map_exotic_task` 严格一致）。
const EXOTIC_TASK_COLS: &str = "id, item_id, plugin_id, capability, status, input_fingerprint, \
    attempts, next_retry_at, claimed_at, lease_owner, last_error_code, last_error_message, \
    output_path, worker_version";

fn map_exotic_task(row: &Row<'_>) -> rusqlite::Result<ExoticTaskRow> {
    let status_i: i64 = row.get(4)?;
    Ok(ExoticTaskRow {
        id: row.get(0)?,
        item_id: row.get(1)?,
        plugin_id: row.get(2)?,
        capability: row.get(3)?,
        // 未知状态码视为 Pending（损坏行不致 panic；领取条件会自然忽略非 0/3）。
        status: ExoticTaskStatus::from_i64(status_i).unwrap_or(ExoticTaskStatus::Pending),
        input_fingerprint: row.get(5)?,
        attempts: row.get(6)?,
        next_retry_at: row.get(7)?,
        claimed_at: row.get(8)?,
        lease_owner: row.get(9)?,
        last_error_code: row.get(10)?,
        last_error_message: row.get(11)?,
        output_path: row.get(12)?,
        worker_version: row.get(13)?,
    })
}

/// 为单个 item 按 capabilities 播种任务（扫描事务内调用）。已存在则 IGNORE（UNIQUE 约束）。
pub fn seed_exotic_tasks_for_item(
    conn: &Connection,
    item_id: i64,
    plugin_id: &str,
    capabilities: &[String],
) -> Result<()> {
    for cap in capabilities {
        conn.execute(
            "INSERT OR IGNORE INTO exotic_tasks (item_id, plugin_id, capability) VALUES (?1,?2,?3)",
            params![item_id, plugin_id, cap],
        )?;
    }
    Ok(())
}

/// 集合式 backfill：为某格式所有未拥有该 (plugin,capability) 任务的现存媒体补建任务。
/// Catalog 更新后调用（只为已登记格式建任务，不重跑 enrichment）。返回新建行数。
pub fn backfill_exotic_tasks_for_format(
    conn: &Connection,
    format: &str,
    plugin_id: &str,
    capability: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "INSERT OR IGNORE INTO exotic_tasks (item_id, plugin_id, capability)
         SELECT id, ?2, ?3 FROM media_items
         WHERE file_format=?1 AND is_deleted=0",
        params![format, plugin_id, capability],
    )?)
}

/// SourceChanged 失效：把该 item 全部 exotic 任务退回 pending，清空输出/指纹/错误/租约。
/// 注意：本函数只管 DB；旧产物文件由调用方/维护任务清理（Part2 Sink 重做时覆盖）。返回受影响行数。
pub fn invalidate_exotic_tasks_for_item(conn: &Connection, item_id: i64) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE exotic_tasks
         SET status=0, input_fingerprint=NULL, attempts=0, next_retry_at=NULL,
             claimed_at=NULL, lease_owner=NULL, last_error_code=NULL, last_error_message=NULL,
             output_path=NULL, worker_version=NULL, updated_at=strftime('%s','now')
         WHERE item_id=?1",
        params![item_id],
    )?)
}

/// 升级 Worker 后失效：把该插件「已完成但 worker_version 不同」的任务退回 pending（指纹会变）。
/// 只失效受影响 capability/版本的任务，不动其他插件。返回受影响行数。
pub fn invalidate_exotic_tasks_for_plugin_version(
    conn: &Connection,
    plugin_id: &str,
    new_worker_version: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE exotic_tasks
         SET status=0, input_fingerprint=NULL, claimed_at=NULL, lease_owner=NULL,
             updated_at=strftime('%s','now')
         WHERE plugin_id=?1 AND status=2 AND (worker_version IS NULL OR worker_version<>?2)",
        params![plugin_id, new_worker_version],
    )?)
}

/// 全量重建 / 清空缩略图语义：把所有 thumbnail exotic 任务退回 pending，清空旧输出/指纹/错误/租约。
/// 必须与主缩略图「全部重做」对齐——否则 done(2) 任务因 worker_version 仍在，**既不**被 Coordinator
/// 重领（claim 只取 0/3），**又因** `NOT_BLOCKED_BY_EXOTIC`（status<>2 才算 blocking）把已清空的 PSD
/// 放回主 generator → UnsupportedFormat（违 R3/R7、Part1 DoD#4，问题1）。
/// 不动 processing(1)：本实例在途任务自然完成，其结果对清空后的 media_items 仍有效（Sink 会重写 thumb_path）。
/// 返回受影响行数。调用方须在重置后 `wake_exotic` 让 Coordinator 重领。
pub fn reset_all_exotic_thumbnail_tasks(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE exotic_tasks
         SET status=0, input_fingerprint=NULL, attempts=0, next_retry_at=NULL,
             claimed_at=NULL, lease_owner=NULL, last_error_code=NULL, last_error_message=NULL,
             output_path=NULL, worker_version=NULL, updated_at=strftime('%s','now')
         WHERE capability='thumbnail' AND status IN (2,3,4)",
        [],
    )?)
}

/// 跨流水线门控：该 item 是否仍有**未完成**的 thumbnail exotic 任务。
/// CLIP/人脸/派生在此为 true 时不应处理该 item（v3 §6.3 / Part1 §2.4）。
pub fn has_blocking_exotic_thumbnail_task(conn: &Connection, item_id: i64) -> Result<bool> {
    let exists: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exotic_tasks
            WHERE item_id=?1 AND capability='thumbnail' AND status<>2)",
        params![item_id],
        |r| r.get(0),
    )?;
    Ok(exists != 0)
}

/// 原子领取就绪任务（pending 或到期 retryable）；写 processing + claimed_at + lease_owner。
/// 单条 UPDATE...RETURNING 完成领取，避免 SELECT/UPDATE 之间的竞态窗口（R2）。
pub fn claim_exotic_tasks(
    conn: &Connection,
    plugin_id: &str,
    capability: &str,
    limit: i64,
    instance_id: &str,
    now: i64,
) -> Result<Vec<ExoticTaskRow>> {
    let sql = format!(
        "UPDATE exotic_tasks
         SET status=1, claimed_at=?3, lease_owner=?4, updated_at=strftime('%s','now')
         WHERE id IN (
             SELECT id FROM exotic_tasks
             WHERE plugin_id=?1 AND capability=?2
               AND ( status=0 OR (status=3 AND (next_retry_at IS NULL OR next_retry_at<=?3)) )
             ORDER BY id LIMIT ?5 )
         RETURNING {EXOTIC_TASK_COLS}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            params![plugin_id, capability, now, instance_id, limit],
            map_exotic_task,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// 续租（Supervisor 周期调用）：仅当仍持有租约时更新 claimed_at。返回是否续租成功。
pub fn renew_exotic_lease(conn: &Connection, id: i64, instance_id: &str, now: i64) -> Result<bool> {
    let n = conn.execute(
        "UPDATE exotic_tasks SET claimed_at=?3 WHERE id=?1 AND status=1 AND lease_owner=?2",
        params![id, instance_id, now],
    )?;
    Ok(n == 1)
}

/// 批量续租：刷新本实例**全部**在途 processing 任务的 claimed_at（R2，问题2）。
/// 一次 claim 可领多个、再经 channel/worker 排队消化，单 claimed_at 不续则排队中任务会在
/// lease_ttl 后被第二实例的孤儿恢复误回收。续租周期须 << lease_ttl。返回续租行数。
pub fn renew_all_exotic_leases(conn: &Connection, instance_id: &str, now: i64) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE exotic_tasks SET claimed_at=?2 WHERE status=1 AND lease_owner=?1",
        params![instance_id, now],
    )?)
}

/// 完成任务：条件更新（必须仍 processing 且租约属本实例），写 done + 指纹/输出/版本。
/// 返回 true=本实例成功落库；false=租约已失（旧结果须丢弃）。
pub fn finish_exotic_task(
    conn: &Connection,
    id: i64,
    instance_id: &str,
    fingerprint: &str,
    output_path: &str,
    worker_version: &str,
) -> Result<bool> {
    let n = conn.execute(
        "UPDATE exotic_tasks
         SET status=2, input_fingerprint=?3, output_path=?4, worker_version=?5,
             last_error_code=NULL, last_error_message=NULL, claimed_at=NULL, lease_owner=NULL,
             updated_at=strftime('%s','now')
         WHERE id=?1 AND status=1 AND lease_owner=?2",
        params![id, instance_id, fingerprint, output_path, worker_version],
    )?;
    Ok(n == 1)
}

/// 失败任务：retryable 且未超次数 → status=3 + attempts+1 + next_retry_at；否则 terminal(4)。
/// 条件更新（仍 processing 且租约属本实例）。返回是否本实例成功记录。
#[allow(clippy::too_many_arguments)]
pub fn fail_exotic_task(
    conn: &Connection,
    id: i64,
    instance_id: &str,
    retryable: bool,
    max_attempts: i64,
    code: &str,
    message: &str,
    next_retry_at: i64,
) -> Result<bool> {
    let n = conn.execute(
        "UPDATE exotic_tasks
         SET attempts = attempts + 1,
             status = CASE WHEN ?3=1 AND attempts+1 < ?4 THEN 3 ELSE 4 END,
             next_retry_at = CASE WHEN ?3=1 AND attempts+1 < ?4 THEN ?7 ELSE NULL END,
             last_error_code=?5, last_error_message=?6,
             claimed_at=NULL, lease_owner=NULL, updated_at=strftime('%s','now')
         WHERE id=?1 AND status=1 AND lease_owner=?2",
        params![
            id,
            instance_id,
            retryable as i64,
            max_attempts,
            code,
            message,
            next_retry_at
        ],
    )?;
    Ok(n == 1)
}

/// 列出已安装插件（安装真相投影）。Part1 安装表为空 → 返回空列表。
pub fn list_installed_exotic_plugins(
    conn: &Connection,
) -> Result<Vec<crate::exotic::InstalledExoticPlugin>> {
    let mut stmt = conn.prepare(
        "SELECT plugin_id, version, package_sequence, install_state, installed_at, updated_at
         FROM exotic_plugins ORDER BY plugin_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(crate::exotic::InstalledExoticPlugin {
            plugin_id: row.get(0)?,
            version: row.get(1)?,
            package_sequence: row.get(2)?,
            install_state: row.get(3)?,
            installed_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// 读单个已安装插件的完整安装真相行（含 manifest_hash）。未安装 → None（Part3 §6）。
pub fn get_exotic_plugin(
    conn: &Connection,
    plugin_id: &str,
) -> Result<Option<crate::exotic::InstalledPluginRecord>> {
    let row = conn
        .query_row(
            "SELECT plugin_id, version, manifest_hash, package_sequence, install_state,
                    installed_at, updated_at
             FROM exotic_plugins WHERE plugin_id=?1",
            params![plugin_id],
            |row| {
                Ok(crate::exotic::InstalledPluginRecord {
                    plugin_id: row.get(0)?,
                    version: row.get(1)?,
                    manifest_hash: row.get(2)?,
                    package_sequence: row.get(3)?,
                    install_state: row.get(4)?,
                    installed_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// upsert 安装真相（安装/升级/修复成功后在同一 DB 事务调用，Part3 §6.4 第 10 步）。
/// 主键冲突时整行覆盖并刷新 updated_at；installed_at 首装时由调用方给定，升级保留原值由上层决定。
pub fn upsert_exotic_plugin(
    conn: &Connection,
    rec: &crate::exotic::InstalledPluginRecord,
) -> Result<()> {
    conn.execute(
        "INSERT INTO exotic_plugins
            (plugin_id, version, manifest_hash, package_sequence, install_state, installed_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(plugin_id) DO UPDATE SET
            version=excluded.version,
            manifest_hash=excluded.manifest_hash,
            package_sequence=excluded.package_sequence,
            install_state=excluded.install_state,
            updated_at=excluded.updated_at",
        params![
            rec.plugin_id,
            rec.version,
            rec.manifest_hash,
            rec.package_sequence,
            rec.install_state,
            rec.installed_at,
            rec.updated_at,
        ],
    )?;
    Ok(())
}

/// 仅更新某插件的安装状态（installed/disabled/broken），刷新 updated_at。返回行数。
/// 完整性校验失败 → broken；禁用 → disabled。
pub fn set_exotic_plugin_state(conn: &Connection, plugin_id: &str, state: &str) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE exotic_plugins SET install_state=?2, updated_at=strftime('%s','now')
         WHERE plugin_id=?1",
        params![plugin_id, state],
    )?)
}

/// 删除安装真相（卸载时原子移走目录后在同一事务调用，Part3 §6.5）。返回是否删除。
/// 不删除媒体记录与历史任务（卸载不丢用户数据，§6.5）。
pub fn delete_exotic_plugin(conn: &Connection, plugin_id: &str) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM exotic_plugins WHERE plugin_id=?1",
        params![plugin_id],
    )?;
    Ok(n == 1)
}

/// 孤儿恢复：只回收**过期租约**的 processing 任务（claimed_at 为空或早于 now-lease_ttl）。
/// 不能在另一合法 App 实例仍工作时全量 1→0（R2）。返回回收行数。
pub fn recover_orphaned_exotic_tasks(
    conn: &Connection,
    lease_ttl_secs: i64,
    now: i64,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE exotic_tasks
         SET status=0, claimed_at=NULL, lease_owner=NULL, updated_at=strftime('%s','now')
         WHERE status=1 AND (claimed_at IS NULL OR claimed_at < ?1)",
        params![now - lease_ttl_secs],
    )?)
}

/// 释放某实例仍持有的全部 processing 租约 → 退回 pending（Pipeline 结束/取消时清理）。
/// 已 finish/fail 的任务 lease_owner 已为 NULL，不受影响；只回收「领了但未最终化」的任务。返回行数。
pub fn release_exotic_instance_leases(conn: &Connection, instance_id: &str) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE exotic_tasks
         SET status=0, claimed_at=NULL, lease_owner=NULL, updated_at=strftime('%s','now')
         WHERE status=1 AND lease_owner=?1",
        params![instance_id],
    )?)
}

/// 统计某 (plugin,capability) 的任务计数：(pending_or_retry, processing, done, error)。
/// 进度/状态命令用。`pending_or_retry` = status 0 或 3（可领取/待重试）。
pub fn count_exotic_tasks_by_status(
    conn: &Connection,
    plugin_id: &str,
    capability: &str,
) -> Result<(i64, i64, i64, i64)> {
    conn.query_row(
        "SELECT
            SUM(CASE WHEN status IN (0,3) THEN 1 ELSE 0 END),
            SUM(CASE WHEN status=1 THEN 1 ELSE 0 END),
            SUM(CASE WHEN status=2 THEN 1 ELSE 0 END),
            SUM(CASE WHEN status=4 THEN 1 ELSE 0 END)
         FROM exotic_tasks WHERE plugin_id=?1 AND capability=?2",
        params![plugin_id, capability],
        |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            ))
        },
    )
    .map_err(AppError::from)
}

/// 单任务重置为 pending（用户「重试此项」命令）：清输出/指纹/错误/租约/退避。返回行数。
pub fn reset_exotic_task_for_retry(
    conn: &Connection,
    item_id: i64,
    capability: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE exotic_tasks
         SET status=0, attempts=0, next_retry_at=NULL, claimed_at=NULL, lease_owner=NULL,
             last_error_code=NULL, last_error_message=NULL, updated_at=strftime('%s','now')
         WHERE item_id=?1 AND capability=?2 AND status IN (3,4)",
        params![item_id, capability],
    )?)
}

/// 某插件全部 error 任务（status 3/4）重置为 pending（用户「重试插件失败」命令）。返回行数。
pub fn reset_exotic_plugin_failures(conn: &Connection, plugin_id: &str) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE exotic_tasks
         SET status=0, attempts=0, next_retry_at=NULL, claimed_at=NULL, lease_owner=NULL,
             last_error_code=NULL, last_error_message=NULL, updated_at=strftime('%s','now')
         WHERE plugin_id=?1 AND status IN (3,4)",
        params![plugin_id],
    )?)
}

/// 是否存在「现在就绪」的任务（status=0，或 status=3 且 next_retry_at 已到）。
/// Coordinator 据此决定是否启动 Pipeline，避免空跑。
pub fn has_ready_exotic_task(
    conn: &Connection,
    plugin_id: &str,
    capability: &str,
    now: i64,
) -> Result<bool> {
    let exists: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exotic_tasks
            WHERE plugin_id=?1 AND capability=?2
              AND ( status=0 OR (status=3 AND (next_retry_at IS NULL OR next_retry_at<=?3)) ))",
        params![plugin_id, capability, now],
        |r| r.get(0),
    )?;
    Ok(exists != 0)
}

/// 取 exotic 任务处理所需的源信息：绝对路径 + cache_key + 小写扩展名。
/// 经 directories JOIN scan_roots 解析绝对路径（与 `get_media_detail` 同路径解析）。
pub fn exotic_item_source(conn: &Connection, item_id: i64) -> Result<ExoticItemSource> {
    let (file_name, file_format, cache_key, rel_path, root_path): (
        String,
        String,
        i64,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT m.file_name, m.file_format, m.cache_key, d.rel_path, r.path
             FROM media_items m
             JOIN directories d ON m.directory_id = d.id
             JOIN scan_roots r ON d.root_id = r.id
             WHERE m.id=?1",
            params![item_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|_| AppError::MediaNotFound(item_id))?;
    let abs_path = crate::utils::path::resolve_media_path(&root_path, &rel_path, &file_name);
    Ok(ExoticItemSource {
        abs_path,
        cache_key,
        file_format,
    })
}

/// [`exotic_item_source`] 返回值。
#[derive(Debug, Clone)]
pub struct ExoticItemSource {
    pub abs_path: String,
    pub cache_key: i64,
    pub file_format: String,
}

// ── 卷可用性 DAO（SCHEMA_V10；供 Part2 卷探测 probe_volumes / 扫描编排调用）─────────
//
// 🔴 离线≠删除硬规则的数据侧落点：`bulk_set_availability` 是仅有的合法整盘状态切换；
// 本模块**不提供**「绕过卷判断的批量删除」DAO——差集删除的 SQL 守门在 Part2。

/// upsert 卷：`stable_id` 冲突则更新 label/kind/last_mount_path/last_seen/is_online（保留 id/created_at）。
/// 返回卷 id（新建或既有）。用 RETURNING 一次拿回 id（与仓内既有 upsert 范式一致；rusqlite bundled SQLite 支持）。
pub fn upsert_volume(conn: &Connection, v: &NewVolume) -> Result<i64> {
    let id = conn.query_row(
        "INSERT INTO volumes (stable_id, label, kind, last_mount_path, last_seen, is_online)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(stable_id) DO UPDATE SET
             label           = excluded.label,
             kind            = excluded.kind,
             last_mount_path = excluded.last_mount_path,
             last_seen       = excluded.last_seen,
             is_online       = excluded.is_online
         RETURNING id",
        params![
            v.stable_id,
            v.label,
            v.kind.as_str(),
            v.last_mount_path,
            v.last_seen,
            v.is_online as i64,
        ],
        |row| row.get(0),
    )?;
    Ok(id)
}

/// 按 stable_id 取卷（不存在返回 None）。
pub fn get_volume_by_stable_id(conn: &Connection, stable_id: &str) -> Result<Option<Volume>> {
    conn.query_row(
        "SELECT id, stable_id, label, kind, last_mount_path, last_seen, is_online, created_at
         FROM volumes WHERE stable_id = ?1",
        params![stable_id],
        map_volume,
    )
    .optional()
    .map_err(AppError::from)
}

/// 列出全部卷（设置「已知卷」面板用）。按登记时间稳定排序。
pub fn list_volumes(conn: &Connection) -> Result<Vec<Volume>> {
    let mut stmt = conn.prepare(
        "SELECT id, stable_id, label, kind, last_mount_path, last_seen, is_online, created_at
         FROM volumes ORDER BY created_at ASC, id ASC",
    )?;
    let rows = stmt.query_map([], map_volume)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// 切换卷在线态（probe 刷新）。online 时刷新 `last_seen`；`mount_path` 为 Some 时更新挂载点，
/// 为 None 时保留最后已知挂载点（离线后仍可向用户提示「上次在 X:」）。
pub fn set_volume_online(
    conn: &Connection,
    stable_id: &str,
    online: bool,
    mount_path: Option<&str>,
    now: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE volumes SET
             is_online       = ?2,
             last_seen       = CASE WHEN ?2 = 1 THEN ?3 ELSE last_seen END,
             last_mount_path = COALESCE(?4, last_mount_path)
         WHERE stable_id = ?1",
        params![stable_id, online as i64, now, mount_path],
    )?;
    Ok(())
}

/// 重命名卷标（用户在「已知卷」面板改名）。
pub fn rename_volume_label(conn: &Connection, volume_id: i64, label: &str) -> Result<()> {
    conn.execute(
        "UPDATE volumes SET label = ?2 WHERE id = ?1",
        params![volume_id, label],
    )?;
    Ok(())
}

/// 删除卷登记。依赖 FK `ON DELETE SET NULL`（需连接开启 `PRAGMA foreign_keys=ON`，迁移已启用）：
/// `scan_roots.volume_id` / `media_items.volume_id` 自动置 NULL；**是否软删媒体由调用方决定**，本 DAO 不删媒体。
pub fn delete_volume(conn: &Connection, volume_id: i64) -> Result<()> {
    conn.execute("DELETE FROM volumes WHERE id = ?1", params![volume_id])?;
    Ok(())
}

/// 列出全部卷 + 各卷「未删除媒体数」（「已知卷」面板展示用）。
/// LEFT JOIN 保证零媒体的卷也出现；`FILTER (is_deleted=0)` 排除回收站项（离线≠删除，回收站另计）。
pub fn list_volumes_with_item_counts(conn: &Connection) -> Result<Vec<(Volume, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT v.id, v.stable_id, v.label, v.kind, v.last_mount_path, v.last_seen,
                v.is_online, v.created_at,
                COUNT(m.id) FILTER (WHERE m.is_deleted = 0) AS item_count
         FROM volumes v
         LEFT JOIN media_items m ON m.volume_id = v.id
         GROUP BY v.id
         ORDER BY v.created_at ASC, v.id ASC",
    )?;
    let rows = stmt.query_map([], |row| Ok((map_volume(row)?, row.get::<_, i64>(8)?)))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// 判定某媒体项当前是否「所在卷离线」：离线返回 `Some(卷标签)`（label 缺省用 stable_id 兜底），
/// 在线 / 无绑定卷返回 `None`。供打开原图/视频等实体访问的 IPC 门控（离线即返 `VolumeOffline`，非破图）。
pub fn get_item_volume_offline_label(conn: &Connection, item_id: i64) -> Result<Option<String>> {
    conn.query_row(
        "SELECT COALESCE(v.label, v.stable_id)
         FROM media_items m JOIN volumes v ON m.volume_id = v.id
         WHERE m.id = ?1 AND v.is_online = 0",
        params![item_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(AppError::from)
}

/// 批量切换整盘可用性（拔出 `'online'→'offline'` / 重连 `'offline'→'online'`），单字段过滤、走 `idx_media_volume`。
/// 返回受影响行数。**绝不**在此写 `is_deleted`——离线≠删除（Part1 §3.3c 硬规则）。
pub fn bulk_set_availability(
    conn: &Connection,
    volume_id: i64,
    from: &str,
    to: &str,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE media_items SET availability = ?3 WHERE volume_id = ?1 AND availability = ?2",
        params![volume_id, from, to],
    )?)
}

/// 缺失检测三重守门差集（Part2 §3.2.2）：把「在线卷上、本 scan_root 子树内、本次未出现」的项
/// 标 `availability='missing'`（**绝不写 is_deleted**，与用户回收站正交）。返回标记数。
///
/// 三重守门（缺一不可）：
///   - 守门1：仅 `online_volume_ids` 内的卷（离线卷的项绝不参与——离线≠删除）。
///   - 守门2：仅本 scan_root 子树（经 directories.root_id 关联；media_items 无 scan_root 列）。
///   - 守门3：本次扫描 `seen_ids` 之外（`seen` 须含 Unchanged，否则未变更文件被误标）。
///
/// `dry_run=true` 时只 `SELECT count(*)`（返回**将标记数**）、绝不 UPDATE——供灰度核对 / 可观测。
/// seen/online 用连接级 TEMP 表装载（百万级友好：`NOT IN` 改 TEMP 表子查询，避免巨型字面量列表；
/// 全程参数绑定）。**调用前置**（由调用方保证）：扫描完整（WalkReport.complete）+ 卷在线已复查（TOCTOU）。
pub fn mark_missing(
    conn: &Connection,
    root_id: i64,
    online_volume_ids: &[i64],
    seen_ids: &std::collections::HashSet<i64>,
    dry_run: bool,
) -> Result<usize> {
    // 连接级 TEMP 表（随连接释放）；同连接复用先清空。
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS _mm_seen(id INTEGER PRIMARY KEY);
         CREATE TEMP TABLE IF NOT EXISTS _mm_online(id INTEGER PRIMARY KEY);
         DELETE FROM _mm_seen;
         DELETE FROM _mm_online;",
    )?;
    {
        let mut s = conn.prepare("INSERT OR IGNORE INTO _mm_seen(id) VALUES (?1)")?;
        for id in seen_ids {
            s.execute(params![id])?;
        }
        let mut o = conn.prepare("INSERT OR IGNORE INTO _mm_online(id) VALUES (?1)")?;
        for v in online_volume_ids {
            o.execute(params![v])?;
        }
    }

    // 守门2(子树) + 守门1(在线卷 TEMP) + is_deleted=0 + 已是 missing 不重标 + 守门3(¬seen TEMP)。
    // 注：volume_id 为 NULL 的孤儿行 `NULL IN (...)`→非真→天然排除（不误标孤儿）。
    if dry_run {
        let n: i64 = conn.query_row(
            "SELECT count(*) FROM media_items
              WHERE directory_id IN (SELECT id FROM directories WHERE root_id = ?1)
                AND volume_id IN (SELECT id FROM _mm_online)
                AND is_deleted = 0
                AND availability != 'missing'
                AND id NOT IN (SELECT id FROM _mm_seen)",
            params![root_id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    } else {
        let n = conn.execute(
            "UPDATE media_items SET availability = 'missing', updated_at = strftime('%s','now')
              WHERE directory_id IN (SELECT id FROM directories WHERE root_id = ?1)
                AND volume_id IN (SELECT id FROM _mm_online)
                AND is_deleted = 0
                AND availability != 'missing'
                AND id NOT IN (SELECT id FROM _mm_seen)",
            params![root_id],
        )?;
        Ok(n)
    }
}

// ── 文档元数据 DAO（document_meta，Phase 2「死表」激活）────────────────────────
//
// 该表 SCHEMA_V1 即建但此前无 DAO（死表）。文档 enrichment（Part3）算出页数/子类型后写入，
// 供阅读器进度条 / 封面派生消费。

/// upsert 文档元数据（页数 / 子类型）。`item_id` 冲突则覆盖（重新 enrich 即更新）。
pub fn upsert_document_meta(
    conn: &Connection,
    item_id: i64,
    page_count: Option<i64>,
    doc_subtype: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO document_meta (item_id, page_count, doc_subtype) VALUES (?1, ?2, ?3)
         ON CONFLICT(item_id) DO UPDATE SET
             page_count  = excluded.page_count,
             doc_subtype = excluded.doc_subtype",
        params![item_id, page_count, doc_subtype],
    )?;
    Ok(())
}

/// 取文档元数据（不存在返回 None）。
pub fn get_document_meta(conn: &Connection, item_id: i64) -> Result<Option<DocumentMeta>> {
    conn.query_row(
        "SELECT item_id, page_count, doc_subtype FROM document_meta WHERE item_id = ?1",
        params![item_id],
        |row| {
            Ok(DocumentMeta {
                item_id: row.get(0)?,
                page_count: row.get(1)?,
                doc_subtype: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(AppError::from)
}

#[cfg(test)]
mod mark_missing_tests {
    use super::*;
    use std::collections::HashSet;

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        // 两个 scan_root + 各一目录（root1→dir10，root2→dir20）。
        c.execute_batch(
            "INSERT INTO scan_roots (id, path, alias) VALUES (1, '/r1', 'R1'), (2, '/r2', 'R2');
             INSERT INTO directories (id, root_id, rel_path, name) VALUES (10, 1, '', 'r1'), (20, 2, '', 'r2');",
        )
        .unwrap();
        c
    }

    fn add(c: &Connection, id: i64, dir: i64, vol: Option<i64>, avail: &str, is_deleted: i64) {
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key, volume_id, availability, is_deleted)
             VALUES (?1, ?2, ?3, 0, 0, 'jpg', 'image', 0, 0, 0, 0, ?4, ?5, ?6)",
            params![id, dir, format!("{id}.jpg"), vol, avail, is_deleted],
        )
        .unwrap();
    }

    fn avail(c: &Connection, id: i64) -> String {
        c.query_row(
            "SELECT availability FROM media_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// 三重守门矩阵：只有「在线卷 + 本根子树 + 未 seen + 未删」的项被标 missing，其余四类各被一道闸拦住。
    #[test]
    fn three_gates_mark_only_genuinely_missing() {
        let c = mem_db();
        add(&c, 100, 10, Some(5), "online", 0); // ✓ 应标：在线卷5 + root1 + 不在 seen
        add(&c, 101, 10, Some(5), "online", 0); // ✗ 在 seen（守门3）
        add(&c, 102, 10, Some(6), "online", 0); // ✗ 卷6 不在线（守门1）
        add(&c, 103, 20, Some(5), "online", 0); // ✗ root2 子树（守门2）
        add(&c, 104, 10, Some(5), "online", 1); // ✗ is_deleted=1（用户回收站）
        add(&c, 105, 10, None, "online", 0); // ✗ 孤儿 volume_id=NULL（天然排除）

        let seen: HashSet<i64> = HashSet::from([101]);
        let n = mark_missing(&c, 1, &[5], &seen, false).unwrap();
        assert_eq!(n, 1, "仅 1 项真缺失");

        assert_eq!(avail(&c, 100), "missing", "真缺失项应标 missing");
        for id in [101, 102, 103, 104, 105] {
            assert_eq!(
                avail(&c, id),
                "online",
                "id={id} 应被某道闸拦住、保持 online"
            );
        }
        // is_deleted 项绝不被 availability 逻辑改动其删除态。
        let d: i64 = c
            .query_row("SELECT is_deleted FROM media_items WHERE id=104", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(d, 1);
    }

    /// dry_run：返回将标记数但 DB 零改动。
    #[test]
    fn dry_run_counts_without_writing() {
        let c = mem_db();
        add(&c, 200, 10, Some(5), "online", 0);
        add(&c, 201, 10, Some(5), "online", 0);
        let seen: HashSet<i64> = HashSet::new(); // 全部未出现

        let n = mark_missing(&c, 1, &[5], &seen, true).unwrap();
        assert_eq!(n, 2, "dry_run 应返回将标记数 2");
        // DB 未变。
        assert_eq!(avail(&c, 200), "online");
        assert_eq!(avail(&c, 201), "online");
    }

    /// 离线卷（online 集为空）：一项都不标——离线 ≠ 删除的第二层冗余防护。
    #[test]
    fn empty_online_set_marks_nothing() {
        let c = mem_db();
        add(&c, 300, 10, Some(5), "online", 0);
        let seen: HashSet<i64> = HashSet::new();
        let n = mark_missing(&c, 1, &[], &seen, false).unwrap();
        assert_eq!(n, 0, "无在线卷 → 一项不标");
        assert_eq!(avail(&c, 300), "online");
    }

    /// 多在线卷：online 集含多卷时，任一在线卷上的未 seen 项都被标；第三卷离线则不标。
    /// 压 `_mm_online` TEMP 表多行 + `volume_id IN (SELECT ...)` 子查询路径。
    #[test]
    fn multiple_online_volumes_all_covered() {
        let c = mem_db();
        add(&c, 100, 10, Some(5), "online", 0); // 在线卷5 → 标
        add(&c, 101, 10, Some(7), "online", 0); // 在线卷7 → 标
        add(&c, 102, 10, Some(9), "online", 0); // 卷9 离线（不在 online 集）→ 不标
        let seen: HashSet<i64> = HashSet::new();

        let n = mark_missing(&c, 1, &[5, 7], &seen, false).unwrap();
        assert_eq!(n, 2, "两在线卷上的未 seen 项均被标");
        assert_eq!(avail(&c, 100), "missing");
        assert_eq!(avail(&c, 101), "missing");
        assert_eq!(avail(&c, 102), "online", "离线卷9 上的项不受影响（守门1）");
    }

    /// 同连接复用：第二次调用必须先清空 TEMP 表，不被首次的 seen 集污染。
    /// 这是连接池复用下的真实数据安全点——清空逻辑若失效，二次扫描会用陈旧集合 → 误标/漏标。
    #[test]
    fn reuse_same_connection_clears_temp_state() {
        let c = mem_db();
        add(&c, 100, 10, Some(5), "online", 0);
        add(&c, 101, 10, Some(5), "online", 0);

        // 第一次：seen={100} → 仅 101 缺失。
        let seen1: HashSet<i64> = HashSet::from([100]);
        let n1 = mark_missing(&c, 1, &[5], &seen1, false).unwrap();
        assert_eq!(n1, 1);
        assert_eq!(avail(&c, 101), "missing");

        // 恢复 101，再以「相反」的 seen 集第二次调用（dry_run 纯验集合隔离、不改库）。
        c.execute(
            "UPDATE media_items SET availability='online' WHERE id=101",
            [],
        )
        .unwrap();
        // 第二次：seen={101} → 仅 100 应计。若 TEMP 未清空，残留 seen={100} 会污染 → 算成 0。
        let seen2: HashSet<i64> = HashSet::from([101]);
        let n2 = mark_missing(&c, 1, &[5], &seen2, true).unwrap();
        assert_eq!(n2, 1, "二次调用仅 100 缺失；若被首次 seen 污染则会误算成 0");
    }

    /// 大集合规模正确性：1000 项，奇数 id 全 seen、偶数 id 未 seen → 恰好 500 偶数项被标。
    /// 压 TEMP 表 join 在规模下的正确性（C1 「large-set temp-table path」加固）。
    #[test]
    fn large_set_marks_exactly_unseen() {
        let c = mem_db();
        let mut seen: HashSet<i64> = HashSet::new();
        for id in 1..=1000i64 {
            add(&c, id, 10, Some(5), "online", 0);
            if id % 2 == 1 {
                seen.insert(id); // 奇数已 seen
            }
        }
        let n = mark_missing(&c, 1, &[5], &seen, false).unwrap();
        assert_eq!(n, 500, "恰好 500 个偶数 id 未 seen → 被标");
        assert_eq!(avail(&c, 2), "missing", "偶数 id（未 seen）应标 missing");
        assert_eq!(avail(&c, 3), "online", "奇数 id（已 seen）应保持 online");
    }

    /// 已是 missing 的项不被重标/重计（`availability != 'missing'` 闸）——保证重复扫描幂等、计数不虚高。
    #[test]
    fn already_missing_not_remarked() {
        let c = mem_db();
        add(&c, 100, 10, Some(5), "online", 0); // online 未 seen → 标
        add(&c, 101, 10, Some(5), "missing", 0); // 已 missing 未 seen → 不再计
        let seen: HashSet<i64> = HashSet::new();

        let n = mark_missing(&c, 1, &[5], &seen, false).unwrap();
        assert_eq!(n, 1, "仅新缺失项计入，已 missing 不重复计");
        assert_eq!(avail(&c, 100), "missing");
        assert_eq!(avail(&c, 101), "missing");
    }
}

#[cfg(test)]
mod fast_scan_upsert_recovery_tests {
    use super::*;

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap(); // 免构造 directory 链
        c
    }

    fn item(name: &str, mtime: i64) -> FastScanItem {
        FastScanItem {
            directory_id: 1,
            file_name: name.into(),
            file_size: 10,
            file_mtime: mtime,
            file_format: "jpg".into(),
            media_type: "image".into(),
            width: 0,
            height: 0,
            sort_datetime: mtime,
            cache_key: 0,
        }
    }

    fn insert_missing(c: &Connection, id: i64, name: &str, mtime: i64) {
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key, availability)
             VALUES (?1, 1, ?2, 10, ?3, 'jpg', 'image', 0, 0, ?3, 0, 'missing')",
            params![id, name, mtime],
        )
        .unwrap();
    }

    fn avail(c: &Connection, id: i64) -> String {
        c.query_row(
            "SELECT availability FROM media_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// 原样重现（同 mtime）：Unchanged + availability 自动 missing→online；is_deleted 不动。
    #[test]
    fn unchanged_reappearance_recovers_missing() {
        let c = mem_db();
        insert_missing(&c, 1, "a.jpg", 100);
        let out = upsert_fast_scan_item(&c, &item("a.jpg", 100), None).unwrap();
        assert_eq!(out, UpsertOutcome::Unchanged(1));
        assert_eq!(avail(&c, 1), "online", "原样重现应自动恢复 online");
        let deleted: i64 = c
            .query_row("SELECT is_deleted FROM media_items WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(deleted, 0, "恢复不得碰 is_deleted");
    }

    /// 变更重现（mtime 变）：SourceChanged + availability 经 CASE 恢复。
    #[test]
    fn changed_reappearance_recovers_missing() {
        let c = mem_db();
        insert_missing(&c, 2, "b.jpg", 100);
        let out = upsert_fast_scan_item(&c, &item("b.jpg", 200), None).unwrap();
        assert_eq!(out, UpsertOutcome::SourceChanged(2));
        assert_eq!(avail(&c, 2), "online");
    }

    /// SourceChanged（mtime 变）：删除旧 image/video/audio_meta（enricher 据 IS NULL 重选重算）。
    /// Unchanged 不删（避免给未变更项白删 meta）。
    #[test]
    fn source_changed_invalidates_meta_but_unchanged_keeps() {
        let c = mem_db();
        // 既有项 + 三类 meta 各一行。
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key)
             VALUES (5, 1, 'd.jpg', 10, 100, 'jpg', 'image', 0, 0, 100, 0)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO image_meta (item_id) VALUES (5)", [])
            .unwrap();
        c.execute("INSERT INTO video_meta (item_id) VALUES (5)", [])
            .unwrap();
        c.execute("INSERT INTO audio_meta (item_id) VALUES (5)", [])
            .unwrap();
        // Part3 Q5：一条已完成派生（status=2，带旧产物路径 + 残留错误）——SourceChanged 须复位重派。
        c.execute(
            "INSERT INTO media_derivations (item_id, kind, status, payload_path, error)
             VALUES (5, 'video_cover', 2, 'old/cover.webp', 'stale')",
            [],
        )
        .unwrap();
        // Part4 T4：AI/人脸 fixtures。ai_status/face_status=2（done），item 5 一条向量；person 1
        // 两脸（item 5 一张、item 6 一张），用于验证源变后 item 5 的向量/脸删、状态复位、person 重算。
        // 嵌入/质心 = X'0000803F00000000' = [1.0, 0.0]（2×f32 LE）。
        c.execute(
            "UPDATE media_items SET ai_status=2, face_status=2 WHERE id=5",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO ai_embeddings (item_id, model_name, embedding) VALUES (5, 'clip', X'0000803F00000000')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO persons (id, name, model_name, centroid, face_count)
             VALUES (1, NULL, 'yunet-sface', X'0000803F00000000', 2)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO faces (id, item_id, person_id, model_name, bbox_x, bbox_y, bbox_w, bbox_h,
                                det_score, quality, embedding, is_confirmed)
             VALUES (50, 5, 1, 'yunet-sface', 0.1, 0.1, 0.2, 0.2, 0.9, 0.5, X'0000803F00000000', 0),
                    (51, 6, 1, 'yunet-sface', 0.1, 0.1, 0.2, 0.2, 0.9, 0.6, X'0000803F00000000', 0)",
            [],
        )
        .unwrap();

        let ai_face_status = |c: &Connection| -> (i64, i64) {
            c.query_row(
                "SELECT ai_status, face_status FROM media_items WHERE id=5",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        let count_where =
            |c: &Connection, sql: &str| -> i64 { c.query_row(sql, [], |r| r.get(0)).unwrap() };

        let meta_count = |c: &Connection| -> i64 {
            let q = |t: &str| -> i64 {
                c.query_row(
                    &format!("SELECT count(*) FROM {t} WHERE item_id=5"),
                    [],
                    |r| r.get(0),
                )
                .unwrap()
            };
            q("image_meta") + q("video_meta") + q("audio_meta")
        };
        // 派生行的 (status, payload_path 是否为空, error 是否为空)。
        let deriv_state = |c: &Connection| -> (i64, bool, bool) {
            c.query_row(
                "SELECT status, payload_path IS NULL, error IS NULL
                   FROM media_derivations WHERE item_id=5 AND kind='video_cover'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        };

        // mtime 未变 → Unchanged → meta 保留 + 派生保持 done（不白删/不白重派）+ AI/人脸不动。
        let out0 = upsert_fast_scan_item(&c, &item("d.jpg", 100), None).unwrap();
        assert_eq!(out0, UpsertOutcome::Unchanged(5));
        assert_eq!(meta_count(&c), 3, "未变更不应删 meta");
        assert_eq!(
            deriv_state(&c),
            (2, false, false),
            "未变更不应复位派生（应仍为 done、保留产物路径）"
        );
        assert_eq!(ai_face_status(&c), (2, 2), "未变更不应复位 AI/人脸状态");
        assert_eq!(
            count_where(&c, "SELECT count(*) FROM ai_embeddings WHERE item_id=5"),
            1,
            "未变更不应删向量"
        );

        // mtime 变 → SourceChanged → meta 全删 + 派生退回 pending + AI/人脸失效（T4）。
        let out = upsert_fast_scan_item(&c, &item("d.jpg", 200), None).unwrap();
        assert_eq!(out, UpsertOutcome::SourceChanged(5));
        assert_eq!(meta_count(&c), 0, "SourceChanged 应删除全部旧 meta");
        assert_eq!(
            deriv_state(&c),
            (0, true, true),
            "SourceChanged 应把派生退回 pending(0) 并清空 payload_path/error"
        );
        // Part4 T4 断言：状态复位 0、item 5 的向量/脸删除、person 1 重算（仅剩 item 6 的脸 51）。
        assert_eq!(
            ai_face_status(&c),
            (0, 0),
            "SourceChanged 应复位 ai_status/face_status"
        );
        assert_eq!(
            count_where(&c, "SELECT count(*) FROM ai_embeddings WHERE item_id=5"),
            0,
            "应删 item 5 的旧 CLIP 向量"
        );
        assert_eq!(
            count_where(&c, "SELECT count(*) FROM faces WHERE item_id=5"),
            0,
            "应删 item 5 的旧人脸"
        );
        assert_eq!(
            count_where(&c, "SELECT count(*) FROM faces WHERE id=51"),
            1,
            "其它 item 的脸不受影响"
        );
        assert_eq!(
            count_where(&c, "SELECT face_count FROM persons WHERE id=1"),
            1,
            "受影响 person 应重算（2→1，仅剩 item 6 的脸）"
        );
    }

    /// 普通 online 未变更：Unchanged，availability 保持 online（CASE/条件均不误改）。
    #[test]
    fn unchanged_online_stays_online() {
        let c = mem_db();
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key)
             VALUES (3, 1, 'c.jpg', 10, 100, 'jpg', 'image', 0, 0, 100, 0)",
            [],
        )
        .unwrap();
        let out = upsert_fast_scan_item(&c, &item("c.jpg", 100), None).unwrap();
        assert_eq!(out, UpsertOutcome::Unchanged(3));
        assert_eq!(avail(&c, 3), "online");
    }

    fn vol_of(c: &Connection, id: i64) -> Option<i64> {
        c.query_row(
            "SELECT volume_id FROM media_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// 新项据扫描上下文卷入库（修复:此前新项 volume_id 恒 NULL → 缺失检测对新数据休眠）。
    #[test]
    fn new_item_inherits_scan_volume() {
        let c = mem_db();
        let out = upsert_fast_scan_item(&c, &item("n.jpg", 100), Some(7)).unwrap();
        let id = match out {
            UpsertOutcome::Inserted(id) => id,
            other => panic!("应为 Inserted，实得 {other:?}"),
        };
        assert_eq!(vol_of(&c, id), Some(7), "新项应继承本根卷 id");
    }

    /// 历史遗留 volume_id=NULL 的项：再扫描时经 COALESCE 顺带治愈（Unchanged 定向写、SourceChanged 顺带）。
    #[test]
    fn legacy_null_volume_healed_on_rescan() {
        let c = mem_db();
        // 模拟修复前插入的项：volume_id 为 NULL。
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key)
             VALUES (9, 1, 'old.jpg', 10, 100, 'jpg', 'image', 0, 0, 100, 0)",
            [],
        )
        .unwrap();
        assert_eq!(vol_of(&c, 9), None, "前置：历史项 volume_id 应为 NULL");

        // 原样重扫（mtime 不变）→ Unchanged，但 NULL 卷应被定向治愈。
        let out = upsert_fast_scan_item(&c, &item("old.jpg", 100), Some(7)).unwrap();
        assert_eq!(out, UpsertOutcome::Unchanged(9));
        assert_eq!(vol_of(&c, 9), Some(7), "Unchanged 路径应治愈历史 NULL 卷");
    }

    /// COALESCE 不覆盖既有卷：已绑卷的项再扫描，volume_id 保持原值（即便传入不同卷）。
    #[test]
    fn existing_volume_not_overwritten() {
        let c = mem_db();
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key, volume_id)
             VALUES (11, 1, 'bound.jpg', 10, 100, 'jpg', 'image', 0, 0, 100, 0, 3)",
            [],
        )
        .unwrap();
        // mtime 变 → SourceChanged，传入卷 9，但既有卷 3 应被 COALESCE 保留。
        let out = upsert_fast_scan_item(&c, &item("bound.jpg", 200), Some(9)).unwrap();
        assert_eq!(out, UpsertOutcome::SourceChanged(11));
        assert_eq!(vol_of(&c, 11), Some(3), "既有卷不应被覆盖");
    }
}

#[cfg(test)]
mod trash_keyset_tests {
    use super::*;

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap(); // 免构造 directory 链
        c
    }

    fn add_trashed(c: &Connection, id: i64, deleted_at: i64) {
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key, is_deleted, deleted_at)
             VALUES (?1, 1, ?2, 0, 0, 'jpg', 'image', 0, 0, 0, 0, 1, ?3)",
            params![id, format!("{id}.jpg"), deleted_at],
        )
        .unwrap();
    }

    /// keyset 逐页拼接 == 全量 (deleted_at DESC, id DESC) 序，无重叠无遗漏；同 deleted_at 时 id 次键生效。
    #[test]
    fn keyset_pages_match_full_order() {
        let c = mem_db();
        add_trashed(&c, 1, 100);
        add_trashed(&c, 2, 100); // 与 id=1 同 deleted_at → 靠 id 次键定序
        add_trashed(&c, 3, 200);
        add_trashed(&c, 4, 50);
        // 期望 (deleted_at DESC, id DESC)：(200,3),(100,2),(100,1),(50,4) → [3,2,1,4]
        let expected = vec![3i64, 2, 1, 4];

        // 基线：一页取全。
        let all = get_trash_keyset(&c, None, 100).unwrap();
        assert_eq!(
            all.iter().map(|m| m.id).collect::<Vec<_>>(),
            expected,
            "全量序错"
        );

        // size=2 keyset 翻页。
        let mut got = Vec::new();
        let mut cursor: Option<(i64, i64)> = None;
        loop {
            let page = get_trash_keyset(&c, cursor, 2).unwrap();
            if page.is_empty() {
                break;
            }
            let last = page.last().unwrap();
            cursor = Some((last.deleted_at.unwrap_or(0), last.id));
            got.extend(page.iter().map(|m| m.id));
        }
        assert_eq!(got, expected, "keyset 逐页拼接应等于全量序，无重叠无遗漏");
    }
}

#[cfg(test)]
mod scan_root_backend_tests {
    use super::*;

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        // backend_id FK→storage_backends；关 FK 免构造后端行（DAO 逻辑测试）。
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c
    }

    /// insert 默认 backend_id=None（本地）；set_scan_root_backend 绑定/解绑；list/get 正确回读。
    #[test]
    fn backend_id_insert_set_and_roundtrip() {
        let c = mem_db();
        let id = insert_scan_root(&c, "/photos", Some("图库"), None).unwrap();
        assert_eq!(
            get_scan_root(&c, id).unwrap().backend_id,
            None,
            "默认应为本地 None"
        );

        // 绑定到后端 7。
        set_scan_root_backend(&c, id, Some(7)).unwrap();
        assert_eq!(get_scan_root(&c, id).unwrap().backend_id, Some(7));
        // list 同样回读该列。
        let listed = list_scan_roots(&c).unwrap();
        assert_eq!(listed[0].backend_id, Some(7));

        // 解绑回本地。
        set_scan_root_backend(&c, id, None).unwrap();
        assert_eq!(get_scan_root(&c, id).unwrap().backend_id, None);

        // insert 时直接带 backend_id。
        let id2 = insert_scan_root(&c, "/net", None, Some(3)).unwrap();
        assert_eq!(get_scan_root(&c, id2).unwrap().backend_id, Some(3));
    }
}

#[cfg(test)]
mod document_meta_tests {
    use super::*;

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        // document_meta.item_id FK→media_items；关 FK 免构造 media 行（DAO 逻辑测试）。
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c
    }

    /// upsert 首写 → get 命中；同 item_id 再 upsert → 覆盖（页数/子类型同时更新）。
    #[test]
    fn upsert_get_and_overwrite() {
        let c = mem_db();
        assert!(
            get_document_meta(&c, 1).unwrap().is_none(),
            "未写入应为 None"
        );

        upsert_document_meta(&c, 1, Some(10), Some("pdf")).unwrap();
        let m = get_document_meta(&c, 1).unwrap().unwrap();
        assert_eq!(m.item_id, 1);
        assert_eq!(m.page_count, Some(10));
        assert_eq!(m.doc_subtype.as_deref(), Some("pdf"));

        // 重新 enrich：覆盖为 epub + 新页数。
        upsert_document_meta(&c, 1, Some(12), Some("epub")).unwrap();
        let m2 = get_document_meta(&c, 1).unwrap().unwrap();
        assert_eq!(m2.page_count, Some(12), "页数应被覆盖");
        assert_eq!(m2.doc_subtype.as_deref(), Some("epub"), "子类型应被覆盖");

        // None 字段也能存（未知页数）。
        upsert_document_meta(&c, 2, None, Some("svg")).unwrap();
        let m3 = get_document_meta(&c, 2).unwrap().unwrap();
        assert_eq!(m3.page_count, None);
    }
}

#[cfg(test)]
mod volume_dao_tests {
    use super::*;
    use crate::db::models::{NewVolume, VolumeKind};

    /// 全新内存库（含 V10 卷表）。FK 状态由各测试**显式**设置——FK 是建连接时（connection.rs）开，
    /// 非 run_migrations 副作用，故不依赖隐式默认。
    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c
    }

    fn new_vol(stable: &str) -> NewVolume {
        NewVolume {
            stable_id: stable.into(),
            label: Some("U盘".into()),
            kind: VolumeKind::Removable,
            last_mount_path: Some("E:\\".into()),
            last_seen: Some(1000),
            is_online: true,
        }
    }

    /// upsert：首次插入返回新 id；同 stable_id 再 upsert 返回**同一 id** 并更新字段（非新建行）。
    #[test]
    fn upsert_inserts_then_updates_on_stable_id_conflict() {
        let c = mem_db();
        let id1 = upsert_volume(&c, &new_vol("vol-A")).unwrap();

        let mut v2 = new_vol("vol-A");
        v2.label = Some("改名盘".into());
        v2.kind = VolumeKind::Network;
        v2.is_online = false;
        let id2 = upsert_volume(&c, &v2).unwrap();
        assert_eq!(id1, id2, "stable_id 冲突应更新同一行、返回同 id");

        let got = get_volume_by_stable_id(&c, "vol-A").unwrap().unwrap();
        assert_eq!(got.label.as_deref(), Some("改名盘"));
        assert!(matches!(got.kind, VolumeKind::Network), "kind 应被覆盖");
        assert!(!got.is_online, "is_online 应被覆盖为 false");
        assert_eq!(list_volumes(&c).unwrap().len(), 1, "不得新建第二行");

        // 未知 stable_id → None。
        assert!(get_volume_by_stable_id(&c, "missing").unwrap().is_none());
    }

    /// 端到端链（C5 Piece1+Piece2 组合）：新根建卷 + 绑定 volume_id → 该根新媒体继承卷。
    /// 证明新增扫描根的媒体能参与缺失检测守门1（修复前新根 volume_id 恒 NULL → 休眠）。
    #[test]
    fn new_root_binds_volume_and_media_inherits() {
        let c = mem_db();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();

        // 1) 建根（add_scan_root 路径：backend_id=None）。
        let root_id = insert_scan_root(&c, "C:\\Photos", Some("照片"), None).unwrap();
        // 2) 建卷 + 绑定（add_scan_root 内 Piece2 逻辑）。
        let vid = upsert_volume(&c, &new_vol("path:C:")).unwrap();
        set_scan_root_volume(&c, root_id, Some(vid)).unwrap();

        // 回读：scan_roots.volume_id 已绑定。
        let bound: Option<i64> = c
            .query_row(
                "SELECT volume_id FROM scan_roots WHERE id=?1",
                params![root_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(bound, Some(vid), "新根应绑定卷 id");

        // 3) 建目录 + 扫描插入一项（Piece1：upsert 传本根卷）。
        upsert_directory(&c, root_id, None, "", "Photos", 0, None).unwrap();
        let dir_id: i64 = c
            .query_row(
                "SELECT id FROM directories WHERE root_id=?1",
                params![root_id],
                |r| r.get(0),
            )
            .unwrap();
        let item = FastScanItem {
            directory_id: dir_id,
            file_name: "x.jpg".into(),
            file_size: 10,
            file_mtime: 100,
            file_format: "jpg".into(),
            media_type: "image".into(),
            width: 0,
            height: 0,
            sort_datetime: 100,
            cache_key: 0,
        };
        let out = upsert_fast_scan_item(&c, &item, bound).unwrap();
        let mid = out.id();

        // 媒体继承了本根卷 → 缺失检测守门1 可见。
        let media_vol: Option<i64> = c
            .query_row(
                "SELECT volume_id FROM media_items WHERE id=?1",
                params![mid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(media_vol, Some(vid), "新根的新媒体应继承本根卷 id");
    }

    /// set_volume_online：离线保留 last_seen + 挂载点；重连刷新 last_seen 且 mount_path=Some 时更新挂载点。
    #[test]
    fn set_volume_online_offline_keeps_last_known() {
        let c = mem_db();
        upsert_volume(&c, &new_vol("vol-B")).unwrap(); // last_seen=1000, mount=E:\

        // 离线：is_online=0，last_seen 不变，挂载点保留（mount_path=None）。
        set_volume_online(&c, "vol-B", false, None, 5000).unwrap();
        let off = get_volume_by_stable_id(&c, "vol-B").unwrap().unwrap();
        assert!(!off.is_online);
        assert_eq!(off.last_seen, Some(1000), "离线不得刷新 last_seen");
        assert_eq!(
            off.last_mount_path.as_deref(),
            Some("E:\\"),
            "离线保留最后已知挂载点"
        );

        // 重连到新盘符：is_online=1，last_seen=now，挂载点更新。
        set_volume_online(&c, "vol-B", true, Some("F:\\"), 9000).unwrap();
        let on = get_volume_by_stable_id(&c, "vol-B").unwrap().unwrap();
        assert!(on.is_online);
        assert_eq!(on.last_seen, Some(9000), "重连应刷新 last_seen");
        assert_eq!(
            on.last_mount_path.as_deref(),
            Some("F:\\"),
            "重连应更新挂载点"
        );
    }

    /// bulk_set_availability：整盘 online→offline 切换受影响行数正确，且**绝不触碰 is_deleted**（离线≠删除）。
    #[test]
    fn bulk_set_availability_never_touches_is_deleted() {
        let c = mem_db();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap(); // 免构造 directory 链，直接插 media
        let vid = upsert_volume(&c, &new_vol("vol-C")).unwrap();
        for i in 1..=3 {
            c.execute(
                "INSERT INTO media_items
                    (id, directory_id, file_name, file_size, file_mtime, file_format,
                     media_type, width, height, sort_datetime, cache_key, volume_id, availability)
                 VALUES (?1, 1, ?2, 0, 0, 'jpg', 'image', 0, 0, 0, 0, ?3, 'online')",
                params![i, format!("{i}.jpg"), vid],
            )
            .unwrap();
        }

        let n = bulk_set_availability(&c, vid, "online", "offline").unwrap();
        assert_eq!(n, 3, "整盘 3 行应全部 online→offline");

        // 再切一次 online→offline：0 行（已无 online）。
        assert_eq!(
            bulk_set_availability(&c, vid, "online", "offline").unwrap(),
            0
        );

        // 离线≠删除：is_deleted 必须全为 0。
        let deleted: i64 = c
            .query_row(
                "SELECT count(*) FROM media_items WHERE is_deleted = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(deleted, 0, "bulk_set_availability 绝不得写 is_deleted");
    }

    /// delete_volume：FK ON 时 `scan_roots.volume_id` 经 ON DELETE SET NULL 自动置空（不删 scan_root）。
    #[test]
    fn delete_volume_cascades_set_null_with_fk_on() {
        let c = mem_db();
        c.execute_batch("PRAGMA foreign_keys=ON;").unwrap(); // 显式开 FK 验级联
        let vid = upsert_volume(&c, &new_vol("vol-D")).unwrap();
        c.execute(
            "INSERT INTO scan_roots (id, path, alias, volume_id) VALUES (1, '/r', 'R', ?1)",
            params![vid],
        )
        .unwrap();

        delete_volume(&c, vid).unwrap();

        assert!(
            get_volume_by_stable_id(&c, "vol-D").unwrap().is_none(),
            "卷应已删除"
        );
        let sr_vol: Option<i64> = c
            .query_row("SELECT volume_id FROM scan_roots WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(
            sr_vol.is_none(),
            "删卷后 scan_roots.volume_id 应 SET NULL（scan_root 本身保留）"
        );
    }

    /// 在某卷上插一条 media（指定 availability + is_deleted）。FK 需先关（免构造 directory 链）。
    fn seed_item(c: &Connection, id: i64, vol_id: i64, avail: &str, deleted: bool) {
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key, volume_id,
                 availability, is_deleted)
             VALUES (?1, 1, ?2, 0,0,'jpg','image',0,0,0,0, ?3, ?4, ?5)",
            params![id, format!("{id}.jpg"), vol_id, avail, deleted as i64],
        )
        .unwrap();
    }

    /// 面板计数：LEFT JOIN 使零媒体卷也出现；item_count 只数未删除项（回收站项不计）。
    #[test]
    fn list_volumes_with_item_counts_excludes_deleted_and_keeps_empty_volumes() {
        let c = mem_db();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        let v1 = upsert_volume(&c, &new_vol("vol-A")).unwrap();
        let v2 = upsert_volume(&c, &new_vol("vol-B")).unwrap(); // 零媒体卷
        seed_item(&c, 1, v1, "online", false);
        seed_item(&c, 2, v1, "offline", false);
        seed_item(&c, 3, v1, "online", true); // 回收站项，不计

        let infos = list_volumes_with_item_counts(&c).unwrap();
        assert_eq!(infos.len(), 2, "两个卷都应出现（含零媒体卷）");
        let c1 = infos.iter().find(|(v, _)| v.id == v1).unwrap().1;
        let c2 = infos.iter().find(|(v, _)| v.id == v2).unwrap().1;
        assert_eq!(c1, 2, "vol-A 未删项应为 2（排除 is_deleted）");
        assert_eq!(c2, 0, "vol-B 零媒体应计 0（LEFT JOIN 仍出现）");
    }

    /// 打开门控：卷离线返回 Some(标签)，在线 / 无绑定卷返回 None。
    #[test]
    fn get_item_volume_offline_label_gates_by_online_state() {
        let c = mem_db();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        // 离线卷（label 缺省用 stable_id 兜底路径另测；此处带 label）。
        let mut off = new_vol("vol-off");
        off.is_online = false;
        let voff = upsert_volume(&c, &off).unwrap();
        let von = upsert_volume(&c, &new_vol("vol-on")).unwrap(); // is_online=true
        seed_item(&c, 1, voff, "offline", false);
        seed_item(&c, 2, von, "online", false);
        // 无绑定卷的项（volume_id=0，JOIN 不中）。
        seed_item(&c, 3, 0, "online", false);

        assert_eq!(
            get_item_volume_offline_label(&c, 1).unwrap().as_deref(),
            Some("U盘"),
            "离线卷上的项应返回卷标签"
        );
        assert!(
            get_item_volume_offline_label(&c, 2).unwrap().is_none(),
            "在线卷上的项应返回 None"
        );
        assert!(
            get_item_volume_offline_label(&c, 3).unwrap().is_none(),
            "无绑定卷的项应返回 None"
        );
    }

    /// stable_id 兜底：卷 label 为 NULL 时，离线门控返回 stable_id。
    #[test]
    fn get_item_volume_offline_label_falls_back_to_stable_id() {
        let c = mem_db();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        let mut off = new_vol("vol-nolabel");
        off.label = None;
        off.is_online = false;
        let voff = upsert_volume(&c, &off).unwrap();
        seed_item(&c, 1, voff, "offline", false);
        assert_eq!(
            get_item_volume_offline_label(&c, 1).unwrap().as_deref(),
            Some("vol-nolabel"),
            "label 缺省应回退 stable_id"
        );
    }
}

#[cfg(test)]
mod exotic_dao_tests {
    use super::*;

    const PID: &str = "exotic-image-psd";
    const CAP: &str = "thumbnail";

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        // 关 FK 以便用任意 item_id 直接插任务（DAO 逻辑测试，不构造完整 media_items）。
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c
    }

    fn seed(c: &Connection, item_id: i64) {
        seed_exotic_tasks_for_item(c, item_id, PID, &[CAP.to_string()]).unwrap();
    }

    fn claim(c: &Connection, limit: i64, owner: &str, now: i64) -> Vec<ExoticTaskRow> {
        claim_exotic_tasks(c, PID, CAP, limit, owner, now).unwrap()
    }

    #[test]
    fn atomic_claim_no_double() {
        let c = mem_db();
        for i in 1..=3 {
            seed(&c, i);
        }
        assert_eq!(claim(&c, 2, "inst-A", 1000).len(), 2);
        assert_eq!(claim(&c, 2, "inst-A", 1000).len(), 1); // 剩 1
        assert_eq!(claim(&c, 2, "inst-A", 1000).len(), 0); // 全 processing
        assert!(claim(&c, 2, "inst-A", 1000).is_empty());
    }

    #[test]
    fn lease_guards_finish() {
        let c = mem_db();
        seed(&c, 1);
        let id = claim(&c, 1, "inst-A", 1000)[0].id;
        // 错误 owner 不能完成（旧 Writer 失租）。
        assert!(!finish_exotic_task(&c, id, "inst-B", "fp", "/p.webp", "1.0.0").unwrap());
        assert!(has_blocking_exotic_thumbnail_task(&c, 1).unwrap());
        // 正确 owner 完成。
        assert!(finish_exotic_task(&c, id, "inst-A", "fp", "/p.webp", "1.0.0").unwrap());
        assert!(!has_blocking_exotic_thumbnail_task(&c, 1).unwrap()); // done 不再阻塞
    }

    #[test]
    fn install_truth_upsert_get_delete() {
        let c = mem_db();
        // 未安装 → None。
        assert!(get_exotic_plugin(&c, PID).unwrap().is_none());

        // upsert 首装。
        let rec = crate::exotic::InstalledPluginRecord {
            plugin_id: PID.into(),
            version: "1.0.0".into(),
            manifest_hash: "h1".into(),
            package_sequence: 3,
            install_state: crate::exotic::install_state::INSTALLED.into(),
            installed_at: 100,
            updated_at: 100,
        };
        upsert_exotic_plugin(&c, &rec).unwrap();
        let got = get_exotic_plugin(&c, PID).unwrap().unwrap();
        assert_eq!(got, rec);

        // upsert 同主键升级（version/sequence/hash 覆盖）。
        let upgraded = crate::exotic::InstalledPluginRecord {
            version: "1.1.0".into(),
            manifest_hash: "h2".into(),
            package_sequence: 4,
            updated_at: 200,
            ..rec.clone()
        };
        upsert_exotic_plugin(&c, &upgraded).unwrap();
        let got = get_exotic_plugin(&c, PID).unwrap().unwrap();
        assert_eq!(got.version, "1.1.0");
        assert_eq!(got.package_sequence, 4);

        // 仅改状态 → broken。
        assert_eq!(
            set_exotic_plugin_state(&c, PID, crate::exotic::install_state::BROKEN).unwrap(),
            1
        );
        assert_eq!(
            get_exotic_plugin(&c, PID).unwrap().unwrap().install_state,
            "broken"
        );

        // 删除。
        assert!(delete_exotic_plugin(&c, PID).unwrap());
        assert!(get_exotic_plugin(&c, PID).unwrap().is_none());
        assert!(!delete_exotic_plugin(&c, PID).unwrap()); // 再删 → false
    }

    #[test]
    fn recover_only_expired_lease() {
        let c = mem_db();
        seed(&c, 1);
        let _ = claim(&c, 1, "inst-A", 1000);
        // ttl=100：now=1010 未过期 → 不回收。
        assert_eq!(recover_orphaned_exotic_tasks(&c, 100, 1010).unwrap(), 0);
        // now=2000：claimed_at=1000 < 1900 → 回收。
        assert_eq!(recover_orphaned_exotic_tasks(&c, 100, 2000).unwrap(), 1);
        // 回收后可被另一实例重新领取。
        assert_eq!(claim(&c, 1, "inst-B", 2000).len(), 1);
    }

    #[test]
    fn invalidate_resets_done_task() {
        let c = mem_db();
        seed(&c, 1);
        let id = claim(&c, 1, "inst-A", 1000)[0].id;
        finish_exotic_task(&c, id, "inst-A", "fp", "/p.webp", "1.0.0").unwrap();
        assert!(!has_blocking_exotic_thumbnail_task(&c, 1).unwrap());
        assert_eq!(invalidate_exotic_tasks_for_item(&c, 1).unwrap(), 1);
        // 退回 pending → 重新阻塞 + 可领取，且输出已清。
        assert!(has_blocking_exotic_thumbnail_task(&c, 1).unwrap());
        let again = claim(&c, 1, "inst-A", 2000);
        assert_eq!(again.len(), 1);
        assert!(again[0].output_path.is_none());
        assert!(again[0].input_fingerprint.is_none());
    }

    #[test]
    fn reset_all_redoes_done_retry_terminal_keeps_processing() {
        let c = mem_db();
        for i in 1..=4 {
            seed(&c, i);
        }
        // item1 → done(2)
        let id1 = claim(&c, 1, "A", 1000)[0].id;
        finish_exotic_task(&c, id1, "A", "fp", "/p.webp", "1.0.0").unwrap();
        // item2 → retry(3)
        let id2 = claim(&c, 1, "A", 1000)[0].id;
        fail_exotic_task(&c, id2, "A", true, 3, "io_error", "busy", 1500).unwrap();
        // item3 → terminal(4)
        let id3 = claim(&c, 1, "A", 1000)[0].id;
        fail_exotic_task(&c, id3, "A", true, 1, "malformed_input", "bad", 0).unwrap();
        // item4 → processing(1)，只领不最终化（模拟在途）
        let _id4 = claim(&c, 1, "A", 1000)[0].id;

        // 重置只动 done/retry/terminal（3 条）；processing 不动（在途结果仍有效）。
        assert_eq!(reset_all_exotic_thumbnail_tasks(&c).unwrap(), 3);

        // item1/2/3 退回 pending → 可领、输出/指纹已清；item4 仍 processing 领不到。
        let again = claim(&c, 9, "B", 9999);
        assert_eq!(again.len(), 3);
        assert!(again
            .iter()
            .all(|r| r.output_path.is_none() && r.input_fingerprint.is_none()));
    }

    #[test]
    fn renew_all_refreshes_inflight_leases() {
        let c = mem_db();
        seed(&c, 1);
        seed(&c, 2);
        let _ = claim(&c, 2, "A", 1000); // 两条 claimed_at=1000
                                         // 续租把本实例在途刷新到 5000。
        assert_eq!(renew_all_exotic_leases(&c, "A", 5000).unwrap(), 2);
        // ttl=100、now=1200：旧 claimed_at(1000<1100) 本会被回收；续租后 claimed_at=5000 不回收。
        assert_eq!(recover_orphaned_exotic_tasks(&c, 100, 1200).unwrap(), 0);
        // 别的实例续租不到本实例任务（lease_owner 不符）。
        assert_eq!(renew_all_exotic_leases(&c, "B", 9000).unwrap(), 0);
    }

    #[test]
    fn retryable_respects_next_retry_at() {
        let c = mem_db();
        seed(&c, 1);
        let id = claim(&c, 1, "inst-A", 1000)[0].id;
        // 可重试，下次重试时刻 1500，最多 3 次。
        assert!(fail_exotic_task(&c, id, "inst-A", true, 3, "io_error", "busy", 1500).unwrap());
        assert_eq!(claim(&c, 1, "inst-A", 1000).len(), 0); // 未到期
        let due = claim(&c, 1, "inst-A", 1600);
        assert_eq!(due.len(), 1); // 到期可再领
        assert_eq!(due[0].attempts, 1);
    }

    #[test]
    fn terminal_when_attempts_exhausted() {
        let c = mem_db();
        seed(&c, 1);
        let id = claim(&c, 1, "inst-A", 1000)[0].id;
        // max_attempts=1：attempts+1=1 不 < 1 → 直接 terminal(4)，不再可领。
        assert!(
            fail_exotic_task(&c, id, "inst-A", true, 1, "malformed_input", "bad", 1500).unwrap()
        );
        assert_eq!(claim(&c, 1, "inst-A", 9999).len(), 0);
    }

    /// 插一条最小 media_items（FK 已关），返回 id。
    fn insert_media(c: &Connection, fmt: &str) -> i64 {
        c.execute(
            "INSERT INTO media_items
                (directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key)
             VALUES (1, ?1, 1, 1, ?2, 'image', 0, 0, 0, ?3)",
            params![format!("f.{fmt}"), fmt, rand_key()],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    fn rand_key() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64
    }

    #[test]
    fn full_gen_pending_excludes_blocked_exotic() {
        let c = mem_db();
        let jpg = insert_media(&c, "jpg"); // 常见格式，无 exotic 任务
        let psd = insert_media(&c, "psd");
        seed(&c, psd); // psd 有 pending thumbnail 任务 → 应被主 generator 的 pending 查询排除

        let pending = get_all_pending_thumb_ids(&c).unwrap();
        assert!(pending.contains(&jpg), "jpg 应进主缩略图 pending");
        assert!(
            !pending.contains(&psd),
            "未完成 exotic 的 psd 不得进主 generator"
        );
        assert_eq!(count_pending_thumb_items(&c).unwrap(), 1);

        // 批量任务状态查询命中 psd=pending，jpg 无任务。
        let map = exotic_thumbnail_task_status_for_items(&c, &[jpg, psd]).unwrap();
        assert_eq!(map.get(&psd), Some(&ExoticTaskStatus::Pending));
        assert!(!map.contains_key(&jpg));

        // 任务完成后 psd 不再被 exotic 谓词阻塞（此时通常 Sink 已置 thumb_status=1）。
        let id = claim(&c, 1, "inst-A", 1000)[0].id;
        finish_exotic_task(&c, id, "inst-A", "fp", "/p.webp", "1.0.0").unwrap();
        assert!(get_all_pending_thumb_ids(&c).unwrap().contains(&psd));
    }

    #[test]
    fn route_info_returns_fingerprint_and_worker_version_for_done() {
        let c = mem_db();
        let psd = insert_media(&c, "psd");
        seed(&c, psd);
        // 未完成：route info 含 pending、无指纹/版本。
        let m = exotic_thumbnail_route_info_for_items(&c, &[psd]).unwrap();
        let info = m.get(&psd).unwrap();
        assert_eq!(info.status, ExoticTaskStatus::Pending);
        assert!(info.input_fingerprint.is_none() && info.worker_version.is_none());

        // 完成后：route info 带回存储的指纹与 worker 版本（供入口重算比对，问题4）。
        let id = claim(&c, 1, "inst-A", 1000)[0].id;
        finish_exotic_task(&c, id, "inst-A", "fp-abc", "/p.webp", "1.0.0").unwrap();
        let m = exotic_thumbnail_route_info_for_items(&c, &[psd]).unwrap();
        let info = m.get(&psd).unwrap();
        assert_eq!(info.status, ExoticTaskStatus::Done);
        assert_eq!(info.input_fingerprint.as_deref(), Some("fp-abc"));
        assert_eq!(info.worker_version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn ai_and_face_counts_exclude_blocked_exotic() {
        let c = mem_db();
        let _jpg = insert_media(&c, "jpg");
        let psd = insert_media(&c, "psd");
        seed(&c, psd); // psd 有未完成 thumbnail 任务

        // 两张图均 ai_status=0/face_status=0，但 psd 被 exotic 门控排除 → 计数为 1。
        assert_eq!(count_pending_ai_items(&c).unwrap(), 1);
        assert_eq!(count_pending_face_items(&c).unwrap(), 1);

        // 任务完成后门控解除，psd 计入（此后 AI/face 优先用其 thumb_path，§2.4）。
        let id = claim(&c, 1, "inst-A", 1000)[0].id;
        finish_exotic_task(&c, id, "inst-A", "fp", "/p.webp", "1.0.0").unwrap();
        assert_eq!(count_pending_ai_items(&c).unwrap(), 2);
        assert_eq!(count_pending_face_items(&c).unwrap(), 2);
    }

    #[test]
    fn upgrade_invalidates_done_with_old_version() {
        let c = mem_db();
        seed(&c, 1);
        let id = claim(&c, 1, "inst-A", 1000)[0].id;
        finish_exotic_task(&c, id, "inst-A", "fp", "/p.webp", "1.0.0").unwrap();
        // 升级到 1.1.0：旧版本 done 任务退回 pending。
        assert_eq!(
            invalidate_exotic_tasks_for_plugin_version(&c, PID, "1.1.0").unwrap(),
            1
        );
        assert!(has_blocking_exotic_thumbnail_task(&c, 1).unwrap());
        // 相同版本不重复失效。
        finish_exotic_task(
            &c,
            claim(&c, 1, "inst-A", 2000)[0].id,
            "inst-A",
            "fp2",
            "/p.webp",
            "1.1.0",
        )
        .unwrap();
        assert_eq!(
            invalidate_exotic_tasks_for_plugin_version(&c, PID, "1.1.0").unwrap(),
            0
        );
    }
}

#[cfg(test)]
mod face_approval_tests {
    //! Part4 T3 §3.5.1 批量审批命令单测（confirm/reassign/unassign/reject/create + list 分组）。
    //! FK 关闭以免铺设完整 media 链；list_likely_matches 的 media JOIN 为 LEFT JOIN，缺失项
    //! 缩略图路径记 None、不影响分组与相似度断言。
    use super::*;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c
    }

    fn bytes(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn add_face(
        c: &Connection,
        id: i64,
        item_id: i64,
        person_id: Option<i64>,
        model: &str,
        emb: &[f32],
        quality: f32,
        confirmed: bool,
    ) {
        c.execute(
            "INSERT INTO faces (id, item_id, person_id, model_name, bbox_x, bbox_y, bbox_w, bbox_h,
                                landmarks, det_score, quality, embedding, is_confirmed)
             VALUES (?1, ?2, ?3, ?4, 0.1, 0.1, 0.2, 0.2, NULL, 0.9, ?5, ?6, ?7)",
            params![
                id,
                item_id,
                person_id,
                model,
                quality,
                bytes(emb),
                confirmed as i64
            ],
        )
        .unwrap();
    }

    fn add_person(
        c: &Connection,
        id: i64,
        name: Option<&str>,
        model: &str,
        centroid: &[f32],
        n: i64,
    ) {
        c.execute(
            "INSERT INTO persons (id, name, is_named, model_name, centroid, face_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, name, name.is_some() as i64, model, bytes(centroid), n],
        )
        .unwrap();
    }

    fn face_person(c: &Connection, id: i64) -> Option<i64> {
        c.query_row(
            "SELECT person_id FROM faces WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }
    fn face_confirmed(c: &Connection, id: i64) -> bool {
        c.query_row(
            "SELECT is_confirmed FROM faces WHERE id=?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            != 0
    }
    fn person_count_col(c: &Connection, id: i64) -> i64 {
        c.query_row(
            "SELECT face_count FROM persons WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }
    fn person_exists(c: &Connection, id: i64) -> bool {
        c.query_row(
            "SELECT COUNT(*) FROM persons WHERE id=?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            == 1
    }

    #[test]
    fn confirm_pins_without_moving() {
        let c = mem();
        add_person(&c, 1, None, "m", &[1.0, 0.0], 1);
        add_face(&c, 10, 100, Some(1), "m", &[1.0, 0.0], 0.5, false);
        confirm_face_assignment(&c, &[10]).unwrap();
        assert!(face_confirmed(&c, 10));
        assert_eq!(face_person(&c, 10), Some(1), "确认不改归属");
    }

    #[test]
    fn reassign_moves_pins_and_recomputes_both() {
        let c = mem();
        add_person(&c, 1, None, "m", &[1.0, 0.0], 2);
        add_person(&c, 2, None, "m", &[0.0, 1.0], 1);
        add_face(&c, 10, 100, Some(1), "m", &[1.0, 0.0], 0.5, false);
        add_face(&c, 11, 101, Some(1), "m", &[1.0, 0.0], 0.6, false);
        add_face(&c, 20, 200, Some(2), "m", &[0.0, 1.0], 0.7, false);
        reassign_face_to_person(&c, &[10], 2).unwrap();
        assert_eq!(face_person(&c, 10), Some(2));
        assert!(face_confirmed(&c, 10), "改派即锁定");
        assert_eq!(person_count_col(&c, 2), 2, "目标 +1");
        assert_eq!(person_count_col(&c, 1), 1, "源 -1（重算）");
    }

    #[test]
    fn reassign_cross_model_rejected() {
        let c = mem();
        add_person(&c, 1, None, "m512", &[1.0, 0.0], 0);
        add_face(&c, 10, 100, None, "m128", &[1.0, 0.0], 0.5, false);
        let r = reassign_face_to_person(&c, &[10], 1);
        assert!(r.is_err(), "跨模型改派必须拒绝");
        assert_eq!(face_person(&c, 10), None, "拒绝后事务回滚、未改派");
    }

    #[test]
    fn unassign_clears_and_recomputes() {
        let c = mem();
        add_person(&c, 1, None, "m", &[1.0, 0.0], 2);
        add_face(&c, 10, 100, Some(1), "m", &[1.0, 0.0], 0.5, true);
        add_face(&c, 11, 101, Some(1), "m", &[1.0, 0.0], 0.6, true);
        unassign_face(&c, &[10]).unwrap();
        assert_eq!(face_person(&c, 10), None);
        assert!(
            !face_confirmed(&c, 10),
            "必须清 is_confirmed（防重聚类回吸）"
        );
        assert_eq!(person_count_col(&c, 1), 1);
    }

    #[test]
    fn unassign_all_deletes_unnamed_keeps_named() {
        let c = mem();
        add_person(&c, 1, None, "m", &[1.0, 0.0], 1);
        add_person(&c, 2, Some("Bob"), "m", &[0.0, 1.0], 1);
        add_face(&c, 10, 100, Some(1), "m", &[1.0, 0.0], 0.5, false);
        add_face(&c, 20, 200, Some(2), "m", &[0.0, 1.0], 0.5, false);
        unassign_face(&c, &[10, 20]).unwrap();
        assert!(!person_exists(&c, 1), "未命名空簇删除");
        assert!(person_exists(&c, 2), "命名空簇保留");
        assert_eq!(person_count_col(&c, 2), 0, "命名空簇 face_count 归零");
    }

    #[test]
    fn reject_records_negative_and_detaches() {
        let c = mem();
        add_person(&c, 1, None, "m", &[1.0, 0.0], 2);
        add_face(&c, 10, 100, Some(1), "m", &[1.0, 0.0], 0.5, false);
        add_face(&c, 11, 101, Some(1), "m", &[1.0, 0.0], 0.6, false);
        reject_face_candidate(&c, &[10], 1).unwrap();
        let rej: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM face_rejections WHERE face_id=10 AND person_id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rej, 1, "负样本已记");
        assert_eq!(face_person(&c, 10), None, "被拒脸即时移出");
        assert_eq!(person_count_col(&c, 1), 1, "源人物重算 -1");
    }

    #[test]
    fn create_person_binds_and_returns_id() {
        let c = mem();
        add_face(&c, 10, 100, None, "m", &[1.0, 0.0], 0.5, false);
        add_face(&c, 11, 101, None, "m", &[0.9, 0.1], 0.6, false);
        let pid = create_person_from_faces(&c, &[10, 11], Some("Alice")).unwrap();
        assert!(pid > 0);
        assert_eq!(face_person(&c, 10), Some(pid));
        assert!(face_confirmed(&c, 11), "建人即锁定");
        assert_eq!(person_count_col(&c, pid), 2);
        let named: i64 = c
            .query_row(
                "SELECT is_named FROM persons WHERE id=?1",
                params![pid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(named, 1, "给了名字即命名");
    }

    #[test]
    fn create_person_cross_model_rejected() {
        let c = mem();
        add_face(&c, 10, 100, None, "m128", &[1.0, 0.0], 0.5, false);
        add_face(&c, 11, 101, None, "m512", &[1.0, 0.0], 0.6, false);
        assert!(
            create_person_from_faces(&c, &[10, 11], None).is_err(),
            "跨模型建人拒绝"
        );
    }

    #[test]
    fn list_likely_matches_groups_unconfirmed_only() {
        let c = mem();
        add_person(&c, 1, Some("Carol"), "m", &[1.0, 0.0], 3);
        add_face(&c, 10, 100, Some(1), "m", &[1.0, 0.0], 0.5, false);
        add_face(&c, 11, 101, Some(1), "m", &[0.99, 0.14], 0.6, false);
        add_face(&c, 12, 102, Some(1), "m", &[1.0, 0.0], 0.7, true); // confirmed → 排除
        let groups = list_likely_matches(&c, None, None).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].person_id, 1);
        assert_eq!(groups[0].candidate_faces.len(), 2, "仅未确认脸入组");
        assert!(groups[0].confidence > 0.9, "与质心高度相似 → 高 confidence");
    }
}

/// T18 S0：`view_to_sql` 编译器快照测试。不依赖 DB —— 断言各 scope/filter 组合编译出的 SQL
/// 片段与参数个数符合预期，间接锁住 `push_query_body`（与 `query_layout_items` 共用）的行为。
#[cfg(test)]
mod view_to_sql_tests {
    use super::*;
    use crate::db::models::{DateRange, GalleryFilter, SortSpec, ViewDescriptor, ViewScope};

    /// 便捷构造：给定 scope + filter，默认排序与 layout_version。
    fn view(scope: ViewScope, filter: GalleryFilter) -> ViewDescriptor {
        ViewDescriptor {
            scope,
            filter,
            sort: SortSpec::default(),
            layout_version: 0,
        }
    }

    #[test]
    fn all_scope_compiles_base_predicate_and_tiebreaker() {
        let (sql, params) = view_to_sql(&view(ViewScope::All, GalleryFilter::default())).unwrap();
        assert!(sql.starts_with("SELECT m.id "), "只取 id 的孪生 SELECT");
        assert!(
            sql.contains("WHERE m.is_deleted=0 AND m.companion_of IS NULL"),
            "All 基础谓词"
        );
        // 默认 group_by=date / sort=desc → 末键统一追加 m.id 同向次键（确定性 tiebreaker）。
        assert!(sql.trim_end().ends_with(", m.id DESC"), "确定性 tiebreaker");
        assert_eq!(params.len(), 0, "无附加筛选 → 零绑定参数");
    }

    #[test]
    fn directory_scope_uses_recursive_subtree() {
        let (sql, params) = view_to_sql(&view(
            ViewScope::Directory { directory_id: 42 },
            GalleryFilter::default(),
        ))
        .unwrap();
        assert!(
            sql.contains("WITH RECURSIVE dir_tree"),
            "目录 scope 走递归子树（复用既有 CTE）"
        );
        assert_eq!(params.len(), 1, "directory_id 一个绑定参数");
    }

    #[test]
    fn collection_scope_restricts_to_album_items() {
        let (sql, params) = view_to_sql(&view(
            ViewScope::Collection { album_id: 7 },
            GalleryFilter::default(),
        ))
        .unwrap();
        assert!(sql.contains("SELECT item_id FROM album_items WHERE album_id ="));
        assert_eq!(params.len(), 1);
    }

    /// 集合重命名（T21）：用户夹可改名；系统夹受 `kind='user'` 守卫保护、为空操作。
    #[test]
    fn rename_collection_user_only() {
        // 该测试模块无 DB helper（纯 SQL-string 测试），自建带 migrations 的内存库。
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        let id = create_collection(&c, "旧名", None).unwrap();
        rename_collection(&c, id, "新名").unwrap();
        let name: String = c
            .query_row("SELECT name FROM albums WHERE id=?1", params![id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "新名");

        // 系统夹（kind='system'）不应被改名（守卫 kind='user'）。
        c.execute(
            "INSERT INTO albums (name, kind) VALUES ('系统夹', 'system')",
            [],
        )
        .unwrap();
        let sys_id = c.last_insert_rowid();
        rename_collection(&c, sys_id, "被改了吗").unwrap();
        let sys_name: String = c
            .query_row(
                "SELECT name FROM albums WHERE id=?1",
                params![sys_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sys_name, "系统夹", "系统夹不应被重命名");
    }

    #[test]
    fn person_scope_restricts_to_faces() {
        let (sql, params) = view_to_sql(&view(
            ViewScope::Person { person_id: 3 },
            GalleryFilter::default(),
        ))
        .unwrap();
        assert!(sql.contains("SELECT item_id FROM faces WHERE person_id ="));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn trash_scope_flips_is_deleted() {
        let (sql, _) = view_to_sql(&view(ViewScope::Trash, GalleryFilter::default())).unwrap();
        assert!(
            sql.contains("WHERE m.is_deleted=1 AND m.companion_of IS NULL"),
            "回收站 scope"
        );
    }

    #[test]
    fn filter_increments_where_and_params() {
        let filter = GalleryFilter {
            media_types: Some(vec!["image".into(), "video".into()]),
            min_rating: Some(3),
            color_label: Some(5),
            date_range: Some(DateRange { from: 100, to: 200 }),
            favorited_only: Some(true),
            ..Default::default()
        };
        let (sql, params) = view_to_sql(&view(ViewScope::All, filter)).unwrap();
        assert!(sql.contains("media_type IN (?1,?2)"), "media_types IN 绑定");
        assert!(sql.contains("rating >= "), "min_rating 谓词");
        assert!(
            sql.contains("color_label = "),
            "color_label 谓词（等值匹配）"
        );
        assert!(sql.contains("sort_datetime >= ") && sql.contains("sort_datetime <= "));
        assert!(sql.contains("is_favorited=1"), "favorited 谓词（无参数）");
        // media_types ×2 + min_rating ×1 + color_label ×1 + date_range ×2 = 6（favorited 字面量谓词不占参数）。
        assert_eq!(params.len(), 6);
    }

    #[test]
    fn semantic_search_scope_is_rejected_in_v1() {
        let v = view(
            ViewScope::SemanticSearch {
                query_embedding_id: 1,
                top_k: 50,
            },
            GalleryFilter::default(),
        );
        // 不能用 unwrap_err()：Ok 型含 Box<dyn ToSql> 未实现 Debug。
        match view_to_sql(&v) {
            Err(AppError::Internal(_)) => {}
            Ok(_) => panic!("SemanticSearch v1 不应支持纯 SQL 解析"),
            Err(e) => panic!("期望 Internal 错误，实得 {e:?}"),
        }
    }

    #[test]
    fn to_media_filter_lowers_scope_and_filter() {
        // scope=Directory + filter.min_rating → MediaFilter.directory_id + min_rating。
        let v = view(
            ViewScope::Directory { directory_id: 9 },
            GalleryFilter {
                min_rating: Some(4),
                ..Default::default()
            },
        );
        let mf = v.to_media_filter();
        assert_eq!(mf.directory_id, Some(9));
        assert_eq!(mf.min_rating, Some(4));
        assert_eq!(mf.album_id, None);
        assert_eq!(mf.person_id, None);
        assert_eq!(mf.trashed_only, None);
    }
}

/// T18 S1：`resolve_selection` / `count_selection` + ViewStale 守门（带真实 DB seed）。
#[cfg(test)]
mod selection_resolve_tests {
    use super::*;
    use crate::db::models::{
        GalleryFilter, SelectionDescriptor, SortSpec, ViewDescriptor, ViewScope,
    };

    const VER: u64 = 5;

    /// seed 一个根 + 目录 + 3 个媒体项（id 1/2/3，sort_datetime 100/200/300）。
    fn seeded_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch(
            "INSERT INTO scan_roots (id, path, alias) VALUES (1, '/r', 'R');
             INSERT INTO directories (id, root_id, rel_path, name) VALUES (10, 1, '', 'r');
             INSERT INTO media_items (id, directory_id, file_name, file_size, file_mtime, file_format, media_type, width, height, sort_datetime, cache_key)
                 VALUES (1, 10, 'a.jpg', 1, 1, 'jpg', 'image', 0, 0, 100, 0),
                        (2, 10, 'b.jpg', 1, 1, 'jpg', 'image', 0, 0, 200, 0),
                        (3, 10, 'c.jpg', 1, 1, 'jpg', 'image', 0, 0, 300, 0);",
        )
        .unwrap();
        c
    }

    fn all_view(version: u64) -> Box<ViewDescriptor> {
        Box::new(ViewDescriptor {
            scope: ViewScope::All,
            filter: GalleryFilter::default(),
            sort: SortSpec::default(),
            layout_version: version,
        })
    }

    #[test]
    fn explicit_returns_ids_regardless_of_version() {
        let c = seeded_db();
        let sel = SelectionDescriptor::Explicit { ids: vec![2, 3] };
        // version 不影响 Explicit。
        assert_eq!(resolve_selection(&c, &sel, 999).unwrap(), vec![2, 3]);
        assert_eq!(count_selection(&c, &sel, 999).unwrap(), 2);
    }

    #[test]
    fn explicit_over_limit_errs() {
        let c = seeded_db();
        let sel = SelectionDescriptor::Explicit {
            ids: vec![0i64; SELECTION_EXPLICIT_MAX + 1],
        };
        assert!(matches!(
            resolve_selection(&c, &sel, 0),
            Err(AppError::Internal(_))
        ));
    }

    #[test]
    fn select_all_resolves_all_in_layout_order() {
        let c = seeded_db();
        let sel = SelectionDescriptor::SelectAll {
            view: all_view(VER),
            excluded_ids: vec![],
        };
        // 默认 date 分组 / desc → sort_datetime 倒序：300,200,100 → id 3,2,1。
        assert_eq!(resolve_selection(&c, &sel, VER).unwrap(), vec![3, 2, 1]);
        assert_eq!(count_selection(&c, &sel, VER).unwrap(), 3);
    }

    /// 锁住 `map_layout_item` 的位置映射 ↔ `query_layout_items` 的 SELECT 列序：插入已知
    /// rating / color_label / is_favorited 的项，跑真实查询，断言映射字段读对位置——防 SELECT
    /// 加列后 `row.get(N)` 静默串列。既有 286 测试不覆盖此端到端路径，rating/color_label 的列位
    /// 此前仅靠肉眼对齐；本测试把"数对了列"从信念变成断言（补 e526952 起的未测缺口）。
    #[test]
    fn query_layout_items_maps_scalar_columns_by_position() {
        let c = seeded_db();
        // 3 项各设不同 rating / color_label / favorite —— 若任一列读错位置，会互相串值被断言抓到。
        c.execute_batch(
            "UPDATE media_items SET rating=5, color_label=3, is_favorited=1 WHERE id=3;
             UPDATE media_items SET rating=2, color_label=7, is_favorited=0 WHERE id=2;
             UPDATE media_items SET rating=0, color_label=0, is_favorited=0 WHERE id=1;",
        )
        .unwrap();

        let items =
            query_layout_items(&c, &MediaFilter::default(), None, None, None, false).unwrap();
        let by_id = |id: i64| items.iter().find(|it| it.id == id).unwrap().clone();

        let it3 = by_id(3);
        assert_eq!(
            (it3.rating, it3.color_label, it3.is_favorited),
            (5, 3, true)
        );
        let it2 = by_id(2);
        assert_eq!(
            (it2.rating, it2.color_label, it2.is_favorited),
            (2, 7, false)
        );
        let it1 = by_id(1);
        assert_eq!(
            (it1.rating, it1.color_label, it1.is_favorited),
            (0, 0, false)
        );
    }

    /// 锁住 `map_media_item` 的位置映射:color_label 追加在末列(索引 25)后,验证 get_media_item
    /// 读对它且不串既有 rating(索引 18)。喂 map_media_item 的三处 SELECT 共用此映射,本测试覆盖
    /// get_media_item 路径——防末列追加时漏改某处 SELECT 致 row.get(25) 越界/串值。
    #[test]
    fn get_media_item_maps_rating_and_color_label() {
        let c = seeded_db();
        c.execute_batch("UPDATE media_items SET rating=4, color_label=6 WHERE id=2;")
            .unwrap();
        let it = get_media_item(&c, 2).unwrap();
        assert_eq!((it.rating, it.color_label), (4, 6));
        // 未设的项取 schema 默认 0（color_label NOT NULL DEFAULT 0），确认非 NULL 越界。
        let it1 = get_media_item(&c, 1).unwrap();
        assert_eq!((it1.rating, it1.color_label), (0, 0));
    }

    #[test]
    fn select_all_stale_version_rejected() {
        let c = seeded_db();
        let sel = SelectionDescriptor::SelectAll {
            view: all_view(VER),
            excluded_ids: vec![],
        };
        // 当前版本 != view 携带版本 → ViewStale（resolve 与 count 都守门）。
        assert!(matches!(
            resolve_selection(&c, &sel, VER + 1),
            Err(AppError::ViewStale)
        ));
        assert!(matches!(
            count_selection(&c, &sel, VER + 1),
            Err(AppError::ViewStale)
        ));
    }

    #[test]
    fn select_all_excludes_ids() {
        let c = seeded_db();
        let sel = SelectionDescriptor::SelectAll {
            view: all_view(VER),
            excluded_ids: vec![2],
        };
        assert_eq!(resolve_selection(&c, &sel, VER).unwrap(), vec![3, 1]);
    }

    #[test]
    fn count_select_all_excluded_is_precise() {
        let c = seeded_db();
        // excluded 含一个真实成员(2) + 一个非成员(999)：精确计数只扣真实交集 → 3-1=2，
        // 不近似为 total - excluded.len()(=3-2=1)。
        let sel = SelectionDescriptor::SelectAll {
            view: all_view(VER),
            excluded_ids: vec![2, 999],
        };
        assert_eq!(count_selection(&c, &sel, VER).unwrap(), 2);
    }

    /// R1-2 wire 契约锁：前端以 camelCase JSON 构造描述符（kind 小驼峰变体名 + excludedIds /
    /// directoryId 字段）。serde 的 enum 级 `rename_all` 不改 struct 变体字段名——此前无前端
    /// 消费者、形状从未被实测；本测试把「前端手写的 JSON 能被后端反序列化」钉死为断言。
    #[test]
    fn selection_descriptor_wire_format_locks_camel_case() {
        use serde_json::json;
        let explicit: SelectionDescriptor =
            serde_json::from_value(json!({ "kind": "explicit", "ids": [1, 2] })).unwrap();
        assert!(matches!(explicit, SelectionDescriptor::Explicit { ids } if ids == vec![1, 2]));

        let select_all: SelectionDescriptor = serde_json::from_value(json!({
            "kind": "selectAll",
            "view": {
                "scope": { "kind": "directory", "directoryId": 10 },
                "filter": { "recentOnly": true },
                "sort": { "groupBy": "date", "sortWithinGroup": "datetime", "sortOrder": "desc" },
                "layoutVersion": 5
            },
            "excludedIds": [2]
        }))
        .unwrap();
        let SelectionDescriptor::SelectAll { view, excluded_ids } = select_all else {
            panic!("应解析为 SelectAll");
        };
        assert_eq!(excluded_ids, vec![2]);
        assert!(matches!(
            view.scope,
            ViewScope::Directory { directory_id: 10 }
        ));
        assert_eq!(view.filter.recent_only, Some(true));
        assert_eq!(view.layout_version, 5);
        // recent_only 须传导到 MediaFilter（R1-2 补字段——此前 recent 视图的 SelectAll 会静默丢谓词）。
        assert_eq!(view.to_media_filter().recent_only, Some(true));
    }

    /// R1-2：SelectAll − excluded 解析后经分块批量写落库（IPC 命令的 db 层路径）。
    #[test]
    fn batch_helpers_write_resolved_selection() {
        let c = seeded_db();
        let sel = SelectionDescriptor::SelectAll {
            view: all_view(VER),
            excluded_ids: vec![2],
        };
        let ids = resolve_selection(&c, &sel, VER).unwrap();
        let affected = batch_set_favorite(&c, &ids, true).unwrap();
        assert_eq!(affected, 2, "3 项全选排除 1 项 → 影响 2 行");
        let fav = |id: i64| -> i64 {
            c.query_row(
                "SELECT is_favorited FROM media_items WHERE id=?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!((fav(1), fav(2), fav(3)), (1, 0, 1), "排除项 2 不得被写");

        // 评分/色签越界钳制在 db 层。
        assert_eq!(batch_set_rating(&c, &ids, 99).unwrap(), 2);
        let r1: i64 = c
            .query_row("SELECT rating FROM media_items WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(r1, 5, "rating 钳到 5");
        assert_eq!(batch_set_color_label(&c, &ids, -3).unwrap(), 2);
        let cl1: i64 = c
            .query_row("SELECT color_label FROM media_items WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(cl1, 0, "color_label 钳到 0");
    }

    /// R1-5 契约锁 ①：SelectAll 解析恒排除软删项（push_query_body 的 is_deleted 谓词）——
    /// 这是「删除后布局缓存有意保持 stale」窗口的写路径安全网之一（cache.rs 失效契约注）。
    #[test]
    fn select_all_excludes_soft_deleted() {
        let c = seeded_db();
        soft_delete_items(&c, &[2]).unwrap();
        let sel = SelectionDescriptor::SelectAll {
            view: all_view(VER),
            excluded_ids: vec![],
        };
        assert_eq!(
            resolve_selection(&c, &sel, VER).unwrap(),
            vec![3, 1],
            "已删项不得进入全选目标集"
        );
        assert_eq!(count_selection(&c, &sel, VER).unwrap(), 2);
    }

    /// R1-5 契约锁 ②：批量写对软删 id 是 no-op（UPDATE 恒带 AND is_deleted=0）——
    /// stale 布局缓存把已删 id 混进选区（如 rangeBetween）也不会误写回收站内容。
    #[test]
    fn batch_write_skips_soft_deleted() {
        let c = seeded_db();
        soft_delete_items(&c, &[2]).unwrap();
        let affected = batch_set_favorite(&c, &[1, 2, 3], true).unwrap();
        assert_eq!(affected, 2, "已删项 2 不计入影响行");
        let fav2: i64 = c
            .query_row("SELECT is_favorited FROM media_items WHERE id=2", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(fav2, 0, "回收站内容不得被批量写触碰");
    }

    /// R1-2：跨 SELECTION_BATCH_CHUNK 边界的分块正确性（单条 IN 会超 SQLite 绑定上限的场景）。
    #[test]
    fn batch_update_chunks_across_boundary() {
        let c = seeded_db();
        // 追加 5001 项（连同 seed 3 项共 5004 > 5000 chunk），FK 已满足（directory 10 存在）。
        {
            let mut stmt = c
                .prepare(
                    "INSERT INTO media_items (id, directory_id, file_name, file_size, file_mtime,
                     file_format, media_type, width, height, sort_datetime, cache_key)
                     VALUES (?1, 10, 'x' || ?1 || '.jpg', 1, 1, 'jpg', 'image', 0, 0, ?1, 0)",
                )
                .unwrap();
            for id in 100..(100 + 5001) {
                stmt.execute(params![id]).unwrap();
            }
        }
        let ids: Vec<i64> = (1..=3).chain(100..(100 + 5001)).collect();
        let affected = batch_set_favorite(&c, &ids, true).unwrap();
        assert_eq!(affected, 5004, "两块（5000+4）应全部落库且计数累加正确");
    }
}

/// T18 S3：`expand_companions` + soft-delete/restore 的 Live Photo companion 连带（D5 孤儿 bug 修复）。
#[cfg(test)]
mod companion_expand_tests {
    use super::*;

    /// seed：id1 静图 + id2 其 companion(companion_of=1) + id3 独立项。FK OFF 免构造目录链。
    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c.execute_batch(
            "INSERT INTO media_items (id, directory_id, file_name, file_size, file_mtime, file_format, media_type, width, height, sort_datetime, cache_key, companion_of)
                VALUES (1, 1, 'live.jpg', 1,1,'jpg','image',0,0,100,0, NULL),
                       (2, 1, 'live.mov', 1,1,'mov','video',0,0,100,0, 1),
                       (3, 1, 'solo.jpg', 1,1,'jpg','image',0,0,200,0, NULL);",
        )
        .unwrap();
        c
    }

    fn is_deleted(c: &Connection, id: i64) -> i64 {
        c.query_row(
            "SELECT is_deleted FROM media_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn expand_includes_companion() {
        let c = mem_db();
        let mut got = expand_companions(&c, &[1]).unwrap();
        got.sort();
        assert_eq!(got, vec![1, 2], "静图展开应含其 companion");
    }

    #[test]
    fn expand_standalone_and_empty() {
        let c = mem_db();
        assert_eq!(expand_companions(&c, &[3]).unwrap(), vec![3], "独立项不变");
        assert_eq!(
            expand_companions(&c, &[]).unwrap(),
            Vec::<i64>::new(),
            "空入参空出"
        );
    }

    #[test]
    fn soft_delete_cascades_to_companion() {
        let c = mem_db();
        soft_delete_items(&c, &[1]).unwrap();
        assert_eq!(is_deleted(&c, 1), 1, "静图已删");
        assert_eq!(is_deleted(&c, 2), 1, "companion 连带删（修孤儿 bug）");
        assert_eq!(is_deleted(&c, 3), 0, "无关项不动");
    }

    #[test]
    fn restore_cascades_to_companion() {
        let c = mem_db();
        soft_delete_items(&c, &[1]).unwrap();
        restore_items(&c, &[1]).unwrap();
        assert_eq!(is_deleted(&c, 1), 0, "静图已恢复");
        assert_eq!(is_deleted(&c, 2), 0, "companion 对称连带恢复");
    }
}

#[cfg(test)]
mod r2_6_query_tests {
    use super::*;
    use std::sync::Mutex;

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        // root1 → A(顶层) → A/B(子);C(顶层,无子)。
        c.execute_batch(
            "INSERT INTO scan_roots (id, path, alias) VALUES (1, '/r', 'R');
             INSERT INTO directories (id, root_id, parent_id, rel_path, name, depth) VALUES
                 (10, 1, NULL, 'A', 'A', 0),
                 (11, 1, 10, 'A/B', 'B', 1),
                 (12, 1, NULL, 'C', 'C', 0);",
        )
        .unwrap();
        c
    }

    #[allow(clippy::too_many_arguments)]
    fn add_item(
        c: &Connection,
        id: i64,
        dir: i64,
        mtype: &str,
        fav: i64,
        del: i64,
        live: i64,
        companion: Option<i64>,
    ) {
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format, media_type,
                 width, height, sort_datetime, cache_key, is_favorited, is_deleted,
                 is_live_photo, companion_of)
             VALUES (?1, ?2, ?3, 0, 0, 'jpg', ?4, 0, 0, 0, 0, ?5, ?6, ?7, ?8)",
            params![
                id,
                dir,
                format!("{id}.jpg"),
                mtype,
                fav,
                del,
                live,
                companion
            ],
        )
        .unwrap();
    }

    /// 目录树/子目录:子树递归计数(排软删/伴随)+ has_children,tagged-CTE 改写的行为锁。
    #[test]
    fn directory_tree_counts_subtree_and_flags_children() {
        let c = mem_db();
        add_item(&c, 1, 10, "image", 0, 0, 0, None);
        add_item(&c, 2, 10, "image", 0, 0, 0, None);
        add_item(&c, 3, 11, "image", 0, 0, 0, None);
        add_item(&c, 4, 11, "image", 0, 0, 0, None);
        add_item(&c, 5, 11, "image", 0, 0, 0, None);
        add_item(&c, 6, 11, "image", 0, 1, 0, None); // 软删 → 不计
        add_item(&c, 7, 11, "video", 0, 0, 0, Some(3)); // Live 伴随 → 不计

        let top = get_directory_tree(&c, 1).unwrap();
        assert_eq!(top.len(), 2, "只返顶层目录");
        assert_eq!(top[0].name, "A");
        assert_eq!(top[0].media_count, 5, "A 子树 = 自身 2 + B 3,排软删/伴随");
        assert!(top[0].has_children);
        assert_eq!(top[1].name, "C");
        assert_eq!(top[1].media_count, 0);
        assert!(!top[1].has_children);

        let kids = get_directory_children(&c, 10).unwrap();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].name, "B");
        assert_eq!(kids[0].media_count, 3);
        assert!(!kids[0].has_children);
    }

    /// stats 合一:三处口径差异逐字锁定(favorited 不排 companion、deleted 不加过滤、
    /// 其余排 companion+软删)。
    #[test]
    fn app_stats_buckets_keep_original_semantics() {
        let c = mem_db();
        add_item(&c, 1, 10, "image", 0, 0, 0, None);
        add_item(&c, 2, 10, "video", 0, 0, 0, None);
        add_item(&c, 3, 10, "audio", 0, 0, 0, None);
        add_item(&c, 4, 10, "document", 0, 0, 0, None);
        add_item(&c, 5, 10, "image", 1, 0, 0, None); // favorited 正常项
        add_item(&c, 6, 10, "video", 1, 0, 0, Some(5)); // favorited 伴随 → 仍计入 favorited
        add_item(&c, 7, 10, "image", 0, 1, 0, None); // 软删
        add_item(&c, 8, 10, "image", 0, 0, 1, None); // live photo

        let s = get_app_stats(&c).unwrap();
        assert_eq!(s.total_items, 6, "排伴随(6)与软删(7)");
        assert_eq!(s.total_images, 3); // 1,5,8
        assert_eq!(s.total_videos, 1); // 2(6 是伴随)
        assert_eq!(s.total_audios, 1);
        assert_eq!(s.total_documents, 1);
        assert_eq!(s.total_favorited, 2, "favorited 口径不排 companion");
        assert_eq!(s.total_deleted, 1);
        assert_eq!(s.total_live_photos, 1);
    }

    fn ai_status(c: &Connection, id: i64) -> i64 {
        c.query_row(
            "SELECT ai_status FROM media_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// reset 分批:batch=2 迫使多轮循环——目标模型向量删净、他模型保留、
    /// 全部 image ai_status 归 0、非 image 不动。
    #[test]
    fn reset_ai_embeddings_batched_clears_and_preserves() {
        let c = mem_db();
        for id in 1..=5 {
            add_item(&c, id, 10, "image", 0, 0, 0, None);
        }
        add_item(&c, 9, 10, "video", 0, 0, 0, None);
        c.execute_batch(
            "UPDATE media_items SET ai_status=2 WHERE id IN (1,2,3,4,5);
             UPDATE media_items SET ai_status=2 WHERE id=9;
             INSERT INTO ai_embeddings (item_id, model_name, embedding) VALUES
                 (1,'m1',x'00'),(2,'m1',x'00'),(3,'m1',x'00'),(1,'m2',x'00');",
        )
        .unwrap();

        let db = Mutex::new(c);
        super::reset_ai_embeddings_batched(&db, "m1", 2).unwrap();

        let c = db.lock().unwrap();
        let m1: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM ai_embeddings WHERE model_name='m1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let m2: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM ai_embeddings WHERE model_name='m2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(m1, 0, "目标模型向量删净");
        assert_eq!(m2, 1, "他模型向量保留");
        for id in 1..=5 {
            assert_eq!(ai_status(&c, id), 0, "image 全部归 0");
        }
        assert_eq!(ai_status(&c, 9), 2, "非 image 不动(既有 media_type 过滤)");
    }

    /// sync 分批 + 不一致谓词:错的行被纠正,已同步行零写(updated_at 不变)。
    #[test]
    fn sync_ai_status_batched_targets_only_mismatched() {
        let c = mem_db();
        add_item(&c, 1, 10, "image", 0, 0, 0, None); // 有向量但 ai=0 → 应改 2
        add_item(&c, 2, 10, "image", 0, 0, 0, None); // 无向量 ai=2 → 应改 0
        add_item(&c, 3, 10, "image", 0, 0, 0, None); // 有向量且 ai=2 → 已同步,不动
        add_item(&c, 4, 10, "image", 0, 1, 0, None); // 软删,有向量 ai=0 → is_deleted 过滤,不动
        c.execute_batch(
            "INSERT INTO ai_embeddings (item_id, model_name, embedding) VALUES
                 (1,'m1',x'00'),(3,'m1',x'00'),(4,'m1',x'00');
             UPDATE media_items SET ai_status=2 WHERE id IN (2,3);
             UPDATE media_items SET updated_at=111 WHERE id IN (1,2,3,4);",
        )
        .unwrap();

        let db = Mutex::new(c);
        super::sync_ai_status_batched(&db, "m1", 1).unwrap();

        let c = db.lock().unwrap();
        let upd = |id: i64| -> i64 {
            c.query_row(
                "SELECT updated_at FROM media_items WHERE id=?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(ai_status(&c, 1), 2);
        assert_eq!(ai_status(&c, 2), 0);
        assert_eq!(ai_status(&c, 3), 2);
        assert_eq!(ai_status(&c, 4), 0, "软删行不参与同步(保持原值)");
        assert_ne!(upd(1), 111, "被纠正的行 bump updated_at");
        assert_ne!(upd(2), 111);
        assert_eq!(upd(3), 111, "已同步行零写(不再翻搅 updated_at/索引)");
        assert_eq!(upd(4), 111);
    }

    /// face reset 分批:face_status 循环清零(batch=1 迫使多轮),非 image 不动。
    #[test]
    fn reset_face_data_batched_clears_face_status() {
        let c = mem_db();
        for id in 1..=3 {
            add_item(&c, id, 10, "image", 0, 0, 0, None);
        }
        add_item(&c, 9, 10, "video", 0, 0, 0, None);
        c.execute_batch("UPDATE media_items SET face_status=2 WHERE id IN (1,2,3,9);")
            .unwrap();

        let db = Mutex::new(c);
        super::reset_face_data_batched(&db, "yunet-sface", 1).unwrap();

        let c = db.lock().unwrap();
        let fs = |id: i64| -> i64 {
            c.query_row(
                "SELECT face_status FROM media_items WHERE id=?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(fs(1), 0);
        assert_eq!(fs(2), 0);
        assert_eq!(fs(3), 0);
        assert_eq!(fs(9), 2, "非 image 不动");
    }
}
