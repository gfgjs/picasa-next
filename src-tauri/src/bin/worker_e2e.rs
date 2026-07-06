// src-tauri/src/bin/worker_e2e.rs
//! Part4 worker e2e 验收 harness(T16 后形态:黄金向量对拍)。
//!
//! 用真实模型 + 真实 ai_cache 驱动真实 ai-worker 子进程(AiWorkerClient →
//! spawn/握手/SessionInit(sha256)/EmbedBatch/EncodeText/FaceDetectEmbed/
//! close_session,与生产路径同一实现),并与 `tests/golden/worker_e2e_vectors.json`
//! 黄金向量逐项对拍。初版黄金由 T16-S2 删除前的**进程内参考侧**导出(`ef17d42`),
//! 语义 =「与进程内实现逐位一致」;此后模型变更须重生成(见 --export-golden),
//! 黄金语义降为「当前 worker 行为快照」的回归基线。
//!
//! 对拍有硬数值预期:黄金与 worker 共用 scrollery-ai-core 同一套 preprocess/encode,
//! B/16 导出为固定 batch=1(批内逐张分块),EP 同为 auto(DirectML)——余弦应 >0.999,
//! 不是「大致相似」。文本塔恒 CPU,close→重 init 后应完全确定(≈1.0)。
//!
//! 覆盖点(对应 T17 交付面):
//!   1. `ai_worker_exe()` 同目录发现(worker_e2e.exe 与 ai-worker.exe 同落 target/debug,
//!      生产分发布局的发现逻辑原样生效,不走 env 覆盖);
//!   2. spawn + 握手(worker_id/能力校验)+ SessionInit 模型完整性(len+sha256 现算);
//!   3. EncodeText(搜索冷路径:先于任何分析运行的首次查询)+ 向量对拍;
//!   4. EmbedBatch(真实 ai_cache webp,批解码/预处理/推理/blob 装配)+ 逐项对拍;
//!   5. 检索一致性:每条查询在两后端各自的「文本×图像」打分下 top-1 必须一致
//!      (语义搜索关心的是排序,不止是向量近似);
//!   6. close_session → 再 EncodeText:会话重建路径 + sha 备忘命中(不重算 GB 级哈希);
//!   7. face 对拍(face 接线波,YuNet+SFace 在盘时):FaceDetectEmbed 几何/嵌入/解码尺寸
//!      三重对拍 + 会话切换(CLIP-only → 合并)+ 超集复用(合并会话服务 CLIP-only 查询
//!      不重 init)的实测覆盖。
//!
//! 用法:
//!   cargo build -p ai-worker && cargo run -p scrollery --bin worker_e2e [-- <models_dir> [ai_cache_dir]]
//! 缺省 models_dir = `%APPDATA%/com.scrollery.app/models`,
//!       ai_cache_dir = `%APPDATA%/com.scrollery.app/cache/ai_thumbs`(开发机常规位置)。
//! 退出码:全部通过 = 0;任一检查失败 = 1(打印 ❌ 明细)。
//!
//! 黄金向量重生成:`worker_e2e --export-golden [<models_dir> [ai_cache_dir]]`
//! (T16 后经 worker 侧生成:8 图 + 3 查询 + face 几何/嵌入,含 profile id 与模型
//! sha256 前缀防错拍;模型文件变更时由 sha 前缀检测出黄金失效并提示本流程)。

use std::path::{Path, PathBuf};
use std::time::Instant;

use exotic_protocol::{EmbedItem, FaceItem};
use scrollery_lib::ai::face_profile::default_face_profile;
use scrollery_lib::ai::profile::{resolve_profile, ModelProfile, DEFAULT_PROFILE_ID};
use scrollery_lib::ai::worker_client::{ai_worker_exe, AiWorkerClient, SessionSpec};
use scrollery_lib::exotic::worker::{EmbedItemOutcome, FaceItemOutcome};

