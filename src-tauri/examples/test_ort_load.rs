// 独立的 ORT 会话加载测试 — 无超时，精确计时
// 用于确定 CPU 上加载 ViT-B/16 模型实际需要多长时间
//
// 运行：cargo run --example test_ort_load
// (从 src-tauri 目录运行)

fn main() {
    // 设置正确的 ORT DLL 路径
    let ort_dll = r"D:\photoapp\picasa-next\src-tauri\target\debug\onnxruntime.dll";
    std::env::set_var("ORT_DYLIB_PATH", ort_dll);
    println!("ORT_DYLIB_PATH = {}", ort_dll);

    // 初始化 ORT (commit() returns bool in this version)
    println!("Initialising ORT...");
    let t0 = std::time::Instant::now();
    ort::init().with_name("test").commit();
    println!("ORT init took: {:?}", t0.elapsed());

    // 测试 FP16 外部数据格式图像模型（header 3.6MB + weights 164MB .extra_file）
    let image_model =
        r"C:\Users\gf\AppData\Roaming\com.picasanext.app\models\vit-b-16.img.fp16.onnx";
    println!("\n[IMAGE MODEL] Loading: {}", image_model);
    println!(
        "[IMAGE MODEL] File size: {:.1} MB (header only, weights in .extra_file)",
        std::fs::metadata(image_model)
            .map(|m| m.len() as f64 / 1024.0 / 1024.0)
            .unwrap_or(0.0)
    );

    // 先用 Disable 测试（最快，仅解析模型图，无任何优化）
    let t1 = std::time::Instant::now();
    println!("[IMAGE MODEL] Starting with GraphOptimizationLevel::Disable...");
    let result = ort::session::Session::builder()
        .expect("builder failed")
        .with_intra_threads(4)
        .expect("threads failed")
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Disable)
        .expect("opt level failed")
        .commit_from_file(image_model);

    match result {
        Ok(session) => {
            let elapsed = t1.elapsed();
            println!("[IMAGE MODEL] ✅ Loaded (Disable) in {:?}", elapsed);
            println!(
                "[IMAGE MODEL] Inputs: {:?}",
                session
                    .inputs()
                    .iter()
                    .map(|i| i.name())
                    .collect::<Vec<_>>()
            );
        }
        Err(e) => {
            println!(
                "[IMAGE MODEL] ❌ FAILED (Disable) in {:?}: {}",
                t1.elapsed(),
                e
            );
        }
    }

    // 再用 Level1 测试
    let t2 = std::time::Instant::now();
    println!("\n[IMAGE MODEL] Starting with GraphOptimizationLevel::Level1...");
    let result_l1 = ort::session::Session::builder()
        .expect("builder failed")
        .with_intra_threads(4)
        .expect("threads failed")
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level1)
        .expect("opt level failed")
        .commit_from_file(image_model);

    match result_l1 {
        Ok(session) => {
            println!("[IMAGE MODEL] ✅ Loaded (Level1) in {:?}", t2.elapsed());
            println!(
                "[IMAGE MODEL] Inputs: {:?}",
                session
                    .inputs()
                    .iter()
                    .map(|i| i.name())
                    .collect::<Vec<_>>()
            );
        }
        Err(e) => {
            println!(
                "[IMAGE MODEL] ❌ FAILED (Level1) in {:?}: {}",
                t2.elapsed(),
                e
            );
        }
    }

    println!("\nTotal elapsed: {:?}", t0.elapsed());
}
