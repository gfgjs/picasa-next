// src-tauri/src/db/queries.rs
// src-tauri/src/db/queries.rs
//! Reusable parameterised SQL query functions.
//! 可重用的参数化 SQL 查询函数。
//! All SQL uses parameter binding — never string concatenation.
//! 所有 SQL 均使用参数绑定 — 绝不使用字符串拼接。

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::db::models::{
    AppStats, DirNode, ImageMeta, LayoutItem, MediaDetail, MediaFilter, MediaItem,
    ScanRoot, SearchResult, ThumbResult,
};
use crate::error::{AppError, Result};
use crate::utils::path::resolve_media_path;

// ── Row mappers ──────────────────────────────────────────────────────────────
// ── 行映射器 ──────────────────────────────────────────────────────────────

fn map_scan_root(row: &Row<'_>) -> rusqlite::Result<ScanRoot> {
    Ok(ScanRoot {
        id:            row.get(0)?,
        path:          row.get(1)?,
        alias:         row.get(2)?,
        scan_status:   row.get(3)?,
        scan_progress: row.get(4)?,
        total_files:   row.get(5)?,
        last_scan_at:  row.get(6)?,
        is_active:     row.get::<_, i64>(7)? != 0,
        created_at:    row.get(8)?,
        updated_at:    row.get(9)?,
    })
}

fn map_media_item(row: &Row<'_>) -> rusqlite::Result<MediaItem> {
    Ok(MediaItem {
        id:                  row.get(0)?,
        directory_id:        row.get(1)?,
        file_name:           row.get(2)?,
        file_size:           row.get(3)?,
        file_mtime:          row.get(4)?,
        file_format:         row.get(5)?,
        media_type:          row.get(6)?,
        width:               row.get(7)?,
        height:              row.get(8)?,
        duration_ms:         row.get(9)?,
        sort_datetime:       row.get(10)?,
        cache_key:           row.get(11)?,
        thumb_status:        row.get(12)?,
        thumb_path:          row.get(13)?,
        thumbhash:           row.get(14)?,
        is_favorited:        row.get::<_, i64>(15)? != 0,
        is_deleted:          row.get::<_, i64>(16)? != 0,
        deleted_at:          row.get(17)?,
        rating:              row.get(18)?,
        is_live_photo:       row.get::<_, i64>(19)? != 0,
        has_embedded_video:  row.get::<_, i64>(20)? != 0,
        companion_of:        row.get(21)?,
        content_hash:        row.get(22)?,
        created_at:          row.get(23)?,
        updated_at:          row.get(24)?,
    })
}

fn map_layout_item(row: &Row<'_>) -> rusqlite::Result<LayoutItem> {
    Ok(LayoutItem {
        id:            row.get(0)?,
        width:         row.get(1)?,
        height:        row.get(2)?,
        file_size:     row.get(3)?,
        sort_datetime: row.get(4)?,
        file_format:   row.get(5)?,
        media_type:    row.get(6)?,
        is_live_photo: row.get::<_, i64>(7)? != 0,
        duration_ms:   row.get(8)?,
        thumb_status:  row.get(9)?,
        thumb_path:    row.get(10)?,
        thumbhash:     row.get(11)?,
        is_favorited:  row.get::<_, i64>(12)? != 0,
        dir_path:      row.get(13)?,
        dir_name:      row.get(14)?,
        file_name:     row.get(15)?,
        dir_id:        row.get(16)?,
        similarity:    row.get(17)?,
    })
}

fn map_dir_node(row: &Row<'_>) -> rusqlite::Result<DirNode> {
    Ok(DirNode {
        id:           row.get(0)?,
        root_id:      row.get(1)?,
        parent_id:    row.get(2)?,
        name:         row.get(3)?,
        rel_path:     row.get(4)?,
        depth:        row.get(5)?,
        media_count:  row.get(6)?,
        has_children: row.get::<_, i64>(7)? != 0,
    })
}

// ── Scan roots ───────────────────────────────────────────────────────────────
// ── 扫描根目录 ───────────────────────────────────────────────────────────────

