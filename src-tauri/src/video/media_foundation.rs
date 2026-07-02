// src-tauri/src/video/media_foundation.rs
//! Media Foundation video backend (§3.2 / §3.3) — Windows-native, zero-bundle decoding via
//! the `windows` crate (no FFmpeg / no external binary). Used in BOTH Lite and Perf variants.
//!
//! Media Foundation 视频后端（§3.2 / §3.3）—— 基于 `windows` crate 的 Windows 原生、零捆绑解码
//! （无 FFmpeg / 无外部二进制）。Lite 与 Perf 两变体都使用。
//!
//! 关键设计：
//!  - 用 `IMFSourceReader` + `MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING`，让 MF 内置视频处理器
//!    把任意输入像素格式转换为 **RGB32**（内存字节序 B,G,R,X）后输出到系统内存。
//!  - MF 内部会在可用时自动走硬件解码器（DXVA）；我们只需系统内存里的 RGBA 像素来编码缩略图/雪碧图，
//!    故无需 D3D manager 回读纹理 —— 对本用途「GPU 回读」收益甚微（见 §3.3「GPU 收益主要在批量」）。
//!  - **旋转**：读取 `MF_MT_VIDEO_ROTATION`，对解码帧做正立旋转（手机竖拍视频否则会躺倒，§3.2）。
//!  - **负 stride**：RGB32 常以 bottom-up（负 stride）交付，按 `MF_MT_DEFAULT_STRIDE` 符号翻转行序。

use std::path::Path;
use std::sync::Once;

use windows::core::{GUID, HSTRING, PROPVARIANT};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PropVariantToInt64;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use crate::engine::traits::DecodedImage;
use crate::error::{AppError, Result};
use crate::video::{VideoBackend, VideoInfo};

/// Containers Media Foundation can typically demux/decode with the OS-installed codecs.
/// mkv/webm/flv/ogv are intentionally excluded — those need FFmpeg (Perf variant, §9).
/// MF 一般能用系统已装编解码器处理的容器。mkv/webm/flv/ogv 有意排除（需 FFmpeg / Perf，§9）。
const MF_VIDEO_EXTS: &[&str] = &[
    "mp4", "m4v", "mov", "wmv", "avi", "3gp", "3g2", "ts", "mts", "m2ts", "asf", "mpg", "mpeg",
];

/// Sprite cell height (px) for keyframe scrub frames (§3.3). Width derives from the video's
/// display aspect, uniform across all frames so the frontend can scrub by `background-position`.
/// 关键帧 scrub 帧的格高（像素，§3.3）。格宽按视频显示比例推导，所有帧统一，
/// 使前端可用 `background-position` 进行 scrub。
const SPRITE_CELL_H: u32 = 200;

/// Stream selectors (typed `u32` sentinels in the MF SDK).
/// 流选择子（MF SDK 中的 `u32` 哨兵值）。
const FIRST_VIDEO_STREAM: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
const FIRST_AUDIO_STREAM: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
const MEDIASOURCE: u32 = MF_SOURCE_READER_MEDIASOURCE.0 as u32;

pub struct MediaFoundationBackend;

