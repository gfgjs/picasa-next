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
    get_audios_needing_meta, get_item_path_info, get_videos_needing_meta, update_live_photo_flags,
    update_media_dimensions, update_sort_datetime, update_video_dimensions, upsert_audio_meta,
    upsert_image_meta, upsert_video_meta,
};
use crate::error::{AppError, Result};
use crate::scanner::live_photo::pair_live_photos;
use crate::scanner::metadata::{
    apply_orientation_swap, detect_motion_photo_xmp, parse_exif_meta, read_raw_dimensions,
};
use crate::utils::path::resolve_media_path;

use serde::{Deserialize, Serialize};

const ENRICHMENT_BATCH: i64 = 500;

/// A rayon pool that **reserves one CPU core for the foreground**, so the single-threaded
/// `compute_layout` stays responsive while the (parallel, partly IO-bound) media probe runs during
/// import (布局被视频任务阻塞 的扫描期分支). Unlike the derivation pipeline — which fully pauses on
/// user interaction — enrichment is the import work the user is *waiting for*, and its own progress
/// events auto-trigger relayout, so a yield-on-interaction would starve it. Reserving a core instead
/// keeps the UI smooth without throttling the import. `None` → caller falls back to the global pool.
/// 一个**为前台保留一个 CPU 核**的 rayon 池，使单线程的 `compute_layout` 在导入期（并行、且部分受 IO
/// 限制的）媒体探测运行时仍跟手。与派生流水线（用户交互即完全暂停）不同：enrichment 是用户正在等待的
/// 导入工作，且其进度事件会自动触发重排，故「交互即让步」会饿死它。改为保留一个核，既保 UI 流畅又不
/// 拖慢导入。`None` → 调用方回退到全局池。
fn reserved_core_pool() -> Option<rayon::ThreadPool> {
    let n = std::thread::available_parallelism()
        .map(|c| c.get().saturating_sub(1).max(1))
        .unwrap_or(1);
    rayon::ThreadPoolBuilder::new().num_threads(n).build().ok()
}

// ── IPC event payloads ────────────────────────────────────────────────────────
// ── IPC 事件负载 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaEnrichedPayload {
    pub root_id: i64,
    pub enriched_count: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentCompletedPayload {
    pub root_id: i64,
    pub elapsed_ms: u64,
    /// 终态错误码：`None` 表示正常完成；`Some(稳定码)` 表示后台补全**异常终止**
    /// （出错 / panic）。前端据此弹 warning，告知用户部分元数据可能缺失——避免
    /// 失败「伪装成正常完成」只进日志、对用户不可见。
    ///
    /// 携带**稳定粗粒度码**（如 `"enrich_failed"` / `"enrich_panicked"`）而非原始
    /// 错误串，遵循 IPC 边界错误契约；细分原因（EXIF/XMP/视频…）已在后端日志中。
    pub error_code: Option<String>,
}

impl EnrichmentCompletedPayload {
    /// 正常完成的终态事件。
    pub fn ok(root_id: i64, elapsed_ms: u64) -> Self {
        Self {
            root_id,
            elapsed_ms,
            error_code: None,
        }
    }

