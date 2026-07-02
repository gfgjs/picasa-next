// src-tauri/src/scanner/walker.rs
//! Recursive directory walker using `walkdir`.
//! 使用 `walkdir` 的递归目录遍历器。
//! Produces a flat list of `WalkedFile` entries classified by media type.
//! 生成按媒体类型分类的 `WalkedFile` 条目的扁平列表。

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tokio_util::sync::CancellationToken;
use walkdir::{DirEntry, WalkDir};

use crate::exotic::catalog::CatalogSnapshot;
use crate::utils::format::{classify_media_type, MediaType};

/// 扫描期分类（R13）：common-first，仅当常见格式表返回 `None` 时查 Catalog。
/// **只依赖扩展名**——exotic 识别不需 enrichment（宽高/时长），扫描事务内即可判定、零额外 IO。
/// Catalog 不可覆盖常见格式（common 优先）。同一轮 walk 共用同一 snapshot，保证分类一致。
fn classify_scanned_file(ext: &str, catalog: &CatalogSnapshot) -> Option<MediaType> {
    classify_media_type(ext).or_else(|| catalog.media_kind(ext).map(Into::into))
}

/// 一条遍历错误（权限/IO/symlink loop/metadata 失败）。
/// 🔴 数据安全关键：这些是**未能进入 `seen` 集的真实文件**——缺失检测的差集若在
/// `errors` 非空时仍执行，会把它们误判为「已删除」。故 `WalkReport.complete` 据此守门。
#[derive(Debug, Clone)]
pub struct WalkError {
    /// 出错路径（取不到则空串）。
    pub path: String,
    /// 错误类别 + 简述（诊断用；分类已足够支撑「不完整不删除」判定）。
    pub reason: String,
}

/// 一轮**流式**遍历的收尾结论（T12）：遍历错误 + 是否「完整」+ 是否被取消。
/// 流式遍历不再全量持有 `files`——文件在 [`MediaWalker`] 迭代中被逐批消费；遍历结束后由
/// [`MediaWalker::finish`] 产出本结论。
///
/// `complete == errors.is_empty() && !cancelled`：唯有完整扫描（零遍历/metadata 错误、未取消）
/// 才允许下游差集删除（不变量「不完整扫描 ≠ 删除」，Part2 §3.2.2）。
#[derive(Debug)]
pub struct WalkOutcome {
    pub errors: Vec<WalkError>,
    pub complete: bool,
    /// 中途被 `CancellationToken` 取消 → 视为不完整（`complete=false`），且调用方应返回 `Cancelled`。
    pub cancelled: bool,
}

/// A single discovered file entry.
/// 单个发现的文件条目。
#[derive(Debug, Clone)]
pub struct WalkedFile {
    /// Absolute path of the file.
    /// 文件的绝对路径。
    pub abs_path: PathBuf,
    /// File name (basename).
    /// 文件名 (basename)。
    pub file_name: String,
    /// Lowercase file extension.
    /// 小写文件扩展名。
    pub extension: String,
    /// Classified media type.
    /// 分类的媒体类型。
    pub media_type: MediaType,
    /// File size in bytes.
    /// 文件大小（以字节为单位）。
    pub file_size: i64,
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

/// **流式**媒体遍历器（T12）：按 `walkdir` 顺序逐个产出 [`WalkedFile`]，**不再全量收 `Vec`**，
/// 使 fast_scan 内存峰值从 O(N)（百万级约 600MB，含 `order_for_view` 的 clone 再翻倍）降到
/// O(batch)（一次只持有 500 项）。
///
/// 实现 [`Iterator`]：消费方（fast_scan）自行按批拉取（`while let Some(f) = w.next()` 攒满一批即
/// 入库）。遍历过程中：
/// - **隐藏目录剪枝**：`filter_entry(!is_hidden)` 整棵剪掉点前缀目录（如 `.git`），不递归进去。
/// - **遍历/metadata 错误不再静默丢弃**，而是累积进 `errors`（🔴 数据安全：这些是未进 seen 的真实
///   文件，差集若在有错误时仍跑会误删它们）。遍历结束调 [`finish`](MediaWalker::finish) 取
///   [`WalkOutcome`]，其 `complete` 即缺失检测「不完整不删」门闩（Part2 §3.2.2）。
/// - **取消**：每步检查 `CancellationToken`，触发即停止产出并置 `cancelled`（→ `complete=false`）。
///
/// 分类未命中（非媒体/非已知 exotic 扩展名）**不算错误**——正常跳过，不影响完整性。
pub struct MediaWalker<'a> {
    // Box<dyn> 抹掉 `filter_entry` 闭包的匿名类型；闭包零捕获、`WalkDir` 自持 root 路径 → 'static。
    inner: Box<dyn Iterator<Item = walkdir::Result<DirEntry>>>,
    catalog: &'a CatalogSnapshot,
    cancel: &'a CancellationToken,
    errors: Vec<WalkError>,
    cancelled: bool,
}

impl<'a> MediaWalker<'a> {
    pub fn new(root: &Path, catalog: &'a CatalogSnapshot, cancel: &'a CancellationToken) -> Self {
        let inner = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_hidden(e));
        Self {
            inner: Box::new(inner),
            catalog,
            cancel,
            errors: Vec::new(),
            cancelled: false,
        }
    }

    /// 消费遍历器，产出本轮收尾结论（错误集 + 完整性 + 取消标志）。**遍历耗尽或取消后调用**。
    pub fn finish(self) -> WalkOutcome {
        let cancelled = self.cancelled;
        let complete = self.errors.is_empty() && !cancelled;
        if !complete && !cancelled {
            // 上报但不致命：调用方据 complete 决定「只展示、不差集删除」。
            tracing::warn!(
                "Walk incomplete: {} error(s) | 扫描不完整：{} 处遍历错误（缺失检测将跳过差集删除）",
                self.errors.len(),
                self.errors.len()
            );
        }
        WalkOutcome {
            errors: self.errors,
            complete,
            cancelled,
        }
    }
}

