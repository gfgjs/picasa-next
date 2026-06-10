// src-tauri/src/ai/engine.rs
//! AI inference engine pool — wraps ort Sessions for CLIP models.
//! AI 推理引擎池 — 封装用于 CLIP 模型的 ort Session。
//!
//! # 踩坑记录（2026-06-03）
//!
//! ## 坑1：ort crate 的 `load-dynamic` 与 `download-binaries` 互斥
//! `load-dynamic` feature 会激活 `ort-sys/disable-linking`，
//! 导致 build.rs 提前返回，`download-binaries` **完全不运行**。
//! **后果**：即使在 Cargo.toml 同时写了两个 feature，DLL 也不会自动下载。
//! **应对**：必须手动管理 DLL，并通过 ORT_DYLIB_PATH 指定路径。
//!
//! ## 坑2：ORT DLL 版本必须 ≥ 1.19（ONNX IR v10 要求）
//! Chinese-CLIP ViT-B/16 模型用 PyTorch 2.11 导出，ONNX IR version = 10。
//! ORT 1.17（旧版/WebView2 System32 自带版本）不支持 IR v10，
//! 会在 CreateSession 时报 "model IR version is higher than supported" 错误，
//! 或在某些路径下**直接无限卡死不报错**。
//!
//! ## 坑3：ORT 版本与 FP16 外部数据格式兼容性
//! - ORT 1.21：无法加载 eisneim/cn-clip_vit-b-16 的 FP16 外部数据格式
//!   （`.onnx` + `.extra_file`），在 `disabled` 和 `Level1` 图优化下均**无限卡死**。
//!   Node.js ORT 1.26 在相同模型 421ms 内快速失败并报 `GetIndexFromName` 错误。
//! - ORT 1.26：正常加载，Level1 优化下 ~200ms。
//! **应对**：从 `onnxruntime-node@1.26.0` 的 `bin/napi-v6/win32/x64/` 中复制 DLL。
//!
//! ## 坑4：FP16 模型在 disabled 图优化下的类型错误
//! `GraphOptimizationLevel::Disable` 下加载 FP16 模型会报：
//! "Type (tensor(float)) of output arg (InsertedPrecisionFreeCast_...) does not match
//! expected type (tensor(float16))"
//! **原因**：FP16 模型内部有 ORT 插入的 PrecisionFreeCast 节点，这些节点依赖
//! Level1+ 的 SimplifiedLayerNormFusion 优化才能正确处理类型。
//! **应对**：必须使用 `Level1`（Basic）或更高级别，**不能用 Disable**。
//!
//! ## 坑5：单体 fp32 ONNX 格式（330MB）在 CPU 上极慢
//! ORT 加载单体格式时必须一次性反序列化整个 Protobuf，
//! 即使 `GraphOptimizationLevel::Disable`，330MB 文件也需要 >5 分钟。
//! **应对**：使用外部数据格式（`.onnx` header + `.extra_file` 权重），
//! ORT 通过内存映射按需读取权重，Session 创建只需解析小 header 文件。
//!
//! ## 坑6：CPU 路径不能用 Level3 图优化
//! Level3（ORT_ENABLE_ALL）对 330MB ViT-B/16 图执行完整图融合和布局变换，
//! 首次加载可能需要 5–10 分钟（无缓存）。
//! **应对**：CPU 路径使用 Level1（Basic）= 常量折叠 + 死节点消除，
//! Session 创建时间为秒级，推理性能影响极小。
//!
//! ## 坑7：新旧模型的张量 I/O 接口完全不同
//! - 旧模型（cn-clip-vit-b16-*.onnx）:
//!   图像: `pixel_values: f32[1,3,224,224]` → `image_features: f32[1,512]`（已L2归一化）
//!   文本: `input_ids + attention_mask + token_type_ids: i64[1,52]` → `text_features`
//! - 新模型（eisneim/cn-clip_vit-b-16）:
//!   图像: `image: f32[1,3,224,224]` → `unnorm_image_features: f32[1,512]`（未归一化！）
//!   文本: `text: i64[1,52]`（仅 token IDs）→ `unnorm_text_features: f32[1,512]`
//! **应对**：推理后必须手动 L2 归一化；文本编码器不再需要 attention_mask/token_type_ids。
//!
//! ## 坑8（致命）：vocab.txt 与模型不匹配 → 准确率降为随机
//! `eisneim/cn-clip_vit-b-16` 仓库附带的 vocab.txt 是**英文 CLIP 的 BPE 词表**
//! （~5594 tokens），而非 Chinese-CLIP 需要的 **bert-base-chinese 词表**
//! （21128 tokens）。误用导致所有中文字符被编码为 [UNK]，
//! 任何查询产生完全相同的嵌入向量（cosine=1.0），搜索准确率降至随机水平。
//! **应对**：从模型原始作者 `OFA-Sys/chinese-clip-vit-base-patch16` 获取 vocab.txt；
//! 加载后校验 vocab_size ≥ 10000，否则立即报错。
//! **详细记录**：见 `clip.rs` 模块头部的踩坑记录。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use tracing::{info, warn};
use crossbeam_channel::{bounded, Receiver, Sender};