impl VideoBackend for MediaFoundationBackend {
    fn name(&self) -> &'static str {
        "media-foundation"
    }

    fn can_handle(&self, ext: &str) -> bool {
        MF_VIDEO_EXTS.contains(&ext)
    }

    fn probe(&self, path: &Path) -> Result<VideoInfo> {
        ensure_mf();
        init_com();
        unsafe {
            let reader = open_reader(path)?;
            // Read geometry/rotation/fps from the NATIVE type (pre-conversion) for accuracy.
            // 从 NATIVE 类型（转换前）读取几何/旋转/帧率，更准确。
            let native = reader
                .GetNativeMediaType(FIRST_VIDEO_STREAM, 0)
                .map_err(mf_err)?;

            let (nw, nh) = attr_size(&native, &MF_MT_FRAME_SIZE).unwrap_or((0, 0));
            let rotation = normalize_rotation(native.GetUINT32(&MF_MT_VIDEO_ROTATION).unwrap_or(0));
            let fps = attr_ratio(&native, &MF_MT_FRAME_RATE)
                .map(|(n, d)| if d > 0 { n as f32 / d as f32 } else { 0.0 })
                .unwrap_or(0.0);
            let bitrate = native.GetUINT32(&MF_MT_AVG_BITRATE).unwrap_or(0);
            let codec = native.GetGUID(&MF_MT_SUBTYPE).ok().and_then(codec_label);

            // 90/270 旋转交换显示宽高（与图片 EXIF orientation 同理）。
            let (width, height) = if rotation == 90 || rotation == 270 {
                (nh, nw)
            } else {
                (nw, nh)
            };

            let duration_ms = read_duration_ms(&reader);
            let has_audio = reader.GetNativeMediaType(FIRST_AUDIO_STREAM, 0).is_ok();

            Ok(VideoInfo {
                width,
                height,
                duration_ms,
                rotation,
                fps,
                bitrate,
                has_audio,
                codec,
            })
        }
    }

    fn cover(&self, path: &Path, t_ms: u64) -> Result<DecodedImage> {
        ensure_mf();
        init_com();
        unsafe {
            let reader = open_reader(path)?;
            let rotation = normalize_rotation(
                reader
                    .GetNativeMediaType(FIRST_VIDEO_STREAM, 0)
                    .ok()
                    .and_then(|t| t.GetUINT32(&MF_MT_VIDEO_ROTATION).ok())
                    .unwrap_or(0),
            );
            configure_rgb32(&reader)?;
            let geom = output_geometry(&reader)?;

            // Avoid the first frame (often black). Try a few timestamps and pick the first
            // sufficiently-bright frame; accept whatever we have on the last attempt.
            // 避开第 0 帧（常为黑帧）。尝试几个时间戳，取首个足够亮的帧；最后一次尝试则照单全收。
            let mut t_100ns = (t_ms as i64) * 10_000;
            let mut last: Option<DecodedImage> = None;
            for attempt in 0..5 {
                match read_frame_at(&reader, t_100ns, &geom) {
                    Ok(img) => {
                        let dark = is_too_dark(&img);
                        if !dark || attempt == 4 {
                            return Ok(apply_rotation(img, rotation));
                        }
                        last = Some(img);
                    }
                    Err(_) => break,
                }
                t_100ns += 5_000_000; // +0.5s
            }
            // 全部偏暗或读取在末尾失败：用最后拿到的一帧，否则回退到第 0 帧。
            if let Some(img) = last {
                return Ok(apply_rotation(img, rotation));
            }
            let img = read_frame_at(&reader, 0, &geom)?;
            Ok(apply_rotation(img, rotation))
        }
    }

    fn keyframes(&self, path: &Path, n: usize) -> Result<Vec<DecodedImage>> {
        ensure_mf();
        init_com();
        let n = n.max(1);
        unsafe {
            let reader = open_reader(path)?;
            let rotation = normalize_rotation(
                reader
                    .GetNativeMediaType(FIRST_VIDEO_STREAM, 0)
                    .ok()
                    .and_then(|t| t.GetUINT32(&MF_MT_VIDEO_ROTATION).ok())
                    .unwrap_or(0),
            );
            let duration_ms = read_duration_ms(&reader);
            configure_rgb32(&reader)?;
            let geom = output_geometry(&reader)?;

            // Uniform cell size from the display aspect so the sprite is a clean horizontal strip.
            // 由显示比例推导统一格尺寸，使雪碧图为整齐的水平条带。
            let (disp_w, disp_h) = if rotation == 90 || rotation == 270 {
                (geom.height, geom.width)
            } else {
                (geom.width, geom.height)
            };
            let aspect = if disp_h > 0 {
                disp_w as f32 / disp_h as f32
            } else {
                16.0 / 9.0
            };
            let cell_h = SPRITE_CELL_H;
            let cell_w = ((cell_h as f32 * aspect).round() as u32).max(1);

            // Sample within [5%, 95%] to skip intro/outro black frames.
            // 在 [5%, 95%] 区间采样，跳过片头/片尾黑帧。
            let dur = duration_ms.max(1) as f64;
            let mut frames = Vec::with_capacity(n);
            for i in 0..n {
                let frac = if n == 1 {
                    0.5
                } else {
                    0.05 + 0.90 * (i as f64 / (n - 1) as f64)
                };
                let t_100ns = (dur * frac * 10_000.0) as i64;
                if let Ok(img) = read_frame_at(&reader, t_100ns, &geom) {
                    let upright = apply_rotation(img, rotation);
                    frames.push(resize_rgba(&upright, cell_w, cell_h));
                }
            }
            if frames.is_empty() {
                return Err(AppError::Internal(
                    "no keyframes decoded | 未解码到关键帧".into(),
                ));
            }
            Ok(frames)
        }
    }
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────
// ── 生命周期 ─────────────────────────────────────────────────────────────────

