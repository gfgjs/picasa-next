// src-tauri/src/ai/profile.rs
//! 再导出薄壳(Part4-T15):模型契约(`ModelProfile`)与架构注册表已迁至
//! `scrollery-ai-core::profile`——ai-worker 按 SessionInit 的 `arch_id` 查同一份
//! 内建注册表(Part6 §3.2.1a:host 不逐字段下发),故注册表必须落共享 crate。

pub use scrollery_ai_core::profile::*;
