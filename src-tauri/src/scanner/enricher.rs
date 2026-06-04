// src-tauri/src/scanner/enricher.rs
//! Phase 2: Background enrichment — EXIF, XMP Motion Photo, Live Photo pairing, sort_datetime correction.
//! 阶段 2：后台信息丰富 — EXIF、XMP 动态照片、实况照片配对、sort_datetime 修正。
//!
//! Runs asynchronously after the fast scan completes.
//! 在快速扫描完成后异步运行。
//! Sends `db:media_enriched` and `enrichment:completed` Tauri events.
//! 发送 `db:media_enriched` 和 `enrichment:completed` Tauri 事件。

use std::sync::Mutex;

use rayon::prelude::*;
use rusqlite::Connection;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::db::models::ImageMeta;
use crate::db::queries::{
    get_item_path_info, update_live_photo_flags, update_sort_datetime,
    upsert_image_meta, update_video_meta,
};
use crate::error::{AppError, Result};
use crate::scanner::live_photo::pair_live_photos;
use crate::scanner::metadata::{
    detect_motion_photo_xmp, parse_exif_meta,
};
use crate::scanner::video_meta::extract_video_meta;
use crate::utils::path::resolve_media_path;

use serde::{Deserialize, Serialize};

const ENRICHMENT_BATCH: i64 = 500;

// ── IPC event payloads ────────────────────────────────────────────────────────
// ── IPC 事件负载 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaEnrichedPayload {
    pub root_id:        i64,
    pub enriched_count: i64,
    pub total:          i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentCompletedPayload {
    pub root_id:    i64,
    pub elapsed_ms: u64,
}

// ── Enrichment entry point ────────────────────────────────────────────────────
// ── 丰富信息入口点 ────────────────────────────────────────────────────