/// Start the MF platform exactly once for the process lifetime. We intentionally never call
/// `MFShutdown` — leaking one startup is the standard pattern for long-running apps and avoids
/// fragile per-task balancing across the rayon derivation pool.
/// 进程生命周期内只启动一次 MF 平台。有意不调用 `MFShutdown` —— 长期运行应用的标准做法，
/// 避免在 rayon 派生池中逐任务配平的脆弱性。
fn ensure_mf() {
    static MF_INIT: Once = Once::new();
    MF_INIT.call_once(|| unsafe {
        // MF_VERSION = (SDK<<16)|API = (0x0002<<16)|0x0070；MFSTARTUP_FULL = 0。
        let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
    });
}

/// Ensure COM is initialised (MTA) on the current thread — MF objects require it. Rayon worker
/// threads start uninitialised; STA elsewhere returns `RPC_E_CHANGED_MODE`, harmless here.
/// 确保当前线程已初始化 COM（MTA）—— MF 对象需要。rayon 工作线程初始为未初始化；
/// 别处若为 STA 会返回 `RPC_E_CHANGED_MODE`，对此处无害。
fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

// ── Source reader setup ─────────────────────────────────────────────────────────

unsafe fn open_reader(path: &Path) -> Result<IMFSourceReader> {
    let url = HSTRING::from(path.as_os_str());
    // Enable the built-in video processor so we can request RGB32 output from any input format.
    // 启用内置视频处理器，使我们可对任意输入格式请求 RGB32 输出。
    let mut attrs: Option<IMFAttributes> = None;
    MFCreateAttributes(&mut attrs, 1).map_err(mf_err)?;
    let attrs =
        attrs.ok_or_else(|| AppError::Internal("MFCreateAttributes returned null".into()))?;
    attrs
        .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
        .map_err(mf_err)?;
    MFCreateSourceReaderFromURL(&url, &attrs).map_err(mf_err)
}

/// Force the first video stream's output type to RGB32 (BGRA in memory).
/// 将首个视频流的输出类型设为 RGB32（内存中为 BGRA）。
unsafe fn configure_rgb32(reader: &IMFSourceReader) -> Result<()> {
    let mt = MFCreateMediaType().map_err(mf_err)?;
    mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
        .map_err(mf_err)?;
    mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
        .map_err(mf_err)?;
    reader
        .SetCurrentMediaType(FIRST_VIDEO_STREAM, None, &mt)
        .map_err(mf_err)?;
    Ok(())
}

/// Output frame geometry after RGB32 negotiation: width/height + signed row stride.
/// RGB32 协商后的输出帧几何：宽/高 + 带符号行 stride。
struct Geometry {
    width: u32,
    height: u32,
    /// Signed stride: negative ⇒ bottom-up rows.
    /// 带符号 stride：负值 ⇒ 行为 bottom-up。
    stride: i32,
}

