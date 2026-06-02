// src-tauri/src/engine/mod.rs
// src-tauri/src/engine/mod.rs
//! Media decode engine registry (EngineArena).
//! 媒体解码引擎注册表（EngineArena）。

pub mod image_rs;
pub mod traits;
pub mod gpu;
// pub mod heic; // Phase 2
// pub mod heic; // 阶段 2
// pub mod raw;  // Phase 2
// pub mod raw;  // 阶段 2

pub use traits::{DecodedImage, ImageEngine};

use std::sync::Arc;
use crate::engine::image_rs::ImageRsEngine;

/// The engine arena dispatches decoding to the appropriate engine based on file format.
/// 引擎竞技场根据文件格式将解码分派给适当的引擎。
pub struct EngineArena {
    engines: Vec<Arc<dyn ImageEngine>>,
}

impl EngineArena {
    /// Build the Phase 1 arena.
    /// 构建阶段 1 的竞技场。
    pub fn phase1() -> Self {
        Self {
            engines: vec![Arc::new(ImageRsEngine)],
        }
    }

    /// Find an engine for the given format. Returns `None` if unsupported.
    /// 为给定格式查找引擎。如果不被支持，则返回 `None`。
    pub fn engine_for(&self, format: &str) -> Option<Arc<dyn ImageEngine>> {
        self.engines
            .iter()
            .find(|e| e.can_handle(format))
            .cloned()
    }
}
