// src-tauri/src/utils/hash.rs
// src-tauri/src/utils/hash.rs
//! xxHash3 (xxh3_64) hashing utilities.
//! xxHash3 (xxh3_64) 哈希实用工具。
//!
//! Q13 from the implementation plan:
//! 来自实施计划的 Q13：
//! `cache_key = xxh3_64("{rel_path}/{file_name}|{file_mtime}")` → stored as i64.
//! `cache_key = xxh3_64("{rel_path}/{file_name}|{file_mtime}")` → 存储为 i64。
//! When used as a file name: `format!("{:016x}", cache_key as u64)` to avoid the negative sign.
//! 当用作文件名时：`format!("{:016x}", cache_key as u64)` 以避免出现负号。

use xxhash_rust::xxh3::xxh3_64;

/// Compute the cache key for a media item.
/// 计算媒体项的缓存键。
///
/// - `rel_path`: relative path within the scan root (empty string if item is at root level)
/// - `rel_path`: 在扫描根目录内的相对路径（如果项目在根级别则为空字符串）
/// - `file_name`: the file's base name
/// - `file_name`: 文件的基本名称
/// - `file_mtime`: Unix timestamp of the last modification time
/// - `file_mtime`: 最后修改时间的 Unix 时间戳
///
/// Returns a i64 (bit-reinterpreted from u64).
/// 返回一个 i64（由 u64 重新解释位）。
pub fn compute_cache_key(rel_path: &str, file_name: &str, file_mtime: i64) -> i64 {
    // Construct a stable input string. Use a leading "/" when rel_path is empty
    // 构建稳定的输入字符串。当 rel_path 为空时使用前导 "/"，
    // so the format is always "{rel_path}/{file_name}|{mtime}".
    // 使得格式始终为 "{rel_path}/{file_name}|{mtime}"。
    let input = if rel_path.is_empty() {
        format!("/{file_name}|{file_mtime}")
    } else {
        format!("{rel_path}/{file_name}|{file_mtime}")
    };
    xxh3_64(input.as_bytes()) as i64
}

/// Convert a cache_key i64 to the hex string used in file names.
/// 将 cache_key i64 转换为文件名中使用的十六进制字符串。
/// Uses the unsigned bit pattern to avoid a leading `-` sign.
/// 使用无符号位模式以避免前导 `-` 符号。
pub fn cache_key_to_hex(cache_key: i64) -> String {
    format!("{:016x}", cache_key as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_same_hash() {
        let a = compute_cache_key("photos/2024", "IMG_001.jpg", 1_700_000_000);
        let b = compute_cache_key("photos/2024", "IMG_001.jpg", 1_700_000_000);
        assert_eq!(a, b);
    }

    #[test]
    fn different_mtime_different_hash() {
        let a = compute_cache_key("photos/2024", "IMG_001.jpg", 1_700_000_000);
        let b = compute_cache_key("photos/2024", "IMG_001.jpg", 1_700_000_001);
        assert_ne!(a, b);
    }

    #[test]
    fn empty_rel_path() {
        let key = compute_cache_key("", "root_file.jpg", 12345);
        // Should not panic and should produce a 16-char hex
        // 不应崩溃并且应生成一个 16 个字符的十六进制字符串
        let hex = cache_key_to_hex(key);
        assert_eq!(hex.len(), 16);
    }

    #[test]
    fn hex_no_negative_sign() {
        // Force a "negative" i64 (high bit set) — hex must not have a minus sign
        // 强制使用“负数” i64（高位被置位）——十六进制不能有负号
        let key = i64::MIN;
        let hex = cache_key_to_hex(key);
        assert!(!hex.starts_with('-'));
        assert_eq!(hex.len(), 16);
    }
}