/// 对拍采样张数(均匀取自 ai_cache 全量,确定性)。
const SAMPLE_COUNT: usize = 8;
/// 吞吐相位采样张数(Phase B2,T18.5 worker 并行解码的回归证据;
/// 兼作两个 SessionSpec 的 batch_size 声明上限,对拍批 8 ≤ 64 不受影响)。
const BENCH_COUNT: usize = 64;
/// 语义搜索代表性查询(覆盖人物/物体/场景三类,检验排序一致性)。
const QUERIES: &[&str] = &[
    "a photo of a person",
    "a red car on the street",
    "food on a plate",
];
/// 双后端余弦一致性阈值:同代码同 EP 同硬件,经验上 >0.999;放到 0.995 容 fp16 抖动。
const COS_THRESHOLD: f32 = 0.995;
/// close→重 init 后同一查询的自洽阈值(CPU 文本塔完全确定,应逐位一致)。
const REINIT_THRESHOLD: f32 = 0.9999;

/// 黄金向量文件路径(编译期定位到 src-tauri/tests/golden,不受运行目录影响)。
const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/worker_e2e_vectors.json"
);

/// 黄金向量文件(T16-S3):进程内参考侧的最后一次导出快照。
/// sha16 = 对应模型文件 sha256 的前 16 hex——模型变更时黄金失效,提示重生成。
#[derive(serde::Serialize, serde::Deserialize)]
struct GoldenFile {
    profile_id: String,
    image_sha16: String,
    text_sha16: String,
    /// 采样键(黄金对拍必须用这组键,不得重新枚举磁盘——缓存会随 GC/新分析漂移)。
    ai_cache_keys: Vec<String>,
    image_vectors: Vec<Vec<f32>>,
    queries: Vec<String>,
    query_vectors: Vec<Vec<f32>>,
    face: Option<GoldenFace>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GoldenFace {
    face_profile_id: String,
    detect_sha16: String,
    embed_sha16: String,
    det_score_thresh: f32,
    /// face 段自有键集(基础采样若全零脸,导出时会扫描补足有脸样本——
    /// 零脸黄金只能锁「无误报」,锁不住嵌入/几何回归)。
    keys: Vec<String>,
    /// 与 `keys` 对齐。
    items: Vec<GoldenFaceItem>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GoldenFaceItem {
    width: u32,
    height: u32,
    faces: Vec<GoldenFaceDet>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GoldenFaceDet {
    bbox: [f32; 4],
    landmarks: [[f32; 2]; 5],
    score: f32,
    embedding: Vec<f32>,
}

/// 模型文件 sha256 前 16 hex(黄金文件的防错拍指纹)。
fn sha16(models_dir: &Path, file: &str) -> String {
    let hex = scrollery_lib::utils::hash::sha256_hex_of_file(&models_dir.join(file))
        .expect("模型文件 sha256 计算失败");
    hex[..16].to_string()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return f32::NAN;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb).max(f32::EPSILON)
}

/// 遍历 `{ai_cache_dir}/{xx}/{hex}.webp` 两级布局,排序后均匀采样 `n` 个 key。
fn collect_samples(ai_cache_dir: &Path, n: usize) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let Ok(prefixes) = std::fs::read_dir(ai_cache_dir) else {
        return keys;
    };
    for prefix in prefixes.flatten() {
        let Ok(files) = std::fs::read_dir(prefix.path()) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().is_some_and(|e| e == "webp") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    keys.push(stem.to_string());
                }
            }
        }
    }
    keys.sort();
    if keys.len() <= n {
        return keys;
    }
    // 均匀间隔采样(而非取前 N):覆盖 key 空间,避免只测同一批次入库的图。
    let step = keys.len() / n;
    (0..n).map(|i| keys[i * step].clone()).collect()
}

/// 与 ai-worker 的 `cache_webp_path` 同构拼路径(白名单校验此处不需要:key 来自实盘枚举)。
fn webp_path(ai_cache_dir: &Path, key: &str) -> PathBuf {
    ai_cache_dir.join(&key[..2]).join(format!("{key}.webp"))
}

