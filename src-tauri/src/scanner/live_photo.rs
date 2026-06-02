// src-tauri/src/scanner/live_photo.rs
//! Apple Live Photo companion pairing.
//! Apple 实况照片关联配对。
//!
//! After the fast scan inserts all items, this module pairs JPEG files with
//! 在快速扫描插入所有项目后，此模块将 JPEG 文件与其
//! their companion .MOV files (same directory, same file stem).
//! 关联的 .MOV 文件配对（相同目录，相同文件主名）。

use std::collections::HashMap;
use rusqlite::Connection;
use tracing::{debug, info};

use crate::error::Result;

/// A lightweight record of a media item for pairing.
/// 用于配对的媒体项目的轻量级记录。
#[derive(Debug)]
struct PairingRecord {
    id:           i64,
    file_stem:    String,
    directory_id: i64,
    extension:    String,
}

/// Run the Apple Live Photo companion pairing for a specific scan root.
/// 运行特定扫描根目录的 Apple 实况照片关联配对。
///
/// Algorithm:
/// 算法：
/// 1. Query all items in the root that are either JPEG images or MOV videos.
/// 1. 查询根目录中所有 JPEG 图像或 MOV 视频的项目。
/// 2. Group by `(directory_id, file_stem)`.
/// 2. 按 `(directory_id, file_stem)` 分组。
/// 3. If a group has both a JPEG and a MOV, the MOV is the companion.
/// 3. 如果一个组同时包含 JPEG 和 MOV，则 MOV 是关联文件。
///    - JPEG: `is_live_photo = 1`
///    - MOV:  `companion_of = JPEG.id`
pub fn pair_live_photos(conn: &Connection, root_id: i64) -> Result<u64> {
    info!("Pairing Live Photos for root_id={root_id}");

    // Fetch candidate items (JPEG or MOV, not already marked as companion)
    // 获取候选项目（JPEG 或 MOV，尚未标记为关联文件）
    let mut stmt = conn.prepare(
        "SELECT m.id, m.file_name, m.directory_id, m.file_format
         FROM media_items m
         JOIN directories d ON d.id = m.directory_id
         WHERE d.root_id = ?1
           AND m.is_deleted = 0
           AND m.companion_of IS NULL
           AND m.file_format IN ('jpg','jpeg','mov')
         ORDER BY m.directory_id, m.file_name",
    )?;

    let records: Vec<PairingRecord> = stmt
        .query_map(rusqlite::params![root_id], |row| {
            let file_name: String = row.get(1)?;
            let ext: String       = row.get(3)?;
            // Derive stem: remove the extension portion
            // 派生文件主名：删除扩展名部分
            let stem = file_name
                .rsplit_once('.')
                .map(|(s, _)| s.to_lowercase())
                .unwrap_or_else(|| file_name.to_lowercase());
            Ok(PairingRecord {
                id:           row.get(0)?,
                file_stem:    stem,
                directory_id: row.get(2)?,
                extension:    ext,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Group by (directory_id, file_stem)
    // 按 (directory_id, file_stem) 分组
    // key → (Option<jpeg_id>, Option<mov_id>)
    // 键 → (Option<jpeg_id>, Option<mov_id>)
    let mut groups: HashMap<(i64, String), (Option<i64>, Option<i64>)> = HashMap::new();

    for rec in &records {
        let entry = groups
            .entry((rec.directory_id, rec.file_stem.clone()))
            .or_insert((None, None));
        match rec.extension.as_str() {
            "jpg" | "jpeg" => entry.0 = Some(rec.id),
            "mov"          => entry.1 = Some(rec.id),
            _ => {}
        }
    }

    let mut paired = 0u64;
    for ((_, _stem), (jpeg_id, mov_id)) in groups {
        if let (Some(jpeg), Some(mov)) = (jpeg_id, mov_id) {
            // Mark the JPEG as a live photo
            // 将 JPEG 标记为实况照片
            conn.execute(
                "UPDATE media_items SET is_live_photo=1, updated_at=strftime('%s','now') WHERE id=?1",
                rusqlite::params![jpeg],
            )?;
            // Mark the MOV as a companion (it will be hidden from the grid)
            // 将 MOV 标记为关联文件（它将从网格中隐藏）
            conn.execute(
                "UPDATE media_items SET companion_of=?1, updated_at=strftime('%s','now') WHERE id=?2",
                rusqlite::params![jpeg, mov],
            )?;
            debug!("Paired LIVE: jpeg_id={jpeg}, companion_mov_id={mov}");
            paired += 1;
        }
    }

    info!("Live Photo pairing complete: {paired} pairs found");
    Ok(paired)
}