unsafe fn output_geometry(reader: &IMFSourceReader) -> Result<Geometry> {
    let mt = reader
        .GetCurrentMediaType(FIRST_VIDEO_STREAM)
        .map_err(mf_err)?;
    let (width, height) = attr_size(&mt, &MF_MT_FRAME_SIZE)
        .ok_or_else(|| AppError::Internal("MF: missing frame size | 缺少帧尺寸".into()))?;
    // Default stride is stored as a u32 but is semantically i32 (sign = orientation).
    // 默认 stride 以 u32 存储，但语义为 i32（符号 = 朝向）。
    let stride = mt
        .GetUINT32(&MF_MT_DEFAULT_STRIDE)
        .map(|s| s as i32)
        .unwrap_or((width as i32) * 4);
    Ok(Geometry {
        width,
        height,
        stride,
    })
}

// ── Frame reading ───────────────────────────────────────────────────────────────

/// Seek to `t_100ns` (100-ns units) and read one decoded RGBA frame.
/// 跳转到 `t_100ns`（100 纳秒单位）并读取一帧解码后的 RGBA。
unsafe fn read_frame_at(
    reader: &IMFSourceReader,
    t_100ns: i64,
    geom: &Geometry,
) -> Result<DecodedImage> {
    if t_100ns > 0 {
        // GUID_NULL time format = 100-ns reference time. `PROPVARIANT::from(i64)` → VT_I8.
        // GUID_NULL 时间格式 = 100 纳秒参考时间。`PROPVARIANT::from(i64)` → VT_I8。
        let pos = PROPVARIANT::from(t_100ns);
        let _ = reader.SetCurrentPosition(&GUID::zeroed(), &pos);
    }
    // Read up to a few samples — null samples (stream ticks / gaps) are skipped.
    // 读取至多数个样本 —— null 样本（流 tick / 间隙）跳过。
    for _ in 0..16 {
        let mut flags: u32 = 0;
        let mut timestamp: i64 = 0;
        let mut sample: Option<IMFSample> = None;
        reader
            .ReadSample(
                FIRST_VIDEO_STREAM,
                0,
                None,
                Some(&mut flags),
                Some(&mut timestamp),
                Some(&mut sample),
            )
            .map_err(mf_err)?;

        if (flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
            return Err(AppError::Internal(
                "MF: end of stream before a frame | 帧前已到流尾".into(),
            ));
        }
        if let Some(sample) = sample {
            return sample_to_rgba(&sample, geom);
        }
        // null sample → continue reading
    }
    Err(AppError::Internal(
        "MF: no decodable sample | 无可解码样本".into(),
    ))
}

/// Convert an `IMFSample` (RGB32, memory order B,G,R,X) into a top-down RGBA `DecodedImage`,
/// honouring the (possibly negative) stride.
/// 将 `IMFSample`（RGB32，内存序 B,G,R,X）转换为 top-down 的 RGBA `DecodedImage`，
/// 并尊重（可能为负的）stride。
unsafe fn sample_to_rgba(sample: &IMFSample, geom: &Geometry) -> Result<DecodedImage> {
    let buffer = sample.ConvertToContiguousBuffer().map_err(mf_err)?;
    let mut data: *mut u8 = std::ptr::null_mut();
    let mut cur_len: u32 = 0;
    buffer
        .Lock(&mut data, None, Some(&mut cur_len))
        .map_err(mf_err)?;

    // RAII-ish: always Unlock even on early return.
    let result = (|| {
        if data.is_null() {
            return Err(AppError::Internal("MF: locked null buffer".into()));
        }
        let cur = cur_len as usize;
        // SAFETY: `data` 已校验非空；MF 的 Lock 契约保证 locked 缓冲区自 `data` 起至少
        // `cur` 字节有效。转成切片后下方逐像素拷贝走安全索引（守卫保证不越界）。
        let src = std::slice::from_raw_parts(data, cur);
        let out = copy_bgr32_to_rgba(
            src,
            geom.width as usize,
            geom.height as usize,
            geom.stride.unsigned_abs() as usize,
            geom.stride < 0,
        );
        Ok(DecodedImage {
            pixels: out,
            width: geom.width,
            height: geom.height,
        })
    })();

    let _ = buffer.Unlock();
    result
}

