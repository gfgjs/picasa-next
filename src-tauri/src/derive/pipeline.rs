// src-tauri/src/derive/pipeline.rs
//! Background derivation pipeline — reuses the AI pipeline pattern (§1.2):
//! 后台派生流水线 —— 复用 AI 流水线模式（§1.2）：
//!
//!   Producer → crossbeam channel → Consumer pool (rayon) → Writer
//!   生产者 → crossbeam 通道 → 消费者池（rayon）→ 写入器
//!   + CancellationToken（暂停/停止）+ should_yield_derivation()（让步）
//!   + 状态机（0待处理/1处理中/2完成/3错误）→ 断点续传 + 孤儿恢复
//!
//! Unlike AI (one model, GPU-batched), each derivation `kind` is a plain function
//! (`kind::run`) — the framework here is kind-agnostic. Adding a kind needs zero changes
//! to this file. P0 ships the framework with stub kinds; backends land in P2/P3/P4.
//! 与 AI（单模型、GPU 批处理）不同，每种派生 `kind` 是一个纯函数（`kind::run`）——
//! 本框架与具体 kind 无关。新增 kind 无需改动本文件。P0 交付框架 + 桩 kind，后端在 P2/P3/P4 落地。

use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{bounded, Receiver, Sender};
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::db::models::ThumbResult;
use crate::db::queries::{
    backfill_derivations, batch_finish_derivations, count_derivations_by_status, get_config,
    get_pending_derivations, mark_derivations_processing, reset_processing_derivations,
    update_thumb_result, DerivationResultRow,
};
use crate::derive::kind::{self, DerivationContext, DerivationKind};
use crate::scanner::enricher::MediaEnrichedPayload;
use crate::state::AppState;

/// Batch size for reading pending tasks / flushing results.
/// 读取待处理任务 / 刷新结果的批次大小。
const BATCH_SIZE: i64 = 256;

/// Channel capacity between producer and consumers.
/// 生产者和消费者之间的通道容量。
const CHANNEL_CAPACITY: usize = 512;

/// Task sent from producer to consumers.
/// 从生产者发送到消费者的任务。
struct DerivationTaskMsg {
    item_id: i64,
    kind: DerivationKind,
    abs_path: PathBuf,
    file_format: String,
    media_type: String,
    cache_key: i64,
}

/// Start the background derivation pipeline. Returns immediately; work runs in background
/// threads. `kind_filter` optionally limits processing to one kind.
/// 启动后台派生流水线。立即返回；工作在后台线程中运行。`kind_filter` 可选地限定只处理某一 kind。
pub fn start_derivation_pipeline(
    app: AppHandle,
    state: Arc<AppState>,
    token: CancellationToken,
    kind_filter: Option<DerivationKind>,
) {
    tokio::spawn(async move {
        let start = std::time::Instant::now();
        // Keep a handle to tell natural completion from a pause/stop cancellation.
        // 保留句柄以区分自然完成与暂停/停止取消。
        let token_outer = token.clone();
        let state_run = Arc::clone(&state);

        let result = tokio::task::spawn_blocking(move || {
            run_pipeline_blocking(&app, &state_run, &token, kind_filter)
        })
        .await;

        let elapsed = start.elapsed().as_millis();
        match result {
            Ok(Ok(())) => info!(
                "Derivation pipeline completed: elapsed={}ms | 派生流水线完成: 耗时={}ms",
                elapsed, elapsed
            ),
            Ok(Err(e)) => warn!("Derivation pipeline error | 派生流水线错误: {}", e),
            Err(e) => warn!("Derivation task panicked | 派生任务崩溃: {}", e),
        }

        // Natural completion (not cancelled) → clear the resume flag; nothing left to resume.
        // 自然完成（未被取消）→ 清除续传标志；无可续传。
        if !token_outer.is_cancelled() {
            let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            let _ = crate::db::queries::set_config(&conn, "derivation_active", "0");
        }

        // Clear the token from state after completion.
        // 完成后从状态中清除令牌。
        state.cancel_derivation();
    });
}

