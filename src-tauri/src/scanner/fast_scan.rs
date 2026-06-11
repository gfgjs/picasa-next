// src-tauri/src/scanner/fast_scan.rs
//! Phase 1 fast scan: lightweight per-file operations, immediate DB insertion.
//! 阶段 1 快速扫描：轻量级单文件操作，立即插入数据库。
//!
//! Per-file work (all CPU-bound, handled by rayon):
//! 单文件工作（全部为 CPU 密集型，由 rayon 处理）：
//!   1. `image::image_dimensions()` → width/height from file header (no decode)
//!   1. `image::image_dimensions()` → 从文件头获取宽度/高度（无解码）
//!   2. JPEG: read Orientation tag (first ~1KB) → swap w/h if needed
//!   2. JPEG：读取方向标签（前 ~1KB）→ 如果需要则交换宽高
//!   3. TIFF: apply 50ms timeout protection
//!   3. TIFF：应用 50ms 超时保护
//!   4. `compute_cache_key`
//!   4. `compute_cache_key`
//!   5. Batch INSERT into `media_items` (500 rows/transaction)
//!   5. 批量 INSERT 到 `media_items`（500 行/事务）
//!
//! On completion, sends `ScanCompletedPayload` via the Tauri Channel.
//! 完成后，通过 Tauri 频道发送 `ScanCompletedPayload`。

use std::path::Path;
use std::sync::Mutex;

use rayon::prelude::*;
use rusqlite::Connection;
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::db::queries::{
    upsert_directory, upsert_fast_scan_item, update_scan_root_status, finish_scan_root, FastScanItem,
};
use crate::error::{AppError, Result};
use crate::scanner::metadata::read_image_dimensions;
use crate::scanner::walker::{walk_media_files, WalkedFile};
use crate::utils::format::{is_phase1_image, MediaType};
use crate::utils::hash::compute_cache_key;
use crate::utils::path::{dir_rel_path, normalize_db_path, path_depth};

use serde::{Deserialize, Serialize};

const BATCH_SIZE: usize = 500;
const PROGRESS_INTERVAL: usize = 500;

/// How many of the first-shown items get real pixel dimensions extracted up
/// front (covers the first few screens). The rest are inserted with a 0×0
/// placeholder (rendered as a square by the layout) and backfilled later by
/// enrichment — so a huge import is no longer blocked on extracting dimensions
/// for every file, while the first paint stays reflow-free.
/// 即时提取真实尺寸的"首屏项"数量（覆盖前几屏）。其余以 0×0 占位入库
/// （布局按正方形渲染），稍后由 enrichment 补全 —— 这样海量导入不再被
/// "逐个文件提尺寸"阻塞，同时首屏不会发生重排。
const EAGER_DIM_COUNT: usize = 500;