/// 把 MF 输出的 RGB32（内存序 B,G,R,X，每像素 4B，可能 bottom-up + stride 对齐填充）
/// 转成 top-down 的紧凑 RGBA。抽成纯函数（输入 `src` 切片）以便对边界单测。
///
/// 🔴 边界守卫（Part3 Q16 / §3.8.1）：行内对像素 `x` 的最大访问下标是 `s+2`（R 通道），
/// 故仅当 `s + 3 > cur`（即 `s+2 >= cur`，下一个像素已越界）才 break。
/// 旧实现用 `s + 3 >= cur`，在 `s + 3 == cur`（恰好最后一个合法像素，`s+2 == cur-1` 仍有效）
/// 时即提前 break，导致宽幅/紧凑对齐视频帧的尾像素被静默填黑。
fn copy_bgr32_to_rgba(
    src: &[u8],
    w: usize,
    h: usize,
    abs_stride: usize,
    bottom_up: bool,
) -> Vec<u8> {
    let cur = src.len();
    let mut out = vec![0u8; w * h * 4];
    for row in 0..h {
        // For bottom-up frames, image row 0 (top) is the last row in memory.
        // bottom-up 帧：图像第 0 行（顶部）是内存中的最后一行。
        let mem_row = if bottom_up { h - 1 - row } else { row };
        let src_off = mem_row * abs_stride;
        let dst_off = row * w * 4;
        for x in 0..w {
            let s = src_off + x * 4;
            if s + 3 > cur {
                break;
            }
            let d = dst_off + x * 4;
            out[d] = src[s + 2]; // R ← byte[2]
            out[d + 1] = src[s + 1]; // G ← byte[1]
            out[d + 2] = src[s]; // B ← byte[0]
            out[d + 3] = 255; // A (RGB32 has no alpha)
        }
    }
    out
}

// ── Attribute helpers ─────────────────────────────────────────────────────────