    /// 异常终止的终态事件，携带稳定错误码。`elapsed_ms` 置 0（耗时对失败无意义）。
    pub fn failed(root_id: i64, code: &str) -> Self {
        Self {
            root_id,
            elapsed_ms: 0,
            error_code: Some(code.to_string()),
        }
    }
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
/// S1：enrichment 批提交后 bump 数据版本——尺寸回写与 EXIF 时间修正改变布局的几何与
/// 顺序输入，items 取数缓存必须失效。经 AppHandle 取 AppState（测试无 managed state 时
/// 静默跳过，行为即「无缓存可失效」）。
fn bump_layout_data_version(app: &AppHandle) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<std::sync::Arc<crate::state::AppState>>() {
        state.bump_data_version();
    }
}

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

    // T13（§3.7.2）：图片段并行 EXIF/尺寸提取也跑在**保留核**的池上（与视频/音频段一致），
    // 为前台 `compute_layout` 留一个核——否则海量图片导入会把全局 rayon 池占满、与「2s 自动重排」
    // 反馈循环互相饿死。一次性建池、整段复用；`None`（建池失败）→ 回退全局池。
    let img_pool = reserved_core_pool();

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
            let x = stmt
                .query_map(rusqlite::params![root_id, ENRICHMENT_BATCH], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
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
            batch
                .iter()
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
        // T13（§3.7.2）：图片段并行解析也跑在保留核池（img_pool）上——闭包内 par_iter 自动用 install 的池。
        let parse_batch = || {
            path_infos
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
                .collect()
        };
        // 每项解析结果元组：(id, EXIF/尺寸解析结果, is_live, has_embedded_video, 像素尺寸)，
        // 局部聚合用、仅此一处，抽 type 别名收益有限。
        #[allow(clippy::type_complexity)]
        let parsed: Vec<(i64, Result<ImageMeta>, bool, bool, Option<(i64, i64)>)> = match &img_pool
        {
            Some(p) => p.install(parse_batch),
            None => parse_batch(),
        };

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
        // S1：本批尺寸/EXIF 时间已提交（几何与顺序输入变化）→ bump。
        bump_layout_data_version(app);

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

    // ── Video probe (§2.1 / §3.2): real dimensions + rotation + video_meta ─────────
    // ── 视频探测（§2.1 / §3.2）：真实宽高 + 旋转 + video_meta ───────────────────────
    // Layout-critical: replaces the 16:9 placeholder with the true aspect (rotation-corrected),
    // so portrait phone videos don't lie down and Justified Layout is correct.
    // 布局关键：用真实比例（含旋转修正）替换 16:9 占位，使竖拍视频不躺倒、Justified Layout 正确。
    if !cancel.is_cancelled() {
        if let Err(e) = enrich_videos(app, writer, root_id, cancel) {
            if !matches!(e, AppError::Cancelled) {
                warn!("Video enrichment error: {e}");
            }
        }
    }

    // ── Audio probe (§3.6): lofty tags/lyrics + duration → audio_meta ──────────────
    // ── 音频探测（§3.6）：lofty 标签/歌词 + 时长 → audio_meta ───────────────────────
    // 音频比例固定 400×400（封面位，fast_scan 已设默认），无需回填尺寸；这里仅补元数据。
    if !cancel.is_cancelled() {
        if let Err(e) = enrich_audios(app, writer, root_id, cancel) {
            if !matches!(e, AppError::Cancelled) {
                warn!("Audio enrichment error: {e}");
            }
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    info!("Enrichment complete: root_id={root_id} enriched={enriched_total} elapsed={elapsed_ms}ms | 增量补全完成: root_id={root_id} 补全={enriched_total} 耗时={elapsed_ms}ms");

    let _ = app.emit(
        "enrichment:completed",
        EnrichmentCompletedPayload::ok(root_id, elapsed_ms),
    );

    Ok(())
}

/// Probe videos in `root_id` lacking `video_meta` and backfill real dimensions / rotation /
/// duration + a `video_meta` row (§2.1 / §3.2). Uses the `VideoBackend` registry (Media
/// Foundation on Windows); on platforms / formats with no backend, a minimal `video_meta` row
/// is written so the item is not re-probed forever (it keeps its 16:9 placeholder — §9).
/// 探测 `root_id` 下缺 `video_meta` 的视频，回填真实宽高/旋转/时长 + 一行 `video_meta`（§2.1 / §3.2）。
/// 使用 `VideoBackend` 注册表（Windows 下为 Media Foundation）；无后端的平台/格式写一行最小
/// `video_meta`，避免反复探测（保留 16:9 占位，§9）。
fn enrich_videos(
    app: &AppHandle,
    writer: &Mutex<Connection>,
    root_id: i64,
    cancel: &CancellationToken,
) -> Result<()> {
    use crate::video::{backend_for, VideoInfo};
    use std::path::Path;

    // Reserve a core for the foreground so layout stays responsive during import (see helper).
    // 为前台保留一个核，使导入期布局保持跟手（见 helper）。
    let probe_pool = reserved_core_pool();

    let mut done_total: i64 = 0;
    loop {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }

        let batch: Vec<(i64, String, String)> = {
            let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
            get_videos_needing_meta(&conn, root_id, ENRICHMENT_BATCH)?
        };
        if batch.is_empty() {
            break;
        }

        // Probe in parallel — each MF call inits COM on its own rayon thread. Run on the
        // core-reserving pool so the foreground keeps a core (布局被视频任务阻塞 的扫描期分支).
        // 并行探测 —— 每个 MF 调用在各自的 rayon 线程上初始化 COM。跑在保留核的池上，使前台留有一个核。
        let probe = || -> Vec<(i64, Option<VideoInfo>)> {
            batch
                .par_iter()
                .map(|(id, abs, ext)| {
                    let info = backend_for(ext).and_then(|b| b.probe(Path::new(abs)).ok());
                    (*id, info)
                })
                .collect()
        };
        let probed: Vec<(i64, Option<VideoInfo>)> = match &probe_pool {
            Some(p) => p.install(probe),
            None => probe(),
        };

        {
            let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
            let tx = conn.unchecked_transaction()?;
            for (id, info) in &probed {
                match info {
                    Some(vi) if vi.width > 0 && vi.height > 0 => {
                        let dur = if vi.duration_ms > 0 {
                            Some(vi.duration_ms as i64)
                        } else {
                            None
                        };
                        let _ = update_video_dimensions(
                            &tx,
                            *id,
                            vi.width as i64,
                            vi.height as i64,
                            dur,
                        );
                        let _ = upsert_video_meta(
                            &tx,
                            *id,
                            vi.codec.as_deref(),
                            if vi.fps > 0.0 {
                                Some(vi.fps as f64)
                            } else {
                                None
                            },
                            if vi.bitrate > 0 {
                                Some(vi.bitrate as i64)
                            } else {
                                None
                            },
                            vi.rotation as i64,
                            vi.has_audio,
                        );
                    }
                    // Probe failed / unsupported container: write a minimal row so the LEFT JOIN
                    // no longer selects it (no infinite re-probe). Mirrors the image-meta pattern.
                    // 探测失败 / 不支持的容器：写最小行使 LEFT JOIN 不再选中（不无限重探）。与 image_meta 同理。
                    _ => {
                        let _ = upsert_video_meta(&tx, *id, None, None, None, 0, false);
                    }
                }
            }
            tx.commit()?;
        }
        // S1：视频真实宽高已回填（几何输入变化）→ bump。
        bump_layout_data_version(app);

        done_total += probed.len() as i64;
        // Nudge the gallery to recompute (corrected aspect ratios). Reuses the enrichment event.
        // 通知画廊重算（修正后的比例）。复用 enrichment 事件。
        let _ = app.emit(
            "db:media_enriched",
            MediaEnrichedPayload {
                root_id,
                enriched_count: done_total,
                total: done_total,
            },
        );
    }

    if done_total > 0 {
        info!("Video enrichment: probed {done_total} video(s) for root_id={root_id} | 视频探测: root_id={root_id} 共 {done_total} 个");
    }
    Ok(())
}

/// Probe audio in `root_id` lacking `audio_meta` and backfill tags/lyrics-source + duration
/// via `lofty` (§3.6). Like the video probe it writes a row even on failure so the LEFT JOIN
/// no longer selects it (no infinite re-probe). Cover art is a separate derivation
/// (`kind=audio_cover`); lyrics *text* is read lazily by `get_audio_detail`, so only the
/// provenance (embedded/lrc/none) + the `.lrc` path are persisted here.
/// 探测 `root_id` 下缺 `audio_meta` 的音频，用 `lofty` 回填标签/歌词来源 + 时长（§3.6）。与视频探测
/// 同理：即便失败也写行，使 LEFT JOIN 不再选中（不无限重探）。封面是独立派生（`kind=audio_cover`）；
/// 歌词**文本**由 `get_audio_detail` 懒加载，故此处仅持久化来源（embedded/lrc/none）+ `.lrc` 路径。
fn enrich_audios(
    app: &AppHandle,
    writer: &Mutex<Connection>,
    root_id: i64,
    cancel: &CancellationToken,
) -> Result<()> {
    use crate::audio::{find_lrc, lyrics_source, read_tags, AudioTags};
    use std::path::Path;

    // Reserve a core for the foreground so layout stays responsive during import (see helper).
    // 为前台保留一个核，使导入期布局保持跟手（见 helper）。
    let probe_pool = reserved_core_pool();

    let mut done_total: i64 = 0;
    loop {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }

        let batch: Vec<(i64, String, String)> = {
            let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
            get_audios_needing_meta(&conn, root_id, ENRICHMENT_BATCH)?
        };
        if batch.is_empty() {
            break;
        }

        // Read tags in parallel (lofty is CPU/IO-bound, no shared state); run on the
        // core-reserving pool so the foreground keeps a core during import.
        // 并行读取标签（lofty 受 CPU/IO 限制，无共享状态）；跑在保留核的池上，使前台在导入期留有一个核。
        let probe = || -> Vec<(i64, AudioTags, &'static str, Option<String>)> {
            batch
                .par_iter()
                .map(|(id, abs, _ext)| {
                    let path = Path::new(abs);
                    let tags = read_tags(path).unwrap_or_default();
                    let lrc = find_lrc(path);
                    let src = lyrics_source(&tags, &lrc);
                    let lrc_path = lrc.map(|p| p.to_string_lossy().replace('\\', "/"));
                    (*id, tags, src, lrc_path)
                })
                .collect()
        };
        let probed: Vec<(i64, AudioTags, &'static str, Option<String>)> = match &probe_pool {
            Some(p) => p.install(probe),
            None => probe(),
        };

        {
            let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
            let tx = conn.unchecked_transaction()?;
            for (id, tags, lyrics_src, lrc_path) in &probed {
                let _ = upsert_audio_meta(
                    &tx,
                    *id,
                    tags.codec.as_deref(),
                    tags.artist.as_deref(),
                    tags.album.as_deref(),
                    tags.title.as_deref(),
                    tags.track_no,
                    tags.year,
                    tags.genre.as_deref(),
                    Some(*lyrics_src),
                    lrc_path.as_deref(),
                );
                // Backfill duration on the main table (audio dims stay at the 400×400 default).
                // 在主表回填时长（音频尺寸保持 400×400 默认）。
                if let Some(dur) = tags.duration_ms {
                    let _ = tx.execute(
                        "UPDATE media_items SET duration_ms=?1 WHERE id=?2 AND media_type='audio'",
                        rusqlite::params![dur, id],
                    );
                }
            }
            tx.commit()?;
        }
        // S1：音频时长已回填（duration 徽标随布局行下发）→ bump。
        bump_layout_data_version(app);

        done_total += probed.len() as i64;
        let _ = app.emit(
            "db:media_enriched",
            MediaEnrichedPayload {
                root_id,
                enriched_count: done_total,
                total: done_total,
            },
        );
    }

    if done_total > 0 {
        info!("Audio enrichment: probed {done_total} audio file(s) for root_id={root_id} | 音频探测: root_id={root_id} 共 {done_total} 个");
    }
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
