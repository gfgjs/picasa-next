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

use crate::utils::hash::cache_key_to_hex;

/// Build the full path for a thumbnail file.
/// 构建缩略图文件的完整路径。
pub fn thumb_path(cache_dir: &Path, size: u32, cache_key: i64) -> PathBuf {
    debug_assert!(
        [120, 240, 480, 960].contains(&size),
        "Thumbnail size {} is not a valid tier | 缩略图尺寸 {} 不是有效档位", size, size
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
        "Thumbnail size {} is not a valid tier | 缩略图尺寸 {} 不是有效档位", size, size
    );
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    format!("{size}/{prefix}/{hex}.webp")
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

/// Ensure the directory for a given thumb path exists.
/// 确保给定缩略图路径的目录存在。
pub fn ensure_thumb_dir(cache_dir: &Path, size: u32, cache_key: i64) -> std::io::Result<()> {
    let p = thumb_path(cache_dir, size, cache_key);
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

    let thumb_dir = cache_dir.join("thumbnails");
    if !thumb_dir.exists() {
        return;
    }

    // Use walkdir to iterate all files | 使用 walkdir 遍历所有文件
    for entry in walkdir::WalkDir::new(&thumb_dir).into_iter().filter_map(|e| e.ok()) {
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

    if total_size <= max_size_bytes {
        tracing::info!(
            "Cache size {} MB is within limit {} MB | 缓存大小 {} MB 在限制 {} MB 内",
            total_size / 1024 / 1024, max_size_mb, total_size / 1024 / 1024, max_size_mb
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
            tracing::warn!("Failed to delete cache file {:?} | 无法删除缓存文件 {:?}: {}", path, path, e);
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
