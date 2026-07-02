// src-tauri/src/utils/hash.rs
// src-tauri/src/utils/hash.rs
//! Hashing utilities: xxHash3 (cache keys) + SHA-256 hex (integrity checks, R2-6 收拢).
//! 哈希实用工具:xxHash3(缓存键)+ SHA-256 hex(完整性校验,R2-6 全仓去重收拢于此)。
//!
//! Q13 from the implementation plan:
//! 来自实施计划的 Q13：
//! `cache_key = xxh3_64("{rel_path}/{file_name}|{file_mtime}")` → stored as i64.
//! `cache_key = xxh3_64("{rel_path}/{file_name}|{file_mtime}")` → 存储为 i64。
//! When used as a file name: `format!("{:016x}", cache_key as u64)` to avoid the negative sign.
//! 当用作文件名时：`format!("{:016x}", cache_key as u64)` 以避免出现负号。

use sha2::{Digest as _, Sha256};
use std::path::Path;
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

// ── SHA-256 → 小写 hex(R2-6:原 download/exotic 五处重复实现收拢) ─────────────

/// 字节序列 → 小写 hex 字符串。
pub fn to_hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 字节 sha256(64 位小写 hex,与 exotic/package.rs is_sha256_hex 校验器契约一致)。
pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex_lower(&Sha256::digest(bytes))
}

/// 文件 sha256(小写 hex)。64 KiB 分块流式读,不全量载入内存(支持 GB 级文件)。
pub fn sha256_hex_of_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read as _;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 16];
    loop {
        match file.read(&mut buf)? {
            0 => break,
            n => hasher.update(&buf[..n]),
        }
    }
    Ok(to_hex_lower(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_known_vector() {
        // sha256("hello") 已知向量,锁 64 位小写 hex 契约。
        assert_eq!(
            sha256_hex(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn sha256_file_matches_bytes_version() {
        let dir = std::env::temp_dir().join(format!("hash-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("v.bin");
        std::fs::write(&p, b"hello").unwrap();
        assert_eq!(sha256_hex_of_file(&p).unwrap(), sha256_hex(b"hello"));
        let _ = std::fs::remove_dir_all(&dir);
    }

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
