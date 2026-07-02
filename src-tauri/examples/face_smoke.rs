// 人脸 F2 冒烟测试 / F8 对拍诊断入口（独立程序，CPU 推理）。
// Face F2 smoke test / F8 cross-check entry (standalone, CPU inference).
//
// 用途：模型文件就位后，立即验证「检测 + 对齐 + 嵌入」端到端可跑，并**打印 YuNet/SFace
// 的实际输入/输出名 + shape** —— 这是 F8 核对 face.rs 中 YuNet 输出命名假设的关键诊断。
//
// 运行（从 src-tauri 目录）：
//   cargo run --example face_smoke -- <图片路径> [models目录]
// 默认 models 目录：C:\Users\gf\AppData\Roaming\com.picasanext.app\models
// 需先手动放入 face_detection_yunet_2023mar.onnx 与 face_recognition_sface_2021dec.onnx
// （F7 模型库下载尚未实现；许可待核实，见 face_profile.rs）。

use std::path::PathBuf;

use ort::session::Session;
use picasa_next_lib::ai::engine::SessionPool;
use picasa_next_lib::ai::face::{detect_faces, embed_faces, DetectedFace};
use picasa_next_lib::ai::face_profile::{default_face_profile, find_face_profile};
use picasa_next_lib::engine::traits::DecodedImage;

fn main() {
    // ORT DLL（download-binaries feature 会把它放到 target/debug）。按机器实际路径调整。
    let ort_dll = r"D:\photoapp\picasa-next\src-tauri\target\debug\onnxruntime.dll";
    std::env::set_var("ORT_DYLIB_PATH", ort_dll);
    ort::init().with_name("face_smoke").commit();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: cargo run --example face_smoke -- <图片路径> [models目录]");
        std::process::exit(2);
    }
    let image_path = PathBuf::from(&args[1]);
    let models_dir = PathBuf::from(
        args.get(2)
            .cloned()
            .unwrap_or_else(|| r"C:\Users\gf\AppData\Roaming\com.picasanext.app\models".into()),
    );

    // 选轨：默认轨 YuNet+SFace；设 FACE_PROFILE=scrfd-arcface-r50 可切到 SCRFD+ArcFace 做 F8b 对拍。
    // Track select: default YuNet+SFace; set FACE_PROFILE=scrfd-arcface-r50 for the SCRFD cross-check.
    let profile = match std::env::var("FACE_PROFILE") {
        Ok(id) if !id.is_empty() => find_face_profile(&id).unwrap_or_else(|| {
            eprintln!("未知 FACE_PROFILE={id}，回退默认轨");
            default_face_profile()
        }),
        _ => default_face_profile(),
    };
    println!(
        "人脸模型: {} (检测 {} / 嵌入 {}, dim={})",
        profile.id, profile.detect_file, profile.embed_file, profile.embed_dim
    );

    // ── 加载检测器 + 嵌入器（CPU）──────────────────────────────────────────────
    let detect_pool = load_pool(&models_dir.join(&profile.detect_file), "检测器");
    let embed_pool = load_pool(&models_dir.join(&profile.embed_file), "嵌入器");
    let (detect_pool, embed_pool) = match (detect_pool, embed_pool) {
        (Some(d), Some(e)) => (d, e),
        _ => {
            eprintln!("模型文件缺失，无法冒烟。请先放入 onnx 到: {:?}", models_dir);
            std::process::exit(1);
        }
    };

    // ── 解码图片 → DecodedImage(RGBA)────────────────────────────────────────────
    let img = match image::open(&image_path) {
        Ok(i) => i.to_rgba8(),
        Err(e) => {
            eprintln!("图片解码失败 {:?}: {}", image_path, e);
            std::process::exit(1);
        }
    };
    let (w, h) = img.dimensions();
    let decoded = DecodedImage {
        pixels: img.into_raw(),
        width: w,
        height: h,
    };
    println!("图片: {:?} ({}×{})", image_path, w, h);

    // ── 检测 ────────────────────────────────────────────────────────────────────
    // F8 隔离实验：若设了 FACE_INPUT（指向 "x,y,w,h,lx0,ly0,...,lx4,ly4"），则用外部关键点
    // 直接对齐+嵌入、跳过检测 —— 以排除「检测关键点差异」变量，单独验对齐+SFace 实现是否对。
    let faces = match std::env::var("FACE_INPUT") {
        Ok(p) if std::path::Path::new(&p).exists() => {
            let s = std::fs::read_to_string(&p).unwrap_or_default();
            let nums: Vec<f32> = s
                .trim()
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            if nums.len() < 14 {
                eprintln!(
                    "FACE_INPUT 需 14 个数(bbox4+landmarks10)，实得 {}",
                    nums.len()
                );
                std::process::exit(1);
            }
            let mut lms = [[0f32; 2]; 5];
            for j in 0..5 {
                lms[j] = [nums[4 + 2 * j], nums[5 + 2 * j]];
            }
            println!("[override] 用外部关键点对齐（跳过检测）: {}", p);
            vec![DetectedFace {
                bbox: [nums[0], nums[1], nums[2], nums[3]],
                landmarks: lms,
                score: 1.0,
            }]
        }
        _ => match detect_faces(&detect_pool, &decoded, &profile) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("检测失败: {}", e);
                std::process::exit(1);
            }
        },
    };
    println!("\n检测到 {} 张人脸:", faces.len());
    for (i, f) in faces.iter().enumerate() {
        println!(
            "  [{}] score={:.3} bbox=[{:.0},{:.0},{:.0},{:.0}] quality={:.3}",
            i,
            f.score,
            f.bbox[0],
            f.bbox[1],
            f.bbox[2],
            f.bbox[3],
            f.quality(w, h)
        );
        println!("      关键点: {:?}", f.landmarks);
    }
    if faces.is_empty() {
        println!("（无脸，结束）");
        return;
    }

    // ── 嵌入 ────────────────────────────────────────────────────────────────────
    let embs = match embed_faces(&embed_pool, &decoded, &faces, &profile) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("嵌入失败: {}", e);
            std::process::exit(1);
        }
    };
    println!(
        "\n嵌入向量: {} 条, 维度 {}",
        embs.len(),
        embs.first().map(|v| v.len()).unwrap_or(0)
    );
    if let Some(v0) = embs.first() {
        let head: Vec<String> = v0.iter().take(5).map(|x| format!("{:.3}", x)).collect();
        println!("  脸0 前5维: [{}]", head.join(", "));
    }
    // F8 对拍：写完整向量到固定文件，供 face_crosscheck.py 算与 OpenCV 的 cosine。
    let mut dump = String::new();
    for v in &embs {
        let line: Vec<String> = v.iter().map(|x| format!("{:.6}", x)).collect();
        dump.push_str(&line.join(","));
        dump.push('\n');
    }
    let dump_path = r"D:\photoapp\picasa-next\src-tauri\target\rust_emb.txt";
    match std::fs::write(dump_path, dump) {
        Ok(_) => println!("Rust 嵌入已写: {}", dump_path),
        Err(e) => eprintln!("写嵌入失败: {}", e),
    }
    // 两两 cosine（同人应高、异人应低；F8 对拍参考实现）。
    if embs.len() >= 2 {
        println!("\n两两 cosine 相似度:");
        for i in 0..embs.len() {
            for j in (i + 1)..embs.len() {
                println!("  [{} × {}] = {:.4}", i, j, cosine(&embs[i], &embs[j]));
            }
        }
    }
}