/// Read a packed (width, height) attribute (e.g. `MF_MT_FRAME_SIZE`).
/// 读取打包的 (宽, 高) 属性（如 `MF_MT_FRAME_SIZE`）。
unsafe fn attr_size(mt: &IMFMediaType, key: &GUID) -> Option<(u32, u32)> {
    // Frame size is a u64 attribute: high 32 bits = width, low 32 bits = height.
    // 帧尺寸为 u64 属性：高 32 位 = 宽，低 32 位 = 高。
    let packed = mt.GetUINT64(key).ok()?;
    Some(((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32))
}

/// Read a packed (numerator, denominator) ratio attribute (e.g. `MF_MT_FRAME_RATE`).
/// 读取打包的 (分子, 分母) 比率属性（如 `MF_MT_FRAME_RATE`）。
unsafe fn attr_ratio(mt: &IMFMediaType, key: &GUID) -> Option<(u32, u32)> {
    let packed = mt.GetUINT64(key).ok()?;
    Some(((packed >> 32) as u32, (packed & 0xFFFF_FFFF) as u32))
}

/// Total duration in ms via the media source's `MF_PD_DURATION` (a 100-ns VT_UI8 PROPVARIANT).
/// 通过媒体源的 `MF_PD_DURATION`（100 纳秒 VT_UI8 PROPVARIANT）取总时长（毫秒）。
unsafe fn read_duration_ms(reader: &IMFSourceReader) -> u64 {
    match reader.GetPresentationAttribute(MEDIASOURCE, &MF_PD_DURATION) {
        Ok(pv) => {
            let v = PropVariantToInt64(&pv).unwrap_or(0);
            if v > 0 {
                (v as u64) / 10_000
            } else {
                0
            }
        }
        Err(_) => 0,
    }
}

/// Map a video subtype GUID to a short codec label, best-effort.
/// 尽力将视频子类型 GUID 映射为简短编解码标签。
fn codec_label(subtype: GUID) -> Option<String> {
    let name = if subtype == MFVideoFormat_H264 {
        "H264"
    } else if subtype == MFVideoFormat_HEVC || subtype == MFVideoFormat_HEVC_ES {
        "HEVC"
    } else if subtype == MFVideoFormat_MPEG2 {
        "MPEG2"
    } else if subtype == MFVideoFormat_MP4V {
        "MPEG4"
    } else if subtype == MFVideoFormat_WMV3 {
        "WMV3"
    } else if subtype == MFVideoFormat_WVC1 {
        "VC1"
    } else {
        return None;
    };
    Some(name.to_string())
}

/// Clamp the raw `MF_MT_VIDEO_ROTATION` to {0, 90, 180, 270}.
/// 将原始 `MF_MT_VIDEO_ROTATION` 归一到 {0, 90, 180, 270}。
fn normalize_rotation(raw: u32) -> i32 {
    match raw {
        90 => 90,
        180 => 180,
        270 => 270,
        _ => 0,
    }
}

// ── Image post-processing (image crate) ─────────────────────────────────────────

/// Rotate a decoded frame to upright per `rotation` degrees (clockwise), swapping w/h for 90/270.
/// 按 `rotation` 度（顺时针）把解码帧旋转正立，90/270 时交换宽高。
fn apply_rotation(img: DecodedImage, rotation: i32) -> DecodedImage {
    if rotation == 0 {
        return img;
    }
    let Some(rgba) = image::RgbaImage::from_raw(img.width, img.height, img.pixels) else {
        // 缓冲尺寸不符（理论上不会）：原样返回，避免 panic。
        return DecodedImage {
            pixels: Vec::new(),
            width: 0,
            height: 0,
        };
    };
    let dyn_img = image::DynamicImage::ImageRgba8(rgba);
    let rotated = match rotation {
        90 => dyn_img.rotate90(),
        180 => dyn_img.rotate180(),
        270 => dyn_img.rotate270(),
        _ => dyn_img,
    };
    let out = rotated.to_rgba8();
    let (w, h) = (out.width(), out.height());
    DecodedImage {
        pixels: out.into_raw(),
        width: w,
        height: h,
    }
}

/// Downscale a decoded frame to `tw × th` (used for uniform sprite cells).
/// 将解码帧缩放到 `tw × th`（用于统一的雪碧图格）。
fn resize_rgba(img: &DecodedImage, tw: u32, th: u32) -> DecodedImage {
    if img.width == tw && img.height == th {
        return DecodedImage {
            pixels: img.pixels.clone(),
            width: tw,
            height: th,
        };
    }
    use fast_image_resize::images::Image as FirImage;
    use fast_image_resize::pixels::PixelType;
    use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};

    // Bind the clone to a local so the borrow outlives `from_slice_u8` (no temporary drop).
    // 将克隆绑定到局部变量，使借用比 `from_slice_u8` 活得更久（避免临时值被释放）。
    let mut src_buf = img.pixels.clone();
    let src = match FirImage::from_slice_u8(
        img.width.max(1),
        img.height.max(1),
        &mut src_buf,
        PixelType::U8x4,
    ) {
        Ok(s) => s,
        Err(_) => {
            return DecodedImage {
                pixels: img.pixels.clone(),
                width: img.width,
                height: img.height,
            }
        }
    };
    let mut dst = FirImage::new(tw.max(1), th.max(1), PixelType::U8x4);
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
    let mut resizer = Resizer::new();
    if resizer.resize(&src, &mut dst, &opts).is_err() {
        return DecodedImage {
            pixels: img.pixels.clone(),
            width: img.width,
            height: img.height,
        };
    }
    DecodedImage {
        pixels: dst.into_vec(),
        width: tw,
        height: th,
    }
}

/// Cheap "is this frame ~black" check (skip leading black frames for covers).
/// 廉价的「该帧是否接近全黑」判断（封面跳过片头黑帧）。
fn is_too_dark(img: &DecodedImage) -> bool {
    if img.pixels.len() < 4 {
        return true;
    }
    // Sample every 64th pixel's luma; cheap and good enough.
    // 每隔 64 个像素采样其亮度；廉价且足够。
    let mut sum: u64 = 0;
    let mut count: u64 = 0;
    let mut i = 0;
    while i + 3 < img.pixels.len() {
        let r = img.pixels[i] as u64;
        let g = img.pixels[i + 1] as u64;
        let b = img.pixels[i + 2] as u64;
        sum += (r * 299 + g * 587 + b * 114) / 1000;
        count += 1;
        i += 4 * 64;
    }
    if count == 0 {
        return true;
    }
    (sum / count) < 16 // 平均亮度 < 16（0-255）视为黑帧
}

