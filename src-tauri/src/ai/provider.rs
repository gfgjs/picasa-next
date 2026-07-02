// src-tauri/src/ai/provider.rs
//! AI hardware provider detection and selection.
//! AI 硬件加速后端探测与选择。
//!
//! Detection order: DirectML → CUDA → CoreML → OpenVINO → CPU
//! 探测顺序：DirectML → CUDA → CoreML → OpenVINO → CPU

use serde::{Deserialize, Serialize};
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
            AiProvider::DirectML => "DirectML (GPU)",
            AiProvider::CUDA => "CUDA (NVIDIA GPU)",
            AiProvider::CoreML => "CoreML (Apple)",
            AiProvider::OpenVINO => "OpenVINO (Intel)",
            AiProvider::Cpu => "CPU",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AiProvider::DirectML => "directml",
            AiProvider::CUDA => "cuda",
            AiProvider::CoreML => "coreml",
            AiProvider::OpenVINO => "openvino",
            AiProvider::Cpu => "cpu",
        }
    }

    // 固有 from_str：返回 Self（非 std FromStr 的 Result）、且不可失败（未知值回退默认），
    // 与标准 trait 语义不同；改名会波及调用点，保留固有方法。
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "directml" => AiProvider::DirectML,
            "cuda" => AiProvider::CUDA,
            "coreml" => AiProvider::CoreML,
            "openvino" => AiProvider::OpenVINO,
            _ => AiProvider::Cpu,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub provider: AiProvider,
    pub gpu_name: String,
}

/// Detect the best available AI execution provider for the current platform.
/// 检测当前平台上可用的最优 AI 执行提供者。
///
/// Currently uses compile-time platform detection. Future versions may add
/// runtime GPU probing (e.g. DirectML capability check, CUDA device query).
/// 目前使用编译期平台检测。未来版本可能会添加运行时 GPU 探测。
pub fn detect_best_provider() -> ProviderInfo {
    #[cfg(target_os = "windows")]
    {
        return ProviderInfo {
            provider: AiProvider::DirectML,
            gpu_name: "DirectML GPU".to_string(),
        };
    }

    #[cfg(target_os = "macos")]
    {
        return ProviderInfo {
            provider: AiProvider::CoreML,
            gpu_name: "Apple Neural Engine".to_string(),
        };
    }

    #[cfg(target_os = "linux")]
    {
        return ProviderInfo {
            provider: AiProvider::Cpu,
            gpu_name: String::new(),
        };
    }

    #[allow(unreachable_code)]
    ProviderInfo {
        provider: AiProvider::Cpu,
        gpu_name: String::new(),
    }
}

/// Detect the dedicated video memory (VRAM) in bytes.
/// 探测专用显存大小（字节）。
pub fn detect_vram_bytes() -> Option<u64> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
        unsafe {
            if let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() {
                if let Ok(adapter) = factory.EnumAdapters1(0) {
                    if let Ok(desc) = adapter.GetDesc1() {
                        return Some(desc.DedicatedVideoMemory as u64);
                    }
                }
            }
        }
    }
    None
}
