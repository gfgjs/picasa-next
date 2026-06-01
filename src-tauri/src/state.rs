// src-tauri/src/state.rs
//! Application state shared across all Tauri commands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tokio_util::sync::CancellationToken;

use crate::db::{DbPool, DbWriter};
use crate::engine::EngineArena;
use crate::layout::LayoutCache;
use crate::layout::cache::new_layout_cache;
use crate::thumbnail::generator::ThumbConfig;

/// Global application state.
pub struct AppState {
    /// Write connection — serialised via Mutex.
    pub db_writer: DbWriter,

    /// Read connection pool (WAL concurrent reads).
    pub db_read_pool: DbPool,

    /// Per-root cancellation tokens for scan operations.
    pub scan_tokens: Mutex<HashMap<i64, CancellationToken>>,

    /// In-memory Justified Layout cache.
    pub layout_cache: LayoutCache,

    /// Image engine arena (format → engine dispatch).
    pub engine_arena: EngineArena,

    /// Thumbnail configuration (cache dir, size, skip threshold).
    pub thumb_config: ThumbConfig,
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
            thumb_config: ThumbConfig {
                cache_dir,
                size: thumb_size,
                skip_max_bytes: thumb_skip_max_kb * 1024,
            },
        }
    }

    /// Create a new cancellation token for a scan root, replacing any existing one.
    pub fn new_scan_token(&self, root_id: i64) -> CancellationToken {
        let token = CancellationToken::new();
        self.scan_tokens
            .lock()
            .unwrap()
            .insert(root_id, token.clone());
        token
    }

    /// Cancel the scan token for a root, if it exists.
    pub fn cancel_scan(&self, root_id: i64) {
        if let Some(token) = self.scan_tokens.lock().unwrap().remove(&root_id) {
            token.cancel();
        }
    }

    /// Cancel all running scans.
    pub fn cancel_all_scans(&self) {
        let mut map = self.scan_tokens.lock().unwrap();
        for token in map.values() {
            token.cancel();
        }
        map.clear();
    }
}
