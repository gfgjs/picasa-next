// src-tauri/src/thumbnail/cache.rs
//! Size-bucketed thumbnail cache management.
//! 尺寸分桶的缩略图缓存管理。
//!
//! Cache layout (§ 8.2):
//! 缓存布局（§ 8.2）：
//! `{app_data_dir}/cache/thumbnails/{size}/{2-char-prefix}/{cache_key_hex}.webp`
//! e.g. `cache/thumbnails/300/a3/a3f4b2c1d0e9f7a1.webp`
//! 例如 `cache/thumbnails/300/a3/a3f4b2c1d0e9f7a1.webp`

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::utils::hash::cache_key_to_hex;

/// Build the full path for a thumbnail file.
/// 构建缩略图文件的完整路径。
pub fn thumb_path(cache_dir: &Path, size: u32, cache_key: i64) -> PathBuf {
    debug_assert!(
        [120, 240, 480, 960].contains(&size),
        "Thumbnail size {} is not a valid tier | 缩略图尺寸 {} 不是有效档位",
        size,
        size
    );
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    cache_dir
        .join("thumbnails")
        .join(size.to_string())
        .join(prefix)
        .join(format!("{hex}.webp"))
}

/// Check whether a thumbnail already exists on disk.
/// 检查磁盘上是否已经存在缩略图。
pub fn thumb_exists(cache_dir: &Path, size: u32, cache_key: i64) -> bool {
    thumb_path(cache_dir, size, cache_key).exists()
}

/// The relative path stored in the DB: `"{size}/{prefix}/{hex}.webp"`.
/// 存储在数据库中的相对路径：`"{size}/{prefix}/{hex}.webp"`。
pub fn thumb_db_path(size: u32, cache_key: i64) -> String {
    debug_assert!(
        [120, 240, 480, 960].contains(&size),
        "Thumbnail size {} is not a valid tier | 缩略图尺寸 {} 不是有效档位",
        size,
        size
    );
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    format!("{size}/{prefix}/{hex}.webp")
}

/// Short edge (px) of the AI-analysis cache. Covers every built-in CLIP model since analysis
/// only ever downscales the short edge to `image_size` (B/16·L/14=224, L/14@336=336), never up.
/// Kept here so both the derivation backend (`derive/image.rs`) and the thumbnail pipeline's
/// one-decode-two-outputs path (`generator.rs`) agree on the size.
/// AI 分析缓存短边（像素）。覆盖所有内置 CLIP 模型（分析只下采样短边到 image_size、绝不上采样）。
/// 放此处使派生后端与缩略图「一次解码两份产物」路径对尺寸保持一致。
/// 注:336 = 当前内置模型集的最大输入边(L/14@336),非随意魔数;但**绑定当前模型集**——
/// 若将来接入需 >336 输入的模型,此值须同步上调,否则该模型的 AI 缓存会偏小(改它使全库 ai_cache 作废)。
pub const AI_CACHE_SHORT_EDGE: u32 = 336;

/// Absolute path of the AI-analysis cache for an image: `cache/ai_thumbs/{prefix}/{hex}.webp`.
/// A short-edge≥336 WebP that CLIP analysis decodes instead of the full-resolution original
/// (keyed by `cache_key`, same prefix scheme as thumbnails). Lives in its own dir so the
/// thumbnail LRU (`enforce_cache_limit`, which only walks `thumbnails/`) never evicts it.
/// 图像 AI 分析缓存的绝对路径：`cache/ai_thumbs/{prefix}/{hex}.webp`。一份短边≥336 的 WebP，
/// 供 CLIP 分析解码以替代全分辨率原图（按 `cache_key` 命名，与缩略图同前缀方案）。独立目录使
/// 缩略图 LRU（`enforce_cache_limit` 只遍历 `thumbnails/`）不会误删它。
pub fn ai_cache_path(cache_dir: &Path, cache_key: i64) -> PathBuf {
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    cache_dir
        .join("ai_thumbs")
        .join(prefix)
        .join(format!("{hex}.webp"))
}

