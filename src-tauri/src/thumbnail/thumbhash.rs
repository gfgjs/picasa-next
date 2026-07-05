// src-tauri/src/thumbnail/thumbhash.rs
//! ThumbHash generation from decoded pixels.
//! 从解码后的像素生成 ThumbHash。
//!
//! Input: RGBA pixel buffer (any size).
//! 输入：RGBA 像素缓冲区（任何尺寸）。
//! Output: ~28 bytes of ThumbHash, stored as BLOB in the DB.
//! 输出：约 28 字节的 ThumbHash，作为 BLOB 存储在数据库中。
//! Frontend receives it as `number[]` → `Uint8Array` → renders 32×32 placeholder.
//! 前端接收其为 `number[]` → `Uint8Array` → 渲染为 32×32 占位符。

use crate::engine::traits::DecodedImage;
use crate::error::{AppError, Result};

/// Maximum dimension to scale the image before hashing (ThumbHash works well at 100×100 or smaller).
/// 在散列之前缩放图像的最大尺寸（ThumbHash 在 100×100 或更小的尺寸下效果很好）。
const HASH_MAX_DIM: u32 = 100;

/// Generate a ThumbHash for a decoded image.
/// 为解码后的图像生成 ThumbHash。
pub fn generate_thumbhash(decoded: &DecodedImage) -> Result<Vec<u8>> {
    // Scale down if needed
    // 如果需要则缩小
    let (pixels, width, height) = if decoded.width > HASH_MAX_DIM || decoded.height > HASH_MAX_DIM {
        let ratio = (HASH_MAX_DIM as f32) / (decoded.width.max(decoded.height) as f32);
        let new_w = ((decoded.width as f32) * ratio).round() as u32;
        let new_h = ((decoded.height as f32) * ratio).round() as u32;

        // Use fast_image_resize v4 for downscaling
        // 使用 fast_image_resize v4 进行降采样（缩小）
        use fast_image_resize::pixels::PixelType;
        use fast_image_resize::{images::Image as FirImage, ResizeOptions, Resizer};

        let src = FirImage::from_vec_u8(
            decoded.width.max(1),
            decoded.height.max(1),
            decoded.pixels.clone(),
            PixelType::U8x4,
        )
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut dst = FirImage::new(new_w.max(1), new_h.max(1), PixelType::U8x4);

        let mut resizer = Resizer::new();
        resizer
            .resize(&src, &mut dst, &ResizeOptions::default())
            .map_err(|e| AppError::Internal(e.to_string()))?;

        (dst.into_vec(), new_w, new_h)
    } else {
        (decoded.pixels.clone(), decoded.width, decoded.height)
    };

    let hash = thumbhash::rgba_to_thumb_hash(width as usize, height as usize, &pixels);
    Ok(hash)
}

#[cfg(test)]
mod tests {
    //! 跨语言金标生成器:为前端 `src/utils/thumbhash.spec.ts` 产出与 Rust 编码 / 解码器
    //! 逐字节对拍的 fixture(TS 源文件)。前端解码器是 evanw thumbhash 算法的 TS 移植,
    //! 其正确性契约 = 「后端 rgba_to_thumb_hash 产出的哈希,前端重建结果与 crate 自带
    //! 解码器一致」,故金标必须由同一 Cargo.lock 锁定的 crate 版本生成。
    //!
    //! 运行(手动,#[ignore] 不进常规测试/CI):
    //!   cargo test -p picasa-next print_thumbhash_golden_fixtures -- --ignored --nocapture
    //! 输出到系统临时目录 thumbhash.golden.ts,人工拷贝至 src/utils/ 并过 prettier。
    //! 五张合成图全为确定性数学图案(无随机 / 时钟),重跑输出逐字节一致。