use crate::ai::clip::ClipTokenizer;
use crate::ai::provider::AiProvider;
use crate::error::Result;

/// A thread-safe pool of ONNX Runtime Sessions.
/// 用于解决 ort rc.12 中 Session::run 需要 &mut self 导致的串行瓶颈。
#[derive(Clone)]
pub struct SessionPool {
    rx: Receiver<Session>,
    tx: Sender<Session>,
}

impl SessionPool {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = bounded(capacity);
        Self { rx, tx }
    }

    pub fn push(&self, session: Session) {
        let _ = self.tx.send(session);
    }

    pub fn get(&self) -> Option<SessionGuard> {
        // Block until a session is available
        match self.rx.recv() {
            Ok(session) => Some(SessionGuard {
                session: Some(session),
                tx: self.tx.clone(),
            }),
            Err(e) => {
                tracing::error!("Session pool channel disconnected: {}", e);
                None
            }
        }
    }
}

/// A RAII guard that automatically returns the Session to the pool on drop.
pub struct SessionGuard {
    session: Option<Session>,
    tx: Sender<Session>,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = self.tx.send(session);
        }
    }
}

impl std::ops::Deref for SessionGuard {
    type Target = Session;
    fn deref(&self) -> &Self::Target {
        self.session.as_ref().expect("SessionGuard accessed after drop")
    }
}

impl std::ops::DerefMut for SessionGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.session.as_mut().expect("SessionGuard accessed after drop")
    }
}

impl std::fmt::Debug for SessionPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionPool").field("available", &self.rx.len()).finish()
    }
}

