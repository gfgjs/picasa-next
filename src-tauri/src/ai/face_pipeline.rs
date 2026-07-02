// src-tauri/src/ai/face_pipeline.rs
//! Background face-recognition pipeline (F3).
//! 后台人脸识别流水线（F3）。
//!
//! # 架构：4 线程，不是 5
//! 早期设计笔记设想 Producer→Preprocessor→Detector→Aligner+Embedder→Writer 五个阶段，但
//! `ai::face` 的 F2 实现已经把"对齐"内联进 `embed_faces`（无独立 Aligner 线程），且 F2 是逐张
//! 推理（无跨图批处理，「正确优先，批量优化留后」）。没有批处理时，把检测、嵌入拆成两个线程
//! 只换来边际的流水线重叠（GPU/DirectML 驱动本就会把两者的提交序列化），代价却是多一条
//! 跨线程通道搬运体积可观的 `DecodedImage`。于是合并为一个「检测+嵌入」线程，解码图像留在
//! 本地不跨线程搬运：
//!
//!   Producer → Preprocessor(rayon 解码) → DetectEmbed(检测+嵌入, 单线程) → Writer
//!
//! 1. Producer：批量查询 `face_status=0` 的 media_items，标记 Processing，发送 `FaceTask`。
//! 2. Preprocessor（rayon 线程池）：解码"短边 ≥ detect_size(640)"的最廉价源（见
//!    `resolve_face_decode_source`），发送 `DetectEmbedTask{decoded}`；解码失败直接发 `FaceResult{None}`。
//! 3. DetectEmbed（单线程，串行持有 `detect_pool`+`embed_pool`）：对每张图调用
//!    `face::detect_faces` → 若有脸再调用 `face::embed_faces`（内含对齐）；bbox/关键点按
//!    `decoded` 自身宽高归一化为 `[0,1]`；零脸图也是成功（`Ok(vec![])`），不是错误。
//! 4. Writer：批量收集 `FaceResult`，对每个成功项**先删后插**（`batch_replace_faces`，处理
//!    "重跑后旧脸残留"），再批量置 `face_status`。
//!
//! # 与 CLIP 的关系（刻意保持最小改动）
//! - 让步机制直接复用既有 `AppState::ai_yield_blockers()`，face 生产者照搬其返回值原样让步
//!   ——这与 CLIP 生产者用的是**同一个**函数，效果上人脸与 CLIP 对 scan/缩略图/派生/**exotic**/
//!   交互这些更高优先级的让步是一致的。
//!   注（v3.1 R9）：该统一阻塞源已新增 `exotic` token（冷门格式插件子进程解码优先于 AI/face），
//!   故此处「复用」≠「ai_yield_blockers 永不改」；新增阻塞源时本函数无需改，自动生效。
//! - **人脸与 CLIP 互斥**（避免两边同时占用 GPU/显存竞争）有意**不在本阶段实现**：
//!   ① 目前没有入口能并发启动两条流水线（人脸的 Tauri 命令是 F5 才接线）；
//!   ② 简单的"双向互相让步"在这里会死锁——双方各自的令牌在「正在 sleep 等待对方让出」期间
//!   仍然是 `Some`，于是 A 见 B 的令牌存在而让步、B 见 A 的令牌存在也让步，永远互等，
//!   谁都不会清空令牌、谁都跑不完。真正的互斥需要 F5 设计一个非对称的"单一持有者"门闩
//!   （谁先启动谁占着，后来者等待或拒绝），而不是对称让步。
//! - 同理，完成时**不**像 `pipeline.rs` 那样把 `state.ai_engine` 置 `None` 来卸载显存——
//!   `AiEnginePool` 是 CLIP 与人脸**共用的同一个**池（同时持有两者的 session），在此置空会
//!   连带卸载 CLIP（如果它已加载）。显存生命周期的协调留给 F5（那时才有真正需要协调的并发场景）。
//!
//! # 解码源：不能复用 CLIP 的 `resolve_decode_source`
//! 该函数的优先级 1 会短路到 AI 分析缓存文件（固定短边 336px）。CLIP 的模型只需 224–336px，
//! 这个捷径在那边永远安全；但 YuNet 需要 **640px**（`FaceProfile::detect_size`），照搬会悄悄
//! 喂给检测器一张过小的图、损害小脸召回率。本文件的 `resolve_face_decode_source` 因此独立
//! 实现，仅在「常规分档缩略图」与「原图」间选择，不引用 AI 缓存。
//!
//! # 验证范围（如实说明）
//! 本阶段仅做到 `cargo build` 编译通过 + 人工代码审查——没有可运行的触发入口（F5 的 Tauri
//! 命令尚不存在），故**未做端到端运行验证**。真正"跑一遍真实图库"的验证留到 F5 接好命令、
//! 应用可以从界面触发分析时再做。

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver, Sender};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::ai::clip::embedding_to_bytes;
use crate::ai::engine::SessionPool;
use crate::ai::face::{detect_faces, embed_faces};
use crate::ai::face_profile::FaceProfile;
use crate::db::models::FaceStatus;
use crate::db::queries::{
    batch_replace_faces, batch_update_face_status, count_pending_face_items,
    get_pending_face_items, reset_processing_face_items, NewFace,
};
use crate::engine::gpu::get_gpu_engine;
use crate::engine::traits::{DecodedImage, ResizeHint};
use crate::error::AppError;
use crate::state::AppState;