    /// 无依赖 base64(标准字母表 + padding),仅供 fixture 序列化,不做解码。
    fn b64(data: &[u8]) -> String {
        const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
        for chunk in data.chunks(3) {
            let n = ((chunk[0] as u32) << 16)
                | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
                | (*chunk.get(2).unwrap_or(&0) as u32);
            out.push(TBL[(n >> 18) as usize & 63] as char);
            out.push(TBL[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                TBL[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                TBL[(n & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    /// (场景名, w, h, rgba):覆盖 横向/纵向/带 alpha/纯色 DC-only/极小图 五类形态。
    fn fixture_images() -> Vec<(&'static str, usize, usize, Vec<u8>)> {
        let mut out = Vec::new();

        // 横向渐变(landscape,无 alpha):R 随 x、G 随 y 线性,B 恒 128
        let (w, h) = (40usize, 28usize);
        let mut px = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&[
                    (x * 255 / (w - 1)) as u8,
                    (y * 255 / (h - 1)) as u8,
                    128,
                    255,
                ]);
            }
        }
        out.push(("landscape_gradient", w, h, px));

        // 纵向色带(portrait,无 alpha):每 5 行换一种高饱和色,考验 P/Q 色度重建
        let (w, h) = (28usize, 40usize);
        let mut px = Vec::with_capacity(w * h * 4);
        const BANDS: [[u8; 3]; 3] = [[220, 60, 40], [40, 180, 90], [50, 80, 200]];
        for y in 0..h {
            let c = BANDS[(y / 5) % 3];
            for _x in 0..w {
                px.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        out.push(("portrait_bands", w, h, px));

        // 方形径向 alpha:固定道奇蓝,alpha 从中心向外衰减(触发 hasAlpha 编码分支)
        let (w, h) = (32usize, 32usize);
        let mut px = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 - 15.5;
                let dy = y as f32 - 15.5;
                let d = (dx * dx + dy * dy).sqrt();
                let a = ((16.0 - d) * 32.0).clamp(0.0, 255.0) as u8;
                px.extend_from_slice(&[30, 144, 255, a]);
            }
        }
        out.push(("square_alpha_radial", w, h, px));

        // 纯色(AC 全零,DC-only 路径)
        let (w, h) = (20usize, 20usize);
        let px = [200u8, 90, 40, 255].repeat(w * h);
        out.push(("solid_color", w, h, px));

        // 极小图(3×5 棋盘):验证小尺寸端到端不越界
        let (w, h) = (3usize, 5usize);
        let mut px = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 255u8 } else { 0 };
                px.extend_from_slice(&[v, v, v, 255]);
            }
        }
        out.push(("tiny_checkerboard", w, h, px));

        out
    }

    #[test]
    #[ignore = "金标生成器:手动运行,产出 src/utils/thumbhash.golden.ts"]
    fn print_thumbhash_golden_fixtures() {
        let mut ts = String::new();
        ts.push_str(
            "// src/utils/thumbhash.golden.ts\n\
             // 自动生成,勿手改。跨语言金标:Rust thumbhash crate 编码(rgba_to_thumb_hash)\n\
             // 与解码(thumb_hash_to_rgba / thumb_hash_to_average_rgba)的逐字节输出。\n\
             // 生成器:src-tauri/src/thumbnail/thumbhash.rs 的 print_thumbhash_golden_fixtures(#[ignore])\n\
             //   cargo test -p picasa-next print_thumbhash_golden_fixtures -- --ignored --nocapture\n\
             // 消费方:thumbhash.spec.ts(前端 TS 解码器 ↔ Rust 解码器对拍)。\n\n\
             export interface ThumbhashGoldenFixture {\n\
             \x20 /** 合成图案场景名 */\n\
             \x20 name: string\n\
             \x20 /** rgba_to_thumb_hash 输出的哈希字节 */\n\
             \x20 hash: number[]\n\
             \x20 /** thumb_hash_to_rgba 输出宽度 */\n\
             \x20 w: number\n\
             \x20 /** thumb_hash_to_rgba 输出高度 */\n\
             \x20 h: number\n\
             \x20 /** thumb_hash_to_rgba 输出的 RGBA 像素(base64,w*h*4 字节) */\n\
             \x20 rgbaBase64: string\n\
             \x20 /** thumb_hash_to_average_rgba 输出(0..1) */\n\
             \x20 avg: { r: number; g: number; b: number; a: number }\n\
             }\n\n\
             export const THUMBHASH_GOLDEN: ThumbhashGoldenFixture[] = [\n",
        );
        for (name, w, h, px) in fixture_images() {
            let hash = thumbhash::rgba_to_thumb_hash(w, h, &px);
            let (dw, dh, rgba) = thumbhash::thumb_hash_to_rgba(&hash).expect("golden decode");
            let (r, g, b, a) = thumbhash::thumb_hash_to_average_rgba(&hash).expect("golden avg");
            let hash_list = hash
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let rgba_b64 = b64(&rgba);
            ts.push_str(&format!(
                "  {{\n    name: '{name}',\n    hash: [{hash_list}],\n    w: {dw},\n    h: {dh},\n    rgbaBase64:\n      '{rgba_b64}',\n    avg: {{ r: {r:?}, g: {g:?}, b: {b:?}, a: {a:?} }},\n  }},\n"
            ));
        }
        ts.push_str("]\n");
        let path = std::env::temp_dir().join("thumbhash.golden.ts");
        std::fs::write(&path, &ts).expect("write golden");
        println!("golden fixtures written to {}", path.display());
    }
}
