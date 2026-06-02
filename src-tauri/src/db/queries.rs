// src-tauri/src/db/queries.rs
//! Reusable parameterised SQL query functions.
//! All SQL uses parameter binding — never string concatenation.

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::db::models::{
    AppStats, DirNode, ImageMeta, LayoutItem, MediaDetail, MediaFilter, MediaItem,
    ScanRoot, SearchResult, ThumbResult,
};
use crate::error::{AppError, Result};
use crate::utils::path::resolve_media_path;

// ── Row mappers ──────────────────────────────────────────────────────────────

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
        sort_datetime: row.get(3)?,
        media_type:    row.get(4)?,
        is_live_photo: row.get::<_, i64>(5)? != 0,
        duration_ms:   row.get(6)?,
        thumb_status:  row.get(7)?,
        thumb_path:    row.get(8)?,
        thumbhash:     row.get(9)?,
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

/// Upsert a directory. Returns the row id.
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
    let id: i64 = conn.query_row(
        "SELECT id FROM directories WHERE root_id=?1 AND rel_path=?2",
        params![root_id, rel_path],
        |row| row.get(0),
    )?;
    Ok(id)
}

pub fn get_directory_tree(conn: &Connection, root_id: i64) -> Result<Vec<DirNode>> {
    let mut stmt = conn.prepare(
        "SELECT d.id, d.root_id, d.parent_id, d.name, d.rel_path, d.depth, d.media_count,
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
        "SELECT d.id, d.root_id, d.parent_id, d.name, d.rel_path, d.depth, d.media_count,
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

// ── Media items ───────────────────────────────────────────────────────────────

/// Batch-upsert helper data for fast scan.
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
/// Returns `(id, is_new)`.
pub fn upsert_fast_scan_item(conn: &Connection, item: &FastScanItem) -> Result<(i64, bool)> {
    // Check if exists with same mtime (no change needed)
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
            return Ok((id, false));
        }
        // Changed — update
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
/// Used by `compute_layout`.
pub fn query_layout_items(
    conn: &Connection,
    filter: &MediaFilter,
) -> Result<Vec<LayoutItem>> {
    let mut sql = String::from(
        "SELECT id, width, height, sort_datetime, media_type, is_live_photo,
                duration_ms, thumb_status, thumb_path, thumbhash
         FROM media_items
         WHERE is_deleted=0 AND companion_of IS NULL",
    );

    let mut param_idx = 0usize;
    let mut extras: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(dir_id) = filter.directory_id {
        param_idx += 1;
        sql.push_str(&format!(" AND directory_id=?{param_idx}"));
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
        sql.push_str(" AND is_live_photo=1");
    }

    sql.push_str(" ORDER BY sort_datetime DESC");

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = extras.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), map_layout_item)?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

/// Get items pending thumbnail generation (thumb_status=0).
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
        "SELECT m.id, m.thumb_status, 
                CASE 
                    WHEN m.thumb_status = 3 THEN 
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

pub fn search_media(
    conn: &Connection,
    query: &str,
    filter: &MediaFilter,
    limit: i64,
) -> Result<Vec<SearchResult>> {
    let pattern = format!("%{query}%");
    let mut sql = String::from(
        "SELECT id, file_name, media_type, thumb_path, thumbhash, thumb_status
         FROM media_items
         WHERE is_deleted=0 AND companion_of IS NULL AND file_name LIKE ?1",
    );

    let mut extras: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(pattern)];
    let mut param_idx = 1usize;

    if let Some(dir_id) = filter.directory_id {
        param_idx += 1;
        sql.push_str(&format!(" AND directory_id=?{param_idx}"));
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
            thumb_path:   row.get(3)?,
            thumbhash:    row.get(4)?,
            thumb_status: row.get(5)?,
        })
    })?;
    rows.map(|r| r.map_err(AppError::from)).collect()
}

// ── App config ────────────────────────────────────────────────────────────────

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

/// Items needing enrichment: those without an `image_meta` row and media_type='image'.
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
pub fn get_companion_item_id(conn: &Connection, item_id: i64) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM media_items WHERE companion_of=?1 LIMIT 1",
        params![item_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(AppError::from)
}
