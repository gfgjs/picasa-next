// src-tauri/src/engine/gpu/mod.rs
pub mod wic_engine;

use crate::engine::traits::ImageEngine;
use wic_engine::WicEngine;

/// Factory to get a GPU engine by name
pub fn get_gpu_engine(name: &str) -> Option<Box<dyn ImageEngine>> {
    match name {
        "wic" => Some(Box::new(WicEngine)),
        // Future GPU engines can be added here (e.g., nvjpeg, dxva, etc.)
        _ => None,
    }
}