/// Batch size for reading from DB and for the writer's clustering flush.
/// 从数据库读取、及写入器聚类刷新的批次大小。
const BATCH_SIZE: i64 = 512;

/// 写入器「脸行 + face_status」的高频小批阈值（问题2 进度平滑）。
/// 脸行落库便宜且必须先于聚类（聚类按 item_id 从库读回脸），故脸+状态一起小批落库——
/// 让前端 `processedItems`（数 `face_status≠0`）平滑增长，且崩溃安全（Done 永远蕴含脸已写）；
/// 真正昂贵的聚类仍按 `BATCH_SIZE` 大批跑（见 `flush_face_rows` / `flush_cluster`）。
const STATUS_FLUSH_EVERY: usize = 16;

/// Channel capacity between stages.
/// 各阶段之间的通道容量。
const CHANNEL_CAPACITY: usize = 1024;

/// Task item sent from producer to preprocessor.
/// 从生产者发送到预处理器的任务项。
struct FaceTask {
    item_id: i64,
    source_path: PathBuf,
    file_format: String,
    thumb_status: i64,
    thumb_path: Option<String>,
    width: i64,
    height: i64,
}

/// Decoded image handed from preprocessor to the detect+embed stage.
/// 从预处理器交给检测+嵌入阶段的解码图像。
struct DetectEmbedTask {
    item_id: i64,
    decoded: DecodedImage,
}

/// Outcome sent from detect+embed (or an early preprocess failure) to the writer.
/// 从检测+嵌入阶段（或预处理早期失败）发送到写入器的结果。
struct FaceResult {
    item_id: i64,
    /// `Some(rows)` on success — possibly empty (zero-face image, still Done).
    /// `None` on decode/detect/embed failure → Error.
    /// 成功时为 `Some(rows)`——可能为空（零脸图，仍算 Done）。解码/检测/嵌入失败为 `None` → Error。
    records: Option<Vec<NewFace>>,
}

/// Start the background face-recognition pipeline.
/// 启动后台人脸识别流水线。
///
/// Returns immediately; all work is done in background threads.
/// 立即返回；所有工作在后台线程中完成。
pub fn start_face_pipeline(state: Arc<AppState>, token: CancellationToken) {
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let start_time = std::time::Instant::now();
        // Keep a token handle so the completion handler can tell natural completion from a
        // pause/stop/restart cancellation (drives the GPU-slot release decision below).
        // 保留一个 token 句柄，使完成回调能区分自然完成与暂停/停止/重启取消（驱动下面 GPU 槽位释放决策）。
        let token_outer = token.clone();
        let result =
            tokio::task::spawn_blocking(move || run_face_pipeline_blocking(&state_clone, &token))
                .await;

        let elapsed_ms = start_time.elapsed().as_millis();
        match result {
            Ok(Ok(())) => info!(
                "Face pipeline completed: elapsed={}ms | 人脸流水线完成: 耗时={}ms",
                elapsed_ms, elapsed_ms
            ),
            Ok(Err(e)) => warn!("Face pipeline error | 人脸流水线错误: {}", e),
            Err(e) => warn!("Face pipeline task panicked | 人脸流水线任务崩溃: {}", e),
        }

        // 仅在自然完成时释放共享 GPU 分析槽（F5 互斥；同 pipeline.rs 的理由——取消由命令释放
        // 或 restart 须保持持有）。判定方式与 set_config 续传标志一致：未被取消即自然完成。
        if !token_outer.is_cancelled() {
            let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            let _ = crate::db::queries::set_config(&conn, "face_analysis_active", "0");
            drop(conn);
            state.release_gpu_analysis(crate::state::GPU_OWNER_FACE);
        }

        // 不卸载共享的 `state.ai_engine`（见模块头："与 CLIP 的关系"）。仅清空本流水线自己的令牌。
        state.cancel_face_analysis();
    });
}