/// Relative DB path of the AI cache (relative to `cache_dir`): `"ai_thumbs/{prefix}/{hex}.webp"`.
/// Stored in `media_derivations.payload_path`; the AI pipeline resolves it under `cache_dir`.
/// AI 缓存的相对 DB 路径（相对 `cache_dir`）：`"ai_thumbs/{prefix}/{hex}.webp"`。
/// 存入 `media_derivations.payload_path`；AI 流水线在 `cache_dir` 下解析。
pub fn ai_cache_db_path(cache_key: i64) -> String {
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    format!("ai_thumbs/{prefix}/{hex}.webp")
}

/// Ensure the directory for an AI cache file exists.
/// 确保 AI 缓存文件所在目录存在。
pub fn ensure_ai_cache_dir(cache_dir: &Path, cache_key: i64) -> std::io::Result<()> {
    let p = ai_cache_path(cache_dir, cache_key);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// 人脸分析缓存短边(像素)= YuNet detect_size(640)。T16-R2 方案 A:缩略图档位短边不足时,
/// host 预解码(WIC 优先)一份短边 640 的 WebP,worker 只解这份小图——镜像 ai_cache 之于 CLIP。
/// 注:绑定当前 face 模型集(yunet-sface);派发侧有防呆(detect_size 超此值即不吃缓存,
/// 见 face_pipeline::face_cache_applies),但升值会使全库 face_thumbs 作废,须一并清理。
pub const FACE_CACHE_SHORT_EDGE: u32 = 640;

/// 人脸分析缓存的绝对路径:`cache/face_thumbs/{prefix}/{hex}.webp`(键与前缀方案同缩略图)。
/// 与 ai_thumbs(336,CLIP)分目录:两者输入尺寸不同不能互用,独立目录也便于按类清理。
pub fn face_cache_path(cache_dir: &Path, cache_key: i64) -> PathBuf {
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    cache_dir
        .join("face_thumbs")
        .join(prefix)
        .join(format!("{hex}.webp"))
}

/// Build the absolute path of the motion video cache directory.
/// 构建动态视频缓存目录的绝对路径。
pub fn motion_video_cache_path(cache_dir: &Path, cache_key: i64) -> PathBuf {
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    cache_dir
        .join("motion_videos")
        .join(prefix)
        .join(format!("{hex}.mp4"))
}

/// Absolute path of a video keyframe sprite (§3.3): `cache/sprites/{prefix}/{hex}.webp`.
/// One horizontal strip per video, keyed by `cache_key` (same scheme as thumbnails).
/// 视频关键帧雪碧图的绝对路径（§3.3）：`cache/sprites/{prefix}/{hex}.webp`。
/// 每个视频一张水平条带，按 `cache_key` 命名（与缩略图同方案）。
pub fn keyframe_sprite_path(cache_dir: &Path, cache_key: i64) -> PathBuf {
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    cache_dir
        .join("sprites")
        .join(prefix)
        .join(format!("{hex}.webp"))
}

/// Relative DB path of a keyframe sprite (relative to `cache_dir`): `"sprites/{prefix}/{hex}.webp"`.
/// Stored in `media_derivations.payload_path`; the frontend resolves it under `cache_dir`.
/// 关键帧雪碧图的相对 DB 路径（相对 `cache_dir`）：`"sprites/{prefix}/{hex}.webp"`。
/// 存入 `media_derivations.payload_path`；前端在 `cache_dir` 下解析。
pub fn keyframe_sprite_db_path(cache_key: i64) -> String {
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    format!("sprites/{prefix}/{hex}.webp")
}

/// Ensure the directory for a given thumb path exists.
/// 确保给定缩略图路径的目录存在。
pub fn ensure_thumb_dir(cache_dir: &Path, size: u32, cache_key: i64) -> std::io::Result<()> {
    let p = thumb_path(cache_dir, size, cache_key);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Ensure the directory for a keyframe sprite exists.
/// 确保关键帧雪碧图所在目录存在。
pub fn ensure_sprite_dir(cache_dir: &Path, cache_key: i64) -> std::io::Result<()> {
    let p = keyframe_sprite_path(cache_dir, cache_key);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Enforce the thumbnail cache limit by LRU.
/// 强制执行缩略图缓存大小限制 (LRU)。
pub fn enforce_cache_limit(cache_dir: &std::path::Path, max_size_mb: u64) {
    let max_size_bytes = max_size_mb.saturating_mul(1024 * 1024);
    let target_size_bytes = (max_size_bytes as f64 * 0.8) as u64;

    let mut total_size = 0;
    let mut files: Vec<(std::path::PathBuf, std::time::SystemTime, u64)> = Vec::new();

    // Both the display thumbnails AND the AI-analysis caches share one cache budget and one LRU
    // eviction pass. (Sprites / motion videos are deliberately excluded — they're tied to their
    // source media's lifetime, not browse-recency.)
    // 显示缩略图与 AI 分析缓存（ai_thumbs + face_thumbs）共用同一缓存预算和同一次 LRU 淘汰。
    // （雪碧图/动态视频有意排除 —— 它们绑定源媒体生命周期，而非浏览近期性。）
    let scan_dirs = [
        cache_dir.join("thumbnails"),
        cache_dir.join("ai_thumbs"),
        cache_dir.join("face_thumbs"),
    ];
    if scan_dirs.iter().all(|d| !d.exists()) {
        return;
    }

    // Use walkdir to iterate all files | 使用 walkdir 遍历所有文件
    for dir in scan_dirs.iter().filter(|d| d.exists()) {
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Ok(metadata) = entry.metadata() {
                    let size = metadata.len();
                    total_size += size;
                    if let Ok(modified) = metadata.modified() {
                        files.push((entry.path().to_path_buf(), modified, size));
                    }
                }
            }
        }
    }

    if total_size <= max_size_bytes {
        tracing::info!(
            "Cache size {} MB is within limit {} MB | 缓存大小 {} MB 在限制 {} MB 内",
            total_size / 1024 / 1024,
            max_size_mb,
            total_size / 1024 / 1024,
            max_size_mb
        );
        return;
    }

    tracing::info!(
        "Cache size {} MB exceeds limit {} MB, starting LRU cleanup... | 缓存大小 {} MB 超过限制 {} MB，开始 LRU 清理...",
        total_size / 1024 / 1024, max_size_mb, total_size / 1024 / 1024, max_size_mb
    );

    // Sort ascending by modified time (oldest first) | 按修改时间升序排序（最旧的在前）
    files.sort_by_key(|&(_, modified, _)| modified);

    let mut freed = 0;
    let mut deleted_count = 0;

    for (path, _, size) in files {
        if total_size.saturating_sub(freed) <= target_size_bytes {
            break;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!(
                "Failed to delete cache file {:?} | 无法删除缓存文件 {:?}: {}",
                path,
                path,
                e
            );
        } else {
            freed += size;
            deleted_count += 1;
        }
    }

    tracing::info!(
        "Cache cleanup finished, deleted {} files, freed {} MB | 缓存清理完成，删除 {} 个文件，释放了 {} MB",
        deleted_count, freed / 1024 / 1024, deleted_count, freed / 1024 / 1024
    );
}

// ════════════════════════════════════════════════════════════════════════════
// 缓存治理：占用统计 / 手动清理 / 孤儿即时清理（Part3 §3.3 / Q6-Q8）
// ════════════════════════════════════════════════════════════════════════════

/// 四个缓存子目录的占用统计（字节）+ 上限，供设置面板展示与「清理缓存」（§3.3.3 / Q8）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    /// 显示缩略图（`thumbnails/`，受 LRU 上限约束）。
    pub thumbnails: u64,
    /// AI 分析缓存(`ai_thumbs/` + `face_thumbs/` 合并计;与缩略图共用 LRU 预算)。
    /// face_thumbs(T16-R2)并入本类目:同属 AI 分析缓存,免动 CacheStats 的 IPC/前端面。
    pub ai_thumbs: u64,
    /// 视频关键帧雪碧图（`sprites/`，绑定源媒体生命周期、不受 LRU 淘汰）。
    pub sprites: u64,
    /// 动态视频缓存（`motion_videos/`，同上）。
    pub motion_videos: u64,
    /// 四者总占用。
    pub total: u64,
    /// LRU 上限（MB，仅约束 thumbnails+ai_thumbs；供前端展示「占用/上限」）。
    pub limit_mb: u64,
}

