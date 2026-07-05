// src-tauri/src/error.rs
//! Unified application error type.
//! 统一的应用程序错误类型。
//! All variants are serialisable so they can be forwarded to the frontend via IPC.
//! 所有变体都是可序列化的，以便可以通过 IPC 转发到前端。

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("Connection pool error: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("EXIF parse error: {0}")]
    Exif(#[from] exif::Error),

    #[error("XMP parse error: {0}")]
    Xmp(#[from] quick_xml::Error),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Image engine error: {0}")]
    Engine(#[from] image::ImageError),

    #[error("Path resolution error: {0}")]
    PathResolution(String),

    #[error("FFmpeg error: {0}")]
    FFmpeg(String),

    #[error("Audio metadata error: {0}")]
    AudioMetadata(String),

    #[error("Document render error: {0}")]
    DocumentRender(String),

    #[error("Layout cache not ready — call compute_layout first")]
    LayoutNotReady,

    /// 选择守门失效（T18）：SelectAll 携带的 `layout_version` 与当前布局不一致 → 视图在全选后已变，
    /// 拒绝按可能已漂移的集合执行批量写。前端据此**重算 layout 拿新版本 → 重发命令**。可恢复、不记错误日志。
    #[error("View stale — layout changed since selection; recompute and retry")]
    ViewStale,

    #[error("Scan root not found: id={0}")]
    ScanRootNotFound(i64),

    #[error("Media item not found: id={0}")]
    MediaNotFound(i64),

    /// 卷离线（T13 §3.7）：打开原图/视频等需实体文件访问的操作，其所在卷当前离线（可自动恢复）。
    /// message 携带卷标签（或 stable_id 兜底）供前端弹「请插入设备 <label>」；前端按稳定 code
    /// `VolumeOffline` 分流（非破图、非硬故障，重连即恢复），不记错误日志。
    #[error("Volume offline: {0}")]
    VolumeOffline(String),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("AI inference error: {0}")]
    Ai(String),

    #[error("AI model not loaded: {0}")]
    AiModelNotLoaded(String),

    #[error("System error: {0}")]
    System(String),

    #[error("OS error: {0}")]
    Os(String),

    #[error("AI tokenization error: {0}")]
    AiTokenizer(String),

    #[error("Internal error: {0}")]
    Internal(String),

    /// 冷门格式插件子系统错误。`code` 为底层稳定标识——直接取自 FetchError / Registry / License /
    /// crypto 各自的 `code()`，或命令层自定的稳定字面量——并原样透到 IPC `code` 字段，让前端可
    /// **按类型**分流（如「回滚攻击被拒」`rollback` 给安全警告、「网络失败」`http` 给重试），
    /// 而非靠匹配 `message` 文案（脆弱、改文案即失效）。message 仅作展示/日志，不承担分流职责。
    #[error("exotic error [{code}]: {message}")]
    Exotic { code: &'static str, message: String },

    #[error("Failed to create folder: {0}")]
    CreateFolder(String),

    #[error("Failed to move file: {0}")]
    MoveFile(String),

    #[error("Failed to copy file: {0}")]
    CopyFile(String),

    #[error("Invalid folder move: {0}")]
    InvalidMove(String),

    #[error("Target already has a folder named: {0}")]
    DirectoryExists(String),
}

/// AI 推理核错误收敛(Part4-T15/T16):picasa-next-ai-core 自持 AiError,此处映射回既有
/// 变体,调用点 `?` 传播与前端 IPC code(Ai/AiTokenizer/Internal)完全不变。T16 收口后
/// host 关闭 ai-core 的 `inference` feature(ort 直依赖已拆),AiError 为 non_exhaustive:
/// 通配臂携带字符串进 Ai 变体——workspace feature 并集把 Ort 臂带回来时同样落此臂。
impl From<picasa_next_ai_core::AiError> for AppError {
    fn from(e: picasa_next_ai_core::AiError) -> Self {
        use picasa_next_ai_core::AiError;
        match e {
            AiError::Internal(m) => AppError::Internal(m),
            AiError::Tokenizer(m) => AppError::AiTokenizer(m),
            other => AppError::Ai(other.to_string()),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if !matches!(
            self,
            AppError::LayoutNotReady
                | AppError::Cancelled
                | AppError::ViewStale
                // 卷离线可自动恢复（重连即好），属预期分支，不记错误日志（同 Cancelled/ViewStale）。
                | AppError::VolumeOffline(_)
        ) {
            tracing::error!("AppError occurred (to frontend): {:?}", self);
        }

        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("AppError", 2)?;

        let (code, msg) = match self {
            AppError::Io(_) => ("Io", "文件读写异常 | IO error"),
            AppError::Db(_) => ("Db", "数据库访问异常 | Database error"),
            AppError::Pool(_) => ("Pool", "数据库连接池异常 | Connection pool error"),
            AppError::Exif(_) => ("Exif", "照片元数据解析异常 | EXIF parse error"),
            AppError::Xmp(_) => ("Xmp", "XMP 数据解析异常 | XMP parse error"),
            AppError::UnsupportedFormat(m) => ("UnsupportedFormat", m.as_str()),
            AppError::Engine(_) => ("Engine", "图像处理引擎异常 | Image engine error"),
            AppError::PathResolution(m) => ("PathResolution", m.as_str()),
            AppError::FFmpeg(m) => ("FFmpeg", m.as_str()),
            AppError::AudioMetadata(m) => ("AudioMetadata", m.as_str()),
            AppError::DocumentRender(m) => ("DocumentRender", m.as_str()),
            AppError::LayoutNotReady => (
                "LayoutNotReady",
                "布局未就绪，请先计算布局 | Layout cache not ready",
            ),
            AppError::ViewStale => (
                "ViewStale",
                "视图已更新，请重试 | View changed since selection, please retry",
            ),
            AppError::ScanRootNotFound(_) => {
                ("ScanRootNotFound", "未找到扫描目录 | Scan root not found")
            }
            AppError::MediaNotFound(_) => {
                ("MediaNotFound", "未找到媒体文件 | Media item not found")
            }
            // message 即卷标签（前端拼「请插入设备 <label>」）——message-passthrough，同 System/Os。
            AppError::VolumeOffline(m) => ("VolumeOffline", m.as_str()),
            AppError::Cancelled => ("Cancelled", "操作已取消 | Operation cancelled"),
            AppError::Ai(_) => ("Ai", "AI 推理异常 | AI inference error"),
            AppError::AiModelNotLoaded(m) => ("AiModelNotLoaded", m.as_str()),
            AppError::System(m) => ("System", m.as_str()),
            AppError::Os(m) => ("Os", m.as_str()),
            AppError::AiTokenizer(m) => ("AiTokenizer", m.as_str()),
            AppError::Internal(m) => ("Internal", m.as_str()),
            // 透出底层稳定 code（而非笼统 "Internal"）：前端按 code 分流 exotic 失败。
            AppError::Exotic { code, message } => (*code, message.as_str()),
            AppError::CreateFolder(m) => ("CreateFolder", m.as_str()),
            AppError::MoveFile(m) => ("MoveFile", m.as_str()),
            AppError::CopyFile(m) => ("CopyFile", m.as_str()),
            AppError::InvalidMove(m) => ("InvalidMove", m.as_str()),
            AppError::DirectoryExists(m) => ("DirectoryExists", m.as_str()),
        };

        state.serialize_field("code", code)?;
        state.serialize_field("message", msg)?;
        state.end()
    }
}

/// Convenience alias used throughout the codebase.
/// 整个代码库中使用的便捷别名。
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// 锁住 exotic 错误契约：序列化后 IPC `code` 字段必须是底层稳定码（如 "rollback"），
    /// **不得**笼统回退为 "Internal"——这正是「前端按类型分流」赖以成立的前提。
    #[test]
    fn exotic_error_surfaces_stable_code_not_internal() {
        let err = AppError::Exotic {
            code: "rollback",
            message: "Registry 验签/接受失败：rollback".into(),
        };
        let v = serde_json::to_value(&err).expect("serialize AppError::Exotic");
        assert_eq!(v["code"], "rollback", "code 字段须为稳定码而非 Internal");
        assert_eq!(v["message"], "Registry 验签/接受失败：rollback");
    }

    /// 对照：泛化 Internal 仍序列化为 "Internal"（确认新变体未污染既有契约）。
    #[test]
    fn internal_error_still_serializes_as_internal() {
        let v = serde_json::to_value(AppError::Internal("x".into())).unwrap();
        assert_eq!(v["code"], "Internal");
    }

    /// 锁住 T18 选择守门契约：ViewStale 序列化后 code 必须是稳定 "ViewStale"，前端据此重算 layout 重发。
    #[test]
    fn view_stale_surfaces_stable_code() {
        let v = serde_json::to_value(AppError::ViewStale).unwrap();
        assert_eq!(v["code"], "ViewStale");
    }

    /// 锁住 T13 离线契约：VolumeOffline 序列化后 code 稳定为 "VolumeOffline"，
    /// message 即卷标签（前端据此弹「请插入设备 <label>」而非破图）。
    #[test]
    fn volume_offline_surfaces_stable_code_and_label() {
        let v = serde_json::to_value(AppError::VolumeOffline("我的移动硬盘".into())).unwrap();
        assert_eq!(v["code"], "VolumeOffline");
        assert_eq!(v["message"], "我的移动硬盘");
    }
}
