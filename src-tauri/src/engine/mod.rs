// src-tauri/src/engine/mod.rs
//! Media decode engine registry (EngineArena).

pub mod image_rs;
pub mod traits;
// pub mod heic; // Phase 2
// pub mod raw;  // Phase 2

pub use traits::{DecodedImage, ImageEngine};

use std::sync::Arc;
use crate::engine::image_rs::ImageRsEngine;

/// The engine arena dispatches decoding to the appropriate engine based on file format.
pub struct EngineArena {
    engines: Vec<Arc<dyn ImageEngine>>,
}

impl EngineArena {
    /// Build the Phase 1 arena.
    pub fn phase1() -> Self {
        Self {
            engines: vec![Arc::new(ImageRsEngine)],
        }
    }

    /// Find an engine for the given format. Returns `None` if unsupported.
    pub fn engine_for(&self, format: &str) -> Option<Arc<dyn ImageEngine>> {
        self.engines
            .iter()
            .find(|e| e.can_handle(format))
            .cloned()
    }
}
