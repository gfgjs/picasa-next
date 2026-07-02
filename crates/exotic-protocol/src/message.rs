// crates/exotic-protocol/src/message.rs
//! 协议消息体（v3 Part2 §3.3）。两端共享同一份定义，JSON 只放控制字段；缩略图等二进制走同帧 blob。

use serde::{Deserialize, Serialize};

/// Host→Worker 握手开场（Hello 帧）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloBody {
    /// Host 语义版本（展示/兼容用，不承担回滚防护，R11）。
    pub host_version: String,
    /// Host 期望的协议版本；与 Worker `PROTOCOL_VERSION` 不一致即握手失败。
    pub protocol_version: u16,
    /// Host 能接收的最大 blob；Worker 据此自限输出。
    pub max_blob_len: u32,
}

/// Worker→Host 握手应答（Ready 帧）。Host 校验 worker_id/version/protocol/capabilities。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadyBody {
    pub worker_id: String,
    pub worker_version: String,
    pub protocol_version: u16,
    /// Worker 实测支持的能力（如 `["thumbnail"]`）；只声明 probe 通过范围。
    pub capabilities: Vec<String>,
    pub max_blob_len: u32,
}

/// Host→Worker 处理请求（Request 帧）。按 `op` 分流；首发只有 thumbnail。
///
/// `target_long_edge` 必须是**吸附后档位**（R5）——与指纹里的 `target_tier` 一致，
/// 否则同档不同请求 size 会算出不同指纹、反复重做。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RequestBody {
    Thumbnail {
        item_id: i64,
        source_path: String,
        target_long_edge: u32,
        input_fingerprint: String,
    },
    Metadata {
        item_id: i64,
        source_path: String,
        input_fingerprint: String,
    },
}

impl RequestBody {
    /// 取请求关联的 item_id（响应核对用）。
    pub fn item_id(&self) -> i64 {
        match self {
            RequestBody::Thumbnail { item_id, .. } => *item_id,
            RequestBody::Metadata { item_id, .. } => *item_id,
        }
    }

    /// 取请求的输入指纹（响应核对用）。
    pub fn input_fingerprint(&self) -> &str {
        match self {
            RequestBody::Thumbnail {
                input_fingerprint, ..
            } => input_fingerprint,
            RequestBody::Metadata {
                input_fingerprint, ..
            } => input_fingerprint,
        }
    }
}

/// Worker→Host 成功（Success 帧 + 同帧 blob）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuccessBody {
    pub item_id: i64,
    pub input_fingerprint: String,
    /// 缩略图固定 `image/webp`；Host 二次校验。
    pub mime: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// metadata 能力的结构化结果（thumbnail 时为 None）。
    pub metadata: Option<serde_json::Value>,
}

/// 稳定错误码（v3 Part2 §3.3）。整数语义跨版本固定，serde 用 snake_case 字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerErrorCode {
    /// 该格式变体不被支持（CMYK/16-bit/PSB/无 merged image 等）→ terminal，等源/版本变化再失效。
    UnsupportedVariant,
    /// 输入畸形（截断/非法字节）→ terminal。
    MalformedInput,
    /// 触及资源上限（尺寸/像素/内存/输出字节）→ terminal。
    ResourceLimit,
    /// 暂时 IO/文件占用 → retryable。
    IoError,
    /// Worker 内部错误（含 panic 兜底）→ retryable。
    InternalError,
}

impl WorkerErrorCode {
    /// 稳定字符串标识（与 DB `exotic_tasks.last_error_code`、序列化形态一致）。
    pub fn as_str(self) -> &'static str {
        match self {
            WorkerErrorCode::UnsupportedVariant => "unsupported_variant",
            WorkerErrorCode::MalformedInput => "malformed_input",
            WorkerErrorCode::ResourceLimit => "resource_limit",
            WorkerErrorCode::IoError => "io_error",
            WorkerErrorCode::InternalError => "internal_error",
        }
    }

    /// 该错误码的**默认** retryable 语义（Worker 也可在 FailureBody 显式覆盖）。
    pub fn default_retryable(self) -> bool {
        matches!(
            self,
            WorkerErrorCode::IoError | WorkerErrorCode::InternalError
        )
    }
}

/// Worker→Host 失败（Failure 帧）。`retryable` 由 Worker 给出，Host 据错误码 + 该位决定重试/终态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailureBody {
    pub item_id: i64,
    pub input_fingerprint: String,
    pub code: WorkerErrorCode,
    pub retryable: bool,
    /// 用户可见诊断信息；**不得**含完整绝对路径或 License token（v3 Part2 §3.3）。
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_tagged_roundtrip() {
        let req = RequestBody::Thumbnail {
            item_id: 42,
            source_path: "a.psd".into(),
            target_long_edge: 480,
            input_fingerprint: "fp".into(),
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""op":"thumbnail""#));
        let back: RequestBody = serde_json::from_str(&s).unwrap();
        assert_eq!(back, req);
        assert_eq!(back.item_id(), 42);
        assert_eq!(back.input_fingerprint(), "fp");
    }

    #[test]
    fn error_code_serde_and_retryable() {
        assert_eq!(
            serde_json::to_string(&WorkerErrorCode::UnsupportedVariant).unwrap(),
            r#""unsupported_variant""#
        );
        assert!(!WorkerErrorCode::UnsupportedVariant.default_retryable());
        assert!(WorkerErrorCode::IoError.default_retryable());
        assert!(WorkerErrorCode::InternalError.default_retryable());
        assert!(!WorkerErrorCode::MalformedInput.default_retryable());
    }
}
