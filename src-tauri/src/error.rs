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

    #[error("Scan root not found: id={0}")]
    ScanRootNotFound(i64),

    #[error("Media item not found: id={0}")]
    MediaNotFound(i64),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("AI inference error: {0}")]
    Ai(#[from] ort::Error),

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

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // 将底层真实错误（包含 Stack Trace / Source）打印在后端日志中
        tracing::error!("AppError occurred (to frontend): {:?}", self);

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
            AppError::LayoutNotReady => ("LayoutNotReady", "布局未就绪，请先计算布局 | Layout cache not ready"),
            AppError::ScanRootNotFound(_) => ("ScanRootNotFound", "未找到扫描目录 | Scan root not found"),
            AppError::MediaNotFound(_) => ("MediaNotFound", "未找到媒体文件 | Media item not found"),
            AppError::Cancelled => ("Cancelled", "操作已取消 | Operation cancelled"),
            AppError::Ai(_) => ("Ai", "AI 推理异常 | AI inference error"),
            AppError::AiModelNotLoaded(m) => ("AiModelNotLoaded", m.as_str()),
            AppError::System(m) => ("System", m.as_str()),
            AppError::Os(m) => ("Os", m.as_str()),
            AppError::AiTokenizer(m) => ("AiTokenizer", m.as_str()),
            AppError::Internal(m) => ("Internal", m.as_str()),
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