impl Iterator for MediaWalker<'_> {
    type Item = WalkedFile;

    fn next(&mut self) -> Option<WalkedFile> {
        if self.cancelled {
            return None;
        }
        loop {
            // 每步先查取消：及时停止深目录遍历（产出 None，置 cancelled → 不完整 → 不差集删除）。
            if self.cancel.is_cancelled() {
                self.cancelled = true;
                return None;
            }

            let entry = match self.inner.next()? {
                Ok(e) => e,
                Err(e) => {
                    // 遍历错误（权限/IO/symlink loop）：记下而非丢——seen 完整性依赖此。
                    self.errors.push(WalkError {
                        path: e
                            .path()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                        reason: format!("traverse: {e}"),
                    });
                    continue;
                }
            };

            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();

            let media_type = match classify_scanned_file(&ext, self.catalog) {
                Some(t) => t,
                None => continue, // 既非常见格式、也非 Catalog 已知 exotic 格式 — 正常跳过（非错误）
            };

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    // metadata 失败的是**已识别媒体文件**：必须计入 errors（否则差集会误删它）。
                    self.errors.push(WalkError {
                        path: path.display().to_string(),
                        reason: format!("metadata: {e}"),
                    });
                    continue;
                }
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

            return Some(WalkedFile {
                abs_path: path.to_path_buf(),
                file_name,
                extension: ext,
                media_type,
                file_size,
                file_mtime,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_common_first_then_catalog() {
        let catalog = CatalogSnapshot::builtin().unwrap();
        // 常见格式：走 common，不查 Catalog。
        assert_eq!(
            classify_scanned_file("jpg", &catalog),
            Some(MediaType::Image)
        );
        // psd 已移出 common → 由 Catalog 识别为 exotic image。
        assert_eq!(
            classify_scanned_file("psd", &catalog),
            Some(MediaType::Image)
        );
        // 既非 common 也非 Catalog 已知 → None（walker 跳过）。
        assert_eq!(classify_scanned_file("xyz", &catalog), None);
    }

    #[test]
    fn empty_catalog_drops_psd() {
        // 无 Catalog（降级）时 psd 不再被任何表识别 → 不入库（而非走会失败的主解码器）。
        let empty = CatalogSnapshot::empty();
        assert_eq!(classify_scanned_file("psd", &empty), None);
        assert_eq!(classify_scanned_file("jpg", &empty), Some(MediaType::Image));
    }

    /// 流式拉取辅助：把 [`MediaWalker`] 迭代到底，返回（文件集, 收尾结论）。
    fn collect(
        root: &Path,
        catalog: &CatalogSnapshot,
        cancel: &CancellationToken,
    ) -> (Vec<WalkedFile>, WalkOutcome) {
        let mut w = MediaWalker::new(root, catalog, cancel);
        let mut files = Vec::new();
        for f in w.by_ref() {
            files.push(f);
        }
        (files, w.finish())
    }

    /// 可读目录：complete==true、无错误、仅识别媒体文件（未知扩展名正常跳过、不计错误）。
    #[test]
    fn walk_complete_on_readable_dir() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("picasa_walk_ok_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = std::fs::File::create(dir.join("a.jpg")).unwrap();
        f.write_all(b"x").unwrap();
        drop(f);
        std::fs::File::create(dir.join("note.xyz")).ok(); // 未知扩展名 → 正常跳过（非错误）

        let catalog = CatalogSnapshot::builtin().unwrap();
        let cancel = CancellationToken::new();
        let (files, outcome) = collect(&dir, &catalog, &cancel);

        assert!(outcome.complete, "可读目录应完整（零遍历错误）");
        assert!(!outcome.cancelled);
        assert!(outcome.errors.is_empty(), "未知扩展名不应计入 errors");
        assert_eq!(files.len(), 1, "仅识别 a.jpg");
        assert_eq!(files[0].file_name, "a.jpg");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 不存在的根：WalkDir 首项即遍历错误 → complete==false、errors 非空、零文件。
    /// 锁住「不完整扫描 ≠ 删除」：下游差集据 complete 守门，绝不在此场景误删。
    #[test]
    fn walk_incomplete_on_unreadable_root() {
        let missing =
            std::env::temp_dir().join(format!("picasa_walk_missing_{}_nope", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);

        let catalog = CatalogSnapshot::builtin().unwrap();
        let cancel = CancellationToken::new();
        let (files, outcome) = collect(&missing, &catalog, &cancel);

        assert!(!outcome.complete, "不存在的根应不完整（遍历错误）");
        assert!(!outcome.errors.is_empty(), "应记录遍历错误而非静默丢弃");
        assert!(files.is_empty());
    }

    /// 取消令牌已触发 → 流式遍历立即停产、cancelled==true、complete==false（不完整不删）。
    #[test]
    fn walk_cancelled_is_incomplete() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("picasa_walk_cancel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = std::fs::File::create(dir.join("a.jpg")).unwrap();
        f.write_all(b"x").unwrap();
        drop(f);

        let catalog = CatalogSnapshot::builtin().unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel(); // 开扫前即取消

        let (files, outcome) = collect(&dir, &catalog, &cancel);
        assert!(files.is_empty(), "取消后不产出文件");
        assert!(outcome.cancelled, "应标记 cancelled");
        assert!(!outcome.complete, "取消即不完整 → 下游不得差集删除");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
