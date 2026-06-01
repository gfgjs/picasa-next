// src-tauri/src/utils/path.rs
//! Path normalisation and resolution utilities.
//!
//! Q12 / Q14 / 5.7 from the implementation plan:
//! - Database always stores forward-slash paths.
//! - Runtime path construction uses `PathBuf` for OS compatibility.

use std::path::{Path, PathBuf};

/// Normalise a path string for storage in the database.
///
/// Converts all OS-specific separators to `/`.
/// Trims leading/trailing whitespace and separators.
pub fn normalize_db_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

/// Build the absolute `PathBuf` from the three stored parts.
///
/// - `root_path`: absolute path of the scan root (from DB)
/// - `rel_path`:  relative path within the root (empty string if at root level)
/// - `file_name`: base file name
pub fn resolve_media_path(root_path: &str, rel_path: &str, file_name: &str) -> String {
    let mut pb = PathBuf::from(root_path);
    if !rel_path.is_empty() {
        pb.push(rel_path);
    }
    pb.push(file_name);
    // Return as forward-slash string (works cross-platform)
    pb.to_string_lossy().replace('\\', "/")
}

/// Extract the relative path of a file relative to a given root.
/// Returns forward-slash relative path, or empty string if the file is directly in root.
pub fn relative_to_root(root: &Path, file: &Path) -> String {
    let parent = file.parent().unwrap_or(Path::new(""));
    if let Ok(rel) = parent.strip_prefix(root) {
        normalize_db_path(&rel.to_string_lossy())
    } else {
        String::new()
    }
}

/// Compute the relative directory path for a file given the scan root.
pub fn dir_rel_path(root: &Path, file_path: &Path) -> String {
    let parent = file_path.parent().unwrap_or(file_path);
    if parent == root {
        // File is directly under root
        String::new()
    } else {
        parent
            .strip_prefix(root)
            .map(|p| normalize_db_path(&p.to_string_lossy()))
            .unwrap_or_default()
    }
}

/// Extract the depth of a relative path (number of `/` separators + 1, or 0 for root).
pub fn path_depth(rel_path: &str) -> i64 {
    if rel_path.is_empty() {
        0
    } else {
        (rel_path.matches('/').count() + 1) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_backslashes() {
        assert_eq!(normalize_db_path(r"photos\2024\january"), "photos/2024/january");
    }

    #[test]
    fn normalise_trims_slashes() {
        assert_eq!(normalize_db_path("/photos/2024/"), "photos/2024");
    }

    #[test]
    fn resolve_path_at_root() {
        let result = resolve_media_path("/data/photos", "", "IMG_001.jpg");
        assert!(result.ends_with("IMG_001.jpg"));
        assert!(result.contains("/data/photos/"));
    }

    #[test]
    fn path_depth_empty() {
        assert_eq!(path_depth(""), 0);
    }

    #[test]
    fn path_depth_nested() {
        assert_eq!(path_depth("a/b/c"), 3);
    }
}
