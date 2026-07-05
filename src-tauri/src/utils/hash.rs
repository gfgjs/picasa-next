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

/// 内容指纹全文阈值:≤ 此值全文 hash(无漏检);> 此值走「头/中/尾 3×256KB + 长度」抽样。
pub const CONTENT_HASH_FULL_LIMIT: u64 = 64 * 1024 * 1024;
const SAMPLE_CHUNK: usize = 256 * 1024;

/// 媒体内容指纹(Part2 §3.3.2「可疑变更」二次确认)。带算法前缀,算法演进时旧基线可被
/// 识别为「非本算法」→ 按 NULL 兜底保守失效,而非静默错比:
/// - ≤64MB:`sha256:<hex>`(全文,无漏检);
/// - 大于 64MB:`sha256s:<hex>`(抽样 = 头 256KB ‖ 中 256KB ‖ 尾 256KB ‖ 文件长度 LE 字节)。
///   已知漏检边界(设计已接受):采样窗外字节变化且总长不变会漏判——元数据编辑通常改头部,
///   落采样窗,覆盖主用例。同一文件的形态只取决于 size,而比较仅发生在 size 相同时 → 新旧
///   指纹必然同形态,不会全文 vs 抽样错比。
///
/// 注:设计定 BLAKE3,离线 cargo 缓存缺该 crate → 以在树 sha2 实现(语义等价,性能取舍;
/// 联网预取后可换,前缀机制保证换算法安全)。
pub fn content_fingerprint(path: &Path, file_size: i64) -> std::io::Result<String> {
    content_fingerprint_with_limit(path, file_size, CONTENT_HASH_FULL_LIMIT)
}

/// 阈值参数化版本(测试用小阈值构造抽样路径,不必生成 64MB 文件)。
pub fn content_fingerprint_with_limit(
    path: &Path,
    file_size: i64,
    full_limit: u64,
) -> std::io::Result<String> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let len = file_size.max(0) as u64;
    if len <= full_limit {
        return Ok(format!("sha256:{}", sha256_hex_of_file(path)?));
    }
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; SAMPLE_CHUNK];
    // 头/中/尾三段(saturating:极小文件时三窗重叠也确定性一致,不会下溢 panic)。
    let offsets = [
        0u64,
        (len / 2).saturating_sub(SAMPLE_CHUNK as u64 / 2),
        len.saturating_sub(SAMPLE_CHUNK as u64),
    ];
    for off in offsets {
        f.seek(SeekFrom::Start(off))?;
        let mut read_total = 0;
        while read_total < SAMPLE_CHUNK {
            let n = f.read(&mut buf[read_total..])?;
            if n == 0 {
                break; // EOF:窗越界部分自然截断
            }
            read_total += n;
        }
        hasher.update(&buf[..read_total]);
    }
    // 长度入摘要:同采样内容、不同总长的文件不得同指纹。
    hasher.update(len.to_le_bytes());
    Ok(format!("sha256s:{}", to_hex_lower(&hasher.finalize())))
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
    fn content_fingerprint_full_vs_sampled() {
        let dir = std::env::temp_dir().join(format!("fp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.bin");
        std::fs::write(&p, b"hello world.").unwrap();
        // 全文路径:sha256: 前缀,与流式文件 hash 一致。
        let full = content_fingerprint_with_limit(&p, 12, 1024).unwrap();
        assert_eq!(full, format!("sha256:{}", sha256_hex_of_file(&p).unwrap()));
        // 抽样路径(小阈值迫使抽样):sha256s: 前缀;同内容稳定;窗内变化即变指纹。
        let s1 = content_fingerprint_with_limit(&p, 12, 4).unwrap();
        assert!(s1.starts_with("sha256s:"));
        assert_eq!(s1, content_fingerprint_with_limit(&p, 12, 4).unwrap());
        std::fs::write(&p, b"hello world!").unwrap(); // 同长度,末字节变(落尾窗)
        let s2 = content_fingerprint_with_limit(&p, 12, 4).unwrap();
        assert_ne!(s1, s2, "同长度、采样窗内变化应改变指纹");
        let _ = std::fs::remove_dir_all(&dir);
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