/// Run background enrichment for a scan root.
/// 运行扫描根目录的后台信息丰富。
///
/// This function is meant to be called from `tokio::task::spawn_blocking`
/// 此函数旨在从 `tokio::task::spawn_blocking` 调用
/// so the async runtime isn't blocked.
/// 因此异步运行时不会被阻塞。
pub fn run_enrichment(
    app: &AppHandle,
    writer: &Mutex<Connection>,
    root_id: i64,
    cancel: &CancellationToken,
) -> Result<()> {
    let started = std::time::Instant::now();
    info!("Enrichment started: root_id={root_id} | 增量补全开始: root_id={root_id}");

    // ── Count total unenriched items ──────────────────────────────────────
    // ── 计算未丰富信息的项目总数 ──────────────────────────────────────
    let total: i64 = {
        let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        conn.query_row(
            "SELECT COUNT(*) FROM media_items m
             LEFT JOIN image_meta im ON im.item_id = m.id
             JOIN directories d ON d.id = m.directory_id
             WHERE d.root_id=?1 AND m.is_deleted=0 AND m.media_type='image' AND im.item_id IS NULL",
            rusqlite::params![root_id],
            |r| r.get(0),
        )?
    };

    info!("Enrichment: {total} items to process for root_id={root_id} | 增量补全: root_id={root_id} 共有 {total} 项待处理");

    let mut enriched_total: i64 = 0;

    loop {
        if cancel.is_cancelled() {
            warn!("Enrichment cancelled at root_id={root_id}");
            return Err(AppError::Cancelled);
        }

        // Fetch next batch of unenriched item IDs (within this root)
        // 获取下一批未丰富信息的项目 ID（在该根目录下）
        let ids: Vec<i64> = {
            let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
            let mut stmt = conn.prepare(
                "SELECT m.id FROM media_items m
                 LEFT JOIN image_meta im ON im.item_id = m.id
                 JOIN directories d ON d.id = m.directory_id
                 WHERE d.root_id=?1 AND m.is_deleted=0 AND m.media_type='image' AND im.item_id IS NULL
                 ORDER BY m.created_at DESC
                 LIMIT ?2",
            )?;
            let x = stmt.query_map(rusqlite::params![root_id, ENRICHMENT_BATCH], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect::<Vec<i64>>();
            x
        };

        if ids.is_empty() {
            break;
        }

        // Collect path info for each item
        // 收集每个项目的路径信息
        let path_infos: Vec<(i64, String)> = {
            let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
            ids.iter()
                .filter_map(|&id| {
                    get_item_path_info(&conn, id)
                        .ok()
                        .map(|(root_p, rel_p, name)| {
                            let abs = resolve_media_path(&root_p, &rel_p, &name);
                            (id, abs)
                        })
                })
                .collect()
        };

        // Parallel EXIF parse
        // 并行 EXIF 解析
        let parsed: Vec<(i64, Result<ImageMeta>, bool, bool)> = path_infos
            .par_iter()
            .map(|(id, abs_path)| {
                let path = std::path::Path::new(abs_path);
                let meta = parse_exif_meta(path);
                let (is_live, has_embedded) = if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| matches!(e.to_lowercase().as_str(), "jpg" | "jpeg"))
                    .unwrap_or(false)
                {
                    detect_motion_photo_xmp(path)
                } else {
                    (false, false)
                };
                (*id, meta, is_live, has_embedded)
            })
            .collect();

        // Write results in a single transaction
        // 在单个事务中写入结果
        {
            let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
            let tx = conn.unchecked_transaction()?;

            for (item_id, meta_result, is_live, has_embedded) in &parsed {
                match meta_result {
                    Ok(meta) => {
                        let mut m = meta.clone();
                        m.item_id = *item_id;

                        if let Err(e) = upsert_image_meta(&tx, &m) {
                            warn!("Failed to upsert image_meta for id={item_id}: {e}");
                        }

                        // Correct sort_datetime = COALESCE(exif_datetime, file_mtime)
                        // 修正 sort_datetime = COALESCE(exif_datetime, file_mtime)
                        if let Some(exif_dt) = m.exif_datetime {
                            let _ = update_sort_datetime(&tx, *item_id, exif_dt);
                        }
                        // NOTE: width/height orientation correction is handled by fast_scan
                        // 注意：宽度/高度方向修正由 fast_scan 处理
                        // for JPEG (the most common case). Do NOT swap here again to avoid
                        // 针对 JPEG（最常见的情况）。不要在这里再次交换以避免
                        // a double-flip. If non-JPEG orientation support is needed in future,
                        // 双重翻转。如果将来需要非 JPEG 方向支持，
                        // add a media_items.dims_corrected flag and only swap when it is 0.
                        // 添加一个 media_items.dims_corrected 标志并仅在其为 0 时进行交换。
                    }
                    Err(e) => {
                        debug!("EXIF parse skipped id={item_id}: {e}");
                        // Insert a minimal row so we don't re-attempt this item
                        // 插入最小行，以便我们不会再次尝试此项目
                        let minimal = ImageMeta {
                            item_id: *item_id,
                            orientation: 1,
                            ..Default::default()
                        };
                        let _ = upsert_image_meta(&tx, &minimal);
                    }
                }

                if *is_live {
                    let _ = update_live_photo_flags(&tx, *item_id, true, *has_embedded);
                }
            }

            tx.commit()?;
        }

        enriched_total += parsed.len() as i64;
        debug!("Enrichment batch done: {enriched_total}/{total}");

        // Emit progress event
        // 发出进度事件
        let _ = app.emit(
            "db:media_enriched",
            MediaEnrichedPayload {
                root_id,
                enriched_count: enriched_total,
                total,
            },
        );
    }

    // ── Live Photo pairing ────────────────────────────────────────────────
    // ── 实况照片配对 ────────────────────────────────────────────────
    if !cancel.is_cancelled() {
        let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        if let Err(e) = pair_live_photos(&conn, root_id) {
            error!("Live Photo pairing error: {e}");
        }
    }

    // ── Phase 2b: Video metadata enrichment (MP4/MOV) ────────────────────────────
    // ── 阶段2b：视频元数据丰富 (MP4/MOV) ────────────────────────────
    if !cancel.is_cancelled() {
        // 查询安将丰富的视频项（宽度为 0，说明尚未提取）| Query video items needing enrichment (width=0)
        let video_ids: Vec<i64> = {
            let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
            let mut stmt = conn.prepare(
                "SELECT m.id FROM media_items m
                 JOIN directories d ON d.id = m.directory_id
                 WHERE d.root_id=?1 AND m.is_deleted=0
                   AND m.media_type='video' AND m.width=0
                 ORDER BY m.created_at DESC",
            )?;
            let x: Vec<i64> = stmt.query_map(rusqlite::params![root_id], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            x
        };

        info!("Video enrichment: {} MP4/MOV items to process for root_id={root_id} | 视频元数据丰富: root_id={root_id} 共 {} 项待处理",
              video_ids.len(), video_ids.len());

        // 串行处理（mp4parse 和文件 I/O 对 rayon 线程池不友好）| Process serially (mp4parse + file I/O unfriendly to rayon)
        for video_id in video_ids {
            if cancel.is_cancelled() {
                break;
            }
            let abs_path = {
                let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
                get_item_path_info(&conn, video_id)
                    .ok()
                    .map(|(root_p, rel_p, name)| resolve_media_path(&root_p, &rel_p, &name))
            };

            if let Some(path) = abs_path {
                let path = std::path::Path::new(&path);
                if let Some(vmeta) = extract_video_meta(path) {
                    let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
                    if let Err(e) = update_video_meta(&conn, video_id, vmeta.width, vmeta.height, vmeta.duration_ms) {
                        warn!("[VideoMeta] Failed to update id={video_id}: {e} | 视频元数据更新失败");
                    } else {
                        tracing::debug!("[VideoMeta] id={video_id} w={} h={} dur={}ms | 提取成功",
                                       vmeta.width, vmeta.height, vmeta.duration_ms);
                    }
                }
            }
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    info!("Enrichment complete: root_id={root_id} enriched={enriched_total} elapsed={elapsed_ms}ms | 增量补全完成: root_id={root_id} 补全={enriched_total} 耗时={elapsed_ms}ms");

    let _ = app.emit(
        "enrichment:completed",
        EnrichmentCompletedPayload { root_id, elapsed_ms },
    );

    Ok(())
}