/// 递归累加目录下所有文件字节数（目录不存在记 0）。只读。
fn dir_size_bytes(dir: &Path) -> u64 {
    if !dir.exists() {
        return 0;
    }
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// 统计四个缓存子目录占用 + 总量 + 上限（§3.3.3）。纯只读，不改磁盘。
/// 调用方应在 `spawn_blocking` 内执行（遍历目录是阻塞 IO）。
pub fn compute_cache_stats(cache_dir: &Path, limit_mb: u64) -> CacheStats {
    let thumbnails = dir_size_bytes(&cache_dir.join("thumbnails"));
    let ai_thumbs = dir_size_bytes(&cache_dir.join("ai_thumbs"))
        + dir_size_bytes(&cache_dir.join("face_thumbs"));
    let sprites = dir_size_bytes(&cache_dir.join("sprites"));
    let motion_videos = dir_size_bytes(&cache_dir.join("motion_videos"));
    CacheStats {
        thumbnails,
        ai_thumbs,
        sprites,
        motion_videos,
        total: thumbnails + ai_thumbs + sprites + motion_videos,
        limit_mb,
    }
}

/// `clear_cache(kind)` 的清理范围映射。未知 kind 与 `"all"` 一律全清（防御式：宁可全清不漏）。
pub fn cache_subdirs_for_kind(kind: &str) -> &'static [&'static str] {
    match kind {
        "thumbnails" => &["thumbnails"],
        "ai" => &["ai_thumbs", "face_thumbs"],
        "sprites" => &["sprites"],
        "motion" => &["motion_videos"],
        _ => &[
            "thumbnails",
            "ai_thumbs",
            "face_thumbs",
            "sprites",
            "motion_videos",
        ],
    }
}

