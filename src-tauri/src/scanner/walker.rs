// src-tauri/src/scanner/walker.rs
//! Recursive directory walker using `walkdir`.
//! Produces a flat list of `WalkedFile` entries classified by media type.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::{DirEntry, WalkDir};

use crate::utils::format::{classify_media_type, MediaType};

/// A single discovered file entry.
#[derive(Debug, Clone)]
pub struct WalkedFile {
    /// Absolute path of the file.
    pub abs_path:   PathBuf,
    /// File name (basename).
    pub file_name:  String,
    /// Lowercase file extension.
    pub extension:  String,
    /// Classified media type.
    pub media_type: MediaType,
    /// File size in bytes.
    pub file_size:  i64,
    /// Last modification time as Unix timestamp.
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
/// Hidden entries (dot-prefixed) are skipped.
pub fn walk_media_files(root: &Path) -> Vec<WalkedFile> {
    let mut results = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok())
    {
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
    }

    results
}