/// Blocking pipeline runner — inside spawn_blocking + rayon.
/// 阻塞式流水线运行器 — 在 spawn_blocking + rayon 中运行。
fn run_pipeline_blocking(
    app: &AppHandle,
    state: &Arc<AppState>,
    token: &CancellationToken,
    kind_filter: Option<DerivationKind>,
) -> crate::error::Result<()> {
    // ── Resume: recover orphaned (status=1) tasks left by a crash/pause/stop → pending. ──
    // ── 续传：把崩溃/暂停/停止遗留的孤儿任务（status=1）恢复为待处理。──
    {
        let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        match reset_processing_derivations(&conn) {
            Ok(n) if n > 0 => info!("Recovered {} orphaned derivations (processing → pending) | 恢复 {} 个孤儿派生（处理中 → 待处理）", n, n),
            Ok(_) => {}
            Err(e) => warn!("Failed to recover orphaned derivations | 恢复孤儿派生失败: {}", e),
        }
    }

    // ── User toggles: whether video cover / keyframe extraction is desired (default ON). ──
    // A disabled kind is neither backfilled (below) nor picked up by the producer (excluded
    // from get_pending_derivations) — already-pending rows just pause until re-enabled, so this
    // is fully non-destructive and reuses the normal resume path.
    // ── 用户开关：是否要提取视频封面 / 关键帧（默认开）。被关闭的 kind 既不会被 backfill 入队（见下），
    // 也不会被生产者领取（在 get_pending_derivations 中排除）—— 已入队的待处理行只是暂停，待开关
    // 重新打开后续传。完全非破坏性，复用正常的续传路径。
    let disabled_kinds: Vec<&'static str> = {
        let conn = state.db_read_pool.get()?;
        // None / 任何非 "false" 值 → 视为开启（默认开）。
        let enabled = |key: &str| {
            get_config(&conn, key)
                .ok()
                .flatten()
                .map(|v| v != "false")
                .unwrap_or(true)
        };
        let mut v = Vec::new();
        if !enabled("enable_video_cover") {
            v.push(DerivationKind::VideoCover.as_str());
        }
        if !enabled("enable_video_keyframes") {
            v.push(DerivationKind::VideoKeyframes.as_str());
        }
        // AI 高清缓存与视频派生相反，是 **opt-in**（默认关）：仅当 `ai_hq_cache_enabled == "true"`
        // 时才入队/处理 ai_thumb，否则把它加入 disabled_kinds（既不 backfill 也不被生产者领取）。
        let ai_hq_enabled = get_config(&conn, "ai_hq_cache_enabled")
            .ok()
            .flatten()
            .map(|val| val == "true")
            .unwrap_or(false);
        if !ai_hq_enabled {
            v.push(DerivationKind::AiThumb.as_str());
        }
        v
    };
    if !disabled_kinds.is_empty() {
        info!(
            "Derivation kinds disabled by user setting | 用户设置禁用的派生 kind: {:?}",
            disabled_kinds
        );
    }

    // ── Enqueue (backfill): insert pending rows for implemented kinds whose source items ──
    // exist but lack a (item, kind) row. Stub kinds (is_implemented=false) are skipped, so
    // P0 enqueues nothing and the pipeline is a clean no-op.
    // ── 入队（backfill）：为已实现的 kind 插入待处理行（源项存在但缺 (item,kind) 行）。──
    // 桩 kind（is_implemented=false）跳过，故 P0 不入队任何项，流水线干净空跑。
    {
        let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        let mut enqueued = 0usize;
        for k in DerivationKind::ALL {
            if !k.is_implemented() {
                continue;
            }
            // 用户在设置中关闭了该视频派生 → 不入队（与生产者的排除保持一致）。
            if disabled_kinds.contains(&k.as_str()) {
                continue;
            }
            if let Some(only) = kind_filter {
                if k != only {
                    continue;
                }
            }
            let n = match k {
                DerivationKind::VideoCover | DerivationKind::VideoKeyframes => {
                    backfill_derivations(&conn, k.as_str(), "video", None)
                }
                DerivationKind::AudioCover | DerivationKind::AudioMeta => {
                    backfill_derivations(&conn, k.as_str(), "audio", None)
                }
                DerivationKind::DocThumb => backfill_derivations(
                    &conn,
                    k.as_str(),
                    "document",
                    Some(&DerivationKind::DOC_THUMB_FORMATS),
                ),
                DerivationKind::AiThumb => backfill_derivations(&conn, k.as_str(), "image", None),
            }
            .unwrap_or_else(|e| {
                warn!(
                    "Backfill failed for kind {} | kind {} 入队失败: {}",
                    k.as_str(),
                    k.as_str(),
                    e
                );
                0
            });
            enqueued += n;
        }
        if enqueued > 0 {
            info!(
                "Enqueued {} new derivation task(s) | 新入队 {} 个派生任务",
                enqueued, enqueued
            );
        }
    }

    // Count pending up-front for logging.
    // 预先统计待处理数用于日志。
    let kind_filter_str = kind_filter.map(|k| k.as_str().to_string());
    {
        let read_conn = state.db_read_pool.get()?;
        let pending =
            get_pending_derivations(&read_conn, 1, kind_filter_str.as_deref(), &disabled_kinds)?
                .len();
        if pending == 0 {
            info!("Derivation pipeline: nothing pending | 派生流水线：无待处理项");
            return Ok(());
        }
    }
    info!("Derivation pipeline starting | 派生流水线启动");

    // Snapshot cache config once (avoid per-task RwLock reads).
    // 一次性快照缓存配置（避免逐任务读 RwLock）。
    let (cache_dir, thumb_size) = {
        let cfg = state.thumb_config.read().unwrap();
        (cfg.cache_dir.clone(), cfg.size)
    };

    // ── Channels ──────────────────────────────────────────────────────────────
    let (task_tx, task_rx) = bounded::<DerivationTaskMsg>(CHANNEL_CAPACITY);
    let (result_tx, result_rx) = bounded::<DerivationResultRow>(CHANNEL_CAPACITY);

    let state_prod = Arc::clone(state);
    let state_consumer = Arc::clone(state);
    let state_writer = Arc::clone(state);
    let token_prod = token.clone();
    let token_consumer = token.clone();
    let token_writer = token.clone();
    let app_writer = app.clone();
    let kind_filter_owned = kind_filter_str.clone();
    let disabled_prod = disabled_kinds.clone();

    // Run producer / consumer / writer on dedicated OS threads (`std::thread::scope`), NOT on
    // rayon workers. The three are long-lived and mostly BLOCK on channels — parking them on
    // rayon workers stole pool threads, throttling the actual parallel decode (and any other
    // par_iter) on low-core machines. With scoped OS threads, the consumer's inner `rayon::scope`
    // gets the FULL rayon pool for cover/keyframe decode. The scope joins all three before return,
    // preserving the original blocking semantics (we're inside spawn_blocking).
    // 把生产者/消费者/写入器放到独立的 OS 线程（`std::thread::scope`），而非 rayon 工作线程。这三者
    // 长生命周期且多数时间阻塞在 channel 上 —— 占用 rayon 线程会拖慢真正的并行解码（及其它 par_iter），
    // 低核机器尤甚。改用作用域 OS 线程后，消费者内部的 `rayon::scope` 可独享整个 rayon 池做封面/关键帧
    // 解码。作用域在返回前 join 全部三者，保持原阻塞语义（仍在 spawn_blocking 内）。
    std::thread::scope(|s| {
        // Producer
        s.spawn(|| {
            produce_tasks(
                &state_prod,
                task_tx,
                &token_prod,
                kind_filter_owned.as_deref(),
                &disabled_prod,
            );
        });

        // Consumer pool (its inner rayon::scope now has the whole rayon pool for decode)
        s.spawn(|| {
            consume_tasks(
                task_rx,
                result_tx,
                &token_consumer,
                &state_consumer,
                cache_dir,
                thumb_size,
            );
        });

        // Writer
        s.spawn(|| {
            write_results(&app_writer, &state_writer, result_rx, &token_writer);
        });
    });

    Ok(())
}

