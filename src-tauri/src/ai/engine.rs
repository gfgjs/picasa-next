// src-tauri/src/ai/engine.rs
//! AI inference engine pool — wraps ort Sessions for CLIP models.
//! AI 推理引擎池 — 封装用于 CLIP 模型的 ort Session。
//!
//! Sessions are lazily initialised on first use.
//! Session 在首次使用时懒加载初始化。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use tracing::{info, warn};

use crate::ai::provider::{AiProvider, ProviderInfo};
use crate::error::Result;

/// Timeout for session loading.
///
/// CPU loading a 330 MB fp32 ViT-B/16 model with ORT graph optimization takes 2–5 minutes
/// on first load (no caching). DirectML hangs are infinite and will still be caught.
/// We use 10 minutes to be safe on slow machines.
///
/// 会话加载超时时间：330 MB fp32 ViT-B/16 模型在 CPU 上首次加载（ORT 图优化）需要 2–5 分钟。
/// DirectML 卡死是无限期的，10 分钟内仍会被捕获。
const SESSION_LOAD_TIMEOUT: Duration = Duration::from_secs(600);


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
    pub fn init(models_dir: &Path, image_model_name: &str, text_model_name: &str, provider_override: &str) -> Result<Self> {
        let image_path = models_dir.join(image_model_name);
        let text_path  = models_dir.join(text_model_name);

        // ── Step 1: provider detection ──────────────────────────────────────
        // ── 步骤 1：提供者探测 ──────────────────────────────────────
        info!("Starting AI provider detection | 开始 AI 提供者探测...");
        let mut provider_info = crate::ai::provider::detect_best_provider();
        
        if provider_override == "cpu" {
            info!("User override: Forcing CPU | 用户强制指定：使用 CPU");
            provider_info.provider = AiProvider::Cpu;
            provider_info.gpu_name = String::new();
        }

        // ── Step 2: load CLIP models ────────────────────────────────────────
        // ── 步骤 2：加载 CLIP 模型 ──────────────────────────────────────
        let mut clip_image_session = load_session(&image_path, &provider_info.provider, "CLIP image encoder | CLIP 图像编码器");

        // Fallback to CPU if GPU failed to load the image encoder
        if clip_image_session.is_none() && provider_info.provider != AiProvider::Cpu {
            tracing::warn!("GPU Execution Provider failed to initialize, falling back to CPU.");
            provider_info.provider = AiProvider::Cpu;
            provider_info.gpu_name = String::new();
            clip_image_session = load_session(&image_path, &AiProvider::Cpu, "CLIP image encoder (CPU) | CLIP 图像编码器 (CPU)");
        }

        let clip_text_session = load_session(&text_path, &provider_info.provider, "CLIP text encoder | CLIP 文本编码器");

        info!(
            "AI provider ready: {} ({}) | AI 提供者就绪: {} ({})",
            provider_info.provider.label(), provider_info.gpu_name, provider_info.provider.label(), provider_info.gpu_name
        );

        Ok(Self {
            provider: provider_info.provider,
            gpu_name: provider_info.gpu_name,
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

/// Load a single ONNX session with the given provider.
/// Uses a background thread with timeout to detect hangs (DirectML shader compilation deadlock).
/// 使用给定提供者加载单个 ONNX Session。
/// 使用带超时的后台线程检测卡死情况（DirectML shader 编译死锁）。
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

    info!(
        "Loading {} with provider {} | 正在用 {} 加载 {}: {:?}",
        label, provider.label(), provider.label(), label, model_path
    );

    // Spawn session loading in a dedicated thread with a timeout.
    // This guards against DirectML / shader-compilation hangs.
    // 在专用线程中加载 session 并设置超时，防止 DirectML 编译 shader 时无限期卡死。
    let path_clone = model_path.clone();
    let provider_clone = provider.clone();
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = build_session(&path_clone, &provider_clone);
        // Ignore send errors if the receiver timed out and dropped
        // 如果接收方已超时退出，忽略发送错误
        let _ = tx.send(result);
    });

    match rx.recv_timeout(SESSION_LOAD_TIMEOUT) {
        Ok(Ok(session)) => {
            info!("Loaded {} successfully | {} 加载成功", label, label);
            Some(Arc::new(Mutex::new(session)))
        }
        Ok(Err(e)) => {
            warn!(
                "Failed to load {}, AI feature degraded | {} 加载失败，AI 功能降级: {}",
                label, label, e
            );
            None
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            warn!(
                "Timeout loading {} after {:?} — provider {} likely hung on shader/graph compilation. \
                 | {} 加载超时（{:?}），提供者 {} 可能在 shader/图优化阶段卡死。",
                label, SESSION_LOAD_TIMEOUT, provider.label(),
                label, SESSION_LOAD_TIMEOUT, provider.label()
            );
            None
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            warn!("Session loader thread panicked while loading {} | 加载 {} 时 Session 加载线程崩溃", label, label);
            None
        }
    }
}

