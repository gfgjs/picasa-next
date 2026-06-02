// src-tauri/src/scanner/walker.rs
//! Recursive directory walker using `walkdir`.
//! 使用 `walkdir` 的递归目录遍历器。
//! Produces a flat list of `WalkedFile` entries classified by media type.
//! 生成按媒体类型分类的 `WalkedFile` 条目的扁平列表。

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::{DirEntry, WalkDir};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, Result};
use crate::utils::format::{classify_media_type, MediaType};

/// A single discovered file entry.
/// 单个发现的文件条目。
#[derive(Debug, Clone)]
pub struct WalkedFile {
    /// Absolute path of the file.
    /// 文件的绝对路径。
    pub abs_path:   PathBuf,
    /// File name (basename).
    /// 文件名 (basename)。
    pub file_name:  String,
    /// Lowercase file extension.
    /// 小写文件扩展名。
    pub extension:  String,
    /// Classified media type.
    /// 分类的媒体类型。
    pub media_type: MediaType,
    /// File size in bytes.
    /// 文件大小（以字节为单位）。
    pub file_size:  i64,
    /// Last modification time as Unix timestamp.
    /// 最后修改时间作为 Unix 时间戳。
    pub file_mtime: i64,
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// Walk `root` recursively and return all recognised media files.
/// 递归遍历 `root` 并返回所有可识别的媒体文件。
/// Hidden entries (dot-prefixed) are skipped.
/// 隐藏条目（点前缀）将被跳过。
pub fn walk_media_files(
    root: &Path,
    cancel: &CancellationToken,
    mut progress_cb: impl FnMut(usize),
) -> Result<Vec<WalkedFile>> {
    let mut results = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok())
    {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        let media_type = match classify_media_type(&ext) {
            Some(t) => t,
            None    => continue, // unknown format — skip
                                 // 未知格式 — 跳过
        };

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let file_size = meta.len() as i64;
        let file_mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        results.push(WalkedFile {
            abs_path: path.to_path_buf(),
            file_name,
            extension: ext,
            media_type,
            file_size,
            file_mtime,
        });

        if results.len() % 1000 == 0 {
            progress_cb(results.len());
        }
    }

    // Emit final count
    progress_cb(results.len());

    Ok(results)
}