/// Producer: batch-query pending tasks, mark processing, push to channel.
/// 生产者：批量查询待处理任务，标记处理中，推送到通道。
fn produce_tasks(
    state: &Arc<AppState>,
    task_tx: Sender<DerivationTaskMsg>,
    token: &CancellationToken,
    kind_filter: Option<&str>,
    exclude_kinds: &[&str],
) {
    loop {
        if token.is_cancelled() {
            info!("Derivation producer cancelled | 派生生产者已取消");
            break;
        }

        // Yield to higher-priority work (scan / thumbnail), sleeping like the AI producer.
        // 让步给更高优先级工作（扫描 / 缩略图），与 AI 生产者一样 sleep。
        if state.should_yield_derivation() {
            debug!("Derivation producer yielding to scan/thumbnail | 派生生产者让步给扫描/缩略图");
            std::thread::sleep(std::time::Duration::from_millis(500));
            continue;
        }

        let conn = match state.db_read_pool.get() {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "DB pool error in derivation producer | 派生生产者 DB 池错误: {}",
                    e
                );
                break;
            }
        };
        let batch = match get_pending_derivations(&conn, BATCH_SIZE, kind_filter, exclude_kinds) {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    "Query pending derivations failed | 查询待处理派生失败: {}",
                    e
                );
                break;
            }
        };
        drop(conn);

        if batch.is_empty() {
            info!("Derivation producer: no more pending tasks | 派生生产者：无更多待处理任务");
            break;
        }

        // Mark processing so a restart won't re-queue them mid-flight.
        // 标记处理中，使在途任务在重启时不被重复排队。
        let claimed: Vec<(i64, String)> = batch
            .iter()
            .map(|(id, kind, _, _, _, _)| (*id, kind.clone()))
            .collect();
        {
            let write_conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = mark_derivations_processing(&write_conn, &claimed) {
                warn!(
                    "Failed to mark derivations processing | 标记派生处理中失败: {}",
                    e
                );
            }
        }

        for (item_id, kind_str, abs_path, file_format, media_type, cache_key) in batch {
            if token.is_cancelled() {
                break;
            }
            // An unknown kind string (e.g. left by a newer build) — leave it processing;
            // a future build that knows it will recover & handle it. Skip here.
            // 未知 kind 字符串（如更高版本遗留）——保持处理中，由认识它的后续构建恢复处理。此处跳过。
            let Some(kind) = DerivationKind::from_str(&kind_str) else {
                warn!(
                    "Unknown derivation kind '{}' for item {} — skipping | 未知派生 kind",
                    kind_str, item_id
                );
                continue;
            };
            if task_tx
                .send(DerivationTaskMsg {
                    item_id,
                    kind,
                    abs_path: PathBuf::from(abs_path),
                    file_format,
                    media_type,
                    cache_key,
                })
                .is_err()
            {
                break;
            }
        }
    }

    info!("Derivation producer finished | 派生生产者已完成");
}