/// 黄金向量重生成(T16 后:经真实 ai-worker 生成——进程内参考侧已删,初版黄金
/// 见 git `ef17d42`;本路径产出「当前 worker 行为快照」作为后续回归基线)。
fn export_golden_via_worker(models_dir: &Path, ai_cache_dir: &Path, profile: &ModelProfile) {
    let keys = collect_samples(ai_cache_dir, SAMPLE_COUNT);
    assert!(!keys.is_empty(), "ai_cache 无存货:先在应用内跑一次 AI 分析");
    println!("采样 {} 张 ai_cache 图:{keys:?}", keys.len());

    let never = || false;
    let mut client = AiWorkerClient::new();
    let spec = SessionSpec {
        profile: profile.clone(),
        face_profile: None,
        models_dir: models_dir.to_path_buf(),
        ai_cache_dir: ai_cache_dir.to_path_buf(),
        image_provider: "auto".to_string(),
        batch_size: BENCH_COUNT as u32,
    };
    let queries: Vec<String> = QUERIES.iter().map(|s| s.to_string()).collect();
    let query_vectors = client
        .encode_text(&spec, &queries, &never)
        .expect("worker EncodeText 失败");
    let items: Vec<EmbedItem> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| EmbedItem {
            item_id: i as i64,
            cache_key: key.clone(),
            fingerprint: key.clone(),
        })
        .collect();
    let image_vectors: Vec<Vec<f32>> = client
        .embed_batch(&spec, &items, &never)
        .expect("worker EmbedBatch 失败")
        .into_iter()
        .map(|o| match o {
            EmbedItemOutcome::Ok(v) => v,
            EmbedItemOutcome::Err(code) => panic!("黄金生成:嵌入项失败[{}]", code.as_str()),
        })
        .collect();

    let face = default_face_profile();
    let face_installed =
        models_dir.join(&face.detect_file).exists() && models_dir.join(&face.embed_file).exists();
    let face_golden = if face_installed {
        let spec_face = SessionSpec {
            face_profile: Some(face.clone()),
            ..spec.clone()
        };
        // 分批经 worker 跑 face(几何+嵌入+实际解码尺寸),批 ≤ BENCH_COUNT。
        let run = |client: &mut AiWorkerClient, ks: &[String]| -> Vec<GoldenFaceItem> {
            let mut out = Vec::with_capacity(ks.len());
            for chunk in ks.chunks(BENCH_COUNT) {
                let fitems: Vec<FaceItem> = chunk
                    .iter()
                    .enumerate()
                    .map(|(i, key)| FaceItem {
                        item_id: i as i64,
                        cache_key: None,
                        source_path: Some(
                            webp_path(ai_cache_dir, key).to_string_lossy().into_owned(),
                        ),
                        fingerprint: format!("{i}:{:.4}", face.det_score_thresh),
                    })
                    .collect();
                let outs = client
                    .face_detect_embed(&spec_face, &fitems, face.det_score_thresh, &|| false)
                    .expect("worker FaceDetectEmbed 失败");
                for o in outs {
                    match o {
                        FaceItemOutcome::Ok {
                            faces,
                            embeddings,
                            width,
                            height,
                        } => out.push(GoldenFaceItem {
                            width,
                            height,
                            faces: faces
                                .iter()
                                .zip(&embeddings)
                                .map(|(f, emb)| GoldenFaceDet {
                                    bbox: f.bbox,
                                    landmarks: f.landmarks,
                                    score: f.score,
                                    embedding: emb.clone(),
                                })
                                .collect(),
                        }),
                        FaceItemOutcome::Err(code) => {
                            panic!("黄金生成:face 项失败[{}]", code.as_str())
                        }
                    }
                }
            }
            out
        };
        let mut face_keys = keys.clone();
        let mut items_g = run(&mut client, &face_keys);
        // 基础采样全零脸时补扫:零脸黄金锁不住嵌入/几何回归。
        if items_g.iter().all(|it| it.faces.is_empty()) {
            println!("基础采样 0 脸 → 扫描候选补足有脸样本(上限 96 候选/4 命中)…");
            let candidates: Vec<String> = collect_samples(ai_cache_dir, 96)
                .into_iter()
                .filter(|k| !face_keys.contains(k))
                .collect();
            let extra = run(&mut client, &candidates);
            let mut hits = 0usize;
            for (k, it) in candidates.into_iter().zip(extra) {
                if !it.faces.is_empty() {
                    face_keys.push(k);
                    items_g.push(it);
                    hits += 1;
                    if hits >= 4 {
                        break;
                    }
                }
            }
            println!("补扫命中 {hits} 张有脸样本");
        }
        Some(GoldenFace {
            face_profile_id: face.id.to_string(),
            detect_sha16: sha16(models_dir, &face.detect_file),
            embed_sha16: sha16(models_dir, &face.embed_file),
            det_score_thresh: face.det_score_thresh,
            keys: face_keys,
            items: items_g,
        })
    } else {
        println!("face 模型未安装:黄金文件不含 face 段(安装后可重导出)");
        None
    };
    client.close_session();

    let golden = GoldenFile {
        profile_id: profile.id.to_string(),
        image_sha16: sha16(models_dir, &profile.image_file),
        text_sha16: sha16(models_dir, &profile.text_file),
        ai_cache_keys: keys,
        image_vectors,
        queries,
        query_vectors,
        face: face_golden,
    };
    let path = Path::new(GOLDEN_PATH);
    std::fs::create_dir_all(path.parent().expect("GOLDEN_PATH 必有父目录"))
        .expect("创建 golden 目录失败");
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&golden).expect("golden 序列化失败"),
    )
    .expect("写 golden 文件失败");
    println!(
        "== 黄金向量已重生成(worker 侧快照):{}(keys={},face={})==",
        path.display(),
        golden.ai_cache_keys.len(),
        golden.face.is_some()
    );
}