/// Map a Windows COM error into our `AppError`.
/// 将 Windows COM 错误映射为我们的 `AppError`。
fn mf_err(e: windows::core::Error) -> AppError {
    AppError::Os(format!("Media Foundation: {e}"))
}

#[cfg(test)]
mod tests {
    use super::copy_bgr32_to_rgba;

    /// 🔴 回归：紧凑对齐（stride==w*4）下，缓冲区恰好覆盖到最后一个像素的 R 通道
    /// （`s+3 == cur`）时，尾像素必须被拷贝、不得填黑。旧 `>=` 守卫会丢这一像素。
    #[test]
    fn tail_pixel_copied_when_buffer_ends_exactly() {
        // w=2,h=1：像素0 用字节[0..3]，像素1 用字节[4..6]（B,G,R）。
        // cur=7 → 像素1 的 s=4，s+3=7==cur：新守卫 7>7=false 仍拷贝；旧守卫 7>=7=true 会丢。
        let src = [10u8, 11, 12, 99, 40, 50, 60]; // 7 字节
        let out = copy_bgr32_to_rgba(&src, 2, 1, 8, false);
        // 像素0：R=src[2],G=src[1],B=src[0]
        assert_eq!(&out[0..4], &[12, 11, 10, 255]);
        // 像素1（尾像素）：R=src[6],G=src[5],B=src[4] —— 不得为黑
        assert_eq!(&out[4..8], &[60, 50, 40, 255]);
    }

    /// bottom-up 帧（stride<0 → 此处 bottom_up=true）：图像第 0 行取自内存最后一行。
    #[test]
    fn bottom_up_reverses_row_order() {
        // w=1,h=2,stride=4：内存行0=[1,2,3,0]，内存行1=[4,5,6,0]。
        let src = [1u8, 2, 3, 0, 4, 5, 6, 0];
        let out = copy_bgr32_to_rgba(&src, 1, 2, 4, true);
        // 图像行0 = 内存行1：R=6,G=5,B=4
        assert_eq!(&out[0..4], &[6, 5, 4, 255]);
        // 图像行1 = 内存行0：R=3,G=2,B=1
        assert_eq!(&out[4..8], &[3, 2, 1, 255]);
    }

    /// stride 含填充（abs_stride > w*4）：每行尾部 padding 字节被跳过，不污染输出。
    #[test]
    fn padded_stride_skips_alignment_bytes() {
        // w=1,h=2,abs_stride=8（4 像素数据 + 4 填充）。
        let src = [9u8, 8, 7, 0, 0xAA, 0xBB, 0xCC, 0xDD, 6, 5, 4, 0, 0, 0, 0, 0];
        let out = copy_bgr32_to_rgba(&src, 1, 2, 8, false);
        assert_eq!(&out[0..4], &[7, 8, 9, 255]); // 行0 像素：R=7,G=8,B=9
        assert_eq!(&out[4..8], &[4, 5, 6, 255]); // 行1 像素：R=4,G=5,B=6
    }

    /// 损坏/截断缓冲区（cur 远小于 w*h*4）：守卫保证不 panic，未覆盖区域留零（黑），
    /// 已覆盖的前缀像素仍正确拷贝。
    #[test]
    fn truncated_buffer_does_not_panic() {
        // 声称 2×2，但只给 6 字节（不足 1.5 像素）。
        let src = [10u8, 20, 30, 0, 40, 50];
        let out = copy_bgr32_to_rgba(&src, 2, 2, 8, false);
        assert_eq!(out.len(), 2 * 2 * 4);
        // 像素(0,0)：s=0，s+3=3<=6 → 拷贝 R=30,G=20,B=10
        assert_eq!(&out[0..4], &[30, 20, 10, 255]);
        // 像素(0,1)：s=4，s+3=7>6 → break，留零
        assert_eq!(&out[4..8], &[0, 0, 0, 0]);
    }
}