/// Blocking pipeline runner — runs inside spawn_blocking + rayon.
/// 阻塞式流水线运行器 — 在 spawn_blocking + rayon 中运行。
fn run_face_pipeline_blocking(
    state: &Arc<AppState>,
    token: &CancellationToken,
) -> crate::error::Result<()> {
    // Snapshot both face sessions + the active face profile once; share read-only across stages.
    // 一次性快照人脸双 session + 当前激活的人脸契约，只读共享给各阶段。
    let (detect_session, embed_session, profile) = {
        let engine = state.ai_engine.read().unwrap();
        let pool = engine
            .as_ref()
            .ok_or_else(|| AppError::Internal("face engine not initialized".into()))?;
        if !pool.face_ready() {
            return Err(AppError::Internal("face models not loaded".into()));
        }
        let detect_session = pool
            .face_detect_session
            .clone()
            .ok_or_else(|| AppError::Internal("face detect session missing".into()))?;
        let embed_session = pool
            .face_embed_session
            .clone()
            .ok_or_else(|| AppError::Internal("face embed session missing".into()))?;
        let profile = pool
            .face_profile
            .clone()
            .ok_or_else(|| AppError::Internal("face profile missing".into()))?;
        (detect_session, embed_session, profile)
    };
    let profile = Arc::new(profile);

    // Resume support (问题7): release any items a previous run claimed but never finished
    // (face_status=Processing) back to Pending — mirrors `reset_processing_ai_items`.
    // 续传支持（问题7）：把上次运行已领取但未完成的项（face_status=Processing）放回 Pending——
    // 镜像 `reset_processing_ai_items`。
    {
        let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        match reset_processing_face_items(&conn) {
            Ok(n) if n > 0 => info!(
                "Recovered {} orphaned face items (processing → pending) | 恢复 {} 个孤儿人脸项（处理中 → 待处理）",
                n, n
            ),
            Ok(_) => {}
            Err(e) => warn!("Failed to recover orphaned face items | 恢复孤儿人脸项失败: {}", e),
        }
    }

    let read_conn = state.db_read_pool.get()?;
    let total = count_pending_face_items(&read_conn)?;
    info!(
        "Face pipeline starting: {} images to analyse | 人脸流水线启动：待分析 {} 张图像",
        total, total
    );
    drop(read_conn);

    // ── Channel setup ─────────────────────────────────────────────────────────
    // ── 通道设置 ─────────────────────────────────────────────────────────────
    let (task_tx, task_rx) = bounded::<FaceTask>(CHANNEL_CAPACITY);
    let (decoded_tx, decoded_rx) = bounded::<DetectEmbedTask>(CHANNEL_CAPACITY);
    let (result_tx, result_rx) = bounded::<FaceResult>(CHANNEL_CAPACITY);

    let token_prod = token.clone();
    let token_writer = token.clone();
    let state_prod = Arc::clone(state);
    let state_writer = Arc::clone(state);

    rayon::scope(|s| {
        // ── Producer thread ───────────────────────────────────────────────────
        // ── 生产者线程 ───────────────────────────────────────────────────────
        s.spawn(|_| {
            produce_face_tasks(&state_prod, task_tx, &token_prod);
        });

        // ── Preprocessor threads (rayon thread pool) ──────────────────────────
        // ── 预处理线程（rayon 线程池）────────────────────────────────────────
        let state_consumer = Arc::clone(state);
        let token_consumer = token.clone();
        let result_tx_preprocess = result_tx.clone();
        let profile_pp = Arc::clone(&profile);
        s.spawn(move |_| {
            preprocess_face_tasks(
                task_rx,
                decoded_tx,
                result_tx_preprocess,
                &state_consumer,
                &token_consumer,
                &profile_pp,
            );
        });

        // ── DetectEmbed thread (GPU-bound, single dedicated thread) ───────────
        // ── 检测+嵌入线程（GPU 密集，单一专用线程）────────────────────────────
        let token_de = token.clone();
        let result_tx_de = result_tx.clone();
        let profile_de = Arc::clone(&profile);
        s.spawn(move |_| {
            detect_embed_faces(
                decoded_rx,
                result_tx_de,
                detect_session,
                embed_session,
                &token_de,
                &profile_de,
            );
        });

        // ── Writer thread ─────────────────────────────────────────────────────
        // ── 写入器线程 ────────────────────────────────────────────────────────
        let profile_writer = Arc::clone(&profile);
        s.spawn(move |_| {
            write_face_results(&state_writer, result_rx, &token_writer, &profile_writer);
        });

        drop(result_tx); // Close so writer can detect completion | 关闭以便写入器可以检测完成
    });

    Ok(())
}

