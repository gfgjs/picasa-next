// src-tauri/src/ai/engine.rs
//! AI inference engine pool — wraps ort Sessions for CLIP models.
//! AI 推理引擎池 — 封装用于 CLIP 模型的 ort Session。
//!
//! Sessions are lazily initialised on first use.
//! Session 在首次使用时懒加载初始化。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ort::session::Session;
use tracing::{info, warn};

use crate::ai::provider::{AiProvider, ProviderInfo};
use crate::error::Result;

/// AI inference engine pool.
/// AI 推理引擎池。
pub struct AiEnginePool {
    /// Best detected execution provider.
    /// 已探测到的最优执行提供者。
    pub provider: AiProvider,

    /// GPU display name (empty string for CPU).
    /// GPU 显示名称（CPU 时为空字符串）。
    pub gpu_name: String,

    /// Chinese-CLIP image encoder session.
    /// Chinese-CLIP 图像编码器 Session。
    pub clip_image_session: Option<Arc<Mutex<Session>>>,

    /// Chinese-CLIP text encoder session.
    /// Chinese-CLIP 文本编码器 Session。
    pub clip_text_session: Option<Arc<Mutex<Session>>>,

    /// Phase 4B: face detection session (placeholder).
    /// 第 4B 阶段：人脸检测 Session（占位符）。
    pub face_detect_session: Option<Arc<Mutex<Session>>>,

    /// Phase 4B: face feature extraction session (placeholder).
    /// 第 4B 阶段：人脸特征提取 Session（占位符）。
    pub face_embed_session: Option<Arc<Mutex<Session>>>,
}

impl AiEnginePool {
    /// Initialise the engine pool from the given models directory.
    /// 从给定的模型目录初始化引擎池。
    pub fn init(models_dir: &Path) -> Result<Self> {
        let probe_path = models_dir.join("probe.onnx");
        let image_path = models_dir.join("cn-clip-vit-b16-image.onnx");
        let text_path  = models_dir.join("cn-clip-vit-b16-text.onnx");

        // ── Step 1: provider detection ──────────────────────────────────────
        // ── 步骤 1：提供者探测 ──────────────────────────────────────
        info!("Starting AI provider detection | 开始 AI 提供者探测...");
        let ProviderInfo { provider, gpu_name } =
            crate::ai::provider::detect_provider(&probe_path);
        info!(
            "AI provider ready: {} ({}) | AI 提供者就绪: {} ({})",
            provider.label(), gpu_name, provider.label(), gpu_name
        );

        // ── Step 2: load CLIP models ────────────────────────────────────────
        // ── 步骤 2：加载 CLIP 模型 ──────────────────────────────────────
        let clip_image_session = load_session(&image_path, &provider, "CLIP image encoder | CLIP 图像编码器");
        let clip_text_session  = load_session(&text_path,  &provider, "CLIP text encoder  | CLIP 文本编码器");

        Ok(Self {
            provider,
            gpu_name,
            clip_image_session,
            clip_text_session,
            face_detect_session: None,
            face_embed_session: None,
        })
    }

    /// Returns `true` if the CLIP image encoder is loaded.
    /// 返回 `true` 如果 CLIP 图像编码器已加载。
    pub fn clip_image_ready(&self) -> bool {
        self.clip_image_session.is_some()
    }

    /// Returns `true` if both CLIP encoders are loaded.
    /// 返回 `true` 如果两个 CLIP 编码器都已加载。
    pub fn clip_ready(&self) -> bool {
        self.clip_image_session.is_some() && self.clip_text_session.is_some()
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────
// ── 辅助函数 ──────────────────────────────────────────────────────────────────

/// Load a single ONNX session with the given provider. Returns `None` and logs a warning on failure.
/// 使用给定提供者加载单个 ONNX Session。失败时返回 `None` 并记录警告。
fn load_session(
    model_path: &PathBuf,
    provider: &AiProvider,
    label: &str,
) -> Option<Arc<Mutex<Session>>> {
    if !model_path.exists() {
        warn!(
            "Model file not found, skipping {} | 模型文件未找到，跳过 {}: {:?}",
            label, label, model_path
        );
        return None;
    }

    info!("Loading {} | 正在加载 {}: {:?}", label, label, model_path);

    let result = build_session(model_path, provider);

    match result {
        Ok(session) => {
            info!("Loaded {} successfully | {} 加载成功", label, label);
            Some(Arc::new(Mutex::new(session)))
        }
        Err(e) => {
            warn!(
                "Failed to load {}, AI feature degraded | {} 加载失败，AI 功能降级: {}",
                label, label, e
            );
            None
        }
    }
}

/// Build a Session with the appropriate EP for the selected provider.
/// 使用选定提供者对应的 EP 构建 Session。
fn build_session(model_path: &PathBuf, provider: &AiProvider) -> ort::Result<Session> {
    macro_rules! with_ep {
        ($ep:expr) => {{
            let b = Session::builder()?.with_intra_threads(4)?;
            let mut b = b.with_execution_providers([$ep])?;
            b.commit_from_file(model_path)
        }};
    }

    match provider {
        #[cfg(target_os = "windows")]
        AiProvider::DirectML => {
            with_ep!(ort::ep::DirectML::default().build())
        }
        AiProvider::CUDA => {
            with_ep!(ort::ep::CUDA::default().build())
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        AiProvider::CoreML => {
            with_ep!(ort::ep::CoreML::default().build())
        }
        AiProvider::OpenVINO => {
            with_ep!(ort::ep::OpenVINO::default().build())
        }
        _ => {
            // CPU — no special EP needed | CPU — 无需特殊 EP
            let mut b = Session::builder()?.with_intra_threads(4)?;
            b.commit_from_file(model_path)
        }
    }
}

impl std::fmt::Debug for AiEnginePool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiEnginePool")
            .field("provider", &self.provider)
            .field("gpu_name", &self.gpu_name)
            .field("clip_image_ready", &self.clip_image_session.is_some())
            .field("clip_text_ready", &self.clip_text_session.is_some())
            .finish()
    }
}
