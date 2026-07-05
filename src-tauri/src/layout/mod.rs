// src-tauri/src/layout/mod.rs
pub mod cache;
// H-Lab 横向画廊实验(plan-docs/2026-07-02-horizontal-gallery-lab.md):算法 + 独立缓存,
// 与生产 justified/cache 平行,互不可见。
pub mod hcache;
pub mod horizontal;
// S1 视图取数缓存(Part2 重排提速):把「取数」从「几何」拆出,滑块/窗宽/轴切换免 SQL。
pub mod items_cache;
pub mod justified;

pub use cache::{LayoutCache, LayoutCacheData};
pub use hcache::HLayoutCache;
pub use justified::compute_justified_layout;