/// Consumer pool: run each task's kind function, emit a result row.
/// 消费者池：运行每个任务的 kind 函数，产出一条结果行。
fn consume_tasks(
    task_rx: Receiver<DerivationTaskMsg>,
    result_tx: Sender<DerivationResultRow>,
    token: &CancellationToken,
    state: &Arc<AppState>,
    cache_dir: PathBuf,
    thumb_size: u32,
) {
    rayon::scope(|s| {
        for task in task_rx {
            if token.is_cancelled() {
                break;
            }
            // Throttle the dispatch of new heavy decodes (video cover/keyframe) while scan/thumbnail
            // is running OR the user is actively interacting — otherwise the foreground relayout is
            // starved of CPU (布局被视频派生阻塞). The producer also yields, but already-queued tasks
            // would still blast all cores; pausing here drains them slowly during interaction.
            // 在扫描/缩略图运行或用户主动交互时，节流「派发新的重型解码」（视频封面/关键帧）—— 否则前台
            // 重排被饿死（布局被视频派生阻塞）。生产者也会让步，但已入队任务仍会占满所有核；在此暂停可
            // 在交互期间缓慢消化它们。
            while !token.is_cancelled() && state.should_yield_derivation() {
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            if token.is_cancelled() {
                break;
            }
            // R4：派发前从**共享后台重活池**取 permit（与 exotic Worker 请求同一预算，FIFO 公平）。
            // 在此派发线程（非 rayon worker）阻塞取 permit = 天然「预取不超过可派发容量」；permit 移入
            // 任务闭包，完成/取消即 Drop 释放。取消时 acquire 返回 None → 退出派发循环。
            let Some(permit) = state.background_heavy_limiter.acquire(token) else {
                break;
            };
            let result_tx = result_tx.clone();
            let token_clone = token.clone();
            let cache_dir = cache_dir.clone();
            s.spawn(move |_| {
                // 持有 permit 直至任务结束（含提前 return 的取消路径）→ 归还额度。
                let _permit = permit;
                if token_clone.is_cancelled() {
                    return;
                }
                let ctx = DerivationContext {
                    item_id: task.item_id,
                    kind: task.kind,
                    abs_path: task.abs_path,
                    file_format: task.file_format,
                    media_type: task.media_type,
                    cache_key: task.cache_key,
                    cache_dir,
                    thumb_size,
                };
                let row: DerivationResultRow = match kind::run(&ctx) {
                    // status=2 done, with optional payload path + (cover) thumbhash.
                    Ok(out) => (
                        task.item_id,
                        task.kind.as_str().to_string(),
                        2,
                        out.payload_path,
                        None,
                        out.thumbhash,
                        out.page_count,
                    ),
                    // status=3 error, store the message for the UI / diagnostics.
                    Err(e) => {
                        debug!(
                            "Derivation {} failed for item {} | 派生 {} 失败: {}",
                            task.kind.as_str(),
                            task.item_id,
                            task.kind.as_str(),
                            e
                        );
                        (
                            task.item_id,
                            task.kind.as_str().to_string(),
                            3,
                            None,
                            Some(e.to_string()),
                            None,
                            None,
                        )
                    }
                };
                let _ = result_tx.send(row);
            });
        }
    });
    info!("Derivation consumers finished | 派生消费者已完成");
}

/// Writer: batch-collect results and persist status/payload to the DB. For cover-producing
/// kinds (video/audio cover, doc thumb) it ALSO mirrors `thumb_status=1 / thumb_path / thumbhash`
/// onto `media_items` and the resident layout cache, then nudges the gallery to refresh — so
/// `MediaThumb` shows freshly-derived covers with zero frontend changes (invariant §1.3.4).
/// 写入器：批量收集结果并把状态/产物持久化到 DB。对封面类 kind（视频/音频封面、文档缩略图），
/// 还把 `thumb_status=1 / thumb_path / thumbhash` 回填到 `media_items` 与常驻布局缓存，
/// 再通知画廊刷新 —— 使 `MediaThumb` 零改动即可显示新派生的封面（不变量 §1.3.4）。
fn write_results(
    app: &AppHandle,
    state: &Arc<AppState>,
    result_rx: Receiver<DerivationResultRow>,
    token: &CancellationToken,
) {
    let mut batch: Vec<DerivationResultRow> = Vec::with_capacity(BATCH_SIZE as usize);
    let mut done = 0u64;
    let mut errored = 0u64;
    let mut covers_landed = false;

    let flush = |state: &Arc<AppState>,
                 batch: &mut Vec<DerivationResultRow>,
                 done: &mut u64,
                 errored: &mut u64,
                 covers_landed: &mut bool| {
        if batch.is_empty() {
            return;
        }
        for (_, _, status, _, _, _, _) in batch.iter() {
            if *status == 2 {
                *done += 1;
            } else {
                *errored += 1;
            }
        }

        // Collect successful cover results to mirror onto media_items + layout cache.
        // 收集成功的封面结果，回填到 media_items + 布局缓存。
        let cover_thumbs: Vec<ThumbResult> = batch
            .iter()
            .filter_map(|(item_id, kind, status, payload_path, _, thumbhash, _)| {
                let k = DerivationKind::from_str(kind)?;
                if *status == 2 && k.produces_thumbnail() {
                    Some(ThumbResult {
                        item_id: *item_id,
                        thumb_status: 1,
                        thumb_path: payload_path.clone(),
                        thumbhash: thumbhash.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        // Collect epub page counts to upsert into document_meta (§3.8.2 / T10). doc_thumb 后端
        // 只处理 epub（见 derive/doc.rs），故 doc_subtype 固定 "epub"。仅成功(status=2)且 page_count
        // 有值的行参与。
        let doc_pages: Vec<(i64, i64)> = batch
            .iter()
            .filter_map(|(item_id, kind, status, _, _, _, page_count)| {
                if *status == 2 && kind == "doc_thumb" {
                    page_count.map(|pc| (*item_id, pc))
                } else {
                    None
                }
            })
            .collect();

        {
            let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = batch_finish_derivations(&conn, batch) {
                warn!("Batch derivation write failed | 批量派生写入失败: {}", e);
            }
            // Mirror cover thumbs onto media_items in the same write lock.
            // 在同一写锁内把封面缩略图回填到 media_items。
            for t in &cover_thumbs {
                if let Err(e) = update_thumb_result(
                    &conn,
                    t.item_id,
                    1,
                    t.thumb_path.as_deref(),
                    t.thumbhash.as_deref(),
                ) {
                    warn!(
                        "Mirror cover to media_items failed for id={} | 回填封面失败: {}",
                        t.item_id, e
                    );
                }
            }
            // Upsert epub page counts into document_meta in the same write lock (§3.8.2 / T10)。
            // 在同一写锁内 upsert epub 页数到 document_meta（doc_subtype 固定 "epub"）。
            for (item_id, pc) in &doc_pages {
                if let Err(e) = crate::db::queries::upsert_document_meta(
                    &conn,
                    *item_id,
                    Some(*pc),
                    Some("epub"),
                ) {
                    warn!(
                        "document_meta upsert failed for id={} | 文档元数据回填失败: {}",
                        item_id, e
                    );
                }
            }
        }

        // Keep the resident layout cache consistent (O(batch) by id index) so covers survive
        // scroll-out/in without a full recompute (invariant §1.3.4).
        // 同步常驻布局缓存（按 id 索引 O(batch)），使封面在滚出再滚回时无需整表重算（不变量 §1.3.4）。
        if !cover_thumbs.is_empty() {
            state.apply_thumb_results(&cover_thumbs);
            *covers_landed = true;
        }
        batch.clear();
    };

    for row in result_rx {
        if token.is_cancelled() {
            info!("Derivation writer cancelled | 派生写入器已取消");
            break;
        }
        batch.push(row);
        if batch.len() >= BATCH_SIZE as usize {
            flush(
                state,
                &mut batch,
                &mut done,
                &mut errored,
                &mut covers_landed,
            );
        }
    }
    flush(
        state,
        &mut batch,
        &mut done,
        &mut errored,
        &mut covers_landed,
    );

    // Nudge the gallery to recompute/refresh visible rows once covers have landed. Reuses the
    // enrichment event — MediaGrid already debounce-recomputes on it (payload ignored).
    // 封面落地后通知画廊重算/刷新可见行。复用 enrichment 事件 —— MediaGrid 已对其防抖重算（忽略 payload）。
    if covers_landed {
        let _ = app.emit(
            "db:media_enriched",
            MediaEnrichedPayload {
                root_id: 0,
                enriched_count: 0,
                total: 0,
            },
        );
    }

    info!(
        "Derivation writer finished: done={} error={} | 派生写入器完成: 完成={} 错误={}",
        done, errored, done, errored
    );
}

/// One-shot count of derivation tasks by status, for the status summary IPC.
/// 派生任务按状态的一次性计数，用于状态摘要 IPC。
pub fn derivation_counts(state: &AppState) -> crate::error::Result<(i64, i64, i64, i64)> {
    let conn = state.db_read_pool.get()?;
    count_derivations_by_status(&conn)
}
