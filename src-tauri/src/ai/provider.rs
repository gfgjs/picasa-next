// src-tauri/src/ai/provider.rs
//! AI hardware provider detection and selection.
//! AI 硬件加速后端探测与选择。
//!
//! Detection order: DirectML → CUDA → CoreML → OpenVINO → CPU
//! 探测顺序：DirectML → CUDA → CoreML → OpenVINO → CPU

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Supported AI execution providers.
/// 支持的 AI 执行提供者。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    /// Windows DirectML (GPU-agnostic, AMD/NVIDIA/Intel)
    DirectML,
    /// NVIDIA CUDA
    CUDA,
    /// Apple CoreML (macOS/iOS)
    CoreML,
    /// Intel OpenVINO
    OpenVINO,
    /// CPU fallback
    #[default]
    Cpu,
}

impl AiProvider {
    pub fn label(&self) -> &'static str {
        match self {
            AiProvider::DirectML  => "DirectML (GPU)",
            AiProvider::CUDA      => "CUDA (NVIDIA GPU)",
            AiProvider::CoreML    => "CoreML (Apple)",
            AiProvider::OpenVINO  => "OpenVINO (Intel)",
            AiProvider::Cpu       => "CPU",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AiProvider::DirectML  => "directml",
            AiProvider::CUDA      => "cuda",
            AiProvider::CoreML    => "coreml",
            AiProvider::OpenVINO  => "openvino",
            AiProvider::Cpu       => "cpu",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "directml"  => AiProvider::DirectML,
            "cuda"      => AiProvider::CUDA,
            "coreml"    => AiProvider::CoreML,
            "openvino"  => AiProvider::OpenVINO,
            _           => AiProvider::Cpu,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub provider: AiProvider,
    pub gpu_name: String,
}

/// Detect the best available execution provider.
/// 探测最优的执行提供者。
pub fn detect_provider(probe_model_path: &std::path::Path) -> ProviderInfo {
    if !probe_model_path.exists() {
        warn!("Probe model not found at {:?}, falling back to CPU | 探针模型未找到，回退到 CPU", probe_model_path);
        return ProviderInfo { provider: AiProvider::Cpu, gpu_name: String::new() };
    }

    #[cfg(target_os = "windows")]
    {
        if try_ep_directml(probe_model_path) {
            let gpu_name = detect_gpu_name_windows();
            info!("AI provider selected: DirectML | AI 提供者选择：DirectML, GPU: {}", gpu_name);
            return ProviderInfo { provider: AiProvider::DirectML, gpu_name };
        }
    }

    if try_ep_cuda(probe_model_path) {
        info!("AI provider selected: CUDA | AI 提供者选择：CUDA");
        return ProviderInfo { provider: AiProvider::CUDA, gpu_name: "NVIDIA GPU".to_string() };
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        if try_ep_coreml(probe_model_path) {
            info!("AI provider selected: CoreML | AI 提供者选择：CoreML");
            return ProviderInfo { provider: AiProvider::CoreML, gpu_name: "Apple Silicon".to_string() };
        }
    }

    if try_ep_openvino(probe_model_path) {
        info!("AI provider selected: OpenVINO | AI 提供者选择：OpenVINO");
        return ProviderInfo { provider: AiProvider::OpenVINO, gpu_name: "Intel GPU/CPU".to_string() };
    }

    info!("AI provider selected: CPU (fallback) | AI 提供者选择：CPU（回退）");
    ProviderInfo { provider: AiProvider::Cpu, gpu_name: String::new() }
}

/// Run a probe inference to check if the EP works.
/// 运行探针推理以检查 EP 是否可用。
fn run_probe(mut session: ort::session::Session) -> bool {
    let flat: Vec<f32> = vec![0.0f32; 1 * 3 * 32 * 32];
    ort::value::Tensor::from_array(([1i64, 3, 32, 32], flat))
        .and_then(|tensor| session.run(ort::inputs!["pixel_values" => tensor]))
        .is_ok()
}

/// Build probe session helper — wraps the ? coercion issue with with_execution_providers.
/// 构建探针 session 辅助函数 — 处理 with_execution_providers 的 ? 强制转换问题。
macro_rules! build_probe {
    ($model:expr, $ep:expr) => {{
        (|| -> ort::Result<ort::session::Session> {
            let b = ort::session::Session::builder()?;
            let b = b.with_execution_providers([$ep])?;
            let mut b = b.with_intra_threads(1)?;
            b.commit_from_file($model)
        })()
    }};
}

#[cfg(target_os = "windows")]
fn try_ep_directml(model: &std::path::Path) -> bool {
    build_probe!(model, ort::ep::DirectML::default().build())
        .map(run_probe)
        .unwrap_or(false)
}

fn try_ep_cuda(model: &std::path::Path) -> bool {
    build_probe!(model, ort::ep::CUDA::default().build())
        .map(run_probe)
        .unwrap_or(false)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn try_ep_coreml(model: &std::path::Path) -> bool {
    build_probe!(model, ort::ep::CoreML::default().build())
        .map(run_probe)
        .unwrap_or(false)
}

fn try_ep_openvino(model: &std::path::Path) -> bool {
    build_probe!(model, ort::ep::OpenVINO::default().build())
        .map(run_probe)
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn detect_gpu_name_windows() -> String {
    use std::process::Command;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command",
            "(Get-CimInstance Win32_VideoController | Select-Object -First 1 -ExpandProperty Name)"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if name.is_empty() { "GPU".to_string() } else { name }
        }
        _ => "GPU".to_string(),
    }
}
