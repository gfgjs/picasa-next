// src-tauri/src/ai/mod.rs
//! AI inference module — CLIP semantic search + (Phase 4B) face recognition.
//! AI 推理模块 — CLIP 语义搜索 + （第 4B 阶段）人脸识别。

pub mod clip;
pub mod face;
pub mod face_cluster;
pub mod face_pipeline;
pub mod face_profile;
pub mod pipeline;
pub mod profile;
pub mod provider;
pub mod remote_registry;
pub mod search;
pub mod vector_store;
pub mod worker_client;
pub mod worker_pipeline;
