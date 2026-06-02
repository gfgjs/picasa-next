// src-tauri/src/state.rs
// src-tauri/src/state.rs
//! Application state shared across all Tauri commands.
//! 在所有 Tauri 命令之间共享的应用程序状态。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use tokio_util::sync::CancellationToken;

use crate::db::{DbPool, DbWriter};
use crate::engine::EngineArena;
use crate::layout::LayoutCache;
use crate::layout::cache::new_layout_cache;
use crate::thumbnail::generator::ThumbConfig;

/// Global application state.
/// 全局应用程序状态。
pub struct AppState {
    /// Write connection — serialised via Mutex.
    /// 写入连接 — 通过 Mutex 序列化。
    pub db_writer: DbWriter,

    /// Read connection pool (WAL concurrent reads).
    /// 读取连接池（WAL 并发读取）。
    pub db_read_pool: DbPool,

    /// Per-root cancellation tokens for scan operations.
    /// 用于扫描操作的每个根目录的取消令牌。
    pub scan_tokens: Mutex<HashMap<i64, CancellationToken>>,

    /// In-memory Justified Layout cache.
    /// 内存中的两端对齐布局缓存。
    pub layout_cache: LayoutCache,

    /// Image engine arena (format → engine dispatch).
    /// 图像引擎容器（格式 → 引擎分发）。
    pub engine_arena: EngineArena,

    /// Thumbnail configuration (cache dir, size, skip threshold).
    /// 缩略图配置（缓存目录、大小、跳过阈值）。
    pub thumb_config: RwLock<ThumbConfig>,
}

impl AppState {
    pub fn new(
        db_writer: DbWriter,
        db_read_pool: DbPool,
        app_data_dir: PathBuf,
        thumb_size: u32,
        thumb_skip_max_kb: u64,
    ) -> Self {
        let cache_dir = app_data_dir.join("cache");

        Self {
            db_writer,
            db_read_pool,
            scan_tokens: Mutex::new(HashMap::new()),
            layout_cache: new_layout_cache(),
            engine_arena: EngineArena::phase1(),
            thumb_config: RwLock::new(ThumbConfig {
                cache_dir,
                size: thumb_size,
                skip_max_bytes: thumb_skip_max_kb * 1024,
            }),
        }
    }

    /// Create a new cancellation token for a scan root, replacing any existing one.
    /// 为扫描根目录创建一个新的取消令牌，替换任何现有的令牌。
    pub fn new_scan_token(&self, root_id: i64) -> CancellationToken {
        let token = CancellationToken::new();
        self.scan_tokens
            .lock()
            .unwrap()
            .insert(root_id, token.clone());
        token
    }

    /// Cancel the scan token for a root, if it exists.
    /// 取消根目录的扫描令牌（如果存在）。
    pub fn cancel_scan(&self, root_id: i64) {
        if let Some(token) = self.scan_tokens.lock().unwrap().remove(&root_id) {
            token.cancel();
        }
    }

    /// Cancel all running scans.
    /// 取消所有正在运行的扫描。
    pub fn cancel_all_scans(&self) {
        let mut map = self.scan_tokens.lock().unwrap();
        for token in map.values() {
            token.cancel();
        }
        map.clear();
    }
}
