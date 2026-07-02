// src-tauri/src/storage/local.rs
//! Local-filesystem `StorageBackend` (pure `std::fs`, both variants). This is what 8A uses:
//! an OS-mounted network drive or UNC share is just a local path here (§3.8).
//! 本地文件系统 `StorageBackend`（纯 `std::fs`，两变体都含）。8A 即用它：OS 映射的网络盘 / UNC
//! 共享在此就是一条本地路径（§3.8）。

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};
use crate::storage::{RemoteEntry, StorageBackend};

/// A `StorageBackend` rooted at a local (or OS-mounted) directory.
/// 以本地（或 OS 挂载）目录为根的 `StorageBackend`。
pub struct LocalFs {
    base: PathBuf,
}

impl LocalFs {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    fn abs(&self, rel_path: &str) -> PathBuf {
        if rel_path.is_empty() {
            self.base.clone()
        } else {
            self.base.join(rel_path)
        }
    }
}

fn entry_from(base: &Path, p: &Path, is_dir: bool, size: u64, mtime: i64) -> RemoteEntry {
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let rel_path = p
        .strip_prefix(base)
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| name.clone());
    RemoteEntry {
        name,
        rel_path,
        is_dir,
        size,
        mtime,
    }
}

fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl StorageBackend for LocalFs {
    fn kind(&self) -> &'static str {
        "local"
    }

    fn list_dir(&self, rel_path: &str) -> Result<Vec<RemoteEntry>> {
        let dir = self.abs(rel_path);
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(AppError::from)? {
            let entry = entry.map_err(AppError::from)?;
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            out.push(entry_from(
                &self.base,
                &entry.path(),
                meta.is_dir(),
                meta.len(),
                mtime_secs(&meta),
            ));
        }
        Ok(out)
    }

    fn stat(&self, rel_path: &str) -> Result<RemoteEntry> {
        let p = self.abs(rel_path);
        let meta = std::fs::metadata(&p).map_err(AppError::from)?;
        Ok(entry_from(
            &self.base,
            &p,
            meta.is_dir(),
            meta.len(),
            mtime_secs(&meta),
        ))
    }

    fn read_range(&self, rel_path: &str, start: u64, len: Option<u64>) -> Result<Vec<u8>> {
        let mut f = std::fs::File::open(self.abs(rel_path)).map_err(AppError::from)?;
        if start > 0 {
            f.seek(SeekFrom::Start(start)).map_err(AppError::from)?;
        }
        let mut buf = Vec::new();
        match len {
            Some(n) => {
                buf.resize(n as usize, 0);
                let read = read_full(&mut f, &mut buf)?;
                buf.truncate(read);
            }
            None => {
                f.read_to_end(&mut buf).map_err(AppError::from)?;
            }
        }
        Ok(buf)
    }
}

/// Read up to `buf.len()` bytes, tolerating short reads near EOF. Returns bytes actually read.
/// 读取至多 `buf.len()` 字节，容忍接近文件尾的短读。返回实际读取字节数。
fn read_full(f: &mut std::fs::File, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match f.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(AppError::from(e)),
        }
    }
    Ok(filled)
}
