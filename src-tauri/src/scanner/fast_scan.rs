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

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::db::queries::{
    finish_scan_root, invalidate_exotic_tasks_for_item, mark_missing, resolve_suspect_change,
    seed_exotic_tasks_for_item, set_directory_media_count, update_scan_root_status,
    upsert_directory, upsert_fast_scan_item, FastScanItem, UpsertOutcome,
};
use crate::error::{AppError, Result};
use crate::exotic::catalog::CatalogSnapshot;
use crate::scanner::metadata::read_image_dimensions;
use crate::scanner::volume_probe::{PathProber, VolumeOnlineCheck};
use crate::scanner::walker::{MediaWalker, WalkedFile};
use crate::utils::format::{is_phase1_image, MediaType};
use crate::utils::hash::compute_cache_key;
use crate::utils::path::{dir_rel_path, normalize_db_path, path_depth};

use serde::{Deserialize, Serialize};

const BATCH_SIZE: usize = 500;

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
    pub root_id: i64,
    pub scanned: u64,
    pub total: u64,
    pub current_dir: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCompletedPayload {
    pub root_id: i64,
    pub total_items: u64,
    pub elapsed_ms: u64,
    /// 本次缺失检测标记为 `availability='missing'` 的项数（四道闸通过才 >0）。前端可据此 toast 提示。
    pub marked_missing: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanErrorPayload {
    pub root_id: i64,
    pub error: String,
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
    walked: WalkedFile,
    width: i64,
    height: i64,
}

/// Cheap, no-file-read placeholder dimensions for Phase-2 media (audio/doc/video).
/// Returns `None` for Phase-1 images, which need a real header read.
/// 阶段 2 媒体（音频/文档/视频）的廉价、无需读文件的占位尺寸。
/// 阶段 1 图像返回 `None`（需要真实读取文件头）。
///
/// 按类型给「默认宽高」，避免非图片项以 0×0 进入 Justified Layout 导致布局错乱（§2.1）。
/// `query_layout_geometry` 对所有类型读取 width/height，0×0 会让布局把整屏算崩。
/// 真实宽高随后在补全/派生阶段回填（视频走 MF probe + rotation 修正、音频走 lofty、
/// 文档走子类型探测），这里只保证「先有一个合理的占位比例」。
fn cheap_phase2_dimensions(walked: &WalkedFile) -> Option<(i64, i64)> {
    if is_phase1_image(walked.extension.as_str()) {
        return None;
    }
    // 穷举所有变体（不用 `_`）：将来新增类型时编译器会强制处理默认尺寸。
    Some(match walked.media_type {
        // 视频默认 16:9，补全后回填真实值（含 rotation 交换，竖拍视频否则会躺倒）。
        MediaType::Video => (1280, 720),
        // 音频用方形封面位。
        MediaType::Audio => (400, 400),
        // 文档：PDF 用 A4 比例（595×842），其它（svg/txt/md/office…）用方形占位。
        MediaType::Document => {
            if walked.extension == "pdf" {
                (595, 842)
            } else {
                (400, 400)
            }
        }
        // 图像不会走到这里（上面已 return None）；保险给方形占位。
        MediaType::Image => (400, 400),
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
    root: &Path,
) -> Result<i64> {
    if let Some(&id) = dir_cache.get(rel_path) {
        return Ok(id);
    }
    let parent_id = if rel_path.is_empty() {
        None
    } else {
        let p = Path::new(rel_path);
        let p_rel = p
            .parent()
            .map(|p| normalize_db_path(&p.to_string_lossy()))
            .unwrap_or_default();
        Some(ensure_dir_chain(
            tx, root_id, &p_rel, dir_cache, root_name, root,
        )?)
    };

    let dir_name = if rel_path.is_empty() {
        root_name.to_string()
    } else {
        Path::new(rel_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    };
    let depth = path_depth(rel_path);

    // T17a 增量剪枝基线：读目录自身的 FS mtime 存入 directories.mtime。一目录仅 stat 一次
    // （dir_cache 记忆化），开销与目录数成正比（远小于文件数）。
    // ⚠️ 已知边界（见 §3.4/T17）：目录 mtime 仅反映**直接子项的增/删/改名**，不反映文件**就地编辑**
    // （同大小 EXIF/评分写回）与**孙级**变化——故仅供 opt-in「快速扫描」剪枝、默认全量扫描不据此跳。
    let abs = if rel_path.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel_path)
    };
    let dir_mtime = std::fs::metadata(&abs)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let id = upsert_directory(
        tx, root_id, parent_id, rel_path, &dir_name, depth, dir_mtime,
    )?;
    dir_cache.insert(rel_path.to_string(), id);
    Ok(id)
}

/// T17b opt-in「快速扫描」剪枝判定：某目录的 FS mtime 与基线（T17a 写入的 `directories.mtime`）
/// 一致 → 其**直接子项无增/删/改名** → 跳过该目录所有直接文件的 per-file 工作（metadata stat /
/// cache_key / upsert / exotic 播种）。可剪枝时把该目录**全部未删媒体 id 回填进 `seen`**，确保
/// 缺失检测差集不把它们误判为已删除（🔴 数据安全：跳过 ≠ 消失）。返回 `true` 表示该目录可剪枝。
///
/// **逐目录、非递归**：仅跳本目录直接文件，**不** `skip_current_dir`——子目录仍由 walkdir 独立下降、
/// 各自比对 mtime，故嵌套目录的新增/删除**不漏**（目录 mtime 不向上冒泡，单祖先 mtime 不能代表子树）。
///
/// **已知且唯一的漏检边界**：文件**就地编辑**（内容变、父目录 mtime 不变，如同大小 EXIF/评分写回）
/// 会被跳过——这是快速扫描的设计取舍，全量扫描兜底。**必须在该目录任何 `ensure_dir_chain` 改写
/// mtime 之前调用**（读的是上一次扫描的旧基线）。
fn decide_dir_pruned(
    tx: &rusqlite::Transaction,
    root_id: i64,
    rel_path_norm: &str,
    file_abs: &Path,
    seen: &mut HashSet<i64>,
) -> Result<bool> {
    // 读旧基线 mtime（务必在 upsert 改写之前）。无行 / mtime 为 NULL（新目录或历史无基线）→ 保守处理。
    let stored: Option<(i64, Option<i64>)> = tx
        .query_row(
            "SELECT id, mtime FROM directories WHERE root_id=?1 AND rel_path=?2",
            params![root_id, rel_path_norm],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((dir_id, Some(old_mtime))) = stored else {
        return Ok(false);
    };

    // 当前目录 FS mtime（取该文件的父目录 = 本目录）。读不到 → 保守处理。
    let cur = file_abs
        .parent()
        .and_then(|d| std::fs::metadata(d).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    if cur != Some(old_mtime) {
        return Ok(false); // mtime 变 → 直接子项结构变化 → 必须处理
    }

    // 未变 → 回填该目录全部未删媒体 id（含 companion：它们都在盘上、须计入 seen 防误删）。
    let mut stmt =
        tx.prepare("SELECT id FROM media_items WHERE directory_id=?1 AND is_deleted=0")?;
    let rows = stmt.query_map(params![dir_id], |r| r.get::<_, i64>(0))?;
    for id in rows {
        seen.insert(id?);
    }
    Ok(true)
}

/// 缺失检测收尾·四道闸判定（抽出以脱离 Channel、可单测闸门排序）。返回标记数（0=被某闸拦下/无缺失）。
///
/// 闸序（任一不过即返回 0、**绝不删除**）：
///   1. 完整门闩 `walk_complete`——不完整扫描（遍历错误）→ seen 不可信，不差集。
///   2. TOCTOU `volume_online`——写删前复查卷在线（防扫描中途拔盘误删）。
///   3. 三重守门 `mark_missing`——在线卷集（本根卷）∩本根子树∩¬seen（守门内置）。
///
/// 入口守门（第四道闸·离线即不进 fast_scan）在调用方 `run_fast_scan` 入口处。
fn finalize_missing_detection(
    conn: &Connection,
    root_id: i64,
    walk_complete: bool,
    walk_error_count: usize,
    volume_online: bool,
    volume_id: Option<i64>,
    seen: &HashSet<i64>,
) -> Result<usize> {
    if !walk_complete {
        warn!(
            "跳过缺失检测：扫描不完整（{walk_error_count} 处遍历错误）→ 不差集 | root_id={root_id}"
        );
        return Ok(0);
    }
    if !volume_online {
        // TOCTOU：扫描中途拔盘 → 写删前复查发现离线 → 放弃差集，防误删。
        warn!("跳过缺失检测：卷在写删前复查时已离线（疑中途拔盘）→ 不删除 | root_id={root_id}");
        return Ok(0);
    }
    // 守门1 在线卷集 = 本根的卷（在线）。volume_id=None（未识别）→ 空集 → 不标（宁可不删，§5 不变量 4）。
    let online_vols: Vec<i64> = volume_id.into_iter().collect();
    let marked = mark_missing(conn, root_id, &online_vols, seen, false)?;
    if marked > 0 {
        info!("缺失检测：标记 {marked} 项 availability=missing（在线卷差集，未碰 is_deleted）| root_id={root_id}");
    }
    Ok(marked)
}

/// Run the fast scan for a single scan root.
/// 运行单个扫描根目录的快速扫描。
///
/// 流式管道（T12，§3.7.1）——**一次只持有一批**，内存峰值 O(batch)（不再全量 `Vec` + 排序 clone）：
/// - 遍历文件系统（`MediaWalker` 单线程流式，I/O 密集型），攒满 `BATCH_SIZE` 即处理一批。
/// - 每批前 `EAGER_DIM_COUNT`（跨批累计预算）项并行提真实尺寸（rayon），其余廉价占位。
/// - 一批一事务写 `media_items`；视图序让渡布局层（`query_layout_geometry` 独立 ORDER BY）。
/// - 每批经 `channel` 发进度（流式不预扫总数 → indeterminate）。
/// - 收尾跑缺失检测四道闸（`finalize_missing_detection`）：完整门闩 + TOCTOU 复查 + 三重守门。
/// - 遵循 `cancel` 令牌——触发即返回 `Err(AppError::Cancelled)`（丢弃当前批、不进差集）。
///
/// `quick`（T17b，§3.4 opt-in）：对 FS mtime 未变的目录跳过其直接文件的 per-file 工作并回填 seen，
/// 提速增量重扫；唯一漏检边界是文件**就地编辑**，由全量扫描兜底。默认 false 即全量逐文件、行为不变。
// 扫描编排参数各自独立（S1 新增批提交回调后达 8 个）、无合理分组，沿用本仓库既有约定标注。
#[allow(clippy::too_many_arguments)]
pub fn run_fast_scan(
    writer: &Mutex<Connection>,
    root_id: i64,
    root_path: &str,
    catalog: &CatalogSnapshot,
    channel: &Channel<ScanChannelPayload>,
    cancel: &CancellationToken,
    // S1：每批事务提交后回调（生产接线 = AppState::bump_data_version，使 items 取数缓存
    // 逐批失效——扫描进行中前端按进度事件逐批重排，必须看到新入库的项）。
    on_batch_committed: &(dyn Fn() + Sync),
    // T17b：opt-in「快速扫描」。true 时对 FS mtime 未变的目录跳过其直接文件的 per-file 工作
    // （仍遍历整棵树、仍处理变更目录与所有子目录），代价是漏「就地编辑」（见 decide_dir_pruned）。
    // false（默认）→ 行为与 T17b 之前完全一致（全量逐文件）。
    quick: bool,
) -> Result<u64> {
    let started = std::time::Instant::now();
    info!("Fast scan started: root_id={root_id} path={root_path} | 快速扫描开始: root_id={root_id} 路径={root_path}");

    let root = Path::new(root_path);

    // ── 缺失检测·入口守门（§3.1.3，第一道闸）──────────────────────────────
    // 卷离线 / 根路径不可访问 → 跳过整次扫描、**绝不动 DB**（离线 ≠ 删除）。
    let prober = PathProber;
    if !prober.is_online(root) {
        warn!("scan_root 卷离线/不可访问，跳过扫描（不动 DB）: root_id={root_id} path={root_path}");
        let _ = channel.send(ScanChannelPayload::Completed(ScanCompletedPayload {
            root_id,
            total_items: 0,
            elapsed_ms: started.elapsed().as_millis() as u64,
            marked_missing: 0,
        }));
        return Ok(0);
    }

    // 本 scan_root 的卷 id（V10 回填）：缺失检测守门1 的「在线卷集」。
    // None（未识别卷/孤儿根）→ 守门为空集 → 不标缺失（宁可不删，§5 不变量 4）。
    let volume_id: Option<i64> = {
        let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
        conn.query_row(
            "SELECT volume_id FROM scan_roots WHERE id = ?1",
            params![root_id],
            |r| r.get::<_, Option<i64>>(0),
        )
        .ok()
        .flatten()
    };

    // ── Step 1-3：流式遍历 + 分块提尺寸 + 分批入库（T12，§3.7.1）─────────────
    // 旧实现把整棵树收进 `Vec<WalkedFile>` 再 `order_for_view` clone 一遍（峰值 ~600MB@100 万）。
    // 改为**流式**：`MediaWalker` 逐项产出，攒满 BATCH_SIZE 即提尺寸+入库，**一次只持有一批**
    // → 内存峰值降至 O(batch)。代价（设计取舍，已纳入 §5 风险表）：
    //   ① **放弃入库前全局排序**——视图序完全交给布局层 `query_layout_geometry`（独立 ORDER BY
    //      sort_datetime/file_name 重排，正确性不受影响）；导入瞬时画廊非时间序，enrichment 补完
    //      sort_datetime 后首次 relayout 即正确。
    //   ② **eager-dim 对齐降级**——首批（前 EAGER_DIM_COUNT 项，按**遍历序**）做真实头读取，
    //      不再保证恰是「最先展示」的项（date 分组下两者不一致）；folder 分组下遍历序≈视图序，
    //      仍大体对齐。残留首屏占位由 enrichment 秒级回填，仅导入态可见。
    //   ③ **进度转 indeterminate**——不预扫总数，故扫描期 `total` 未知（发 0）；完成事件携带准确计数。
    let mut walker = MediaWalker::new(root, catalog, cancel);

    // We need a directory cache to avoid repeated upserts for the same dir
    // 我们需要一个目录缓存来避免对同一目录的重复更新插入 (upsert)
    let mut dir_cache: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    // T17a：每目录「直接媒体计数」累积器（dir_id → 本次扫描在该目录直接命中的媒体文件数）。
    // 跨批累积，收尾一次性写回 directories.media_count 作增量剪枝基线（详见 set_directory_media_count）。
    let mut dir_media_counts: std::collections::HashMap<i64, i64> =
        std::collections::HashMap::new();
    // T17b 快速扫描：每目录剪枝判定缓存（rel_path → 是否可剪枝）。一目录仅判一次（首个文件触发，
    // 含 mtime 比对 + 回填 seen），后续同目录文件直接复用。quick=false 时此 map 恒空、零开销。
    let mut dir_decision: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();
    let mut inserted = 0u64;
    let mut batch_count = 0usize;
    // 缺失检测 seen 集（守门3）：本次扫描命中的全部 id（含 Unchanged）。流式下容量未知，按需增长。
    let mut seen: HashSet<i64> = HashSet::new();
    // eager-dim 预算（跨批累计）：仅最先入库的前 EAGER_DIM_COUNT 项做真实头读取，其余占位。
    let mut eager_remaining = EAGER_DIM_COUNT;
    // 复用的当前批缓冲（drain 后保留容量，避免每批重新分配）。
    let mut chunk: Vec<WalkedFile> = Vec::with_capacity(BATCH_SIZE);

    loop {
        // 拉取至多 BATCH_SIZE 项（流式：一次只持有一批 → 内存 O(batch)）。
        chunk.clear();
        for f in walker.by_ref() {
            chunk.push(f);
            if chunk.len() >= BATCH_SIZE {
                break;
            }
        }
        // 取消：丢弃本批、立即返回（与原行为一致——取消即 Err，不进 finalize 差集）。
        if cancel.is_cancelled() {
            warn!("Fast scan cancelled at root_id={root_id}");
            return Err(AppError::Cancelled);
        }
        if chunk.is_empty() {
            break; // 遍历耗尽
        }

        // 分块提尺寸：本批前 `eager` 项并行做真实头读取（含 JPEG orientation / TIFF 超时），
        // 其余廉价占位（Phase-2 常量 / Phase-1 图像 0×0，由 enrichment 回填）。
        let eager = chunk.len().min(eager_remaining);
        let eager_dims: Vec<(i64, i64)> =
            chunk[..eager].par_iter().map(extract_dimensions).collect();
        eager_remaining -= eager;

        // 移动消费本批 WalkedFile → FileInfo（**不 clone**；drain 留空复用 chunk 容量）。
        let file_infos: Vec<FileInfo> = chunk
            .drain(..)
            .enumerate()
            .map(|(i, walked)| {
                let (width, height) = if i < eager {
                    eager_dims[i]
                } else {
                    cheap_phase2_dimensions(&walked).unwrap_or((0, 0))
                };
                FileInfo {
                    walked,
                    width,
                    height,
                }
            })
            .collect();

        // 本批一事务入库。
        // 可疑变更(mtime 变 size 同,§3.3.2)攒批:指纹计算是文件 IO,绝不进写事务——
        // 本批提交、写锁释放后,在事务外算指纹、逐项短写定案。
        let mut suspects: Vec<(i64, FastScanItem, std::path::PathBuf)> = Vec::new();
        let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
        let tx = conn.unchecked_transaction()?;
        let root_name = root.file_name().and_then(|n| n.to_str()).unwrap_or("");

        for fi in &file_infos {
            let rel_path = dir_rel_path(root, &fi.walked.abs_path);
            let rel_path_norm = normalize_db_path(&rel_path);

            // T17b 快速扫描：若本文件所在目录被判可剪枝（mtime 未变），跳过其全部 per-file 工作。
            // 该目录全部未删媒体 id 已在判定时回填 seen（防误删），故此处仅 `continue` 即可。
            if quick {
                let pruned = match dir_decision.get(&rel_path_norm).copied() {
                    Some(p) => p,
                    None => {
                        // 首个文件触发本目录判定：务必在 ensure_dir_chain 改写 mtime 之前读旧基线。
                        let p = decide_dir_pruned(
                            &tx,
                            root_id,
                            &rel_path_norm,
                            &fi.walked.abs_path,
                            &mut seen,
                        )?;
                        dir_decision.insert(rel_path_norm.clone(), p);
                        p
                    }
                };
                if pruned {
                    continue;
                }
            }

            // Get or create the directory record and its parents recursively
            // 递归获取或创建目录记录及其父目录
            let dir_id = ensure_dir_chain(
                &tx,
                root_id,
                &rel_path_norm,
                &mut dir_cache,
                root_name,
                root,
            )?;
            // T17a：每发现一个该目录的直接媒体文件 +1（口径＝walker 已分类媒体，与 T17b 剪枝时
            // 现算 read_dir 分类计数一致）。Inserted/Unchanged/SourceChanged 一律计入——它们都是
            // 「本次存在」的直接子项。
            *dir_media_counts.entry(dir_id).or_insert(0) += 1;

            let cache_key =
                compute_cache_key(&rel_path_norm, &fi.walked.file_name, fi.walked.file_mtime);

            let fast_item = FastScanItem {
                directory_id: dir_id,
                file_name: fi.walked.file_name.clone(),
                file_size: fi.walked.file_size,
                file_mtime: fi.walked.file_mtime,
                file_format: fi.walked.extension.clone(),
                media_type: fi.walked.media_type.as_str().to_string(),
                width: fi.width,
                height: fi.height,
                sort_datetime: fi.walked.file_mtime, // will be refined in enrichment
                // 将在丰富信息阶段细化
                cache_key,
            };

            // 传本根卷 id：新项据此入库、历史 NULL 项顺带治愈 → 新数据可参与缺失检测守门1。
            let outcome = upsert_fast_scan_item(&tx, &fast_item, volume_id)?;
            // 🔴 必须在下方 exotic `continue` 之前收 seen：Unchanged 也要进 seen，
            // 否则未变更文件会被 mark_missing 误判为「本次未出现」而误删（Part2 §3.4/T5）。
            seen.insert(outcome.id());
            if matches!(outcome, UpsertOutcome::Inserted(_)) {
                inserted += 1;
            }
            if let UpsertOutcome::SuspectChanged(id) = outcome {
                // 可疑变更:留待本批事务外定案(exotic 动作也延后到定案分支)。
                suspects.push((id, fast_item.clone(), fi.walked.abs_path.clone()));
                continue;
            }

            // ── exotic 任务播种/失效（R13：扫描事务内完成，不等 enrichment）──────────
            // 只依赖扩展名查 Catalog；命中即为 exotic 格式（如 psd）。
            if let Some(off) = catalog.resolve_format(&fast_item.file_format) {
                let item_id = outcome.id();
                match outcome {
                    UpsertOutcome::SourceChanged(_) => {
                        // 源文件变化：先把旧任务退回 pending、清旧产物/指纹/租约。
                        invalidate_exotic_tasks_for_item(&tx, item_id)?;
                    }
                    UpsertOutcome::Inserted(_) => {}
                    // Unchanged：任务已存在且源未变，无需动作。
                    UpsertOutcome::Unchanged(_) => continue,
                    // SuspectChanged 已在上方收集并 continue,不会到达此处。
                    UpsertOutcome::SuspectChanged(_) => unreachable!("suspect 已提前 continue"),
                }
                // 按 capabilities 播种（INSERT OR IGNORE，幂等）。SourceChanged 后补齐可能的新能力。
                let caps: Vec<String> = off
                    .capabilities
                    .iter()
                    .map(|c| c.as_str().to_string())
                    .collect();
                seed_exotic_tasks_for_item(&tx, item_id, &off.plugin_id, &caps)?;
            }
        }

        tx.commit()?;
        drop(conn);
        on_batch_committed();

        // ── 可疑变更定案(Part2 §3.3.2 三环)──────────────────────────────────────
        // 批事务已提交、写锁已释放:此刻才做指纹 IO(读文件),再逐项短写定案——
        // touch(滤 mtime 抖动,派生全保留)或 SourceChanged(同大小元数据编辑,全失效)。
        for (sid, s_item, abs_path) in suspects {
            let fp = match crate::utils::hash::content_fingerprint(&abs_path, s_item.file_size) {
                Ok(h) => Some(h),
                Err(e) => {
                    // 读失败(竞态删除/权限):无法证明内容未变 → 交 resolve 保守失效。
                    warn!(
                        "可疑变更指纹计算失败,保守判 SourceChanged | suspect fingerprint failed {:?}: {e}",
                        abs_path
                    );
                    None
                }
            };
            let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;
            let outcome = resolve_suspect_change(&conn, sid, &s_item, volume_id, fp.as_deref())?;
            if matches!(outcome, UpsertOutcome::SourceChanged(_)) {
                // 与主循环 SourceChanged 分支同款 exotic 处理(失效旧任务 + 补种能力)。
                if let Some(off) = catalog.resolve_format(&s_item.file_format) {
                    invalidate_exotic_tasks_for_item(&conn, sid)?;
                    let caps: Vec<String> = off
                        .capabilities
                        .iter()
                        .map(|c| c.as_str().to_string())
                        .collect();
                    seed_exotic_tasks_for_item(&conn, sid, &off.plugin_id, &caps)?;
                }
            }
            drop(conn);
        }

        batch_count += file_infos.len();
        debug!("Fast scan batch committed: {} files so far", batch_count);

        // 进度：流式不预扫总数 → `total=0`（indeterminate），完成事件再携带准确计数。
        let _ = channel.send(ScanChannelPayload::Progress(ScanProgressPayload {
            root_id,
            scanned: batch_count as u64,
            total: 0,
            current_dir: String::new(),
            status: "scanning".to_string(),
        }));
        if let Ok(conn) = writer.lock() {
            let _ = update_scan_root_status(
                &conn,
                root_id,
                "scanning",
                batch_count as i64,
                batch_count as i64,
            );
        }
    }

    // 流式遍历收尾门闩（§3.2.2，第二道闸）：唯有 complete==true（零遍历/metadata 错误、未取消）
    // 才允许后续 mark_missing 差集——seen 不完整即绝不删除（不变量「不完整扫描 ≠ 删除」）。
    let outcome = walker.finish();
    let walk_complete = outcome.complete;
    let walk_error_count = outcome.errors.len();
    info!(
        "Walker streamed {} media file(s) | 扫描器流式发现 {} 个媒体文件",
        batch_count, batch_count
    );

    // ── Step 4: Finalise + 缺失检测 ───────────────────────────────────────
    // ── 第 4 步：收尾 + 缺失检测（四道闸）────────────────────────────────────
    let marked_missing: u64 = {
        let conn = writer.lock().map_err(|e| AppError::System(e.to_string()))?;

        // 缺失检测·四道闸（§3.2）。TOCTOU 复查（第三道闸）在写删前再查一次卷在线（防中途拔盘）。
        // 判定逻辑抽到 `finalize_missing_detection`（不依赖 Channel，可单测闸门排序）。
        let marked = finalize_missing_detection(
            &conn,
            root_id,
            walk_complete,
            walk_error_count,
            prober.is_online(root), // TOCTOU：写删前再查一次
            volume_id,
            &seen,
        )?;

        // T17a：写回每目录「直接媒体计数」基线（与 ensure_dir_chain 已写的 mtime 一起构成剪枝判据）。
        // 即便本次扫描不完整（walk_complete=false），写回也无害：下次剪枝若计数/ mtime 不符即正常重走，
        // 只是少一次提速、绝不漏扫（T17b 剪枝另以 walk_complete 与逐目录复核守门）。
        for (dir_id, count) in &dir_media_counts {
            set_directory_media_count(&conn, *dir_id, *count)?;
        }

        finish_scan_root(&conn, root_id, inserted as i64)?;
        marked as u64
    };

    let elapsed_ms = started.elapsed().as_millis() as u64;
    info!("Fast scan done: root_id={root_id} inserted={inserted} elapsed={elapsed_ms}ms | 快速扫描完成: root_id={root_id} 插入={inserted} 耗时={elapsed_ms}ms");

    let _ = channel.send(ScanChannelPayload::Completed(ScanCompletedPayload {
        root_id,
        total_items: inserted,
        elapsed_ms,
        marked_missing,
    }));

    Ok(inserted)
}

#[cfg(test)]
mod finalize_tests {
    use super::*;

    /// 内存库 + 一个 scan_root(id=1)/目录(id=10)/卷=5，一项在线媒体(id=100，未在 seen)。
    fn db_with_one_missing_candidate() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c.execute_batch(
            "INSERT INTO scan_roots (id, path, alias) VALUES (1, '/r1', 'R1');
             INSERT INTO directories (id, root_id, rel_path, name) VALUES (10, 1, '', 'r1');
             INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key, volume_id, availability)
             VALUES (100, 10, 'a.jpg', 0,0,'jpg','image',0,0,0,0, 5, 'online');",
        )
        .unwrap();
        c
    }

    fn avail(c: &Connection, id: i64) -> String {
        c.query_row(
            "SELECT availability FROM media_items WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// 四道闸全过：完整 + 在线 + 卷5 + 项不在 seen → 标 missing。
    #[test]
    fn all_gates_pass_marks_missing() {
        let c = db_with_one_missing_candidate();
        let seen = HashSet::new(); // 100 未出现
        let n = finalize_missing_detection(&c, 1, true, 0, true, Some(5), &seen).unwrap();
        assert_eq!(n, 1);
        assert_eq!(avail(&c, 100), "missing");
    }

    /// 完整门闩拦截：扫描不完整（有遍历错误）→ 即便项真缺失也**不标**（最关键红线）。
    #[test]
    fn incomplete_walk_blocks_deletion() {
        let c = db_with_one_missing_candidate();
        let seen = HashSet::new();
        let n = finalize_missing_detection(&c, 1, false, 3, true, Some(5), &seen).unwrap();
        assert_eq!(n, 0, "不完整扫描绝不删除");
        assert_eq!(avail(&c, 100), "online");
    }

    /// TOCTOU 拦截：写删前复查卷已离线 → 不标（防中途拔盘误删）。
    #[test]
    fn offline_at_recheck_blocks_deletion() {
        let c = db_with_one_missing_candidate();
        let seen = HashSet::new();
        let n = finalize_missing_detection(&c, 1, true, 0, false, Some(5), &seen).unwrap();
        assert_eq!(n, 0, "卷离线绝不删除");
        assert_eq!(avail(&c, 100), "online");
    }

    /// 卷未识别（volume_id=None）：在线集为空 → 不标（宁可不删）。
    #[test]
    fn unidentified_volume_marks_nothing() {
        let c = db_with_one_missing_candidate();
        let seen = HashSet::new();
        let n = finalize_missing_detection(&c, 1, true, 0, true, None, &seen).unwrap();
        assert_eq!(n, 0, "未识别卷 → 空在线集 → 不标");
        assert_eq!(avail(&c, 100), "online");
    }

    /// 项在 seen（本次出现）→ 不标（守门3）。
    #[test]
    fn seen_item_not_marked() {
        let c = db_with_one_missing_candidate();
        let seen = HashSet::from([100i64]);
        let n = finalize_missing_detection(&c, 1, true, 0, true, Some(5), &seen).unwrap();
        assert_eq!(n, 0);
        assert_eq!(avail(&c, 100), "online");
    }
}

#[cfg(test)]
mod dir_baseline_tests {
    //! T17a 目录剪枝基线：扫描期写入 directories.mtime + 直接 media_count。
    use super::*;

    /// ensure_dir_chain 应把目录的文件系统 mtime 写入 directories.mtime（含递归创建的祖先目录）。
    #[test]
    fn ensure_dir_chain_persists_dir_mtime() {
        let tmp = std::env::temp_dir().join(format!("scrollery_t17a_mtime_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("sub")).unwrap();

        let mut c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c.execute(
            "INSERT INTO scan_roots (id, path, alias) VALUES (1, ?1, 'R')",
            params![tmp.to_string_lossy()],
        )
        .unwrap();

        let tx = c.transaction().unwrap();
        let mut cache = std::collections::HashMap::new();
        let id = ensure_dir_chain(&tx, 1, "sub", &mut cache, "R", &tmp).unwrap();
        let stored: Option<i64> = tx
            .query_row(
                "SELECT mtime FROM directories WHERE id=?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        // 根目录（rel_path=""）由递归创建，同样应有 mtime 基线。
        let root_mtime: Option<i64> = tx
            .query_row(
                "SELECT mtime FROM directories WHERE root_id=1 AND rel_path=''",
                [],
                |r| r.get(0),
            )
            .unwrap();
        tx.commit().unwrap();

        let actual = std::fs::metadata(tmp.join("sub"))
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(stored, Some(actual), "sub 目录应写入其 FS mtime 基线");
        assert!(root_mtime.is_some(), "递归创建的根目录也应有 mtime 基线");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// set_directory_media_count 是**绝对覆盖**（非累加）——剪枝基线须反映本次真实直接计数。
    #[test]
    fn set_directory_media_count_overwrites() {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c.execute_batch(
            "INSERT INTO scan_roots (id, path, alias) VALUES (1, '/r', 'R');
             INSERT INTO directories (id, root_id, rel_path, name, media_count)
                VALUES (10, 1, '', 'r', 99);",
        )
        .unwrap();
        set_directory_media_count(&c, 10, 7).unwrap();
        let n: i64 = c
            .query_row("SELECT media_count FROM directories WHERE id=10", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 7, "应绝对覆盖为 7，而非在 99 上累加");
    }
}

#[cfg(test)]
mod quick_scan_tests {
    //! T17b opt-in 快速扫描剪枝判定（decide_dir_pruned）：mtime 比对 + 回填 seen 防误删。
    use super::*;

    fn fs_mtime(p: &Path) -> i64 {
        std::fs::metadata(p)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    /// 建临时目录（取其真实 FS mtime）+ 内存库（scan_root=1，根目录 id=10），返回 (Connection, tmp)。
    /// `baseline_mtime` = 写入 directories.mtime 的基线值（用 None 表示不写、即 NULL）。
    fn setup(tag: &str, baseline_mtime: Option<i64>) -> (Connection, std::path::PathBuf) {
        let tmp = std::env::temp_dir().join(format!("scrollery_t17b_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let c = Connection::open_in_memory().unwrap();
        crate::db::migration::run_migrations(&c).unwrap();
        c.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        c.execute(
            "INSERT INTO scan_roots (id, path, alias) VALUES (1, ?1, 'R')",
            params![tmp.to_string_lossy()],
        )
        .unwrap();
        c.execute(
            "INSERT INTO directories (id, root_id, rel_path, name, mtime) VALUES (10, 1, '', 'r', ?1)",
            params![baseline_mtime],
        )
        .unwrap();
        (c, tmp)
    }

    /// mtime 未变 → 可剪枝，且该目录全部**未删**媒体 id 回填 seen；**已删**项不得回填。
    #[test]
    fn unchanged_dir_is_pruned_and_reseeds() {
        let (c, tmp) = setup("prune", None);
        // 基线对齐到 setup 建目录后的真实 FS mtime（模拟「上次扫描已记录、之后未变」）。
        let actual = fs_mtime(&tmp);
        c.execute(
            "UPDATE directories SET mtime=?1 WHERE id=10",
            params![actual],
        )
        .unwrap();
        c.execute_batch(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key, is_deleted)
             VALUES (100,10,'a.jpg',0,0,'jpg','image',0,0,0,0,0),
                    (101,10,'a.mov',0,0,'mov','video',0,0,0,0,0),
                    (102,10,'gone.jpg',0,0,'jpg','image',0,0,0,0,1);",
        )
        .unwrap();

        let tx = c.unchecked_transaction().unwrap();
        let mut seen = HashSet::new();
        let pruned = decide_dir_pruned(&tx, 1, "", &tmp.join("a.jpg"), &mut seen).unwrap();
        drop(tx);

        assert!(pruned, "mtime 未变应判可剪枝");
        assert!(
            seen.contains(&100) && seen.contains(&101),
            "目录全部未删媒体 id（含 companion mov）应回填 seen 防误删"
        );
        assert!(!seen.contains(&102), "已删项不得回填 seen（不复活）");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// mtime 变（基线比真实早）→ 不剪枝（直接子项可能增/删/改名，必须处理），seen 不动。
    #[test]
    fn changed_mtime_not_pruned() {
        let (c, tmp) = setup("changed", Some(0)); // 基线 mtime=0，必与真实不符
        let tx = c.unchecked_transaction().unwrap();
        let mut seen = HashSet::new();
        let pruned = decide_dir_pruned(&tx, 1, "", &tmp.join("a.jpg"), &mut seen).unwrap();
        drop(tx);
        assert!(!pruned, "mtime 不符必须处理");
        assert!(seen.is_empty(), "未剪枝不应回填 seen");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 基线 mtime 为 NULL（新目录 / 历史无基线）→ 保守不剪枝。
    #[test]
    fn null_baseline_not_pruned() {
        let (c, tmp) = setup("null", None);
        let tx = c.unchecked_transaction().unwrap();
        let mut seen = HashSet::new();
        let pruned = decide_dir_pruned(&tx, 1, "", &tmp.join("a.jpg"), &mut seen).unwrap();
        drop(tx);
        assert!(!pruned, "无基线应保守处理");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 目录在 DB 无行（全新目录）→ 不剪枝。
    #[test]
    fn missing_dir_row_not_pruned() {
        let (c, tmp) = setup("missing", Some(0));
        let tx = c.unchecked_transaction().unwrap();
        let mut seen = HashSet::new();
        // rel_path="sub" 在 directories 中不存在 → optional None。
        let pruned = decide_dir_pruned(&tx, 1, "sub", &tmp.join("sub/a.jpg"), &mut seen).unwrap();
        drop(tx);
        assert!(!pruned, "DB 无该目录行应不剪枝");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