// ── IPC payloads ─────────────────────────────────────────────────────────────
// ── IPC 负载 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressPayload {
    pub root_id:     i64,
    pub scanned:     u64,
    pub total:       u64,
    pub current_dir: String,
    pub status:      String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCompletedPayload {
    pub root_id:     i64,
    pub total_items: u64,
    pub elapsed_ms:  u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanErrorPayload {
    pub root_id: i64,
    pub error:   String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ScanChannelPayload {
    Progress(ScanProgressPayload),
    Completed(ScanCompletedPayload),
    Error(ScanErrorPayload),
}

// ── Per-file dimension extraction ─────────────────────────────────────────────
// ── 单文件尺寸提取 ─────────────────────────────────────────────

struct FileInfo {
    walked:  WalkedFile,
    width:   i64,
    height:  i64,
}

/// Cheap, no-file-read placeholder dimensions for Phase-2 media (audio/doc/video).
/// Returns `None` for Phase-1 images, which need a real header read.
/// 阶段 2 媒体（音频/文档/视频）的廉价、无需读文件的占位尺寸。
/// 阶段 1 图像返回 `None`（需要真实读取文件头）。
fn cheap_phase2_dimensions(walked: &WalkedFile) -> Option<(i64, i64)> {
    if is_phase1_image(walked.extension.as_str()) {
        return None;
    }
    Some(match walked.media_type {
        MediaType::Audio    => (400, 400),
        MediaType::Document => (595, 842),
        _                   => (0, 0),
    })
}

/// Real pixel dimensions for a single file (Phase-2 → cheap constants;
/// Phase-1 image → orientation-corrected header read).
/// 单文件的真实尺寸（阶段 2 → 廉价常量；阶段 1 图像 → 经方向校正的文件头读取）。
fn extract_dimensions(walked: &WalkedFile) -> (i64, i64) {
    cheap_phase2_dimensions(walked)
        .unwrap_or_else(|| read_image_dimensions(&walked.abs_path, walked.extension.as_str()))
}

// ── Main fast scan entry point ────────────────────────────────────────────────
// ── 快速扫描主入口点 ────────────────────────────────────────────────

fn ensure_dir_chain(
    tx: &rusqlite::Transaction,
    root_id: i64,
    rel_path: &str,
    dir_cache: &mut std::collections::HashMap<String, i64>,
    root_name: &str,
) -> Result<i64> {
    if let Some(&id) = dir_cache.get(rel_path) {
        return Ok(id);
    }
    let parent_id = if rel_path.is_empty() {
        None
    } else {
        let p = Path::new(rel_path);
        let p_rel = p.parent().map(|p| normalize_db_path(&p.to_string_lossy())).unwrap_or_default();
        Some(ensure_dir_chain(tx, root_id, &p_rel, dir_cache, root_name)?)
    };

    let dir_name = if rel_path.is_empty() {
        root_name.to_string()
    } else {
        Path::new(rel_path).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string()
    };
    let depth = path_depth(rel_path);

    let id = upsert_directory(tx, root_id, parent_id, rel_path, &dir_name, depth, None)?;
    dir_cache.insert(rel_path.to_string(), id);
    Ok(id)
}

/// Run the fast scan for a single scan root.
/// 运行单个扫描根目录的快速扫描。
///
/// - Walks the file system (single thread, I/O bound)
/// - 遍历文件系统（单线程，I/O 密集型）
/// - Extracts dimensions in parallel (rayon)
/// - 并行提取尺寸（rayon）
/// - Inserts in batches of 500 rows (write connection)
/// - 分批插入 500 行（写连接）
/// - Sends progress updates every 500 items via `channel`
/// - 通过 `channel` 每 500 项发送进度更新
/// - Respects `cancel` token — returns `Err(AppError::Cancelled)` if triggered
/// - 遵循 `cancel` 令牌 — 如果触发则返回 `Err(AppError::Cancelled)`
pub fn run_fast_scan(
    writer: &Mutex<Connection>,
    root_id: i64,
    root_path: &str,
    channel: &Channel<ScanChannelPayload>,
    cancel: &CancellationToken,
) -> Result<u64> {
    let started = std::time::Instant::now();
    info!("Fast scan started: root_id={root_id} path={root_path} | 快速扫描开始: root_id={root_id} 路径={root_path}");

    let root = Path::new(root_path);

    // ── Step 1: Walk files ────────────────────────────────────────────────
    // ── 第 1 步：遍历文件 ────────────────────────────────────────────────
    let mut walked_files = walk_media_files(root, cancel, |count| {
        let _ = channel.send(ScanChannelPayload::Progress(ScanProgressPayload {
            root_id,
            scanned: count as u64,
            total: 0,
            current_dir: String::new(),
            status: "discovering".to_string(),
        }));
    })?;
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }
    let total = walked_files.len() as u64;
    info!("Walker found {} files | 扫描器发现 {} 个文件", total, total);

    // ── Step 2: Dimensions — eager for first screens, placeholder for the rest ──
    // ── 第 2 步：尺寸 — 首屏即时提取，其余占位 ──────────────────────────────
    // Sort by mtime DESC so the rows inserted first are exactly the ones the
    // default view shows first (newest first) → correct, reflow-free first paint.
    // 按 mtime 倒序：最先入库的行正是默认视图最先展示的项（最新在前）→ 首屏正确、无重排。
    walked_files.sort_by(|a, b| b.file_mtime.cmp(&a.file_mtime));

    // Only the first `eager` items pay the per-file header-read cost (in parallel).
    // Files beyond the first few screens keep cheap Phase-2 constants, while
    // Phase-1 images are deferred to a 0×0 placeholder (rendered square) and
    // backfilled by enrichment — this removes the "extract dimensions for every
    // file" stall that previously blocked huge imports for 10s+.
    // 仅前 `eager` 项并行支付逐文件读取头成本。首屏之外：阶段2保留廉价常量，
    // 阶段1图像延后为 0×0 占位（按正方形渲染），由 enrichment 补全 —— 由此消除
    // 之前"逐个文件提尺寸"导致海量导入空等 10s+ 的卡顿。
    let eager = walked_files.len().min(EAGER_DIM_COUNT);
    let eager_dims: Vec<(i64, i64)> = walked_files[..eager]
        .par_iter()
        .map(extract_dimensions)
        .collect();

    let file_infos: Vec<FileInfo> = walked_files
        .into_iter()
        .enumerate()
        .map(|(i, walked)| {
            let (width, height) = if i < eager {
                eager_dims[i]
            } else {
                cheap_phase2_dimensions(&walked).unwrap_or((0, 0))
            };
            FileInfo { walked, width, height }
        })
        .collect();

    // ── Step 3: Batch insert ──────────────────────────────────────────────
    // ── 第 3 步：批量插入 ──────────────────────────────────────────────
    // We need a directory cache to avoid repeated upserts for the same dir
    // 我们需要一个目录缓存来避免对同一目录的重复更新插入 (upsert)
    let mut dir_cache: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut inserted = 0u64;
    let mut batch_count = 0usize;

    for chunk in file_infos.chunks(BATCH_SIZE) {
        if cancel.is_cancelled() {
            warn!("Fast scan cancelled at root_id={root_id}");
            return Err(AppError::Cancelled);
        }

        let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;

        // Wrap the whole batch in a transaction
        // 将整个批处理包装在一个事务中
        let tx = conn.unchecked_transaction()?;

        let root_name = root.file_name().and_then(|n| n.to_str()).unwrap_or("");

        for fi in chunk {
            let rel_path = dir_rel_path(root, &fi.walked.abs_path);
            let rel_path_norm = normalize_db_path(&rel_path);

            // Get or create the directory record and its parents recursively
            // 递归获取或创建目录记录及其父目录
            let dir_id = ensure_dir_chain(&tx, root_id, &rel_path_norm, &mut dir_cache, root_name)?;

            let cache_key = compute_cache_key(
                &rel_path_norm,
                &fi.walked.file_name,
                fi.walked.file_mtime,
            );

            let fast_item = FastScanItem {
                directory_id:  dir_id,
                file_name:     fi.walked.file_name.clone(),
                file_size:     fi.walked.file_size,
                file_mtime:    fi.walked.file_mtime,
                file_format:   fi.walked.extension.clone(),
                media_type:    fi.walked.media_type.as_str().to_string(),
                width:         fi.width,
                height:        fi.height,
                sort_datetime: fi.walked.file_mtime, // will be refined in enrichment
                                                     // 将在丰富信息阶段细化
                cache_key,
            };

            let (_, is_new) = upsert_fast_scan_item(&tx, &fast_item)?;
            if is_new {
                inserted += 1;
            }
        }

        tx.commit()?;
        drop(conn);

        batch_count += chunk.len();
        debug!("Fast scan batch committed: {}/{}", batch_count, total);

        // Progress update
        // 进度更新
        if batch_count.is_multiple_of(PROGRESS_INTERVAL) || batch_count as u64 >= total {
            let _ = channel.send(ScanChannelPayload::Progress(ScanProgressPayload {
                root_id,
                scanned: batch_count as u64,
                total,
                current_dir: String::new(),
                status: "scanning".to_string(),
            }));

            // Update DB scan status
            // 更新数据库扫描状态
            if let Ok(conn) = writer.lock() {
                let _ = update_scan_root_status(&conn, root_id, "scanning", batch_count as i64, total as i64);
            }
        }
    }

    // ── Step 4: Finalise ──────────────────────────────────────────────────
    // ── 第 4 步：最终确定 ──────────────────────────────────────────────────
    {
        let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
        finish_scan_root(&conn, root_id, inserted as i64)?;
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    info!("Fast scan done: root_id={root_id} inserted={inserted} elapsed={elapsed_ms}ms | 快速扫描完成: root_id={root_id} 插入={inserted} 耗时={elapsed_ms}ms");

    let _ = channel.send(ScanChannelPayload::Completed(ScanCompletedPayload {
        root_id,
        total_items: inserted,
        elapsed_ms,
    }));

    Ok(inserted)
}