fn main() {
    // Supervisor/AiWorkerClient 的 tracing 日志对诊断 e2e 失败至关重要,接到 stderr。
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut raw: Vec<String> = std::env::args().skip(1).collect();
    let export_golden = raw.first().is_some_and(|s| s == "--export-golden");
    if export_golden {
        raw.remove(0);
    }
    let mut args = raw.into_iter();
    let appdata = || std::env::var("APPDATA").expect("APPDATA 环境变量缺失,请显式传参");
    let models_dir: PathBuf = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(appdata())
            .join("com.scrollery.app")
            .join("models")
    });
    let ai_cache_dir: PathBuf = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(appdata())
            .join("com.scrollery.app")
            .join("cache")
            .join("ai_thumbs")
    });

    println!("== Part4 worker e2e 验收 ==");
    println!("models_dir   = {}", models_dir.display());
    println!("ai_cache_dir = {}", ai_cache_dir.display());

    // ── Phase 0:资产检查(快失败,给出可操作指引)────────────────────────────────
    assert!(
        models_dir.exists(),
        "models 目录不存在:先在应用内下载模型或显式传参"
    );
    let worker_exe = ai_worker_exe().unwrap_or_else(|e| {
        panic!("{e}\n→ 先构建 worker:cargo build -p ai-worker(与本 harness 同落 target 目录)")
    });
    println!("ai-worker    = {}(同目录发现 ✅)", worker_exe.display());

    let profile = resolve_profile(DEFAULT_PROFILE_ID, None).expect("默认 CLIP profile 必须可解析");
    println!(
        "profile      = {}(embed_dim={}, image_size={})",
        profile.id, profile.embed_dim, profile.image_size
    );

    // ── 黄金重生成模式(T16 后经 worker 侧)────────────────────────────────────────
    if export_golden {
        export_golden_via_worker(&models_dir, &ai_cache_dir, &profile);
        return;
    }

    let mut failures: Vec<String> = Vec::new();

    // ── Phase A:黄金向量装载 + 防错拍 ────────────────────────────────────────────
    println!();
    println!("── Phase A:黄金向量装载({GOLDEN_PATH})──");
    let golden: GoldenFile = serde_json::from_slice(
        &std::fs::read(GOLDEN_PATH).expect("黄金文件缺失:先跑 --export-golden 生成"),
    )
    .expect("黄金文件解析失败(损坏?重跑 --export-golden)");
    assert_eq!(
        golden.profile_id, profile.id,
        "黄金 profile 与当前默认不符:重跑 --export-golden"
    );
    assert_eq!(
        golden.image_sha16,
        sha16(&models_dir, &profile.image_file),
        "图像塔模型已变更,黄金失效:重跑 --export-golden"
    );
    assert_eq!(
        golden.text_sha16,
        sha16(&models_dir, &profile.text_file),
        "文本塔模型已变更,黄金失效:重跑 --export-golden"
    );
    assert_eq!(
        golden.queries,
        QUERIES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "QUERIES 常量与黄金不一致:重跑 --export-golden"
    );
    let keys = golden.ai_cache_keys.clone();
    for key in &keys {
        assert!(
            webp_path(&ai_cache_dir, key).exists(),
            "黄金采样键 {key} 的 ai_cache 已不在盘(GC/重分析漂移):重跑 --export-golden"
        );
    }
    let ref_images = golden.image_vectors.clone();
    let ref_texts = golden.query_vectors.clone();
    println!(
        "[A] 黄金装载 ✅:keys={},查询={},face 段={}(sha16 全部吻合)",
        keys.len(),
        ref_texts.len(),
        golden.face.is_some()
    );

    // ── Phase B:worker 后端(与 ai_backend=worker 生产路径同一实现)──────────────────
    println!();
    println!("── Phase B:worker 后端(AiWorkerClient → 真实 ai-worker 子进程)──");
    let spec = SessionSpec {
        profile: profile.clone(),
        face_profile: None,
        models_dir: models_dir.clone(),
        ai_cache_dir: ai_cache_dir.clone(),
        image_provider: "auto".to_string(),
        batch_size: BENCH_COUNT as u32,
    };
    let never = || false;
    let mut client = AiWorkerClient::new();

    // 冷路径:spawn + 握手 + sha256(~0.4GB 模型流式哈希)+ SessionInit(模型加载)+ 编码。
    let t = Instant::now();
    let worker_texts = client
        .encode_text(&spec, &[QUERIES[0].to_string()], &never)
        .expect("worker EncodeText(冷启动)失败");
    println!(
        "[B] 冷启动首查 {:.1}s(spawn+握手+sha256+SessionInit+EncodeText)",
        t.elapsed().as_secs_f64()
    );
    let mut worker_texts = worker_texts; // [0] = QUERIES[0]

    // 热路径:会话已建,补齐其余查询(顺带验证快照匹配零帧复用)。
    let t = Instant::now();
    let rest: Vec<String> = QUERIES[1..].iter().map(|s| s.to_string()).collect();
    worker_texts.extend(
        client
            .encode_text(&spec, &rest, &never)
            .expect("worker EncodeText(热路径)失败"),
    );
    println!(
        "[B] 热路径 {} 条查询 {:.2}s",
        rest.len(),
        t.elapsed().as_secs_f64()
    );

    let items: Vec<EmbedItem> = keys
        .iter()
        .enumerate()
        .map(|(i, key)| EmbedItem {
            item_id: i as i64,
            cache_key: key.clone(),
            fingerprint: key.clone(),
        })
        .collect();
    let t = Instant::now();
    let outcomes = client
        .embed_batch(&spec, &items, &never)
        .expect("worker EmbedBatch 失败");
    println!(
        "[B] {} 张图像 EmbedBatch {:.2}s",
        items.len(),
        t.elapsed().as_secs_f64()
    );
    let mut worker_images: Vec<Option<Vec<f32>>> = Vec::with_capacity(outcomes.len());
    for (key, o) in keys.iter().zip(outcomes) {
        match o {
            EmbedItemOutcome::Ok(v) => worker_images.push(Some(v)),
            EmbedItemOutcome::Err(code) => {
                failures.push(format!("EmbedBatch 项 {key} 失败[{}]", code.as_str()));
                worker_images.push(None);
            }
        }
    }

    // ── Phase B2:吞吐实测(T18.5 worker 并行解码回归证据;会话已热)───────────────────
    // 不设硬性时限断言(机器相关);逐项成功是硬检查。修复前基线(2026-07-03 本机
    // debug 构建):~145ms/张(串行解码);用户 GUI 实测 1550 张 ≈ 5min。
    println!();
    println!("── Phase B2:吞吐实测({BENCH_COUNT} 张单批)──");
    let bench_keys = collect_samples(&ai_cache_dir, BENCH_COUNT);
    let bitems: Vec<EmbedItem> = bench_keys
        .iter()
        .enumerate()
        .map(|(i, key)| EmbedItem {
            item_id: 10_000 + i as i64,
            cache_key: key.clone(),
            fingerprint: key.clone(),
        })
        .collect();
    let t = Instant::now();
    let bouts = client
        .embed_batch(&spec, &bitems, &never)
        .expect("worker 吞吐批失败");
    let secs = t.elapsed().as_secs_f64();
    let ok_n = bouts
        .iter()
        .filter(|o| matches!(o, EmbedItemOutcome::Ok(_)))
        .count();
    let per_ms = secs * 1000.0 / bitems.len().max(1) as f64;
    println!(
        "[B2] {} 张单批 {secs:.2}s = {per_ms:.0}ms/张(Ok {ok_n}/{};×1550 外推 ≈{:.0}s)",
        bitems.len(),
        bitems.len(),
        per_ms * 1550.0 / 1000.0
    );
    if ok_n != bitems.len() {
        failures.push(format!("吞吐批逐项成功 {ok_n}/{} 不足额", bitems.len()));
    }

    // ── Phase C:对拍 ────────────────────────────────────────────────────────────────
    println!();
    println!("── Phase C:双后端对拍 ──");
    let mut img_cos: Vec<f32> = Vec::new();
    for ((key, r), w) in keys.iter().zip(&ref_images).zip(&worker_images) {
        let Some(w) = w else { continue };
        let c = cosine(r, w);
        img_cos.push(c);
        let mark = if c >= COS_THRESHOLD { "✅" } else { "❌" };
        println!("  图像 {key}:cos = {c:.6} {mark}");
        // NaN(维度不符等)也判失败,故不用 `c < T`(NaN 时为 false 会漏放)。
        if c.is_nan() || c < COS_THRESHOLD {
            failures.push(format!("图像 {key} 余弦 {c:.6} < {COS_THRESHOLD}"));
        }
    }
    if !img_cos.is_empty() {
        let min = img_cos.iter().copied().fold(f32::INFINITY, f32::min);
        let mean = img_cos.iter().sum::<f32>() / img_cos.len() as f32;
        println!(
            "  图像塔:min = {min:.6},mean = {mean:.6}(n={})",
            img_cos.len()
        );
    }

    for ((q, r), w) in QUERIES.iter().zip(&ref_texts).zip(&worker_texts) {
        let c = cosine(r, w);
        let mark = if c >= COS_THRESHOLD { "✅" } else { "❌" };
        println!("  文本 {q:?}:cos = {c:.6} {mark}");
        // NaN(维度不符等)也判失败,故不用 `c < T`(NaN 时为 false 会漏放)。
        if c.is_nan() || c < COS_THRESHOLD {
            failures.push(format!("文本 {q:?} 余弦 {c:.6} < {COS_THRESHOLD}"));
        }
    }

    // 检索一致性:每条查询在各自后端打分下 top-1 须一致(排序才是搜索的语义)。
    for (qi, q) in QUERIES.iter().enumerate() {
        let top = |txt: &[f32], imgs: &[Option<Vec<f32>>]| -> Option<(usize, f32)> {
            imgs.iter()
                .enumerate()
                .filter_map(|(i, v)| v.as_ref().map(|v| (i, cosine(txt, v))))
                .max_by(|a, b| a.1.total_cmp(&b.1))
        };
        let ref_imgs: Vec<Option<Vec<f32>>> = ref_images.iter().cloned().map(Some).collect();
        let (ri, rs) = top(&ref_texts[qi], &ref_imgs).expect("参考侧无可比图像");
        let Some((wi, ws)) = top(&worker_texts[qi], &worker_images) else {
            failures.push(format!("查询 {q:?} worker 侧无可比图像"));
            continue;
        };
        let mark = if ri == wi { "✅" } else { "❌" };
        println!(
            "  检索 {q:?}:top-1 参考={}({rs:.4}) vs worker={}({ws:.4}) {mark}",
            keys[ri], keys[wi]
        );
        if ri != wi {
            failures.push(format!(
                "查询 {q:?} top-1 不一致:参考 {} vs worker {}",
                keys[ri], keys[wi]
            ));
        }
    }

    // ── Phase D:close_session → 重 init(会话重建 + sha 备忘命中)────────────────────
    println!();
    println!("── Phase D:close_session → 重 init ──");
    client.close_session();
    let t = Instant::now();
    let again = client
        .encode_text(&spec, &[QUERIES[0].to_string()], &never)
        .expect("close 后重查失败(会话重建路径)");
    let c = cosine(&again[0], &worker_texts[0]);
    let mark = if c >= REINIT_THRESHOLD { "✅" } else { "❌" };
    println!(
        "  重建后同查询 cos = {c:.6} {mark}(重 init {:.1}s;sha 备忘应命中,不重算哈希)",
        t.elapsed().as_secs_f64()
    );
    if c.is_nan() || c < REINIT_THRESHOLD {
        failures.push(format!(
            "close→重 init 后自洽性 {c:.6} < {REINIT_THRESHOLD}"
        ));
    }
    // 刻意不 close:让 Phase E 的 face 批从「CLIP-only 会话在」状态进入,
    // 真实覆盖 ensure_session 的切换语义(close → 合并会话重 init)。

    // ── Phase E:face 黄金对拍(FaceDetectEmbed;黄金含 face 段且模型在盘时)─────────
    println!();
    println!("── Phase E:face 黄金对拍(FaceDetectEmbed)──");
    let face = default_face_profile();
    let face_installed =
        models_dir.join(&face.detect_file).exists() && models_dir.join(&face.embed_file).exists();
    match &golden.face {
        None => println!("  黄金无 face 段(导出时未安装),跳过 Phase E(非失败)"),
        Some(_) if !face_installed => {
            println!("  face 模型不在盘,跳过 Phase E(非失败;安装 YuNet+SFace 后重跑)")
        }
        Some(gf) => {
            assert_eq!(
                gf.face_profile_id, face.id,
                "face profile 漂移:重跑 --export-golden"
            );
            assert_eq!(
                gf.detect_sha16,
                sha16(&models_dir, &face.detect_file),
                "检测模型已变更,黄金失效:重跑 --export-golden"
            );
            assert_eq!(
                gf.embed_sha16,
                sha16(&models_dir, &face.embed_file),
                "嵌入模型已变更,黄金失效:重跑 --export-golden"
            );
            let spec_face = SessionSpec {
                profile: profile.clone(),
                face_profile: Some(face.clone()),
                models_dir: models_dir.clone(),
                ai_cache_dir: ai_cache_dir.clone(),
                image_provider: "auto".to_string(),
                batch_size: BENCH_COUNT as u32,
            };
            let fitems: Vec<FaceItem> = gf
                .keys
                .iter()
                .enumerate()
                .map(|(i, key)| FaceItem {
                    item_id: i as i64,
                    cache_key: None,
                    source_path: Some(webp_path(&ai_cache_dir, key).to_string_lossy().into_owned()),
                    fingerprint: format!("{i}:{:.4}", gf.det_score_thresh),
                })
                .collect();
            // 会话切换实测:当前是 CLIP-only 会话,face 批应触发 close → 合并会话重 init。
            let t = Instant::now();
            let outcomes = client
                .face_detect_embed(&spec_face, &fitems, gf.det_score_thresh, &never)
                .expect("worker FaceDetectEmbed 失败");
            println!(
                "[E] worker face 批 {:.1}s(含会话切换:close CLIP-only → init 合并会话)",
                t.elapsed().as_secs_f64()
            );

            let mut worker_total = 0usize;
            let mut golden_total = 0usize;
            for ((key, g), o) in gf.keys.iter().zip(&gf.items).zip(&outcomes) {
                golden_total += g.faces.len();
                match o {
                    FaceItemOutcome::Ok {
                        faces,
                        embeddings,
                        width,
                        height,
                    } => {
                        worker_total += faces.len();
                        if (*width, *height) != (g.width, g.height) {
                            failures.push(format!(
                                "face {key} 解码尺寸不一致:worker {width}×{height} vs 黄金 {}×{}",
                                g.width, g.height
                            ));
                            continue;
                        }
                        if faces.len() != g.faces.len() {
                            failures.push(format!(
                                "face {key} 脸数不一致:worker {} vs 黄金 {}",
                                faces.len(),
                                g.faces.len()
                            ));
                            continue;
                        }
                        for (fi, (wf, gfd)) in faces.iter().zip(&g.faces).enumerate() {
                            let bbox_diff = wf
                                .bbox
                                .iter()
                                .zip(gfd.bbox)
                                .map(|(a, b)| (a - b).abs())
                                .fold(0f32, f32::max);
                            let c = cosine(&embeddings[fi], &gfd.embedding);
                            let ok = bbox_diff < 1e-3 && c >= COS_THRESHOLD;
                            let mark = if ok { "✅" } else { "❌" };
                            println!("  face {key}#{fi}:bboxΔ={bbox_diff:.5},cos={c:.6} {mark}");
                            if !ok {
                                failures.push(format!(
                                    "face {key}#{fi} 偏差:bboxΔ={bbox_diff},cos={c}"
                                ));
                            }
                        }
                        if faces.is_empty() {
                            println!("  face {key}:0 脸(与黄金一致)✅");
                        }
                    }
                    FaceItemOutcome::Err(code) => {
                        failures.push(format!("face {key} worker 侧失败[{}]", code.as_str()));
                    }
                }
            }
            println!("[E] 脸数合计:worker {worker_total} vs 黄金 {golden_total}");

            // 超集会话复用实测(matches 放宽裁决):合并会话在,CLIP-only 请求不应触发重 init
            // (重 init ~1.4s,热调用毫秒级;1s 阈值余量充足)。
            let t = Instant::now();
            let _ = client
                .encode_text(&spec, &[QUERIES[0].to_string()], &never)
                .expect("超集复用查询失败");
            let reuse_s = t.elapsed().as_secs_f64();
            let mark = if reuse_s < 1.0 { "✅" } else { "❌" };
            println!("[E] 合并会话服务 CLIP-only 查询 {reuse_s:.2}s(超集复用,应无重 init){mark}");
            if reuse_s >= 1.0 {
                failures.push(format!("超集复用疑似失效:CLIP-only 查询耗时 {reuse_s:.2}s"));
            }
            client.close_session();
        }
    }

    // ── 结论 ────────────────────────────────────────────────────────────────────────
    println!();
    if failures.is_empty() {
        println!("== worker e2e 验收:全部通过 ✅(T16 门槛证据;本机真实模型+真实 ai_cache)==");
    } else {
        println!("== worker e2e 验收:{} 项失败 ❌ ==", failures.len());
        for f in &failures {
            println!("  - {f}");
        }
        std::process::exit(1);
    }
}
