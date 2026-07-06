// crates/scrollery-ai-core/src/decoded.rs
//! 解码后图像的最小载体(T15 自 src-tauri engine/traits.rs 迁入)。
//!
//! 这是推理核与图像解码层之间的唯一数据契约:RGBA8 平面 + 宽高。
//! src-tauri 的 `engine::traits` 再导出本类型,主进程各解码后端签名不变;
//! ai-worker 子进程自行解码(ai_cache WebP / 源文件)后构造同一类型喂推理。

/// A decoded image: raw RGBA pixels plus dimensions.
/// 解码后的图像:原始 RGBA 像素 + 尺寸。
#[derive(Debug, Clone)]
pub struct DecodedImage {
    /// Raw RGBA pixel data.
    /// 原始 RGBA 像素数据。
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
