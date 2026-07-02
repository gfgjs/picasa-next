// src-tauri/src/exotic/fetch.rs
//! 冷门格式插件 · 包下载（v3 Part3 §6.4 第 2 步）。
//!
//! **R10 收敛后（Part6 §3.1.2）的薄适配器**：下载机制（安全 client / Range / 镜像回退 / sha256 /
//! `.part` 原子改名）已下沉 `crate::download` 通用引擎；本文件只保留 exotic 的**领域契约**——
//! 稳定的 `FetchError` 错误码（供 `exotic_commands` 跨 IPC 边界使用，调用方零改动）+ Registry/包
//! 两个下载入口。机制不再在此重复实现。
//!
//! 安全：只接受 HTTPS；下载量精确封顶到 `expected_size`（超出立即中止）；size+sha256 双校验后才
//! `.part` → 原子 rename——这些策略现由通用引擎统一保证。

use std::path::Path;

use crate::download::{self, DownloadError, TimeoutPolicy};

/// 下载/校验错误。`code()` 稳定，可安全输出。
/// （exotic 的公开错误契约；机制错误来自通用引擎的 `DownloadError`，此处 1:1 折叠以保持稳定码。）
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("非 HTTPS 下载地址")]
    NotHttps,
    #[error("HTTP 请求失败：{0}")]
    Http(String),
    #[error("HTTP 状态码 {0}")]
    Status(u16),
    #[error("下载量超出声明大小（服务器超发）")]
    TooLarge,
    #[error("大小校验失败：期望 {expected} 实得 {got}")]
    SizeMismatch { expected: u64, got: u64 },
    #[error("sha256 校验失败（包损坏或被篡改）")]
    HashMismatch,
    #[error("IO 失败：{0}")]
    Io(String),
}

impl FetchError {
    pub fn code(&self) -> &'static str {
        match self {
            FetchError::NotHttps => "not_https",
            FetchError::Http(_) => "http",
            FetchError::Status(_) => "status",
            FetchError::TooLarge => "too_large",
            FetchError::SizeMismatch { .. } => "size_mismatch",
            FetchError::HashMismatch => "hash_mismatch",
            FetchError::Io(_) => "io",
        }
    }
}

/// 通用引擎错误 → exotic 稳定错误码（1:1，码值不变）。
impl From<DownloadError> for FetchError {
    fn from(e: DownloadError) -> Self {
        match e {
            DownloadError::NotHttps => FetchError::NotHttps,
            DownloadError::Http(s) => FetchError::Http(s),
            DownloadError::Status(c) => FetchError::Status(c),
            DownloadError::TooLarge => FetchError::TooLarge,
            DownloadError::SizeMismatch { expected, got } => {
                FetchError::SizeMismatch { expected, got }
            }
            DownloadError::HashMismatch => FetchError::HashMismatch,
            DownloadError::Io(s) => FetchError::Io(s),
        }
    }
}

/// Registry index/sig 单文件大小上限（index 仅 KB 级；防服务器超发巨型输入）。
/// 与 registry.rs 的 `MAX_INDEX_LEN`(4MiB) 同量级——下载层先封顶，解析层再校验，双保险。
const MAX_REGISTRY_FILE: u64 = 4 * 1024 * 1024;

/// 下载 `url` 到 `dest`，对照 `expected_size`/`expected_sha256` 校验通过后原子就位。
/// 失败不留下 `dest`（清理 `.part`）。`expected_size` 即硬上限：候选源失败回退时据此判断残留续传。
///
/// 包体量小（MB 级），暂不接进度回调（插件商店安装进度 UI 待 Part8）；但已走通用引擎，
/// **Range 续传/镜像回退能力随引擎天然具备**，后续接入仅需传候选源列表 + 进度回调。
pub async fn fetch_package(
    url: &str,
    dest: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), FetchError> {
    let client = download::secure_client(TimeoutPolicy::SmallFile)?;
    let part = dest.with_extension("part");

    // 下载（单一源；引擎支持镜像回退，exotic Registry 暂无镜像字段，故传单元素候选）。
    let noop = |_: u64| {};
    let dl =
        download::download_with_fallback(&client, &[url], &part, 0, expected_size, &noop).await;
    if let Err(e) = dl {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e.into());
    }

    // 下载后完整性校验（size + sha256）。
    if let Err(e) = download::verify_size_sha(&part, expected_size, Some(expected_sha256)) {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e.into());
    }

    tokio::fs::rename(&part, dest)
        .await
        .map_err(|e| FetchError::Io(e.to_string()))?;
    Ok(())
}

/// 下载 Registry 的 `index.json` + `index.sig`（**验签前**的原始字节）。
/// **不在此验签/防回滚**——交 `RegistryCache::accept`（验签 + 单调防回滚先于解析，registry.rs:242）。
/// 返回 `(index_bytes, sig_bytes)`。`base_url` 末尾斜杠容错。
pub async fn download_registry_index(base_url: &str) -> Result<(Vec<u8>, Vec<u8>), FetchError> {
    let client = download::secure_client(TimeoutPolicy::SmallFile)?;
    let base = base_url.trim_end_matches('/');
    let index =
        download::download_to_vec(&client, &format!("{base}/index.json"), MAX_REGISTRY_FILE)
            .await?;
    let sig =
        download::download_to_vec(&client, &format!("{base}/index.sig"), MAX_REGISTRY_FILE).await?;
    Ok((index, sig))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_https() {
        // 非 https 在引擎层即拒（不发请求）；适配器透传为 FetchError::NotHttps。
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let dest = std::env::temp_dir().join("exotic-fetch-nohttps.zip");
        let r = rt.block_on(fetch_package("http://x.invalid/a.zip", &dest, 1, "00"));
        assert!(matches!(r, Err(FetchError::NotHttps)));
    }

    #[test]
    fn registry_index_rejects_non_https() {
        // Registry 下载同样强制 HTTPS：非 https 基址在发请求前即拒（不触网）。
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let r = rt.block_on(download_registry_index("http://x.invalid/exotic"));
        assert!(matches!(r, Err(FetchError::NotHttps)));
    }
}
