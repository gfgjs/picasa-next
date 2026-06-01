// src-tauri/src/thumbnail/cache.rs
//! Size-bucketed thumbnail cache management.
//!
//! Cache layout (§ 8.2):
//! `{app_data_dir}/cache/thumbnails/{size}/{2-char-prefix}/{cache_key_hex}.webp`
//! e.g. `cache/thumbnails/300/a3/a3f4b2c1d0e9f7a1.webp`

use std::path::{Path, PathBuf};

use crate::utils::hash::cache_key_to_hex;

/// Build the full path for a thumbnail file.
pub fn thumb_path(cache_dir: &Path, size: u32, cache_key: i64) -> PathBuf {
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    cache_dir
        .join("thumbnails")
        .join(size.to_string())
        .join(prefix)
        .join(format!("{hex}.webp"))
}

/// Check whether a thumbnail already exists on disk.
pub fn thumb_exists(cache_dir: &Path, size: u32, cache_key: i64) -> bool {
    thumb_path(cache_dir, size, cache_key).exists()
}

/// The relative path stored in the DB: `"{size}/{prefix}/{hex}.webp"`.
pub fn thumb_db_path(size: u32, cache_key: i64) -> String {
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    format!("{size}/{prefix}/{hex}.webp")
}

/// Build the absolute path of the motion video cache directory.
pub fn motion_video_cache_path(cache_dir: &Path, cache_key: i64) -> PathBuf {
    let hex = cache_key_to_hex(cache_key);
    let prefix = &hex[..2];
    cache_dir
        .join("motion_videos")
        .join(prefix)
        .join(format!("{hex}.mp4"))
}

/// Ensure the directory for a given thumb path exists.
pub fn ensure_thumb_dir(cache_dir: &Path, size: u32, cache_key: i64) -> std::io::Result<()> {
    let p = thumb_path(cache_dir, size, cache_key);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
