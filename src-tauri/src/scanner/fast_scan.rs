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
use crate::scanner::metadata::{orientation_needs_swap, read_jpeg_orientation};
use crate::scanner::walker::{walk_media_files, WalkedFile};
use crate::utils::format::{is_phase1_image, MediaType};
use crate::utils::hash::compute_cache_key;
use crate::utils::path::{dir_rel_path, normalize_db_path, path_depth};

use serde::{Deserialize, Serialize};

const BATCH_SIZE: usize = 500;
const PROGRESS_INTERVAL: usize = 500;

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

fn extract_dimensions(walked: &WalkedFile) -> (i64, i64) {
    let ext = walked.extension.as_str();

    if !is_phase1_image(ext) {
        // Phase 2 media — use format-specific defaults
        // 阶段 2 媒体 — 使用特定于格式的默认值
        return match walked.media_type {
            MediaType::Audio    => (400, 400),
            MediaType::Document => (595, 842),
            _                   => (0, 0),
        };
    }

    // TIFF: apply timeout protection (parse can read many bytes)
    // TIFF: 应用超时保护（解析可能读取大量字节）
    if ext == "tif" || ext == "tiff" {
        let path = walked.abs_path.clone();
        let result = std::thread::scope(|s| {
            s.spawn(|| image::image_dimensions(&path).ok()).join().ok().flatten()
        });
        return result
            .map(|(w, h)| (w as i64, h as i64))
            .unwrap_or((0, 0));
    }

    // JPEG: also read orientation
    // JPEG: 同时也读取方向
    if ext == "jpg" || ext == "jpeg" {
        if let Ok((w, h)) = image::image_dimensions(&walked.abs_path) {
            let orientation = read_jpeg_orientation(&walked.abs_path);
            return if orientation_needs_swap(orientation) {
                (h as i64, w as i64)
            } else {
                (w as i64, h as i64)
            };
        }
        return (0, 0);
    }

    // All other Phase 1 formats
    // 所有其他阶段 1 格式
    image::image_dimensions(&walked.abs_path)
        .map(|(w, h)| (w as i64, h as i64))
        .unwrap_or((0, 0))
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
    let walked_files = walk_media_files(root, cancel, |count| {
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

    // ── Step 2: Parallel dimension extraction ─────────────────────────────
    // ── 第 2 步：并行提取尺寸 ─────────────────────────────
    let file_infos: Vec<FileInfo> = walked_files
        .into_par_iter()
        .map(|walked| {
            let (width, height) = extract_dimensions(&walked);
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

        let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;

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
        let conn = writer.lock().map_err(|e| AppError::Db(e.to_string()))?;
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