/// Producer: batch-query pending items, push tasks to channel.
/// 生产者：批量查询待处理项，推送任务到通道。
fn produce_face_tasks(state: &Arc<AppState>, task_tx: Sender<FaceTask>, token: &CancellationToken) {
    loop {
        if token.is_cancelled() {
            info!("Face producer cancelled | 人脸生产者已取消");
            break;
        }

        // 复用既有让步层（scan/缩略图/派生/exotic/交互）——与 CLIP 生产者调用同一函数。
        // v3.1 R9：阻塞源已含 exotic（冷门格式子进程解码优先于 AI/face），单点改动两边自动生效。
        // 人脸↔CLIP 互斥不在此实现，见模块头。
        let blockers = state.ai_yield_blockers();
        if !blockers.is_empty() {
            debug!(
                blockers = %blockers.join(","),
                "Face producer yielding to higher priority task | 人脸生产者让步给高优先级任务"
            );
            std::thread::sleep(std::time::Duration::from_millis(500));
            continue;
        }

        let conn = match state.db_read_pool.get() {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "DB pool error in face producer | 人脸生产者 DB 池错误: {}",
                    e
                );
                break;
            }
        };

        let batch = match get_pending_face_items(&conn, BATCH_SIZE) {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    "Query pending face items failed | 查询待处理人脸项失败: {}",
                    e
                );
                break;
            }
        };

        if batch.is_empty() {
            info!("Face producer: no more pending items | 人脸生产者：没有更多待处理项");
            break;
        }

        // Mark items as "processing" to avoid re-queuing on restart.
        // 将项标记为"处理中"，避免重启时重新排队。
        let write_conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<i64> = batch.iter().map(|it| it.id).collect();
        if let Err(e) = batch_update_face_status(&write_conn, &ids, FaceStatus::Processing.as_i64())
        {
            warn!(
                "Failed to mark face items as processing | 标记人脸项为处理中失败: {}",
                e
            );
        }
        drop(write_conn);

        for item in batch {
            if token.is_cancelled() {
                break;
            }

            if task_tx
                .send(FaceTask {
                    item_id: item.id,
                    source_path: PathBuf::from(item.abs_path),
                    file_format: item.file_format,
                    thumb_status: item.thumb_status,
                    thumb_path: item.thumb_path,
                    width: item.width,
                    height: item.height,
                })
                .is_err()
            {
                break;
            }
        }
    }

    info!("Face producer finished | 人脸生产者已完成");
}

/// Preprocessor: receive tasks, decode the cheapest sufficient source, send to detect+embed.
/// 预处理器：接收任务，解码最廉价且足够的源，发送到检测+嵌入阶段。
fn preprocess_face_tasks(
    task_rx: Receiver<FaceTask>,
    decoded_tx: Sender<DetectEmbedTask>,
    result_tx: Sender<FaceResult>,
    state: &Arc<AppState>,
    token: &CancellationToken,
    profile: &Arc<FaceProfile>,
) {
    rayon::scope(|s| {
        for task in task_rx {
            if token.is_cancelled() {
                break;
            }

            let decoded_tx = decoded_tx.clone();
            let result_tx = result_tx.clone();
            let state = Arc::clone(state);
            let token_clone = token.clone();
            let profile = Arc::clone(profile);

            s.spawn(move |_| {
                if token_clone.is_cancelled() {
                    return;
                }

                match process_face_preprocess_task(&task, &state, &profile) {
                    Ok(decoded) => {
                        let _ = decoded_tx.send(DetectEmbedTask {
                            item_id: task.item_id,
                            decoded,
                        });
                    }
                    Err(e) => {
                        debug!(
                            "Face preprocess failed for item {} | 项 {} 人脸预处理失败: {}",
                            task.item_id, task.item_id, e
                        );
                        let _ = result_tx.send(FaceResult {
                            item_id: task.item_id,
                            records: None,
                        });
                    }
                }
            });
        }
    });

    info!("Face preprocessors finished | 人脸预处理器已完成");
}