/// 会话加载超时时间。
///
/// FP16 外部数据格式（eisneim/cn-clip_vit-b-16）在 ORT 1.26 + CPU + Level1 下
/// 加载仅需 ~200ms；设 600s 超时是为了应对极端情况（NAS/慢速 HDD）或
/// DirectML shader 编译（DirectML 卡死是无限期的，600s 内会被捕获）。
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

    /// Chinese-CLIP image encoder session pool.
    /// Chinese-CLIP 图像编码器 Session 池。
    pub clip_image_session: Option<SessionPool>,

    /// Chinese-CLIP text encoder session pool.
    /// Chinese-CLIP 文本编码器 Session 池。
    pub clip_text_session: Option<SessionPool>,

    /// Cached BERT tokenizer (loaded from vocab.txt).
    /// 缓存的 BERT 分词器（从 vocab.txt 加载）。
    /// Avoids re-reading vocab.txt on every semantic search call.
    /// 避免每次语义搜索都重新读取 vocab.txt。
    pub clip_tokenizer: Option<ClipTokenizer>,

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

        let pool_size = match provider_info.provider {
            AiProvider::Cpu => std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4),
            _ => 1, // GPU providers: DirectML/CUDA drivers handle internal concurrency; multiple sessions cause severe DX12 lock contention
        };

        // ── Step 2: load CLIP models ────────────────────────────────────────
        // ── 步骤 2：加载 CLIP 模型 ──────────────────────────────────────
        let mut clip_image_session = load_session_pool(&image_path, &provider_info.provider, "CLIP image encoder | CLIP 图像编码器", pool_size);

        let mut clip_text_session = load_session_pool(&text_path, &provider_info.provider, "CLIP text encoder | CLIP 文本编码器", pool_size);

        // Fallback to CPU if GPU failed to load EITHER encoder
        if (clip_image_session.is_none() || clip_text_session.is_none()) && provider_info.provider != AiProvider::Cpu {
            tracing::warn!("GPU acceleration failed for one or more models, falling back to CPU for both... | GPU 加速失败，正在将两者统一回退至 CPU...");
            provider_info.provider = AiProvider::Cpu;
            provider_info.gpu_name = String::new();
            
            // Clear any partially loaded GPU sessions
            clip_image_session = None;
            clip_text_session = None;
            
            let cpu_pool_size = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
            clip_image_session = load_session_pool(&image_path, &AiProvider::Cpu, "CLIP image encoder (CPU) | CLIP 图像编码器 (CPU)", cpu_pool_size);
            clip_text_session = load_session_pool(&text_path, &AiProvider::Cpu, "CLIP text encoder (CPU) | CLIP 文本编码器 (CPU)", cpu_pool_size);
        }

        info!(
            "AI provider ready: {} ({}) | AI 提供者就绪: {} ({})",
            provider_info.provider.label(), provider_info.gpu_name, provider_info.provider.label(), provider_info.gpu_name
        );

        Ok(Self {
            provider: provider_info.provider,
            gpu_name: provider_info.gpu_name,
            clip_image_session,
            clip_text_session,
            clip_tokenizer: None,  // loaded lazily from models_dir in ai_commands
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

fn load_session_pool(
    model_path: &PathBuf,
    provider: &AiProvider,
    label: &str,
    pool_size: usize,
) -> Option<SessionPool> {
    if !model_path.exists() {
        warn!(
            "Model file not found, skipping {} | 模型文件未找到，跳过 {}: {:?}",
            label, label, model_path
        );
        return None;
    }

    info!(
        "Loading {} (pool size: {}) with provider {} | 正在用 {} 加载 {} (容量: {}): {:?}",
        label, pool_size, provider.label(), provider.label(), label, pool_size, model_path
    );

    let pool = SessionPool::new(pool_size);

    for i in 0..pool_size {
        let path_clone = model_path.clone();
        let provider_clone = provider.clone();
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let result = build_session(&path_clone, &provider_clone);
            let _ = tx.send(result);
        });

        match rx.recv_timeout(SESSION_LOAD_TIMEOUT) {
            Ok(Ok(session)) => {
                if i == 0 {
                    let input_names: Vec<&str> = session.inputs().iter().map(|i| i.name()).collect();
                    let output_names: Vec<&str> = session.outputs().iter().map(|o| o.name()).collect();
                    info!(
                        "Loaded {} [1/{}] — inputs: {:?}, outputs: {:?} | {} 加载成功 [1/{}] — 输入: {:?}, 输出: {:?}",
                        label, pool_size, input_names, output_names,
                        label, pool_size, input_names, output_names
                    );
                } else {
                    info!("Loaded {} [{}/{}] | {} 加载成功 [{}/{}]", label, i + 1, pool_size, label, i + 1, pool_size);
                }
                pool.push(session);
            }
            Ok(Err(e)) => {
                warn!("Failed to load {} [{}/{}], AI feature degraded | {} 加载失败 [{}/{}], AI 功能降级: {}", label, i + 1, pool_size, label, i + 1, pool_size, e);
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                warn!("Timeout loading {} [{}/{}] after {:?} | {} 加载超时 [{}/{}] ({:?})", label, i + 1, pool_size, SESSION_LOAD_TIMEOUT, label, i + 1, pool_size, SESSION_LOAD_TIMEOUT);
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                warn!("Session loader thread panicked while loading {} [{}/{}] | 加载 {} 时 Session 加载线程崩溃 [{}/{}]", label, i + 1, pool_size, label, i + 1, pool_size);
                break;
            }
        }
    }

    if pool.rx.is_empty() {
        None
    } else {
        if pool.rx.len() < pool_size {
            warn!("{} pool loaded with degraded capacity: {}/{}", label, pool.rx.len(), pool_size);
        }
        Some(pool)
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
                .with_intra_threads(1)?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_execution_providers([ort::ep::CUDA::default().build()])?
                ;
            b.commit_from_file(model_path)
        }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        AiProvider::CoreML => {
            let mut b = Session::builder()?
                .with_intra_threads(1)?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_execution_providers([ort::ep::CoreML::default().build()])?
                ;
            b.commit_from_file(model_path)
        }
        AiProvider::OpenVINO => {
            let mut b = Session::builder()?
                .with_intra_threads(1)?
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
            // CPU 路径 — 既然我们外层使用了多 Session 实例并行，
            // 内部必须强制限制单线程（with_intra_threads(1)），否则会产生 N*N 级别的线程风暴，导致 CPU 剧烈颠簸反而变慢。
            let mut b = Session::builder()?
                .with_intra_threads(1)?
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
