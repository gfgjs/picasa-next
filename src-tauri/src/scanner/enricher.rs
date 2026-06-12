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
    get_item_path_info, update_live_photo_flags, update_media_dimensions, update_sort_datetime,
    upsert_image_meta,
};
use crate::error::{AppError, Result};
use crate::scanner::live_photo::pair_live_photos;
use crate::scanner::metadata::{
    apply_orientation_swap, detect_motion_photo_xmp, parse_exif_meta, read_raw_dimensions,
};
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
    group_by: &str,
    sort_within_group: &str,
    sort_order: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let started = std::time::Instant::now();
    info!("Enrichment started: root_id={root_id} | 增量补全开始: root_id={root_id}");

    // Process in the gallery's current view order so the placeholder dimensions
    // are backfilled top-down — following the user's likely scroll — instead of
    // by insertion id. Mirrors the ORDER BY in `query_layout_geometry`.
    // 按画廊当前视图顺序处理，使占位尺寸自上而下补全 —— 贴合用户可能的滚动 ——
    // 而非按插入 id。与 `query_layout_geometry` 的 ORDER BY 对齐。
    let order_clause = enrichment_order_clause(group_by, sort_within_group, sort_order);
    let batch_sql = format!(
        "SELECT m.id, m.width, m.height FROM media_items m
         LEFT JOIN image_meta im ON im.item_id = m.id
         JOIN directories d ON d.id = m.directory_id
         WHERE d.root_id=?1 AND m.is_deleted=0 AND m.media_type='image' AND im.item_id IS NULL
         {order_clause}
         LIMIT ?2"
    );

    // ── Count total unenriched items ──────────────────────────────────────
    // ── 计算未丰富信息的项目总数 ──────────────────────────────────────
    let total: i64 = {
        let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
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

        // Fetch next batch of unenriched items (within this root), with their
        // current dimensions so we can backfill any 0×0 placeholders from the
        // fast scan's deferred-dimension path.
        // 获取下一批未丰富信息的项目（在该根目录下），并带上当前尺寸，
        // 以便补全快速扫描"延后尺寸"路径留下的 0×0 占位。
        let batch: Vec<(i64, i64, i64)> = {
            let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
            let mut stmt = conn.prepare(&batch_sql)?;
            let x = stmt.query_map(rusqlite::params![root_id, ENRICHMENT_BATCH], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
            })?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();
            x
        };

        if batch.is_empty() {
            break;
        }

        // Collect path info for each item (carry width/height through).
        // 收集每个项目的路径信息（一并带上宽/高）。
        let path_infos: Vec<(i64, String, i64, i64)> = {
            let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
            batch.iter()
                .filter_map(|&(id, w, h)| {
                    get_item_path_info(&conn, id)
                        .ok()
                        .map(|(root_p, rel_p, name)| {
                            let abs = resolve_media_path(&root_p, &rel_p, &name);
                            (id, abs, w, h)
                        })
                })
                .collect()
        };

        // Parallel EXIF parse + (for 0×0 placeholders) real dimension extraction.
        // 并行 EXIF 解析 +（针对 0×0 占位）真实尺寸提取。
        let parsed: Vec<(i64, Result<ImageMeta>, bool, bool, Option<(i64, i64)>)> = path_infos
            .par_iter()
            .map(|(id, abs_path, w, h)| {
                let path = std::path::Path::new(abs_path);
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                let meta = parse_exif_meta(path);
                let (is_live, has_embedded) = if matches!(ext.as_str(), "jpg" | "jpeg") {
                    detect_motion_photo_xmp(path)
                } else {
                    (false, false)
                };
                // Only read dimensions for placeholder items — keeps the eager
                // first-screen dims (and their orientation) untouched (no double-flip).
                // Reuse the orientation just parsed above (meta) instead of opening
                // the JPEG a second time for its Orientation tag.
                // 仅对占位项读取尺寸 — 保持首屏即时尺寸（及其方向）不变（不双重翻转）。
                // 复用上面刚解析出的方向（meta），而不是为读 Orientation 再开一次 JPEG。
                let dims = if *w == 0 || *h == 0 {
                    let raw = read_raw_dimensions(path, &ext);
                    if raw.0 > 0 && raw.1 > 0 {
                        let oriented = if matches!(ext.as_str(), "jpg" | "jpeg") {
                            let orientation =
                                meta.as_ref().map(|m| m.orientation as u32).unwrap_or(1);
                            apply_orientation_swap(raw, orientation)
                        } else {
                            raw
                        };
                        Some(oriented)
                    } else {
                        None
                    }
                } else {
                    None
                };
                (*id, meta, is_live, has_embedded, dims)
            })
            .collect();

        // Write results in a single transaction
        // 在单个事务中写入结果
        {
            let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
            let tx = conn.unchecked_transaction()?;

            for (item_id, meta_result, is_live, has_embedded, dims) in &parsed {
                // Backfill real dimensions for placeholder (0×0) items.
                // 为占位（0×0）项补全真实尺寸。
                if let Some((w, h)) = dims {
                    if let Err(e) = update_media_dimensions(&tx, *item_id, *w, *h) {
                        warn!("Failed to backfill dimensions for id={item_id}: {e}");
                    }
                }

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
        let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
        if let Err(e) = pair_live_photos(&conn, root_id) {
            error!("Live Photo pairing error: {e}");
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

/// Build the enrichment batch ORDER BY clause so it matches the gallery's view
/// order (mirrors `query_layout_geometry`, minus the AI-similarity branch which
/// has no data during import). Inputs come from a fixed option set → injection-safe.
/// 构建 enrichment 批次的 ORDER BY，使其与画廊视图顺序一致（对齐
/// `query_layout_geometry`，去掉导入期无数据的 AI 相似度分支）。入参取自固定选项集 → 无注入风险。
fn enrichment_order_clause(group_by: &str, sort_within_group: &str, sort_order: &str) -> String {
    let dir = if sort_order == "asc" { "ASC" } else { "DESC" };
    let secondary = if sort_within_group == "filename" {
        format!("m.file_name COLLATE NATURAL_CMP {dir}")
    } else {
        // 'datetime' (or 'similarity', which has no scores at import) → sort_datetime
        format!("m.sort_datetime {dir}")
    };
    match group_by {
        "folder" => format!("ORDER BY d.rel_path ASC, {secondary}"),
        "date" => {
            if sort_within_group == "filename" {
                format!("ORDER BY date(m.sort_datetime,'unixepoch','localtime') {dir}, {secondary}")
            } else {
                format!("ORDER BY m.sort_datetime {dir}")
            }
        }
        _ => format!("ORDER BY {secondary}"),
    }
}