/// A resolved decode source: which file to decode and as what format.
/// 已解析的解码源：解码哪个文件、按什么格式。
struct FaceDecodeSource {
    path: PathBuf,
    format: String,
}

/// Pick the cheapest decode source whose short edge still satisfies `detect_size` (640).
///
/// Deliberately NOT shared with CLIP's `resolve_decode_source` in `pipeline.rs` (see module
/// header for why: that function's priority-1 AI-cache shortcut is hard-baked at a 336px short
/// edge, which is too small for YuNet's 640px input). This version only ever considers the
/// regular tiered thumbnail or the original — no AI cache.
///
/// 选择短边仍满足 `detect_size`（640）的最廉价解码源。
///
/// 故意不与 `pipeline.rs` 的 `resolve_decode_source` 共用（原因见模块头：那个函数优先级 1 的
/// AI 缓存捷径固定短边 336px，对 YuNet 的 640px 输入而言太小）。此版本只在「常规分档缩略图」
/// 与「原图」之间选择——不涉及 AI 缓存。
fn resolve_face_decode_source(
    task: &FaceTask,
    state: &AppState,
    detect_size: u32,
) -> FaceDecodeSource {
    let original = FaceDecodeSource {
        path: task.source_path.clone(),
        format: task.file_format.clone(),
    };

    if task.thumb_status != 1 || task.width <= 0 || task.height <= 0 {
        return original;
    }
    let Some(rel) = task.thumb_path.as_deref() else {
        return original;
    };

    // Parse the tier (long edge) from the rel path "{tier}/{prefix}/{hex}.webp".
    // 从相对路径 "{档位}/{前缀}/{hex}.webp" 解析档位（长边）。
    let Some(tier) = rel.split('/').next().and_then(|s| s.parse::<u32>().ok()) else {
        return original;
    };

    // Thumbnail is LongEdge(tier) but never upscaled → long edge = min(tier, max(W,H)).
    // Predict short edge WITHOUT touching disk; use the thumbnail only if it's ≥ detect_size.
    // 缩略图按长边=tier 缩放但绝不放大 → 长边 = min(tier, max(W,H))；不读盘预测短边，
    // 仅当 ≥ detect_size 时采用。
    let (w, h) = (task.width as u32, task.height as u32);
    let (long, short) = (w.max(h), w.min(h));
    let thumb_short = if long <= tier {
        short
    } else {
        (short as f32 * tier as f32 / long as f32).round() as u32
    };
    if thumb_short < detect_size {
        return original;
    }

    let cache_dir = state.thumb_config.read().unwrap().cache_dir.clone();
    let thumb_full = cache_dir.join("thumbnails").join(rel);
    if thumb_full.exists() {
        // Thumbnail is already EXIF-upright (baked in at generation) — "webp" keeps WIC's
        // rotation branch (jpg/heic only) off so orientation isn't re-applied.
        // 缩略图生成时已转正——用"webp"格式使 WIC 的旋转分支（仅 jpg/heic）不触发，避免二次转向。
        FaceDecodeSource {
            path: thumb_full,
            format: "webp".to_string(),
        }
    } else {
        original
    }
}