/// 加载单个 onnx 为 CPU SessionPool(容量1)，并打印输入/输出元数据（F8 诊断核心）。
fn load_pool(path: &std::path::Path, label: &str) -> Option<SessionPool> {
    if !path.exists() {
        eprintln!("[{}] 文件不存在: {:?}", label, path);
        return None;
    }
    let session = match build_cpu_session(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[{}] 加载失败: {}", label, e);
            return None;
        }
    };
    // 打印 I/O 名与 shape —— face.rs 的 YuNet 输出命名假设须据此核实。
    println!("[{}] 输入:", label);
    for i in session.inputs() {
        println!("    {} : {:?}", i.name(), i.dtype().tensor_shape());
    }
    println!("[{}] 输出:", label);
    for o in session.outputs() {
        println!("    {} : {:?}", o.name(), o.dtype().tensor_shape());
    }
    let pool = SessionPool::new(1);
    pool.push(session);
    Some(pool)
}

/// 加载 onnx 为 CPU Session。`?` 链需统一 ort::Error（各 builder 步的 Error<T> 泛型不同，
/// and_then 不会做 From 转换），故单列此函数，仿 engine.rs 的 build_session。
fn build_cpu_session(path: &std::path::Path) -> ort::Result<Session> {
    Session::builder()?
        .with_intra_threads(4)?
        .commit_from_file(path)
}

/// 两个已 L2 归一化向量的 cosine（= 点积）。
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
