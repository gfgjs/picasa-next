// src-tauri/src/scanner/mod.rs
pub mod enricher;
pub mod fast_scan;
pub mod live_photo;
pub mod metadata;
pub mod walker;
// pub mod watcher; // Phase 3

pub use fast_scan::run_fast_scan;
pub use enricher::run_enrichment;