/// Decode the cheapest sufficient source for one face task via ImageEngine (GPU-accelerated WIC
/// with CPU fallback) — same fallback shape as CLIP's `process_preprocess_task`.
/// 通过 ImageEngine（GPU 加速 WIC，带 CPU 回退）为单个人脸任务解码最廉价且足够的源——
/// 回退结构与 CLIP 的 `process_preprocess_task` 相同。
fn process_face_preprocess_task(
    task: &FaceTask,
    state: &AppState,
    profile: &FaceProfile,
) -> crate::error::Result<DecodedImage> {
    let gpu_engine_name = state.thumb_config.read().unwrap().gpu_engine.clone();
    let resize_hint = Some(ResizeHint::ShortEdge(profile.detect_size));
    let src = resolve_face_decode_source(task, state, profile.detect_size);

    match get_gpu_engine(&gpu_engine_name) {
        Some(gpu) if gpu.can_handle(&src.format) => match gpu.decode(&src.path, resize_hint) {
            Ok(d) => Ok(d),
            Err(e) => {
                debug!(
                    "GPU decode failed for face item {}, falling back to CPU | 人脸项 {} GPU 解码失败，回退 CPU: {}",
                    task.item_id, task.item_id, e
                );
                state
                    .engine_arena
                    .engine_for(&src.format)
                    .ok_or_else(|| AppError::UnsupportedFormat(src.format.clone()))?
                    .decode(&src.path, resize_hint)
            }
        },
        _ => state
            .engine_arena
            .engine_for(&src.format)
            .ok_or_else(|| AppError::UnsupportedFormat(src.format.clone()))?
            .decode(&src.path, resize_hint),
    }
}

/// DetectEmbed: receive decoded images, run YuNet detect then SFace/ArcFace embed, send results.
/// 检测+嵌入：接收解码图，先跑 YuNet 检测再跑 SFace/ArcFace 嵌入，发送结果。
///
/// 问题6c：并发 worker 数 = 两池**可用 session** 的较小值（流水线启动瞬间无人借出 → 等于池
/// 实际容量，部分加载失败时自动取较小值）。GPU 下 `face_pool_size=2` → 2 个 worker 各持一组
/// detect+embed session 并发吃 GPU。⚠️ 见 `engine.rs::face_pool_size`：GPU 多 session 有 DX12
/// 锁争用风险，待用户实测；若变慢把 `face_pool_size` 改回 1，worker 自动退回单线程（行为同旧）。
fn detect_embed_faces(
    decoded_rx: Receiver<DetectEmbedTask>,
    result_tx: Sender<FaceResult>,
    detect_pool: SessionPool,
    embed_pool: SessionPool,
    token: &CancellationToken,
    profile: &Arc<FaceProfile>,
) {
    let workers = detect_pool.available().min(embed_pool.available()).max(1);

    if workers <= 1 {
        // 单 worker：最简路径（CPU 单推理线程 / GPU 回退 1）。
        run_detect_embed_worker(
            &decoded_rx,
            &result_tx,
            &detect_pool,
            &embed_pool,
            token,
            profile,
        );
        info!("Face detect+embed finished (1 worker) | 人脸检测+嵌入已完成（1 worker）");
        return;
    }

    // 多 worker：crossbeam Receiver 为 MPMC，各 worker 持一份 clone 共拉同一队列、自动负载均衡。
    // std::thread::scope 使 worker 可安全借用栈上的 token/profile（无需 'static）。
    std::thread::scope(|s| {
        for _ in 0..workers {
            let rx = decoded_rx.clone();
            let tx = result_tx.clone();
            let dp = detect_pool.clone();
            let ep = embed_pool.clone();
            let prof = Arc::clone(profile);
            s.spawn(move || {
                run_detect_embed_worker(&rx, &tx, &dp, &ep, token, &prof);
            });
        }
    });
    info!(
        "Face detect+embed finished ({} workers) | 人脸检测+嵌入已完成（{} workers）",
        workers, workers
    );
}

/// 单个检测+嵌入 worker：从 `decoded_rx` 取图，检测→嵌入，结果发 `result_tx`，直到通道关闭。
fn run_detect_embed_worker(
    decoded_rx: &Receiver<DetectEmbedTask>,
    result_tx: &Sender<FaceResult>,
    detect_pool: &SessionPool,
    embed_pool: &SessionPool,
    token: &CancellationToken,
    profile: &FaceProfile,
) {
    for task in decoded_rx.iter() {
        if token.is_cancelled() {
            break;
        }

        let records = match detect_and_embed_one(
            task.item_id,
            &task.decoded,
            detect_pool,
            embed_pool,
            profile,
        ) {
            Ok(recs) => Some(recs),
            Err(e) => {
                debug!(
                    "Face detect/embed failed for item {} | 项 {} 人脸检测/嵌入失败: {}",
                    task.item_id, task.item_id, e
                );
                None
            }
        };

        let _ = result_tx.send(FaceResult {
            item_id: task.item_id,
            records,
        });
    }
}

