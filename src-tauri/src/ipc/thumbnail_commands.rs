// src-tauri/src/ipc/thumbnail_commands.rs
//! Tauri IPC commands for thumbnail generation (§ 6.1 — thumbnail).
//! 用于缩略图生成的 Tauri IPC 命令（§ 6.1 — 缩略图）。

use std::sync::Arc;

use crossbeam_channel::bounded;
use rayon::prelude::*;
use tauri::State;
use tracing::{error, info};

use crate::db::models::ThumbResult;
use crate::error::{AppError, Result};
use crate::exotic::{ExoticHost, ExoticTaskStatus};
use crate::state::AppState;
use crate::thumbnail::{
    decode_media_step, encode_media_step, generate_thumbnail, process_deferred_cpu,
    route_thumbnail, DecodeResult, ThumbResultOrDeferred, ThumbnailRoute, ThumbnailRouteInput,
};

/// 【一键切换架构开关】
/// true: 使用方案二（多阶段流水线解耦），适合未来进行深度的并发与 IO 性能调优。
/// false: 使用方案一（直接通过 Rayon 进行无锁并发迭代），由于 Rayon 的工作窃取调度极其成熟，目前在多核 CPU 上性能最好。
const USE_PIPELINE: bool = true;

#[tauri::command]
pub async fn batch_request_thumbnails(
    item_ids: Vec<i64>,
    target_size: Option<u32>,
    on_result: tauri::ipc::Channel<ThumbResult>,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    let state_arc = state.inner().clone();
    let mut config = { state_arc.thumb_config.read().unwrap().clone() };
    if let Some(size) = target_size {
        config.size = size;
    }

    // R1-3：批量缓存查询走 read_blocking。
    let ids_for_query = item_ids.clone();
    let (fast_results, route_fmt, route_cache_key) =
        super::blocking::read_blocking(&state, move |conn| {
            // Check cache in batch
            let mut fast_results = std::collections::HashMap::new();
            // id → file_format（R7：缓存查询前置扩列 file_format，供 Router 判定，避免 N+1）。
            let mut route_fmt: std::collections::HashMap<i64, String> =
                std::collections::HashMap::new();
            // id → cache_key（问题4：done 任务重算期望指纹需要，避免 Router 内回查）。
            let mut route_cache_key: std::collections::HashMap<i64, i64> =
                std::collections::HashMap::new();
            let placeholders = ids_for_query
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");

            if !placeholders.is_empty() {
                let sql = format!(
                    "SELECT id, thumb_status, thumb_path, thumbhash, file_format, cache_key FROM media_items WHERE id IN ({})",
                    placeholders
                );
                let mut stmt = conn.prepare(&sql).map_err(AppError::Db)?;
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(&ids_for_query), |row| {
                        Ok((
                            ThumbResult {
                                item_id: row.get(0)?,
                                thumb_status: row.get(1)?,
                                thumb_path: row.get(2)?,
                                thumbhash: row.get(3)?,
                            },
                            row.get::<_, String>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    })
                    .map_err(AppError::Db)?;

                for (r, fmt, cache_key) in rows.flatten() {
                    route_cache_key.insert(r.item_id, cache_key);
                    route_fmt.insert(r.item_id, fmt);
                    if r.thumb_status == 1 || r.thumb_status == 3 || r.thumb_status == 2 {
                        fast_results.insert(r.item_id, r);
                    }
                }
            }
            Ok((fast_results, route_fmt, route_cache_key))
        })
        .await?;
    let mut needs_gen = Vec::new();

    for &id in &item_ids {
        if let Some(r) = fast_results.get(&id) {
            if on_result.send(r.clone()).is_err() {
                tracing::debug!("Channel disconnected, ignoring thumb result send");
            }
        } else {
            needs_gen.push(id);
        }
    }

    // Sync fast-path results into layout_cache so fetchRowsByY returns
    // up-to-date thumb_status (avoids stale status=0 after prior generation).
    // 将快速路径结果同步到 layout_cache，使 fetchRowsByY 返回
    // 最新的 thumb_status（避免先前生成后仍返回陈旧的 status=0）。
    if !fast_results.is_empty() {
        let fast_vec: Vec<ThumbResult> = fast_results.values().cloned().collect();
        state.apply_thumb_results(&fast_vec);
    }

    // ── 冷门格式让路（R3）：needs_gen 中命中未完成 Exotic 的项不进主 generator ──────────
    // 在 needs_gen 过滤点接入 route_thumbnail（与 full 命令共享同一纯函数判定）。
    // 让路项绝不调 generate_thumbnail/decode_media_step，也绝不写 thumb_status=2。
    if !needs_gen.is_empty() {
        let snap = state_arc.exotic_catalog.snapshot();
        // 仅当批内确有 catalog 已认领的格式时才走完整路由，常见库零额外成本。
        let has_exotic = needs_gen.iter().any(|id| {
            route_fmt
                .get(id)
                .map(|f| snap.resolve_format(f).is_some())
                .unwrap_or(false)
        });
        if has_exotic {
            // R1-3：路由段含读池 SQL（任务态批查）+ 写锁 SQL（指纹失效退回 pending），与指纹
            // 计算一并下沉 blocking；闭包返回过滤后的 needs_gen（kept）。
            let state_gate = state_arc.clone();
            let on_result_gate = on_result.clone();
            needs_gen = tokio::task::spawn_blocking(move || -> Result<Vec<i64>> {
                // 路由仅需 catalog 认领 + 任务态（route_thumbnail 不读 availability）——未授权/未安装的 PSD
                // 同样不能进主 generator（主解码必失败）。故用 stub Host（无 DB/keyring），避免每项安装/授权
                // 查询造成 N+1（R7）；真实安装/授权真相由 get_exotic_item_state 等命令按需读取。
                let host = ExoticHost::new(state_gate.exotic_catalog.clone());
                let task_map = {
                    let conn = state_gate.db_read_pool.get().map_err(AppError::from)?;
                    crate::db::queries::exotic_thumbnail_route_info_for_items(&conn, &needs_gen)
                        .unwrap_or_default()
                };
                // 指纹档位用全局 thumb_config（与 Coordinator/Pipeline 同源），非 batch 的 target_size
                // override——exotic 不经 batch 生成，指纹须对齐 Coordinator 所用档位（问题4）。
                let global_size = { state_gate.thumb_config.read().unwrap().size };
                let mut kept = Vec::with_capacity(needs_gen.len());
                let mut gated = Vec::new();
                // done 但指纹已失效（如用户改档位）→ 须先失效为 pending 再让路重做（Part2 §4.3，问题4）。
                let mut stale = Vec::new();
                for &id in &needs_gen {
                    let fmt = route_fmt.get(&id).map(|s| s.as_str()).unwrap_or("");
                    if snap.resolve_format(fmt).is_none() {
                        kept.push(id); // 常见格式快速路径，不构造 resolution。
                        continue;
                    }
                    let res = host.resolve_format(fmt);
                    let info = task_map.get(&id);
                    let task_status = info.map(|i| i.status);
                    // done 任务重算期望指纹比对存储指纹；缺任一指纹输入 → 保守失效、让路重做。
                    let fingerprint_valid = match (task_status, info) {
                        (Some(ExoticTaskStatus::Done), Some(i)) => match (
                            route_cache_key.get(&id),
                            i.worker_version.as_deref(),
                            i.input_fingerprint.as_deref(),
                            res.plugin_id.as_deref(),
                        ) {
                            (Some(&ck), Some(wv), Some(stored), Some(pid)) => {
                                crate::exotic::fingerprint::thumbnail_fingerprint(
                                    ck,
                                    pid,
                                    wv,
                                    global_size,
                                )
                                .fingerprint
                                    == stored
                            }
                            _ => false,
                        },
                        _ => false, // 非 done：router 不使用该值
                    };
                    let route = route_thumbnail(&ThumbnailRouteInput {
                        item_id: id,
                        file_format: fmt,
                        thumb_status: 0, // needs_gen 项均为 thumb_status=0
                        resolution: Some(&res),
                        task_status,
                        fingerprint_valid,
                    });
                    match route {
                        // offering 不认领 thumbnail（如仅 metadata）→ 主 generator。
                        ThumbnailRoute::Common => kept.push(id),
                        // exotic 项一律不送主 generator（PSD 主解码必失败）；done+valid 走此。
                        ThumbnailRoute::Existing => gated.push(id),
                        ThumbnailRoute::Exotic(_) => {
                            // done 却被判 Exotic ⟺ 指纹失效（done+valid 会判 Existing）→ 失效重做。
                            if task_status == Some(ExoticTaskStatus::Done) {
                                stale.push(id);
                            }
                            gated.push(id);
                        }
                    }
                }
                // 指纹失效的 done 先退回 pending（否则 Coordinator claim 只取 0/3，永不重做）。
                if !stale.is_empty() {
                    if let Ok(conn) = state_gate.db_writer.lock() {
                        for &id in &stale {
                            let _ = crate::db::queries::invalidate_exotic_tasks_for_item(&conn, id);
                        }
                    }
                    info!(
                        "batch_request_thumbnails: {} 项 exotic done 指纹失效 → 退回 pending 重做",
                        stale.len()
                    );
                }

                // 让路项回送当前状态（thumb_status=0、无产物），平衡前端在途计数（问题9），
                // 绝不写 thumb_status=2。真正出图由 exotic Worker 流水线完成。
                if !gated.is_empty() {
                    info!(
                        "batch_request_thumbnails: {} 项让路冷门格式插件（不调主 generator）",
                        gated.len()
                    );
                    for id in &gated {
                        let _ = on_result_gate.send(ThumbResult {
                            item_id: *id,
                            thumb_status: 0,
                            thumb_path: None,
                            thumbhash: None,
                        });
                    }
                }
                // 合并发一次 wake：让 Coordinator 领取 pending（含刚失效的 stale）。
                if !gated.is_empty() || !stale.is_empty() {
                    state_gate.wake_exotic(crate::exotic::coordinator::WakeReason::ConfigChanged);
                }
                Ok(kept)
            })
            .await
            .map_err(|e| AppError::System(e.to_string()))??;
        }
    }

    info!(
        "batch_request_thumbnails: total={} needs_gen={} | 批量请求缩略图: 总计={} 需要生成={}",
        item_ids.len(),
        needs_gen.len(),
        item_ids.len(),
        needs_gen.len()
    );

    if !needs_gen.is_empty() {
        let config = config.clone();

        tokio::task::spawn_blocking(move || {
            if !USE_PIPELINE {
                // 方案一：Rayon 直线并发 (Scheme 1)
                let intermediate: Vec<Result<ThumbResultOrDeferred>> = needs_gen
                    .par_iter()
                    .filter_map(|&id| {
                        if state_arc.cancelled_thumb_ids.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
                            return None;
                        }

                        let pool = match state_arc.db_read_pool.get() {
                            Ok(p) => p,
                            Err(e) => {
                                error!("Failed to get read pool: {e}");
                                return None;
                            }
                        };

                        let item = match crate::db::queries::get_media_item(&pool, id) {
                            Ok(i) => i,
                            Err(e) => {
                                error!("Failed to get item {id}: {e}");
                                return None;
                            }
                        };

                        let (root_path, rel_path, file_name) = match crate::db::queries::get_item_path_info(&pool, id) {
                            Ok(paths) => paths,
                            Err(e) => {
                                error!("Failed to get path info for {id}: {e}");
                                return None;
                            }
                        };

                        let abs_path_str = crate::utils::path::resolve_media_path(&root_path, &rel_path, &file_name);
                        let abs_path = std::path::Path::new(&abs_path_str);

                        Some(generate_thumbnail(&item, abs_path, &state_arc.engine_arena, &config))
                    })
                    .collect();

                let mut results = Vec::new();
                let mut deferred = Vec::new();
                for (id, res) in needs_gen.iter().zip(intermediate) {
                    match res {
                        Ok(ThumbResultOrDeferred::Done(r)) => {
                            if on_result.send(r.clone()).is_err() {
                                tracing::debug!("Channel disconnected, ignoring thumb result send");
                            }
                            results.push(r);
                        }
                        Ok(ThumbResultOrDeferred::Deferred { item, abs_path }) => {
                            deferred.push((item, abs_path));
                        }
                        Err(e) => {
                            let r = ThumbResult {
                                item_id:      *id,
                                thumb_status: 2,
                                thumb_path:   None,
                                thumbhash:    None,
                            };
                            if on_result.send(r.clone()).is_err() {
                                tracing::debug!("Channel disconnected, ignoring thumb result send");
                            }
                            error!("Thumbnail gen failed for id={id}: {e}");
                            results.push(r);
                        }
                    }
                }

                if !deferred.is_empty() {
                    let cpu_results: Vec<ThumbResult> = deferred
                        .into_par_iter()
                        .filter_map(|(item, abs_path)| {
                            if state_arc.cancelled_thumb_ids.lock().unwrap_or_else(|e| e.into_inner()).remove(&item.id) {
                                return None;
                            }
                            match process_deferred_cpu(&item, &abs_path, &state_arc.engine_arena, &config) {
                                Ok(r) => {
                                    if on_result.send(r.clone()).is_err() {
                                        tracing::debug!("Channel disconnected, ignoring thumb result send");
                                    }
                                    Some(r)
                                }
                                Err(e) => {
                                    let r = ThumbResult {
                                        item_id:      item.id,
                                        thumb_status: 2,
                                        thumb_path:   None,
                                        thumbhash:    None,
                                    };
                                    if on_result.send(r.clone()).is_err() {
                                        tracing::debug!("Channel disconnected, ignoring thumb result send");
                                    }
                                    error!("Thumbnail cpu gen failed for id={}: {}", item.id, e);
                                    Some(r)
                                }
                            }
                        })
                        .collect();
                    results.extend(cpu_results);
                }

                if !results.is_empty() {
                    if let Ok(mut conn) = state_arc.db_writer.lock() {
                        if let Ok(tx) = conn.transaction() {
                            for res in &results {
                                let _ = crate::db::queries::update_thumb_result(
                                    &tx,
                                    res.item_id,
                                    res.thumb_status,
                                    res.thumb_path.as_deref(),
                                    res.thumbhash.as_deref()
                                );
                            }
                            let _ = tx.commit();
                        }
                    }
                    // Sync results into layout_cache so fetchRowsByY returns fresh data
                    // 同步结果到 layout_cache，使 fetchRowsByY 返回最新数据
                    state_arc.apply_thumb_results(&results);
                }
                info!("batch_request_thumbnails: finished parallel block | 批量请求生成完成 (Rayon Scheme 1)");
                return;
            }

            // 方案二：多阶段流水线解耦 (Scheme 2)
            let (decode_tx, decode_rx) = bounded(1024);
            let (encode_tx, encode_rx) = bounded(1024);
            let (result_tx, result_rx) = bounded(needs_gen.len().max(1024));
            // T12(§3.5.1):deferred CPU 专用通道——CPU 密集回退绝不在 decode worker 上
            // inline 跑(会占住 decode 线程、反让 GPU 提交空等),转投下方独立小池消化。
            let (deferred_tx, deferred_rx) = bounded(1024);

            let needs_gen_clone = needs_gen.clone();
            let state_dispatcher = state_arc.clone();
            let result_tx_dispatch = result_tx.clone();
            let decode_tx_dispatch = decode_tx.clone();
            std::thread::spawn(move || {
                for id in needs_gen_clone {
                    // 取消请求只消费一次。否则同一 id 滚回视口后的新请求会被旧取消标记永久跳过。
                    if state_dispatcher.cancelled_thumb_ids.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
                        continue;
                    }
                    // Load the item; emit a FAILURE result for anything we can't load so that
                    // every requested id yields exactly one result. Silently skipping an id
                    // leaves the frontend's in-flight counter unbalanced and the "处理中 N 项"
                    // indicator stuck forever (问题9).
                    // 加载项；对任何无法加载的项也发一个失败结果，使每个请求 id 都恰好产出一个结果。
                    // 静默跳过会让前端在途计数失衡，「处理中 N 项」指示永久卡住（问题9）。
                    let loaded = state_dispatcher.db_read_pool.get().ok().and_then(|pool| {
                        let item = crate::db::queries::get_media_item(&pool, id).ok()?;
                        let (root_path, rel_path, file_name) =
                            crate::db::queries::get_item_path_info(&pool, id).ok()?;
                        let abs_path_str = crate::utils::path::resolve_media_path(&root_path, &rel_path, &file_name);
                        Some((item, std::path::PathBuf::from(abs_path_str)))
                    });
                    match loaded {
                        Some((item, abs_path)) => {
                            if decode_tx_dispatch.send((item, abs_path)).is_err() {
                                return;
                            }
                        }
                        None => {
                            error!("[batch_thumb] could not load item id={id}; emitting failure result | 无法加载项，发送失败结果");
                            let _ = result_tx_dispatch.send(ThumbResult {
                                item_id: id,
                                thumb_status: 2,
                                thumb_path: None,
                                thumbhash: None,
                            });
                        }
                    }
                }
            });
            // 关闭原始发送端；调度线程的 clone 结束后，decode workers 才能退出，
            // result_rx 才会收尾，避免批次结果已发完但 invoke 永远不返回。
            drop(decode_tx);

            let config_decode = config.clone();
            let cpu_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
            let decode_threads = (cpu_cores * 2).max(4);
            for _ in 0..decode_threads {
                let rx = decode_rx.clone();
                let tx = encode_tx.clone();
                let res_tx = result_tx.clone();
                let def_tx = deferred_tx.clone();
                let cfg = config_decode.clone();
                let state_worker = state_arc.clone();
                std::thread::spawn(move || {
                    while let Ok((item, abs_path)) = rx.recv() {
                        match decode_media_step(&item, &abs_path, &state_worker.engine_arena, &cfg) {
                            Ok(DecodeResult::Ready(res)) => {
                                let _ = res_tx.send(res);
                            }
                            Ok(DecodeResult::ToEncode { item_id, cache_key, decoded }) => {
                                let _ = tx.send((item_id, cache_key, decoded));
                            }
                            Ok(DecodeResult::DeferredToCpu { item, abs_path }) => {
                                // T12(§3.5.1):不再 inline——CPU 密集回退会占住本 decode worker、
                                // 反让 GPU decode 空等;转投专用 deferred 小池。通道已关(池退出)
                                // 时兜底发失败结果,保持「每 id 恰一结果」不变量(问题9)。
                                if let Err(e) = def_tx.send((item, abs_path)) {
                                    let (item, _abs) = e.into_inner();
                                    error!("Deferred channel closed for id={} | deferred 通道已关", item.id);
                                    let _ = res_tx.send(ThumbResult { item_id: item.id, thumb_status: 2, thumb_path: None, thumbhash: None });
                                }
                            }
                            Err(e) => {
                                error!("Decode failed for id={}: {}", item.id, e);
                                let _ = res_tx.send(ThumbResult { item_id: item.id, thumb_status: 2, thumb_path: None, thumbhash: None });
                            }
                        }
                    }
                });
            }
            drop(encode_tx);
            // 主句柄仅供 decode worker 克隆;此处即弃——decode 阶段全部退出后 deferred 池
            // 随通道关闭收尾(否则其 result_tx 克隆悬活,result_rx 永不结束、invoke 不返回)。
            drop(deferred_tx);

            // T12(§3.5.1)deferred CPU 专用小池:与 decode/encode 阶段解耦。池宽 max(1, cores/2)
            // ——CPU 密集解码本就吃核,池小不损吞吐,却保证 decode 通道永不被 CPU 回退占住。
            let deferred_threads = (cpu_cores / 2).max(1);
            let config_deferred = config.clone();
            for _ in 0..deferred_threads {
                let rx = deferred_rx.clone();
                let res_tx = result_tx.clone();
                let cfg = config_deferred.clone();
                let state_worker = state_arc.clone();
                std::thread::spawn(move || {
                    while let Ok((item, abs_path)) = rx.recv() {
                        match process_deferred_cpu(&item, &abs_path, &state_worker.engine_arena, &cfg)
                        {
                            Ok(res) => {
                                let _ = res_tx.send(res);
                            }
                            Err(e) => {
                                error!("Deferred CPU Decode failed for id={}: {}", item.id, e);
                                let _ = res_tx.send(ThumbResult {
                                    item_id: item.id,
                                    thumb_status: 2,
                                    thumb_path: None,
                                    thumbhash: None,
                                });
                            }
                        }
                    }
                });
            }

            let config_encode = config.clone();
            for _ in 0..cpu_cores {
                let rx = encode_rx.clone();
                let tx = result_tx.clone();
                let cfg = config_encode.clone();
                std::thread::spawn(move || {
                    while let Ok((item_id, cache_key, decoded)) = rx.recv() {
                        match encode_media_step(item_id, cache_key, decoded, &cfg) {
                            Ok(res) => { let _ = tx.send(res); }
                            Err(e) => {
                                error!("Encode failed for id={}: {}", item_id, e);
                                let _ = tx.send(ThumbResult { item_id, thumb_status: 2, thumb_path: None, thumbhash: None });
                            }
                        }
                    }
                });
            }
            drop(result_tx);

            let mut results = Vec::new();
            while let Ok(res) = result_rx.recv() {
                if on_result.send(res.clone()).is_err() {
                    tracing::debug!("Channel disconnected, ignoring thumb result send");
                }
                results.push(res);
            }

            if !results.is_empty() {
                if let Ok(mut conn) = state_arc.db_writer.lock() {
                    if let Ok(tx) = conn.transaction() {
                        for res in &results {
                            let _ = crate::db::queries::update_thumb_result(
                                &tx,
                                res.item_id,
                                res.thumb_status,
                                res.thumb_path.as_deref(),
                                res.thumbhash.as_deref()
                            );
                        }
                        let _ = tx.commit();
                    }
                }
                // Sync results into layout_cache so fetchRowsByY returns fresh data
                // 同步结果到 layout_cache，使 fetchRowsByY 返回最新数据
                state_arc.apply_thumb_results(&results);
            }

            info!("batch_request_thumbnails: finished pipeline | 批量请求生成完成 (Pipeline Scheme 2)");
        })
        .await
        .map_err(|e| AppError::Io(e.into()))?;
    }

    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullThumbProgressPayload {
    pub generated: u64,
    pub total: u64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_item: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

#[tauri::command]
pub async fn start_full_thumbnail_generation(
    on_progress: tauri::ipc::Channel<FullThumbProgressPayload>,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    // R1-3：全表重置（写锁）走 write_blocking。
    super::blocking::write_blocking(&state, |conn| {
        conn.execute("UPDATE media_items SET thumb_status = 0, thumb_path = NULL, thumbhash = NULL WHERE is_deleted = 0", [])
            .map_err(AppError::Db)?;
        // 同步失效 exotic thumbnail 任务（问题1）：否则 done PSD 既不被重领、又被放回主 generator。
        crate::db::queries::reset_all_exotic_thumbnail_tasks(conn)
    })
    .await?;
    // 让 Coordinator 重领被退回 pending 的 exotic 任务（重做覆盖同路径产物）。
    state.wake_exotic(crate::exotic::coordinator::WakeReason::ConfigChanged);

    // R1-3：计数（读池）走 read_blocking。
    let total = super::blocking::read_blocking(&state, |conn| {
        crate::db::queries::count_pending_thumb_items(conn)
    })
    .await?;

    if total == 0 {
        let _ = on_progress.send(FullThumbProgressPayload {
            generated: 0,
            total: 0,
            status: "completed".to_string(),
            current_item: None,
            phase: None,
        });
        return Ok(());
    }

    state.cancel_thumb_gen();
    let cancel_token = state.new_thumb_gen_token();

    let state_arc = Arc::clone(&*state);
    let generated_count = Arc::new(std::sync::atomic::AtomicU64::new(0));

    tokio::task::spawn_blocking(move || -> Result<()> {
        let start_time = std::time::Instant::now();
        let _ = on_progress.send(FullThumbProgressPayload {
            generated: 0,
            total: total as u64,
            status: "running".to_string(),
            current_item: None,
            phase: Some("GPU".to_string()),
        });

        let config = state_arc.thumb_config.read().unwrap().clone();
        info!(
            "[FullThumbGen] START: total={} strategy={} gpu_engine={} size={} skip_max_bytes={} cache_dir={:?} pipeline={} | 全量缩略图生成开始",
            total, config.strategy, config.gpu_engine, config.size, config.skip_max_bytes, config.cache_dir, USE_PIPELINE
        );

        if !USE_PIPELINE {
            // 方案一：Rayon 直线并发 (Scheme 1)
            let all_ids = {
                let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
                crate::db::queries::get_all_pending_thumb_ids(&pool).unwrap_or_default()
            };

            for batch_ids in all_ids.chunks(50) {
                if cancel_token.is_cancelled() {
                    break;
                }

                let batch_results: Vec<(i64, Result<ThumbResult>)> = batch_ids
                    .par_iter()
                    .filter_map(|&id| {
                        if cancel_token.is_cancelled() {
                            return None;
                        }

                        let pool = match state_arc.db_read_pool.get() {
                            Ok(p) => p,
                            Err(_) => return None,
                        };

                        let item = match crate::db::queries::get_media_item(&pool, id) {
                            Ok(i) => i,
                            Err(_) => return None,
                        };

                        let (root_path, rel_path, file_name) =
                            match crate::db::queries::get_item_path_info(&pool, id) {
                                Ok(p) => p,
                                Err(_) => return None,
                            };

                        let abs_path_str = crate::utils::path::resolve_media_path(
                            &root_path, &rel_path, &file_name,
                        );
                        let abs_path = std::path::Path::new(&abs_path_str);

                        let current = generated_count.load(std::sync::atomic::Ordering::Relaxed);
                        let _ = on_progress.send(FullThumbProgressPayload {
                            generated: current,
                            total: total as u64,
                            status: "running".to_string(),
                            current_item: Some(file_name),
                            phase: Some("GPU".to_string()),
                        });

                        let res =
                            generate_thumbnail(&item, abs_path, &state_arc.engine_arena, &config);
                        // In scheme 1 full gen, if it's deferred, we just immediately process it for simplicity
                        // (since scheme 1 is deprecated for two-phase)
                        let res = match res {
                            Ok(ThumbResultOrDeferred::Done(r)) => Ok(r),
                            Ok(ThumbResultOrDeferred::Deferred { item, abs_path }) => {
                                process_deferred_cpu(
                                    &item,
                                    &abs_path,
                                    &state_arc.engine_arena,
                                    &config,
                                )
                            }
                            Err(e) => Err(e),
                        };

                        generated_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        Some((id, res))
                    })
                    .collect();

                let mut successful_results = Vec::new();

                if let Ok(mut conn) = state_arc.db_writer.lock() {
                    if let Ok(tx) = conn.transaction() {
                        for (id, res) in &batch_results {
                            match res {
                                Ok(r) => {
                                    successful_results.push(r.clone());
                                    let _ = crate::db::queries::update_thumb_result(
                                        &tx,
                                        r.item_id,
                                        r.thumb_status,
                                        r.thumb_path.as_deref(),
                                        r.thumbhash.as_deref(),
                                    );
                                }
                                Err(e) => {
                                    error!("Full gen failed for id={}: {}", id, e);
                                    let _ = crate::db::queries::update_thumb_result(
                                        &tx, *id, 2, None, None,
                                    );
                                }
                            }
                        }
                        let _ = tx.commit();
                    }
                }

                if !successful_results.is_empty() {
                    state_arc.apply_thumb_results(&successful_results);
                }

                let current_gen = generated_count.load(std::sync::atomic::Ordering::Relaxed);
                let _ = on_progress.send(FullThumbProgressPayload {
                    generated: current_gen,
                    total: total as u64,
                    status: "running".to_string(),
                    current_item: None,
                    phase: Some("GPU".to_string()),
                });
            }
        } else {
            // 方案二：多阶段流水线解耦 (Scheme 2)
            let (decode_tx, decode_rx) = bounded(1024);
            let (encode_tx, encode_rx) = bounded(1024);
            let (result_tx, result_rx) = bounded::<
                std::result::Result<
                    ThumbResult,
                    (crate::db::models::MediaItem, std::path::PathBuf),
                >,
            >(1024);

            let state_dispatcher = state_arc.clone();
            let cancel_dispatcher = cancel_token.clone();
            std::thread::spawn(move || {
                let all_ids = {
                    let pool = match state_dispatcher.db_read_pool.get() {
                        Ok(p) => p,
                        Err(_) => return,
                    };
                    crate::db::queries::get_all_pending_thumb_ids(&pool).unwrap_or_default()
                };
                info!("[FullThumbGen] Dispatcher: {} pending IDs fetched | 调度器: 获取到 {} 个待处理 ID", all_ids.len(), all_ids.len());

                for chunk in all_ids.chunks(50) {
                    if cancel_dispatcher.is_cancelled() {
                        break;
                    }
                    let pool = match state_dispatcher.db_read_pool.get() {
                        Ok(p) => p,
                        Err(_) => break,
                    };

                    for &id in chunk {
                        if cancel_dispatcher.is_cancelled() {
                            break;
                        }
                        if let Ok(item) = crate::db::queries::get_media_item(&pool, id) {
                            if let Ok((root_path, rel_path, file_name)) =
                                crate::db::queries::get_item_path_info(&pool, id)
                            {
                                let abs_path_str = crate::utils::path::resolve_media_path(
                                    &root_path, &rel_path, &file_name,
                                );
                                let abs_path = std::path::PathBuf::from(abs_path_str);
                                if decode_tx.send((item, abs_path)).is_err() {
                                    return;
                                }
                            } else {
                                error!("[FullThumbGen] path_info failed for id={}", id);
                            }
                        } else {
                            error!("[FullThumbGen] get_media_item failed for id={}", id);
                        }
                    }
                }
                info!("[FullThumbGen] Dispatcher: done sending items | 调度器: 发送完毕");
            });

            let config_decode = config.clone();
            let cancel_decode = cancel_token.clone();
            let cpu_cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(8);
            let decode_threads = (cpu_cores * 2).max(4);
            for _ in 0..decode_threads {
                let rx = decode_rx.clone();
                let tx = encode_tx.clone();
                let res_tx = result_tx.clone();
                let state_worker = state_arc.clone();
                let cfg = config_decode.clone();
                let cancel = cancel_decode.clone();
                std::thread::spawn(move || {
                    while let Ok((item, abs_path)) = rx.recv() {
                        if cancel.is_cancelled() {
                            break;
                        }
                        match decode_media_step(&item, &abs_path, &state_worker.engine_arena, &cfg)
                        {
                            Ok(DecodeResult::Ready(res)) => {
                                let _ = res_tx.send(Ok(res));
                            }
                            Ok(DecodeResult::ToEncode {
                                item_id,
                                cache_key,
                                decoded,
                            }) => {
                                if tx.send((item_id, cache_key, decoded)).is_err() {
                                    break;
                                }
                            }
                            Ok(DecodeResult::DeferredToCpu { item, abs_path }) => {
                                let _ = res_tx.send(Err((item, abs_path)));
                            }
                            Err(e) => {
                                error!("Full gen decode failed for id={}: {}", item.id, e);
                                let _ = res_tx.send(Ok(ThumbResult {
                                    item_id: item.id,
                                    thumb_status: 2,
                                    thumb_path: None,
                                    thumbhash: None,
                                }));
                            }
                        }
                    }
                });
            }
            drop(encode_tx);

            let config_encode = config.clone();
            let cancel_encode = cancel_token.clone();
            for _ in 0..cpu_cores {
                let rx = encode_rx.clone();
                let tx = result_tx.clone();
                let cfg = config_encode.clone();
                let cancel = cancel_encode.clone();
                std::thread::spawn(move || {
                    while let Ok((item_id, cache_key, decoded)) = rx.recv() {
                        if cancel.is_cancelled() {
                            break;
                        }
                        match encode_media_step(item_id, cache_key, decoded, &cfg) {
                            Ok(res) => {
                                let _ = tx.send(Ok(res));
                            }
                            Err(e) => {
                                error!("Full gen encode failed for id={}: {}", item_id, e);
                                let _ = tx.send(Ok(ThumbResult {
                                    item_id,
                                    thumb_status: 2,
                                    thumb_path: None,
                                    thumbhash: None,
                                }));
                            }
                        }
                    }
                });
            }
            drop(result_tx);

            let mut successful_results = Vec::new();
            let mut deferred_items = Vec::new();

            // Throttle progress IPC: emitting per image floods the channel (80k+ msgs)
            // and thrashes the sidebar's reactive progress UI (stalls the rAF timer).
            // Send at most once per PROGRESS_THROTTLE; the final completed/cancelled
            // message below is always sent so the bar still ends at 100%.
            // 节流进度 IPC：逐张发送会刷爆通道（8 万+ 条）并使侧边栏响应式进度 UI 抖动
            // （挤掉 rAF 计时器）。最多每 PROGRESS_THROTTLE 发一次；下方最终 completed/
            // cancelled 消息始终发送，进度条仍会走到 100%。
            const PROGRESS_THROTTLE: std::time::Duration = std::time::Duration::from_millis(100);
            let mut last_progress_emit = std::time::Instant::now();

            while let Ok(msg) = result_rx.recv() {
                if cancel_token.is_cancelled() {
                    break;
                }

                match msg {
                    Ok(res) => {
                        successful_results.push(res.clone());
                        generated_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        let now = std::time::Instant::now();
                        if now.duration_since(last_progress_emit) >= PROGRESS_THROTTLE {
                            last_progress_emit = now;
                            let current =
                                generated_count.load(std::sync::atomic::Ordering::Relaxed);
                            let _ = on_progress.send(FullThumbProgressPayload {
                                generated: current,
                                total: total as u64,
                                status: "running".to_string(),
                                current_item: None,
                                phase: Some("GPU".to_string()),
                            });
                        }
                    }
                    Err(deferred) => {
                        deferred_items.push(deferred);
                    }
                }

                if successful_results.len() >= 50 {
                    // Log batch status breakdown
                    // 记录批次状态分布
                    let n_encoded = successful_results
                        .iter()
                        .filter(|r| r.thumb_status == 1)
                        .count();
                    let n_direct = successful_results
                        .iter()
                        .filter(|r| r.thumb_status == 3)
                        .count();
                    let n_failed = successful_results
                        .iter()
                        .filter(|r| r.thumb_status == 2)
                        .count();
                    info!(
                        "[FullThumbGen] Batch flush: {} results (encoded={}, direct={}, failed={}) | 批次写入",
                        successful_results.len(), n_encoded, n_direct, n_failed
                    );

                    if let Ok(mut conn) = state_arc.db_writer.lock() {
                        if let Ok(tx) = conn.transaction() {
                            for r in &successful_results {
                                let _ = crate::db::queries::update_thumb_result(
                                    &tx,
                                    r.item_id,
                                    r.thumb_status,
                                    r.thumb_path.as_deref(),
                                    r.thumbhash.as_deref(),
                                );
                            }
                            let _ = tx.commit();
                        }
                    }

                    state_arc.apply_thumb_results(&successful_results);
                    successful_results.clear();
                }
            }

            // Flush remaining
            if !successful_results.is_empty() {
                if let Ok(mut conn) = state_arc.db_writer.lock() {
                    if let Ok(tx) = conn.transaction() {
                        for r in &successful_results {
                            let _ = crate::db::queries::update_thumb_result(
                                &tx,
                                r.item_id,
                                r.thumb_status,
                                r.thumb_path.as_deref(),
                                r.thumbhash.as_deref(),
                            );
                        }
                        let _ = tx.commit();
                    }
                }
                state_arc.apply_thumb_results(&successful_results);
            }

            // Phase 2: CPU processing for deferred items
            if !deferred_items.is_empty() && !cancel_token.is_cancelled() {
                info!("[FullThumbGen] Phase 2: Processing {} deferred CPU tasks | 阶段2：处理延迟的 CPU 任务", deferred_items.len());
                let mut cpu_successful = Vec::new();
                for (item, abs_path) in deferred_items {
                    if cancel_token.is_cancelled() {
                        break;
                    }

                    let res = match process_deferred_cpu(
                        &item,
                        &abs_path,
                        &state_arc.engine_arena,
                        &config,
                    ) {
                        Ok(r) => r,
                        Err(e) => {
                            error!("Full gen CPU fallback failed for id={}: {}", item.id, e);
                            ThumbResult {
                                item_id: item.id,
                                thumb_status: 2,
                                thumb_path: None,
                                thumbhash: None,
                            }
                        }
                    };

                    cpu_successful.push(res.clone());
                    generated_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    let now = std::time::Instant::now();
                    if now.duration_since(last_progress_emit) >= PROGRESS_THROTTLE {
                        last_progress_emit = now;
                        let current = generated_count.load(std::sync::atomic::Ordering::Relaxed);
                        let _ = on_progress.send(FullThumbProgressPayload {
                            generated: current,
                            total: total as u64,
                            status: "running".to_string(),
                            current_item: None,
                            phase: Some("CPU".to_string()),
                        });
                    }

                    // Flush every 10 for CPU
                    if cpu_successful.len() >= 10 {
                        if let Ok(mut conn) = state_arc.db_writer.lock() {
                            if let Ok(tx) = conn.transaction() {
                                for r in &cpu_successful {
                                    let _ = crate::db::queries::update_thumb_result(
                                        &tx,
                                        r.item_id,
                                        r.thumb_status,
                                        r.thumb_path.as_deref(),
                                        r.thumbhash.as_deref(),
                                    );
                                }
                                let _ = tx.commit();
                            }
                        }
                        cpu_successful.clear();
                    }
                }

                // Flush final CPU
                if !cpu_successful.is_empty() {
                    if let Ok(mut conn) = state_arc.db_writer.lock() {
                        if let Ok(tx) = conn.transaction() {
                            for r in &cpu_successful {
                                let _ = crate::db::queries::update_thumb_result(
                                    &tx,
                                    r.item_id,
                                    r.thumb_status,
                                    r.thumb_path.as_deref(),
                                    r.thumbhash.as_deref(),
                                );
                            }
                            let _ = tx.commit();
                        }
                    }
                }
            }
        }

        let final_gen = generated_count.load(std::sync::atomic::Ordering::Relaxed);
        info!(
            "[FullThumbGen] FINISHED: generated={} total={} cancelled={} elapsed={}ms | 全量缩略图生成完成",
            final_gen, total, cancel_token.is_cancelled(), start_time.elapsed().as_millis()
        );
        if cancel_token.is_cancelled() {
            let _ = on_progress.send(FullThumbProgressPayload {
                generated: final_gen,
                total: total as u64,
                status: "cancelled".to_string(),
                current_item: None,
                phase: None,
            });
        } else {
            let _ = on_progress.send(FullThumbProgressPayload {
                generated: final_gen,
                total: total as u64,
                status: "completed".to_string(),
                current_item: None,
                phase: None,
            });
        }

        // Clear the token after completion/cancellation so AI pipeline won't yield forever.
        // This mirrors how the AI pipeline itself clears ai_analysis_token on completion.
        // 清除 token，防止 AI pipeline 永远让步（与 AI pipeline 的 cancel_ai_analysis() 模式一致）。
        *state_arc
            .thumb_gen_token
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        tracing::info!("Thumbnail gen token cleared after completion | 全量缩略图 token 已清除");

        // Invalidate layout cache so the next compute_layout reads fresh
        // thumb_status / thumb_path from DB instead of serving stale data.
        // 清空布局缓存，使下一次 compute_layout 从数据库读取最新的
        // thumb_status / thumb_path，而非提供陈旧数据。
        *state_arc.layout_cache.write().unwrap() = None;
        tracing::info!(
            "Layout cache invalidated after full thumb gen | 全量缩略图后已清空布局缓存"
        );

        Ok(())
    });

    Ok(())
}

#[tauri::command]
pub fn stop_full_thumbnail_generation(state: State<'_, Arc<AppState>>) -> Result<()> {
    info!("User action: Stopping full thumbnail generation | 用户操作：停止全量缩略图生成");
    state.cancel_thumb_gen();
    Ok(())
}

#[tauri::command]
pub async fn cancel_thumbnail_request(id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    state
        .cancelled_thumb_ids
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id);
    Ok(())
}

