// src-tauri/src/utils/path.rs
// src-tauri/src/utils/path.rs
//! Path normalisation and resolution utilities.
//! 路径规范化和解析实用工具。
//!
//! Q12 / Q14 / 5.7 from the implementation plan:
//! 来自实施计划的 Q12 / Q14 / 5.7：
//! - Database always stores forward-slash paths.
//! - 数据库始终存储正斜杠路径。
//! - Runtime path construction uses `PathBuf` for OS compatibility.
//! - 运行时路径构建使用 `PathBuf` 以实现操作系统兼容性。

use std::path::{Path, PathBuf};

/// Normalise a path string for storage in the database.
/// 规范化路径字符串以便在数据库中存储。
///
/// Converts all OS-specific separators to `/`.
/// 将所有操作系统特定的分隔符转换为 `/`。
/// Trims leading/trailing whitespace and separators.
/// 去除前导/尾随空格和分隔符。
pub fn normalize_db_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

/// Build the absolute `PathBuf` from the three stored parts.
/// 根据存储的三个部分构建绝对 `PathBuf`。
///
/// - `root_path`: absolute path of the scan root (from DB)
/// - `root_path`: 扫描根目录的绝对路径（来自数据库）
/// - `rel_path`:  relative path within the root (empty string if at root level)
/// - `rel_path`:  根目录内的相对路径（如果处于根级别则为空字符串）
/// - `file_name`: base file name
/// - `file_name`: 基本文件名
pub fn resolve_media_path(root_path: &str, rel_path: &str, file_name: &str) -> String {
    let mut pb = PathBuf::from(root_path);
    if !rel_path.is_empty() {
        pb.push(rel_path);
    }
    pb.push(file_name);
    // Return as forward-slash string (works cross-platform)
    // 返回正斜杠字符串（跨平台工作）
    pb.to_string_lossy().replace('\\', "/")
}

/// Extract the relative path of a file relative to a given root.
/// 提取文件相对于给定根目录的相对路径。
/// Returns forward-slash relative path, or empty string if the file is directly in root.
/// 返回正斜杠相对路径，如果文件直接位于根目录中则返回空字符串。
pub fn relative_to_root(root: &Path, file: &Path) -> String {
    let parent = file.parent().unwrap_or(Path::new(""));
    if let Ok(rel) = parent.strip_prefix(root) {
        normalize_db_path(&rel.to_string_lossy())
    } else {
        String::new()
    }
}

/// Compute the relative directory path for a file given the scan root.
/// 给定扫描根目录，计算文件的相对目录路径。
pub fn dir_rel_path(root: &Path, file_path: &Path) -> String {
    let parent = file_path.parent().unwrap_or(file_path);
    if parent == root {
        // File is directly under root
        // 文件直接位于根目录下
        String::new()
    } else {
        parent
            .strip_prefix(root)
            .map(|p| normalize_db_path(&p.to_string_lossy()))
            .unwrap_or_default()
    }
}

/// Extract the depth of a relative path (number of `/` separators + 1, or 0 for root).
/// 提取相对路径的深度（`/` 分隔符的数量 + 1，对于根目录则为 0）。
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
