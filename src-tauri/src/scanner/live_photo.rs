// src-tauri/src/scanner/live_photo.rs
//! Live Photo / Motion Photo companion pairing.
//! 实况照片 / 动态照片关联配对。
//!
//! After the fast scan inserts all items, this module pairs a still image with
//! 在快速扫描插入所有项目后，此模块将静图与其
//! its companion video file (same directory, same file stem). Two families:
//! 关联的视频文件配对（相同目录，相同文件主名）。两类：
//! - Apple Live Photo: JPEG/HEIC/HEIF + `.MOV` — stem-based (T7).
//! - Apple 实况照片：JPEG/HEIC/HEIF + `.MOV` —— 纯 stem 配对（T7）。
//! - Split Motion Photo: JPG + `.MP4` — stem-based **but motion-gated** (T15),
//! - 分体式动态照片：JPG + `.MP4` —— stem 配对 **但带 motion 守门**（T15），
//!   only when the still side already carries the XMP Motion Photo signal
//!   仅当静图侧已带 XMP 动态照片信号（`has_embedded_video=1`，由
//!   (`has_embedded_video=1`, set by `detect_motion_photo_xmp` during enrichment),
//!   enrichment 阶段的 `detect_motion_photo_xmp` 置位）时才配，
//!   so a normal photo + coincidentally same-named normal `.mp4` is NOT swallowed.
//!   避免把普通照片 + 恰好同名的普通 `.mp4` 误吞为 companion。

use rusqlite::Connection;
use std::collections::HashMap;
use tracing::{debug, info};

use crate::error::Result;

/// A lightweight record of a media item for pairing.
/// 用于配对的媒体项目的轻量级记录。
#[derive(Debug)]
struct PairingRecord {
    id: i64,
    file_stem: String,
    directory_id: i64,
    extension: String,
    /// 静图侧的 motion 信号（`has_embedded_video`）。仅 mp4 分体式配对据此守门；
    /// 视频行恒为 false。
    is_motion: bool,
}

/// Per-(directory, stem) accumulator: the still image plus any candidate videos.
/// 每个 (目录, stem) 的聚合：静图 + 各候选视频。
#[derive(Default)]
struct Group {
    /// 静图（jpg/jpeg/heic/heif）id。
    image_id: Option<i64>,
    /// 静图是否带 XMP motion 信号（决定 mp4 是否可配）。
    image_is_motion: bool,
    /// Apple Live Photo 候选视频（.mov）。
    mov_id: Option<i64>,
    /// 分体式 Motion Photo 候选视频（.mp4）。
    mp4_id: Option<i64>,
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
    info!("Pairing Live Photos for root_id={root_id} | 正在配对实况照片 root_id={root_id}");

    // Fetch candidate items (still image JPEG/HEIC/HEIF, or MOV/MP4 video; not already a companion).
    // 获取候选项目（静图 JPEG/HEIC/HEIF，或 MOV/MP4 视频；尚未标记为关联文件）。
    // HEIC/HEIF：现代 iPhone 默认拍 HEIC，其 Live Photo 为 HEIC + MOV（T7）。
    // mp4：分体式 Motion Photo 候选，仅在静图带 has_embedded_video 时才配（T15）。
    let mut stmt = conn.prepare(
        "SELECT m.id, m.file_name, m.directory_id, m.file_format, m.has_embedded_video
         FROM media_items m
         JOIN directories d ON d.id = m.directory_id
         WHERE d.root_id = ?1
           AND m.is_deleted = 0
           AND m.companion_of IS NULL
           AND m.file_format IN ('jpg','jpeg','heic','heif','mov','mp4')
         ORDER BY m.directory_id, m.file_name",
    )?;

