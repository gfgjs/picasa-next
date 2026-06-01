// src-tauri/src/thumbnail/mod.rs
pub mod cache;
pub mod exif_thumb;
pub mod generator;
pub mod thumbhash;

pub use generator::generate_thumbnail;