/// Build a Session with the appropriate EP for the selected provider.
/// Key DirectML constraints applied:
///   - `with_intra_threads(1)` — DirectML requires single-threaded session
///   - `disable_mem_pattern()` — DirectML does not support memory pattern optimization
///   - `with_optimization_level(Level1)` — Only Basic graph optimization to avoid
///     expensive shader pre-compilation that hangs on ViT/transformer models
/// 使用选定提供者对应的 EP 构建 Session。
/// DirectML 必须满足的约束：
///   - 单线程（intra_threads=1）
///   - 禁用内存模式优化
///   - 只使用 Basic 图优化（避免 ViT 模型 shader 预编译导致的无限期卡死）
fn build_session(model_path: &PathBuf, provider: &AiProvider) -> ort::Result<Session> {
    match provider {
        #[cfg(target_os = "windows")]
        AiProvider::DirectML => {
            // DirectML requires sequential execution and no memory pattern.
            // Graph optimization must be LIMITED to Basic (Level1) — ORT_ENABLE_ALL (Level3)
            // triggers full DML shader pre-compilation which hangs indefinitely on
            // ViT/transformer models with complex attention or Int64 ops.
            // DirectML 需要顺序执行且不能使用内存模式优化。
            // 图优化级别必须限制为 Basic（Level1）——Level3 会触发完整的 DML shader 预编译，
            // 在含有复杂 Attention 或 Int64 算子的 ViT/Transformer 模型上会无限期卡死。
            let mut b = Session::builder()?
                .with_intra_threads(1)?
                .with_inter_threads(1)?
                .with_parallel_execution(false)?
                .with_optimization_level(GraphOptimizationLevel::Level1)?
                .with_memory_pattern(false)?
                .with_execution_providers([ort::ep::DirectML::default().build()])?
                ;
            b.commit_from_file(model_path)
        }
        AiProvider::CUDA => {
            let mut b = Session::builder()?
                .with_intra_threads(4)?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_execution_providers([ort::ep::CUDA::default().build()])?
                ;
            b.commit_from_file(model_path)
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        AiProvider::CoreML => {
            let mut b = Session::builder()?
                .with_intra_threads(4)?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_execution_providers([ort::ep::CoreML::default().build()])?
                ;
            b.commit_from_file(model_path)
        }
        AiProvider::OpenVINO => {
            let mut b = Session::builder()?
                .with_intra_threads(4)?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_execution_providers([ort::ep::OpenVINO::default().build()])?
                ;
            b.commit_from_file(model_path)
        }
        _ => {
            // CPU path — use Level1 (Basic) optimization for faster session creation.
            //
            // Level3 (ORT_ENABLE_ALL) runs expensive graph fusion and layout passes on the full
            // 330 MB fp32 ViT-B/16 graph, which can take 5–10 minutes on first load with no caching.
            // Level1 (ORT_ENABLE_BASIC) = constant folding + dead node elimination only,
            // creating the session in seconds with minimal inference performance impact for CPU fp32.
            //
            // CPU 路径 — 使用 Level1（Basic）图优化以加快 Session 创建速度。
            // Level3 对 330 MB fp32 ViT-B/16 图执行完整的图融合和布局变换，首次加载可能耗时 5–10 分钟。
            // Level1 仅执行常量折叠和死节点消除，Session 创建时间为秒级，对 CPU fp32 推理性能影响极小。
            let mut b = Session::builder()?
                .with_intra_threads(4)?
                .with_optimization_level(GraphOptimizationLevel::Level1)?
                ;
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