pub fn insert_scan_root(conn: &Connection, path: &str, alias: Option<&str>) -> Result<i64> {
    conn.execute(
        "INSERT INTO scan_roots (path, alias) VALUES (?1, ?2)",
        params![path, alias],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_scan_root(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM scan_roots WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_scan_roots(conn: &Connection) -> Result<Vec<ScanRoot>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, alias, scan_status, scan_progress, total_files,
                last_scan_at, is_active, created_at, updated_at
         FROM scan_roots ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], map_scan_root)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn get_scan_root(conn: &Connection, id: i64) -> Result<ScanRoot> {
    conn.query_row(
        "SELECT id, path, alias, scan_status, scan_progress, total_files,
                last_scan_at, is_active, created_at, updated_at
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
    let mut stmt = conn.prepare(
        "SELECT d.id, d.root_id, d.parent_id, d.name, d.rel_path, d.depth,
                (
                    WITH RECURSIVE dir_tree(id) AS (
                        SELECT d.id
                        UNION ALL
                        SELECT child.id FROM directories child
                        JOIN dir_tree t ON child.parent_id = t.id
                    )
                    SELECT COUNT(*) FROM media_items m
                    WHERE m.directory_id IN dir_tree AND m.is_deleted=0 AND m.companion_of IS NULL
                ) AS media_count,
                (SELECT COUNT(*)>0 FROM directories c WHERE c.parent_id=d.id) AS has_children
         FROM directories d
         WHERE d.root_id=?1 AND d.parent_id IS NULL
         ORDER BY d.name ASC",
    )?;
    let rows = stmt.query_map(params![root_id], map_dir_node)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn get_directory_children(conn: &Connection, parent_id: i64) -> Result<Vec<DirNode>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.root_id, d.parent_id, d.name, d.rel_path, d.depth,
                (
                    WITH RECURSIVE dir_tree(id) AS (
                        SELECT d.id
                        UNION ALL
                        SELECT child.id FROM directories child
                        JOIN dir_tree t ON child.parent_id = t.id
                    )
                    SELECT COUNT(*) FROM media_items m
                    WHERE m.directory_id IN dir_tree AND m.is_deleted=0 AND m.companion_of IS NULL
                ) AS media_count,
                (SELECT COUNT(*)>0 FROM directories c WHERE c.parent_id=d.id) AS has_children
         FROM directories d
         WHERE d.parent_id=?1
         ORDER BY d.name ASC",
    )?;
    let rows = stmt.query_map(params![parent_id], map_dir_node)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn increment_directory_media_count(conn: &Connection, dir_id: i64, delta: i64) -> Result<()> {
    conn.execute(
        "UPDATE directories SET media_count = media_count + ?1 WHERE id = ?2",
        params![delta, dir_id],
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
         SELECT id FROM ancestors;"
    )?;
    
    let mut rows = stmt.query(params![id])?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next()? {
        ids.push(row.get::<_, i64>(0)?);
    }
    
    ids.reverse();
    Ok(ids)
}
// ── Media items ───────────────────────────────────────────────────────────────
// ── 媒体项 ───────────────────────────────────────────────────────────────

/// Batch-upsert helper data for fast scan.
/// 快速扫描的批量插入/更新辅助数据。
pub struct FastScanItem {
    pub directory_id: i64,
    pub file_name:    String,
    pub file_size:    i64,
    pub file_mtime:   i64,
    pub file_format:  String,
    pub media_type:   String,
    pub width:        i64,
    pub height:       i64,
    pub sort_datetime: i64,
    pub cache_key:    i64,
}

/// Insert or update a media item from the fast scan phase.
/// 插入或更新来自快速扫描阶段的媒体项。
/// Returns `(id, is_new)`.
/// 返回 `(id, is_new)`。
pub fn upsert_fast_scan_item(conn: &Connection, item: &FastScanItem) -> Result<(i64, bool)> {
    // Check if exists with same mtime (no change needed)
    // 检查是否存在具有相同 mtime 的项（无需更改）
    let existing: Option<(i64, i64)> = conn
        .query_row(
            "SELECT id, file_mtime FROM media_items WHERE directory_id=?1 AND file_name=?2",
            params![item.directory_id, item.file_name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    if let Some((id, mtime)) = existing {
        if mtime == item.file_mtime {
            // Unchanged — skip
            // 未更改 — 跳过
            return Ok((id, false));
        }
        // Changed — update
        // 已更改 — 更新
        conn.execute(
            "UPDATE media_items SET file_size=?1, file_mtime=?2, file_format=?3,
                      media_type=?4, width=?5, height=?6, sort_datetime=?7,
                      cache_key=?8, thumb_status=0, thumb_path=NULL, thumbhash=NULL,
                      updated_at=strftime('%s','now')
             WHERE id=?9",
            params![
                item.file_size, item.file_mtime, item.file_format,
                item.media_type, item.width, item.height, item.sort_datetime,
                item.cache_key, id
            ],
        )?;
        return Ok((id, false));
    }

    // New item
    // 新项
    conn.execute(
        "INSERT INTO media_items
             (directory_id, file_name, file_size, file_mtime, file_format,
              media_type, width, height, sort_datetime, cache_key)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            item.directory_id, item.file_name, item.file_size, item.file_mtime,
            item.file_format, item.media_type, item.width, item.height,
            item.sort_datetime, item.cache_key
        ],
    )?;
    Ok((conn.last_insert_rowid(), true))
}

pub fn get_media_item(conn: &Connection, id: i64) -> Result<MediaItem> {
    conn.query_row(
        "SELECT id, directory_id, file_name, file_size, file_mtime, file_format,
                media_type, width, height, duration_ms, sort_datetime, cache_key,
                thumb_status, thumb_path, thumbhash, is_favorited, is_deleted,
                deleted_at, rating, is_live_photo, has_embedded_video, companion_of,
                content_hash, created_at, updated_at
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
                    item_id:           row.get(0)?,
                    orientation:       row.get(1)?,
                    exif_datetime:     row.get(2)?,
                    exif_make:         row.get(3)?,
                    exif_model:        row.get(4)?,
                    exif_lens:         row.get(5)?,
                    exif_focal_length: row.get(6)?,
                    exif_aperture:     row.get(7)?,
                    exif_shutter:      row.get(8)?,
                    exif_iso:          row.get(9)?,
                    exif_gps_lat:      row.get(10)?,
                    exif_gps_lng:      row.get(11)?,
                    dominant_hue:      row.get(12)?,
                    dominant_sat:      row.get(13)?,
                    dominant_lum:      row.get(14)?,
                    dominant_hex:      row.get(15)?,
                    is_monochrome:     row.get::<_, i64>(16)? != 0,
                })
            },
        )
        .ok();

    Ok(MediaDetail {
        item,
        abs_path,
        image_meta,
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
) -> Result<Vec<LayoutItem>> {
    let mut sql = String::from(
        "SELECT m.id, m.width, m.height, m.file_size, m.sort_datetime, m.file_format, m.media_type, m.is_live_photo,
                m.duration_ms, m.thumb_status, m.thumb_path, m.thumbhash, m.is_favorited,
                d.rel_path as dir_path, d.name as dir_name, m.file_name, m.directory_id as dir_id, "
    );

    if filter.ai_search == Some(true) {
        sql.push_str("ai.similarity\n");
    } else {
        sql.push_str("NULL as similarity\n");
    }

    sql.push_str("         FROM media_items m\n         JOIN directories d ON m.directory_id = d.id");

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
    let mut extras: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if filter.ai_search == Some(true) {
        if let Some(threshold) = filter.ai_threshold {
            param_idx += 1;
            // Match the frontend's visual rounding (e.g. Math.round(similarity * 100))
            sql.push_str(&format!(" AND ROUND(ai.similarity * 100.0) >= ?{param_idx}"));
            extras.push(Box::new((threshold * 100.0).round()));
        }
    }

    if let Some(dir_id) = filter.directory_id {
        param_idx += 1;
        sql.push_str(&format!(" AND directory_id IN (
            WITH RECURSIVE dir_tree(id) AS (
                SELECT ?{param_idx}
                UNION ALL
                SELECT d.id FROM directories d
                JOIN dir_tree t ON d.parent_id = t.id
            )
            SELECT id FROM dir_tree
        )"));
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

    if let Some(min_r) = filter.min_rating {
        param_idx += 1;
        sql.push_str(&format!(" AND rating >= ?{param_idx}"));
        extras.push(Box::new(min_r));
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
                    sql.push_str(&format!(" AND (d.rel_path LIKE {} OR d.name LIKE {})", p1, p2));
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
                _ => { // "filename"
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
            sql.push_str(&format!(" ORDER BY d.rel_path ASC, ai.similarity {}", order_dir));
        } else if sort_within == Some("filename") {
            sql.push_str(&format!(" ORDER BY d.rel_path ASC, m.file_name COLLATE NATURAL_CMP {}", order_dir));
        } else {
            sql.push_str(&format!(" ORDER BY d.rel_path ASC, m.sort_datetime {}", order_dir));
        }
    } else if group_by == Some("date") {
        let date_expr = "date(m.sort_datetime, 'unixepoch', 'localtime')";
        if sort_within == Some("similarity") && filter.ai_search == Some(true) {
            sql.push_str(&format!(" ORDER BY {} {}, ai.similarity {}", date_expr, order_dir, order_dir));
        } else if sort_within == Some("filename") {
            sql.push_str(&format!(" ORDER BY {} {}, m.file_name COLLATE NATURAL_CMP {}", date_expr, order_dir, order_dir));
        } else {
            sql.push_str(&format!(" ORDER BY m.sort_datetime {}", order_dir));
        }
    } else {
        if sort_within == Some("similarity") && filter.ai_search == Some(true) {
            sql.push_str(&format!(" ORDER BY ai.similarity {}", order_dir));
        } else if sort_within == Some("filename") {
            sql.push_str(&format!(" ORDER BY m.file_name COLLATE NATURAL_CMP {}", order_dir));
        } else {
            sql.push_str(&format!(" ORDER BY m.sort_datetime {}", order_dir));
        }
    }

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = extras.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), map_layout_item)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn get_pending_thumb_items(conn: &Connection, limit: i64) -> Result<Vec<(i64, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT id, cache_key FROM media_items
         WHERE thumb_status=0 AND is_deleted=0
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn get_all_pending_thumb_ids(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM media_items
         WHERE thumb_status=0 AND is_deleted=0
         ORDER BY created_at DESC"
    )?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

pub fn count_pending_thumb_items(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE thumb_status=0 AND is_deleted=0",
        [],
        |row| row.get(0),
    )
    .map_err(AppError::from)
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
            item_id:      row.get(0)?,
            thumb_status: row.get(1)?,
            thumb_path:   row.get(2)?,
            thumbhash:    row.get(3)?,
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
            meta.item_id, meta.orientation, meta.exif_datetime, meta.exif_make,
            meta.exif_model, meta.exif_lens, meta.exif_focal_length,
            meta.exif_aperture, meta.exif_shutter, meta.exif_iso,
            meta.exif_gps_lat, meta.exif_gps_lng
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

pub fn soft_delete_items(conn: &Connection, item_ids: &[i64]) -> Result<()> {
    if item_ids.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for &id in item_ids {
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
    let tx = conn.unchecked_transaction()?;
    for &id in item_ids {
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
                content_hash, created_at, updated_at
         FROM media_items WHERE is_deleted=1
         ORDER BY deleted_at DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], map_media_item)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

// ── Stats ─────────────────────────────────────────────────────────────────────
// ── 统计 ─────────────────────────────────────────────────────────────────────

pub fn get_app_stats(conn: &Connection) -> Result<AppStats> {
    let total_items: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE is_deleted=0 AND companion_of IS NULL",
        [],
        |r| r.get(0),
    )?;
    let total_images: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE is_deleted=0 AND companion_of IS NULL AND media_type='image'",
        [], |r| r.get(0),
    )?;
    let total_videos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE is_deleted=0 AND companion_of IS NULL AND media_type='video'",
        [], |r| r.get(0),
    )?;
    let total_audios: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE is_deleted=0 AND companion_of IS NULL AND media_type='audio'",
        [], |r| r.get(0),
    )?;
    let total_documents: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE is_deleted=0 AND companion_of IS NULL AND media_type='document'",
        [], |r| r.get(0),
    )?;
    let total_favorited: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE is_favorited=1 AND is_deleted=0",
        [], |r| r.get(0),
    )?;
    let total_deleted: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE is_deleted=1",
        [], |r| r.get(0),
    )?;
    let total_live_photos: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE is_live_photo=1 AND is_deleted=0 AND companion_of IS NULL",
        [], |r| r.get(0),
    )?;

    Ok(AppStats {
        total_items,
        total_images,
        total_videos,
        total_audios,
        total_documents,
        total_favorited,
        total_deleted,
        total_live_photos,
    })
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
        sql.push_str(&format!(" AND directory_id IN (
            WITH RECURSIVE dir_tree(id) AS (
                SELECT ?{param_idx}
                UNION ALL
                SELECT d.id FROM directories d
                JOIN dir_tree t ON d.parent_id = t.id
            )
            SELECT id FROM dir_tree
        )"));
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
            id:           row.get(0)?,
            file_name:    row.get(1)?,
            media_type:   row.get(2)?,
            width:        row.get(3)?,
            height:       row.get(4)?,
            thumb_path:   row.get(5)?,
            thumbhash:    row.get(6)?,
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
    rows: &[(i64, String, Vec<u8>, i64)],  // (item_id, model_name, embedding, version)
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
pub fn get_all_embeddings(
    conn: &Connection,
    model_name: &str,
) -> Result<Vec<(i64, Vec<u8>)>> {
    let mut stmt = conn.prepare(
        "SELECT item_id, embedding FROM ai_embeddings WHERE model_name=?1",
    )?;
    let rows = stmt.query_map(params![model_name], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
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
/// Returns `(id, abs_path, file_format)` with the absolute path resolved
/// via JOIN on directories + scan_roots, so the AI pipeline can decode
/// from source files directly (using GPU-accelerated engines like WIC).
///
/// 返回 `(id, abs_path, file_format)`，绝对路径通过 JOIN directories + scan_roots 解析，
/// 使 AI 流水线可以直接从源文件解码（使用 WIC 等 GPU 加速引擎）。
pub fn get_pending_ai_items(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<(i64, String, String)>> {  // (id, abs_path, file_format)
    let mut stmt = conn.prepare(
        "SELECT m.id,
                CASE WHEN d.rel_path = '' THEN r.path || '/' || m.file_name
                     ELSE r.path || '/' || d.rel_path || '/' || m.file_name
                END,
                m.file_format
         FROM media_items m
         JOIN directories d ON m.directory_id = d.id
         JOIN scan_roots r ON d.root_id = r.id
         WHERE m.ai_status=0 AND m.is_deleted=0 AND m.media_type='image'
         ORDER BY m.created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Count pending AI items.
/// 统计待处理的 AI 项数量。
pub fn count_pending_ai_items(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE ai_status=0 AND is_deleted=0 AND media_type='image'",
        [],
        |row| row.get(0),
    )
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

/// Reset all AI embeddings — set ai_status back to 0 and delete embeddings.
/// 重置所有 AI 嵌入向量 — 将 ai_status 设回 0 并删除嵌入向量。
pub fn reset_ai_embeddings(conn: &Connection, model_name: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM ai_embeddings WHERE model_name=?1",
        params![model_name],
    )?;
    tx.execute(
        "UPDATE media_items SET ai_status=0, updated_at=strftime('%s','now')
         WHERE media_type='image'",
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
            id:           row.get(0)?,
            file_name:    row.get(1)?,
            media_type:   row.get(2)?,
            width:        row.get(3)?,
            height:       row.get(4)?,
            thumb_path:   row.get(5)?,
            thumbhash:    row.get(6)?,
            thumb_status: row.get(7)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}