/// 删除指定 kind 的缓存子目录（整棵子树），返回释放字节数（best-effort，失败仅 warn）。
/// 与既有 LRU 一致：只删磁盘文件、不改 DB `thumb_status`——缺图由生成流水线按需重建
/// （`enforce_cache_limit` 早已如此，系统对「DB 有记录但文件缺失」健壮）。
/// 调用方应在 `spawn_blocking` 内执行。
pub fn clear_cache_kind(cache_dir: &Path, kind: &str) -> u64 {
    let mut freed = 0;
    for sub in cache_subdirs_for_kind(kind) {
        let dir = cache_dir.join(sub);
        if !dir.exists() {
            continue;
        }
        freed += dir_size_bytes(&dir);
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            tracing::warn!(
                "清理缓存子目录失败 {:?} | clear cache subdir failed: {}",
                dir,
                e
            );
        }
    }
    freed
}

/// 某 `cache_key` 对应的全部缓存产物绝对路径(4 档缩略图 + AI 缓存 + face 缓存 + 雪碧图 + 动态视频)。
/// 单一事实源：硬删媒体即时清理孤儿（§3.3.2）按此枚举。纯函数——路径由 cache_key 确定，不查 DB
/// （`media_derivations.payload_path` 会被 FK CASCADE 一并删除，故不可依赖；而路径方案是确定的）。
pub fn cache_files_for_key(cache_dir: &Path, cache_key: i64) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(8);
    for tier in [120u32, 240, 480, 960] {
        paths.push(thumb_path(cache_dir, tier, cache_key));
    }
    paths.push(ai_cache_path(cache_dir, cache_key));
    paths.push(face_cache_path(cache_dir, cache_key));
    paths.push(keyframe_sprite_path(cache_dir, cache_key));
    paths.push(motion_video_cache_path(cache_dir, cache_key));
    paths
}

/// 删除某 `cache_key` 的全部缓存产物（best-effort）。返回成功删除的文件数。
/// NotFound 属常态（多数产物本就不存在），不记日志；其它 IO 错误 warn 但不阻塞调用方。
pub fn remove_cache_files_for_key(cache_dir: &Path, cache_key: i64) -> usize {
    let mut removed = 0;
    for p in cache_files_for_key(cache_dir, cache_key) {
        match std::fs::remove_file(&p) {
            Ok(()) => removed += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(
                "孤儿缓存删除失败 {:?} | orphan cache delete failed: {}",
                p,
                e
            ),
        }
    }
    removed
}