#[tauri::command]
pub async fn clear_all_thumbnails(state: State<'_, Arc<AppState>>) -> Result<()> {
    info!("User action: Clearing all thumbnails | 用户操作：清除所有缩略图");

    // R1-3：全表重置（写锁 SQL）+ 缓存目录递归删除（可达数万文件的重阻塞 IO）一并下沉 blocking。
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        // 1. Reset database thumb_status
        {
            let conn = state_arc
                .db_writer
                .lock()
                .map_err(|e| AppError::System(e.to_string()))?;
            conn.execute("UPDATE media_items SET thumb_status = 0, thumb_path = NULL, thumbhash = NULL WHERE thumb_status != 0", [])
                .map_err(AppError::Db)?;
            // 同步失效 exotic thumbnail 任务（问题1）：删目录已清掉 exotic 产物（与主缩略图同一缓存布局），
            // 但 done 任务仍 status=2，不重置则永不重做且会被放回主 generator。
            crate::db::queries::reset_all_exotic_thumbnail_tasks(&conn)?;
        }

        // 2. Delete cache directory
        let cache_dir = state_arc.thumb_config.read().unwrap().cache_dir.clone();
        let thumb_dir = cache_dir.join("thumbnails");
        if thumb_dir.exists() {
            std::fs::remove_dir_all(&thumb_dir).map_err(AppError::Io)?;
        }

        // 3. Clear layout cache
        *state_arc.layout_cache.write().unwrap() = None;
        Ok(())
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))??;

    // 4. 唤醒 Coordinator 重做被退回 pending 的 exotic 缩略图。
    state.wake_exotic(crate::exotic::coordinator::WakeReason::ConfigChanged);

    Ok(())
}
