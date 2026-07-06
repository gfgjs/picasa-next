// src-tauri/src/ai/face_profile.rs
//! 再导出薄壳(Part4-T15):人脸模型契约与双轨注册表已迁至
//! `scrollery-ai-core::face_profile`(含 Part4-T1 合规断言测试,随迁)。
//! `face-noncommercial` feature 由 src-tauri 转发至 ai-core(单一事实源在 ai-core 的 cfg 门控)。

pub use scrollery_ai_core::face_profile::*;