/// Detect + embed all faces in one decoded image, returning DB-ready rows with bbox/landmarks
/// normalized against `decoded`'s own width/height (NOT the original file's DB-stored
/// dimensions — the decode source may be a smaller thumbnail). `Ok(vec![])` = zero faces, which
/// is still success (caller marks the item Done, not Error).
/// 检测+嵌入一张解码图中的所有人脸，返回 bbox/关键点已按 `decoded` 自身宽高（非数据库存的原图
/// 尺寸——解码源可能是更小的缩略图）归一化的、DB 就绪的行。`Ok(vec![])` = 零脸，仍算成功
/// （调用方标记该项为 Done，非 Error）。
fn detect_and_embed_one(
    item_id: i64,
    decoded: &DecodedImage,
    detect_pool: &SessionPool,
    embed_pool: &SessionPool,
    profile: &FaceProfile,
) -> crate::error::Result<Vec<NewFace>> {
    let faces = detect_faces(detect_pool, decoded, profile)?;
    if faces.is_empty() {
        return Ok(Vec::new());
    }
    let embeddings = embed_faces(embed_pool, decoded, &faces, profile)?;
    let (w, h) = (decoded.width.max(1) as f32, decoded.height.max(1) as f32);

    Ok(faces
        .iter()
        .zip(embeddings)
        .map(|(face, emb)| {
            let quality = face.quality(decoded.width, decoded.height);
            let mut lm_flat = [0f32; 10];
            for i in 0..5 {
                lm_flat[i * 2] = (face.landmarks[i][0] / w).clamp(0.0, 1.0);
                lm_flat[i * 2 + 1] = (face.landmarks[i][1] / h).clamp(0.0, 1.0);
            }
            NewFace {
                item_id,
                bbox_x: (face.bbox[0] / w).clamp(0.0, 1.0),
                bbox_y: (face.bbox[1] / h).clamp(0.0, 1.0),
                bbox_w: (face.bbox[2] / w).clamp(0.0, 1.0),
                bbox_h: (face.bbox[3] / h).clamp(0.0, 1.0),
                landmarks: embedding_to_bytes(&lm_flat),
                det_score: face.score,
                quality,
                embedding: embedding_to_bytes(&emb),
            }
        })
        .collect())
}

/// Writer: batch-collect results, delete+insert `faces` rows, update `face_status`.
/// 写入器：批量收集结果，先删后插 `faces` 行，更新 `face_status`。
fn write_face_results(
    state: &Arc<AppState>,
    result_rx: Receiver<FaceResult>,
    token: &CancellationToken,
    profile: &FaceProfile,
) {
    // 问题2 双节奏：
    // - 小批（STATUS_FLUSH_EVERY=16）：写脸行 + 置 face_status=Done，并把成功项累入 `cluster_pending`。
    //   `done_ids` 须含本批**所有**成功项（含零脸图），它驱动 `batch_replace_faces` 的删除阶段。
    // - 大批（BATCH_SIZE=512）：对累积的 `cluster_pending` 跑一次昂贵的增量聚类。
    // 解耦后 `processedItems`（数 face_status≠0）平滑增长，而聚类仍低频跑。
    let mut done_ids: Vec<i64> = Vec::with_capacity(STATUS_FLUSH_EVERY);
    let mut rows: Vec<NewFace> = Vec::new();
    let mut failed_ids: Vec<i64> = Vec::new();
    // 脸已落库、status 已 Done、但尚未聚类的 item_id。聚类按 item_id 从库读回脸，故此处累积安全。
    let mut cluster_pending: Vec<i64> = Vec::with_capacity(BATCH_SIZE as usize);
    let mut total_written: u64 = 0;

    for result in result_rx {
        if token.is_cancelled() {
            break;
        }

        match result.records {
            Some(mut recs) => {
                done_ids.push(result.item_id);
                rows.append(&mut recs);
            }
            None => {
                failed_ids.push(result.item_id);
            }
        }

        if done_ids.len() >= STATUS_FLUSH_EVERY {
            flush_face_rows(
                state,
                &mut done_ids,
                &mut rows,
                profile,
                &mut cluster_pending,
                &mut total_written,
            );
        }
        if failed_ids.len() >= STATUS_FLUSH_EVERY {
            flush_face_failed(state, &mut failed_ids);
        }
        if cluster_pending.len() >= BATCH_SIZE as usize {
            flush_cluster(state, &mut cluster_pending, profile);
        }
    }

    if !done_ids.is_empty() {
        flush_face_rows(
            state,
            &mut done_ids,
            &mut rows,
            profile,
            &mut cluster_pending,
            &mut total_written,
        );
    }
    if !failed_ids.is_empty() {
        flush_face_failed(state, &mut failed_ids);
    }
    // 收尾：把剩余未达大批阈值的项聚类掉。
    if !cluster_pending.is_empty() {
        flush_cluster(state, &mut cluster_pending, profile);
    }

    info!(
        "Face writer finished: {} images written | 人脸写入器已完成：写入 {} 张图像",
        total_written, total_written
    );
}