    let records: Vec<PairingRecord> = stmt
        .query_map(rusqlite::params![root_id], |row| {
            let file_name: String = row.get(1)?;
            let ext: String = row.get(3)?;
            // Derive stem: remove the extension portion
            // 派生文件主名：删除扩展名部分
            let stem = file_name
                .rsplit_once('.')
                .map(|(s, _)| s.to_lowercase())
                .unwrap_or_else(|| file_name.to_lowercase());
            Ok(PairingRecord {
                id: row.get(0)?,
                file_stem: stem,
                directory_id: row.get(2)?,
                extension: ext,
                is_motion: row.get::<_, i64>(4)? != 0,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Group by (directory_id, file_stem)
    // 按 (directory_id, file_stem) 分组
    let mut groups: HashMap<(i64, String), Group> = HashMap::new();

    for rec in &records {
        let g = groups
            .entry((rec.directory_id, rec.file_stem.clone()))
            .or_default();
        match rec.extension.as_str() {
            // 静图侧：JPEG 或 HEIC/HEIF（HEIC Live Photo，T7）。记录其 motion 信号供 mp4 守门。
            "jpg" | "jpeg" | "heic" | "heif" => {
                g.image_id = Some(rec.id);
                g.image_is_motion = rec.is_motion;
            }
            "mov" => g.mov_id = Some(rec.id),
            "mp4" => g.mp4_id = Some(rec.id),
            _ => {}
        }
    }

    let mut paired = 0u64;
    for (_, g) in groups {
        let Some(image) = g.image_id else { continue };

        // 收集本组要标为 companion 的视频：
        // - MOV：Apple Live Photo，纯 stem 配对（沿用 T7，不守门——mov 几乎是 iPhone 专属容器）。
        // - MP4：分体式 Motion Photo，**仅当静图带 has_embedded_video 时**才配（T15 守门），
        //   否则普通照片 + 同名普通 mp4 会被误吞。
        let mp4_companion = if g.image_is_motion { g.mp4_id } else { None };
        let companions = [g.mov_id, mp4_companion];

        let mut linked = false;
        for vid in companions.into_iter().flatten() {
            // Mark the video as a companion (it will be hidden from the grid)
            // 将视频标记为关联文件（它将从网格中隐藏）
            conn.execute(
                "UPDATE media_items SET companion_of=?1, updated_at=strftime('%s','now') WHERE id=?2",
                rusqlite::params![image, vid],
            )?;
            debug!("Paired LIVE/MOTION: image_id={image}, companion_video_id={vid}");
            linked = true;
            paired += 1;
        }

        if linked {
            // Mark the still image as a live/motion photo
            // 将静图标记为实况/动态照片
            conn.execute(
                "UPDATE media_items SET is_live_photo=1, updated_at=strftime('%s','now') WHERE id=?1",
                rusqlite::params![image],
            )?;
        }
    }

    info!("Live/Motion Photo pairing complete: {paired} pairs found | 实况/动态照片配对完成：发现 {paired} 对");
    Ok(paired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn mem_db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c.execute_batch(
            "INSERT INTO scan_roots (id, path, alias) VALUES (1, '/r', 'R');
             INSERT INTO directories (id, root_id, rel_path, name) VALUES (10, 1, '', 'r');",
        )
        .unwrap();
        c
    }

    fn add(c: &Connection, id: i64, name: &str, fmt: &str, media_type: &str) {
        c.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key)
             VALUES (?1, 10, ?2, 0, 0, ?3, ?4, 0, 0, 0, 0)",
            params![id, name, fmt, media_type],
        )
        .unwrap();
    }

    /// 插入一张已带 motion 信号的静图（模拟 enrichment 已对 XMP `GCamera:MotionPhoto=1`
    /// 置位 has_embedded_video=1），供分体式 Motion Photo 配对测试。
    fn add_motion_image(c: &Connection, id: i64, name: &str, fmt: &str) {
        add(c, id, name, fmt, "image");
        c.execute(
            "UPDATE media_items SET has_embedded_video=1 WHERE id=?1",
            params![id],
        )
        .unwrap();
    }

    fn is_live(c: &Connection, id: i64) -> bool {
        c.query_row(
            "SELECT is_live_photo FROM media_items WHERE id=?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
            != 0
    }
    fn companion_of(c: &Connection, id: i64) -> Option<i64> {
        c.query_row(
            "SELECT companion_of FROM media_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// HEIC + 同名 MOV → 配对（HEIC 标 live、MOV 标 companion）。这是现代 iPhone 的 Live Photo（T7）。
    #[test]
    fn heic_mov_pairs() {
        let c = mem_db();
        add(&c, 1, "IMG_1.heic", "heic", "image");
        add(&c, 2, "IMG_1.mov", "mov", "video");
        let n = pair_live_photos(&c, 1).unwrap();
        assert_eq!(n, 1);
        assert!(is_live(&c, 1), "HEIC 应标 is_live_photo");
        assert_eq!(companion_of(&c, 2), Some(1), "MOV 应标 companion_of=HEIC");
    }

    /// HEIF 扩展名同样配对。
    #[test]
    fn heif_mov_pairs() {
        let c = mem_db();
        add(&c, 1, "IMG_2.heif", "heif", "image");
        add(&c, 2, "IMG_2.mov", "mov", "video");
        assert_eq!(pair_live_photos(&c, 1).unwrap(), 1);
        assert!(is_live(&c, 1));
    }

    /// 回归：JPEG + MOV 仍配对（不因加 HEIC 而退化）。
    #[test]
    fn jpeg_mov_still_pairs() {
        let c = mem_db();
        add(&c, 1, "IMG_3.jpg", "jpg", "image");
        add(&c, 2, "IMG_3.mov", "mov", "video");
        assert_eq!(pair_live_photos(&c, 1).unwrap(), 1);
        assert!(is_live(&c, 1));
        assert_eq!(companion_of(&c, 2), Some(1));
    }

    /// HEIC 无同名 MOV → 不配对（不误标 live）。
    #[test]
    fn heic_without_mov_not_paired() {
        let c = mem_db();
        add(&c, 1, "IMG_4.heic", "heic", "image");
        assert_eq!(pair_live_photos(&c, 1).unwrap(), 0);
        assert!(!is_live(&c, 1), "无 MOV 伴随的 HEIC 不应标 live");
    }

    /// T15：分体式 Motion Photo —— JPG（带 motion 信号）+ 同名 MP4 → 配对。
    #[test]
    fn motion_jpg_mp4_pairs() {
        let c = mem_db();
        add_motion_image(&c, 1, "PXL_5.jpg", "jpg");
        add(&c, 2, "PXL_5.mp4", "mp4", "video");
        assert_eq!(pair_live_photos(&c, 1).unwrap(), 1);
        assert!(is_live(&c, 1), "motion JPG 应标 is_live_photo");
        assert_eq!(
            companion_of(&c, 2),
            Some(1),
            "同名 MP4 应标 companion_of=JPG"
        );
    }

    /// T15 守门核心：普通 JPG（无 motion 信号）+ 同名 MP4 → **不配对**，
    /// 否则普通照片 + 恰好同名的普通视频会被误吞、从画廊隐藏。
    #[test]
    fn plain_jpg_mp4_not_paired() {
        let c = mem_db();
        add(&c, 1, "VID_6.jpg", "jpg", "image"); // 无 has_embedded_video
        add(&c, 2, "VID_6.mp4", "mp4", "video");
        assert_eq!(
            pair_live_photos(&c, 1).unwrap(),
            0,
            "无 motion 信号不应配 mp4"
        );
        assert!(!is_live(&c, 1));
        assert_eq!(
            companion_of(&c, 2),
            None,
            "普通 mp4 不应被标 companion（仍可见）"
        );
    }

    /// T15 边界：MOV 配对不受 motion 守门影响（Apple Live Photo 纯 stem，沿用 T7）。
    /// 即便静图无 has_embedded_video，JPG + MOV 仍应配对。
    #[test]
    fn mov_pairing_not_motion_gated() {
        let c = mem_db();
        add(&c, 1, "IMG_7.jpg", "jpg", "image"); // 无 motion 信号
        add(&c, 2, "IMG_7.mov", "mov", "video");
        assert_eq!(pair_live_photos(&c, 1).unwrap(), 1, "MOV 不守门，应配对");
        assert!(is_live(&c, 1));
        assert_eq!(companion_of(&c, 2), Some(1));
    }

    /// T15 边界：motion JPG 同时有同名 MOV + MP4 → 两个视频都标 companion（都从画廊隐藏）。
    #[test]
    fn motion_jpg_both_mov_and_mp4_paired() {
        let c = mem_db();
        add_motion_image(&c, 1, "MIX_8.jpg", "jpg");
        add(&c, 2, "MIX_8.mov", "mov", "video");
        add(&c, 3, "MIX_8.mp4", "mp4", "video");
        assert_eq!(pair_live_photos(&c, 1).unwrap(), 2, "MOV + MP4 各计一对");
        assert!(is_live(&c, 1));
        assert_eq!(companion_of(&c, 2), Some(1));
        assert_eq!(companion_of(&c, 3), Some(1));
    }

    /// T15 回归：孤立 MP4（无同名静图）不受影响，保持可见。
    #[test]
    fn lone_mp4_untouched() {
        let c = mem_db();
        add(&c, 1, "CLIP_9.mp4", "mp4", "video");
        assert_eq!(pair_live_photos(&c, 1).unwrap(), 0);
        assert_eq!(companion_of(&c, 1), None);
    }
}
