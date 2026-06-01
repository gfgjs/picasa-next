// src-tauri/src/scanner/enricher.rs
//! Phase 2: Background enrichment — EXIF, XMP Motion Photo, Live Photo pairing, sort_datetime correction.
//!
//! Runs asynchronously after the fast scan completes.
//! Sends `db:media_enriched` and `enrichment:completed` Tauri events.

use std::sync::Mutex;

use rayon::prelude::*;
use rusqlite::Connection;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::db::models::ImageMeta;
use crate::db::queries::{
    get_item_path_info, get_unenriched_image_ids, update_live_photo_flags, update_sort_datetime,
    upsert_image_meta,
};
use crate::error::{AppError, Result};
use crate::scanner::live_photo::pair_live_photos;
use crate::scanner::metadata::{
    detect_motion_photo_xmp, orientation_needs_swap, parse_exif_meta,
};
use crate::utils::path::resolve_media_path;

use serde::{Deserialize, Serialize};

const ENRICHMENT_BATCH: i64 = 500;

// ── IPC event payloads ────────────────────────────────────────────────────────

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

/// Run background enrichment for a scan root.
///
/// This function is meant to be called from `tokio::task::spawn_blocking`
/// so the async runtime isn't blocked.
pub fn run_enrichment(
    app: &AppHandle,
    writer: &Mutex<Connection>,
    root_id: i64,
    cancel: &CancellationToken,
) -> Result<()> {
    let started = std::time::Instant::now();
    info!("Enrichment started: root_id={root_id}");

    // ── Count total unenriched items ──────────────────────────────────────
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

    info!("Enrichment: {total} items to process for root_id={root_id}");

    let mut enriched_total: i64 = 0;

    loop {
        if cancel.is_cancelled() {
            warn!("Enrichment cancelled at root_id={root_id}");
            return Err(AppError::Cancelled);
        }

        // Fetch next batch of unenriched item IDs (within this root)
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
                        if let Some(exif_dt) = m.exif_datetime {
                            let _ = update_sort_datetime(&tx, *item_id, exif_dt);
                        }

                        // Handle dimension swap for rotated images
                        if orientation_needs_swap(m.orientation as u32) {
                            // Swap width/height in DB if not already done
                            let _ = tx.execute(
                                "UPDATE media_items
                                 SET width=height, height=width,
                                     updated_at=strftime('%s','now')
                                 WHERE id=?1 AND width > height",
                                rusqlite::params![item_id],
                            );
                        }
                    }
                    Err(e) => {
                        debug!("EXIF parse skipped id={item_id}: {e}");
                        // Insert a minimal row so we don't re-attempt this item
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
    if !cancel.is_cancelled() {
        let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
        if let Err(e) = pair_live_photos(&conn, root_id) {
            error!("Live Photo pairing error: {e}");
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    info!("Enrichment complete: root_id={root_id} enriched={enriched_total} elapsed={elapsed_ms}ms");

    let _ = app.emit(
        "enrichment:completed",
        EnrichmentCompletedPayload { root_id, elapsed_ms },
    );

    Ok(())
}