/// Flush a small batch of successfully-processed items: delete+insert their `faces` rows, mark
/// `face_status=Done`, then queue them for the (deferred, large-batch) clustering pass. On DB
/// failure, mark the batch `Error` instead (mirrors CLIP's `flush_batch`).
///
/// 刷新一小批处理成功的项：先删后插其 `faces` 行，标记 `face_status=Done`，再排入（延后的、
/// 大批的）聚类队列。DB 失败则改标 `Error`（镜像 CLIP 的 `flush_batch`）。脸与状态在此一起原子
/// 落库 → `Done` 永远蕴含脸已写，崩溃/取消时不会出现「Done 但零脸」的漏脸。
fn flush_face_rows(
    state: &Arc<AppState>,
    done_ids: &mut Vec<i64>,
    rows: &mut Vec<NewFace>,
    profile: &FaceProfile,
    cluster_pending: &mut Vec<i64>,
    total_written: &mut u64,
) {
    let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
    match batch_replace_faces(&conn, done_ids, &profile.id, rows) {
        Ok(()) => {
            if let Err(e) = batch_update_face_status(&conn, done_ids, FaceStatus::Done.as_i64()) {
                warn!(
                    "Failed to mark face items as done | 标记人脸项为完成失败: {}",
                    e
                );
            }
            *total_written += done_ids.len() as u64;
            drop(conn);
            // 脸已落库 → 排入聚类队列（聚类延后到大批，见 `flush_cluster`）。
            cluster_pending.extend(done_ids.iter().copied());
        }
        Err(e) => {
            warn!("Batch face write failed | 批量写入人脸失败: {}", e);
            let _ = batch_update_face_status(&conn, done_ids, FaceStatus::Error.as_i64());
            drop(conn);
        }
    }
    done_ids.clear();
    rows.clear();
}

/// Run the (expensive) incremental clustering on a large batch of already-persisted items, then
/// clear the queue. Kept separate from `flush_face_rows` so status/progress can advance at a fine
/// granularity while clustering stays coarse.
///
/// 对一大批**已落库**的项跑（昂贵的）增量聚类，然后清空队列。与 `flush_face_rows` 分离，使
/// 状态/进度可细粒度推进、聚类保持粗粒度。`cluster_new_faces` 自做短读+短写，不持外层写锁。
fn flush_cluster(state: &Arc<AppState>, cluster_pending: &mut Vec<i64>, profile: &FaceProfile) {
    // 阈值取「运行期 override 或 profile 默认」——可不重编译调参做实测比较（无 override 时同改动前）。
    let (threshold, min_quality) = crate::ai::face_cluster::effective_thresholds(state, profile);
    crate::ai::face_cluster::cluster_new_faces(
        state,
        cluster_pending,
        &profile.id,
        threshold,
        min_quality,
    );
    cluster_pending.clear();
}

/// Mark a batch of failed items as `face_status=Error` (mirrors CLIP's `flush_failed`).
/// 把一批失败项标记为 `face_status=Error`（镜像 CLIP 的 `flush_failed`）。
fn flush_face_failed(state: &Arc<AppState>, failed_ids: &mut Vec<i64>) {
    let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = batch_update_face_status(&conn, failed_ids, FaceStatus::Error.as_i64()) {
        warn!(
            "Failed to mark face items as error | 标记人脸项为错误失败: {}",
            e
        );
    }
    failed_ids.clear();
}
