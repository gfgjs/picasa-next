// src-tauri/src/utils/hash.rs
//! xxHash3 (xxh3_64) hashing utilities.
//!
//! Q13 from the implementation plan:
//! `cache_key = xxh3_64("{rel_path}/{file_name}|{file_mtime}")` → stored as i64.
//! When used as a file name: `format!("{:016x}", cache_key as u64)` to avoid the negative sign.

use xxhash_rust::xxh3::xxh3_64;

/// Compute the cache key for a media item.
///
/// - `rel_path`: relative path within the scan root (empty string if item is at root level)
/// - `file_name`: the file's base name
/// - `file_mtime`: Unix timestamp of the last modification time
///
/// Returns a i64 (bit-reinterpreted from u64).
pub fn compute_cache_key(rel_path: &str, file_name: &str, file_mtime: i64) -> i64 {
    // Construct a stable input string. Use a leading "/" when rel_path is empty
    // so the format is always "{rel_path}/{file_name}|{mtime}".
    let input = if rel_path.is_empty() {
        format!("/{file_name}|{file_mtime}")
    } else {
        format!("{rel_path}/{file_name}|{file_mtime}")
    };
    xxh3_64(input.as_bytes()) as i64
}

/// Convert a cache_key i64 to the hex string used in file names.
/// Uses the unsigned bit pattern to avoid a leading `-` sign.
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
        let hex = cache_key_to_hex(key);
        assert_eq!(hex.len(), 16);
    }

    #[test]
    fn hex_no_negative_sign() {
        // Force a "negative" i64 (high bit set) — hex must not have a minus sign
        let key = i64::MIN;
        let hex = cache_key_to_hex(key);
        assert!(!hex.starts_with('-'));
        assert_eq!(hex.len(), 16);
    }
}