/// 文件名主干恰为 16 位小写 hex → 解析回 cache_key(i64);否则 None。
/// 对账 GC 的识别护栏:原子写残留的临时文件(`xxx.webp.tmp` 的 stem 是 "xxx.webp")、
/// 任何外来文件都解析失败 → 永不触碰。
fn parse_cache_key_stem(path: &Path) -> Option<i64> {
    let stem = path.file_stem()?.to_str()?;
    if stem.len() != 16 {
        return None;
    }
    // cache_key_to_hex 只产小写;大写一律视为外来文件,不冒险。
    if !stem
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    u64::from_str_radix(stem, 16).ok().map(|v| v as i64)
}

/// 后台对账 GC(Part3 §3.3.2 兜底路径):删除五个产物子目录中 DB 已无对应 cache_key 的
/// 孤儿文件。与「硬删即时清理」(`remove_cache_files_for_key`)互补——即时清理漏掉的
/// (崩溃/删除失败/历史遗留)由本函数周期收敛。
///
/// 安全护栏(复审 §467 校准):
/// ① 只删 mtime 早于 `epoch`(进程启动时刻)的文件——本会话新写的产物可能尚未落 DB 行
///    (写文件与写 DB 行之间有窗口),一律不动,留到下次会话收敛;
/// ② 只识别文件名恰为 16 位小写 hex 的产物文件(`parse_cache_key_stem`);
/// ③ best-effort:单文件删除失败仅 warn,不中断整轮。
/// 调用方应在 `spawn_blocking` 内执行(全程阻塞 IO)。返回(删除数,释放字节)。
pub fn reconcile_orphan_gc(
    cache_dir: &Path,
    live_keys: &std::collections::HashSet<i64>,
    epoch: std::time::SystemTime,
) -> (usize, u64) {
    // 五个子目录的产物都按 cache_key 的 16 位 hex 命名(thumbnails 多一层 {size} 层级,
    // walkdir 递归天然覆盖)。软删行的 key 在 live 集内(软删可恢复,其缓存不算孤儿)。
    let mut removed = 0usize;
    let mut freed = 0u64;
    for sub in [
        "thumbnails",
        "ai_thumbs",
        "face_thumbs",
        "sprites",
        "motion_videos",
    ] {
        let dir = cache_dir.join(sub);
        if !dir.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let Some(key) = parse_cache_key_stem(entry.path()) else {
                continue;
            };
            if live_keys.contains(&key) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            // 护栏①:mtime 取不到或不早于 epoch → 保守跳过。
            match meta.modified() {
                Ok(mtime) if mtime < epoch => {}
                _ => continue,
            }
            let size = meta.len();
            match std::fs::remove_file(entry.path()) {
                Ok(()) => {
                    removed += 1;
                    freed += size;
                }
                Err(e) => tracing::warn!(
                    "对账 GC 删除孤儿失败 {:?} | orphan GC delete failed: {}",
                    entry.path(),
                    e
                ),
            }
        }
    }
    if removed > 0 {
        tracing::info!(
            "对账 GC 完成:删除 {} 个孤儿,释放 {} MB | orphan GC removed {} files, freed {} MB",
            removed,
            freed / 1024 / 1024,
            removed,
            freed / 1024 / 1024
        );
    }
    (removed, freed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 唯一临时目录（按 tag + 进程号隔离并行测试），返回前清空。
    fn unique_tmp(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "scrollery_cache_test_{}_{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_file(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; bytes]).unwrap();
    }

    /// `cache_files_for_key` 枚举 8 条路径:4 档缩略图 + ai_thumb + face_thumb + sprite + motion,且与各 path 助手一致。
    #[test]
    fn enumerates_all_eight_artifacts_for_key() {
        let dir = Path::new("C:/cache"); // 纯路径计算，不触磁盘
        let files = cache_files_for_key(dir, 0x1234);
        assert_eq!(files.len(), 8);
        // 与单产物助手逐一吻合（确保枚举不漏不串）。
        assert!(files.contains(&thumb_path(dir, 120, 0x1234)));
        assert!(files.contains(&thumb_path(dir, 960, 0x1234)));
        assert!(files.contains(&ai_cache_path(dir, 0x1234)));
        assert!(files.contains(&face_cache_path(dir, 0x1234)));
        assert!(files.contains(&keyframe_sprite_path(dir, 0x1234)));
        assert!(files.contains(&motion_video_cache_path(dir, 0x1234)));
    }

    /// 即时孤儿清理：删除该 key 落在磁盘上的产物，返回删除数；不存在的产物不计入、不报错。
    #[test]
    fn remove_cache_files_deletes_existing_only() {
        let dir = unique_tmp("orphan");
        let key = 0xABCD_i64;
        // 落 3 个产物（120 档缩略图 + ai + sprite），另 4 个不存在。
        write_file(&thumb_path(&dir, 120, key), 10);
        write_file(&ai_cache_path(&dir, key), 10);
        write_file(&keyframe_sprite_path(&dir, key), 10);
        let removed = remove_cache_files_for_key(&dir, key);
        assert_eq!(removed, 3);
        // 再删一次：全不存在 → 0，且不 panic。
        assert_eq!(remove_cache_files_for_key(&dir, key), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 占用统计：分目录字节累加 + total 求和 + limit 透传。
    #[test]
    fn stats_sum_per_subdir() {
        let dir = unique_tmp("stats");
        write_file(&thumb_path(&dir, 240, 1), 100);
        write_file(&thumb_path(&dir, 480, 2), 200);
        write_file(&ai_cache_path(&dir, 3), 50);
        write_file(&keyframe_sprite_path(&dir, 4), 30);
        write_file(&motion_video_cache_path(&dir, 5), 70);
        let s = compute_cache_stats(&dir, 512);
        assert_eq!(s.thumbnails, 300);
        assert_eq!(s.ai_thumbs, 50);
        assert_eq!(s.sprites, 30);
        assert_eq!(s.motion_videos, 70);
        assert_eq!(s.total, 450);
        assert_eq!(s.limit_mb, 512);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 分类清理：clear "ai" 只删 ai_thumbs、返回其字节；其余子目录不动。
    #[test]
    fn clear_kind_removes_only_target_subdir() {
        let dir = unique_tmp("clear");
        write_file(&thumb_path(&dir, 120, 1), 100);
        write_file(&ai_cache_path(&dir, 2), 60);
        let freed = clear_cache_kind(&dir, "ai");
        assert_eq!(freed, 60);
        assert!(!dir.join("ai_thumbs").exists());
        assert!(
            dir.join("thumbnails").exists(),
            "thumbnails 不应被 ai 清理触及"
        );
        // "all" 清掉剩余。
        let freed_all = clear_cache_kind(&dir, "all");
        assert_eq!(freed_all, 100);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 对账 GC:孤儿(不在 live 集)删除;在册文件、epoch 之后的新文件、非 16-hex 文件不动。
    #[test]
    fn orphan_gc_respects_liveset_epoch_and_naming() {
        let dir = unique_tmp("gc");
        let live_key = 0x11_i64;
        let orphan_key = 0x22_i64;
        write_file(&thumb_path(&dir, 120, live_key), 10);
        write_file(&thumb_path(&dir, 120, orphan_key), 10);
        write_file(&ai_cache_path(&dir, orphan_key), 10);
        // 外来文件:文件名非 16 位 hex,任何情况下不触碰。
        let alien = dir.join("thumbnails").join("readme.txt");
        write_file(&alien, 5);
        let live: std::collections::HashSet<i64> = [live_key].into_iter().collect();

        // epoch = 远古 → 所有文件 mtime 都不早于它 → 一个不删(护栏①:会话内新文件保护)。
        let past = std::time::SystemTime::UNIX_EPOCH;
        assert_eq!(reconcile_orphan_gc(&dir, &live, past), (0, 0));
        assert!(thumb_path(&dir, 120, orphan_key).exists());

        // epoch = 未来 → 两个孤儿(120 档缩略图 + ai 缓存)删除;在册与外来文件保留。
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let (removed, freed) = reconcile_orphan_gc(&dir, &live, future);
        assert_eq!(removed, 2);
        assert_eq!(freed, 20);
        assert!(thumb_path(&dir, 120, live_key).exists());
        assert!(!thumb_path(&dir, 120, orphan_key).exists());
        assert!(!ai_cache_path(&dir, orphan_key).exists());
        assert!(alien.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
