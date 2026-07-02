// src-tauri/src/ipc/scan_commands.rs
//! Tauri IPC commands for scan management (§ 6.1 — scan management).
//! 用于扫描管理的 Tauri IPC 命令（§ 6.1 — 扫描管理）。

use std::sync::Arc;

use serde::Serialize;

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::info;

use crate::db::models::{NewVolume, ScanRoot};
use crate::db::queries as q;
use crate::error::{AppError, Result};
use crate::scanner::fast_scan::ScanChannelPayload;
use crate::scanner::volume_probe::{PlatformVolumeResolver, VolumeResolver};
use crate::scanner::{run_enrichment, run_fast_scan};
use crate::state::AppState;
use crate::utils::path::normalize_root_path;

/// Add a new scan root directory.
/// 添加新的扫描根目录。
#[tauri::command]
pub async fn add_scan_root(
    app: AppHandle,
    path: String,
    alias: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<ScanRoot> {
    // 8A：用 root 专用规范化（保留 UNC `//server/share` 前缀），使网络盘/挂载盘可作扫描根（§3.8）。
    let norm = normalize_root_path(&path);

    // Grant asset-protocol read access to this root so its images load via convertFileSrc
    // (the static config no longer opens whole drives). See E1 in perf_hardening_plan_v2.md.
    // 为该根目录授予 asset 协议读取权限，使其图片可经 convertFileSrc 加载
    // （静态配置不再开放整盘）。见 perf_hardening_plan_v2.md 的 E1。
    if let Err(e) = app.asset_protocol_scope().allow_directory(&norm, true) {
        tracing::warn!(
            "Failed to allow scan root {} in asset scope | 扫描根授权失败: {}",
            norm,
            e
        );
    }

    // R1-3：查重（读池）→ 落库（写锁）→ 卷探测（原生 syscall）全是同步阻塞，整段下沉 blocking；
    // 读写混排 + 中途 early-return，不拆套 read/write_blocking 助手（见 blocking.rs 头注释豁免口）。
    let state_arc = Arc::clone(&*state);
    tokio::task::spawn_blocking(move || {
        // Check if the root already exists
        // 检查根目录是否已存在
        {
            let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
            let roots = q::list_scan_roots(&pool)?;
            if let Some(existing) = roots.into_iter().find(|r| r.path == norm) {
                info!(
                    "Scan root already exists: id={} path={} | 扫描根目录已存在: id={} path={}",
                    existing.id, norm, existing.id, norm
                );
                return Ok(existing);
            }
        }

        let conn = state_arc
            .db_writer
            .lock()
            .map_err(|e| AppError::System(e.to_string()))?;
        // backend_id=None：本地/OS 挂载盘（网络盘绑定经 Part5 设置 UI 走 set_scan_root_backend）。
        let id = q::insert_scan_root(&conn, &norm, alias.as_deref(), None)?;

        // 为新根解析并登记卷，绑定 volume_id（C5 Piece2）：缺失检测守门1 的「在线卷集」依赖它；
        // 未绑卷 → 该根媒体 volume_id 恒 NULL → 缺失检测对此根休眠（见 C5 Piece1）。
        // Windows 用原生卷 GUID（抗盘符重映射），失败/非 Windows 回退路径派生（C5 Piece A）。
        {
            let resolved = PlatformVolumeResolver.resolve(&norm);
            let new_vol = NewVolume {
                stable_id: resolved.stable_id,
                label: None,
                kind: resolved.kind,
                last_mount_path: Some(resolved.mount_path),
                last_seen: None,
                is_online: true, // 刚添加 = 在场
            };
            let volume_id = q::upsert_volume(&conn, &new_vol)?;
            q::set_scan_root_volume(&conn, id, Some(volume_id))?;
        }

        // 立即创建顶级目录记录，以便前端可以立刻加载和选中
        // Immediately create the top-level directory record so the frontend can load and select it immediately
        let dir_name = alias.clone().unwrap_or_else(|| {
            std::path::Path::new(&norm)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string()
        });
        q::upsert_directory(&conn, id, None, "", &dir_name, 0, None)?;

        let root = q::get_scan_root(&conn, id)?;
        info!("Scan root added: id={id} path={norm} | 已添加扫描根目录: id={id} path={norm}");
        Ok(root)
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

/// Remove a scan root and all its data (CASCADE).
/// 移除扫描根目录及其所有数据 (CASCADE)。
#[tauri::command]
pub async fn remove_scan_root(id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    info!(
        "User action: Removing scan root ID: {} | 用户操作：正在移除扫描根目录 ID: {}",
        id, id
    );
    state.cancel_scan(id);
    // R1-3：级联删除（可涉及大量行）走 write_blocking。
    super::blocking::write_blocking(&state, move |conn| q::delete_scan_root(conn, id)).await?;
    info!("Scan root removed: id={id} | 已移除扫描根目录: id={id}");
    Ok(())
}

/// Result of removing a scan root with options
/// 带选项删除扫描根的结果
#[derive(Serialize)]
pub struct RemoveRootResult {
    /// Number of thumbnail files scheduled for cleanup
    /// 计划清理的缩略图文件数
    pub cleared_count: usize,
}

/// Remove a scan root with options for thumbnail cleanup.
/// 带缩略图清理选项删除扫描根。
#[tauri::command]
pub async fn remove_scan_root_with_options(
    state: State<'_, Arc<AppState>>,
    id: i64,
    clear_thumbnails: bool,
) -> Result<RemoveRootResult> {
    // 1. Cancel any ongoing scan for this root
    //    取消该根的任何正在进行的扫描
    state.cancel_scan(id);

    // 2. If clear_thumbnails, collect cache_keys list BEFORE cascade delete
    //    如果 clear_thumbnails，在级联删除前收集 cache_key 列表
    // R1-3：读池查询走 read_blocking。
    let cache_keys: Vec<i64> = if clear_thumbnails {
        super::blocking::read_blocking(&state, move |conn| {
            let mut stmt = conn.prepare(
                "SELECT m.cache_key FROM media_items m \
                 JOIN directories d ON m.directory_id = d.id \
                 WHERE d.root_id = ?1 AND m.thumb_status = 1",
            )?;
            let keys: Vec<i64> = stmt
                .query_map([id], |row| row.get::<_, i64>(0))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(keys)
        })
        .await?
    } else {
        vec![]
    };

    let cleared_count = cache_keys.len();

    // 3. CASCADE delete DB records
    //    级联删除数据库记录（R1-3：走 write_blocking）
    super::blocking::write_blocking(&state, move |conn| q::delete_scan_root(conn, id)).await?;

    // 4. Async background delete thumbnail files
    //    异步后台删除缩略图文件
    if !cache_keys.is_empty() {
        let cache_dir = state.thumb_config.read().unwrap().cache_dir.clone();
        tokio::spawn(async move {
            let mut deleted = 0u32;
            for key in &cache_keys {
                for size in [120, 240, 480, 960] {
                    let full = crate::thumbnail::cache::thumb_path(&cache_dir, size, *key);
                    if tokio::fs::remove_file(&full).await.is_ok() {
                        deleted += 1;
                    }
                }
            }
            tracing::info!(
                "Cleaned {} thumbnails for {} items | 为 {} 个项目清理了 {} 个缩略图",
                deleted,
                cache_keys.len(),
                cache_keys.len(),
                deleted
            );
        });
    }

    Ok(RemoveRootResult { cleared_count })
}

/// Information about an overlapping scan root
/// 重叠扫描根的信息
#[derive(Serialize, Clone)]
pub struct OverlapInfo {
    pub id: i64,
    pub path: String,
    pub alias: Option<String>,
}

/// Result of folder overlap check
/// 文件夹重叠检查结果
#[derive(Serialize)]
pub struct FolderOverlapResult {
    /// Existing roots that are children of the new path
    /// 新路径包含的已有根（新路径是父级）
    pub children: Vec<OverlapInfo>,
    /// Existing roots that are parents of the new path
    /// 包含新路径的已有根（新路径是子级）
    pub parents: Vec<OverlapInfo>,
}

/// Check if a new folder path overlaps with existing scan roots.
/// 检查新文件夹路径是否与现有扫描根重叠。
#[tauri::command]
pub async fn check_folder_overlap(
    state: State<'_, Arc<AppState>>,
    new_path: String,
) -> Result<FolderOverlapResult> {
    // Normalize path separators to forward slashes for comparison
    // 标准化路径分隔符为正斜杠以便比较
    let normalized = new_path.replace('\\', "/");
    let normalized_with_sep = format!("{}/", normalized.trim_end_matches('/'));

    // R1-3：读池查询走 read_blocking；后面的重叠判定是小数据纯 CPU，留在 async 侧。
    let roots: Vec<(i64, String, Option<String>)> =
        super::blocking::read_blocking(&state, |conn| {
            let mut stmt =
                conn.prepare("SELECT id, path, alias FROM scan_roots WHERE is_active = 1")?;
            let roots = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .filter_map(|r| r.ok())
                .collect();
            Ok(roots)
        })
        .await?;

    let mut children = vec![];
    let mut parents = vec![];

    for (id, path, alias) in &roots {
        let root_normalized = path.replace('\\', "/");
        let root_with_sep = format!("{}/", root_normalized.trim_end_matches('/'));

        if root_with_sep.starts_with(&normalized_with_sep) && root_normalized != normalized {
            // Existing root is a child of the new path
            // 已有根是新路径的子级（新路径是父级）
            children.push(OverlapInfo {
                id: *id,
                path: path.clone(),
                alias: alias.clone(),
            });
        } else if normalized_with_sep.starts_with(&root_with_sep) && root_normalized != normalized {
            // Existing root is a parent of the new path
            // 已有根是新路径的父级（新路径是子级）
            parents.push(OverlapInfo {
                id: *id,
                path: path.clone(),
                alias: alias.clone(),
            });
        }
    }

    Ok(FolderOverlapResult { children, parents })
}

/// List all scan roots.
/// 列出所有扫描根目录。
#[tauri::command]
pub async fn list_scan_roots(state: State<'_, Arc<AppState>>) -> Result<Vec<ScanRoot>> {
    // R1-3：读池查询走 read_blocking。
    super::blocking::read_blocking(&state, q::list_scan_roots).await
}

/// Start a scan for a root (both fast scan + background enrichment).
/// 启动根目录扫描（包括快速扫描和后台内容丰富）。
///
/// This command returns when the fast scan completes (UI ready).
/// 此命令在快速扫描完成时返回（UI 准备就绪）。
/// Background enrichment continues and emits Tauri events.
/// 后台内容丰富继续进行并发出 Tauri 事件。
// Tauri 命令参数与前端 invoke 字段一一对应，塞进 struct 会改 IPC 形状（且需前端同步改）；
// 故沿用本仓库既有约定（如 ai_commands.rs）直接 allow。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_scan(
    root_id: i64,
    on_progress: Channel<ScanChannelPayload>,
    group_by: Option<String>,
    sort_within_group: Option<String>,
    sort_order: Option<String>,
    // T17b：opt-in「快速扫描」。None/false → 全量逐文件（默认）；true → mtime 未变目录跳过 per-file
    // 工作（仅漏就地编辑，全量扫描兜底）。前端增量重扫时传 true 提速。
    quick: Option<bool>,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    info!(
        "User action: Starting scan for root ID: {} | 用户操作：开始扫描根目录 ID: {}",
        root_id, root_id
    );
    // Cancel any existing scan for this root
    // 取消该根目录任何现有的扫描
    state.cancel_scan(root_id);
    let cancel = state.new_scan_token(root_id);

    // View order for first-screen prioritisation (defaults match the UI defaults).
    // 用于首屏优先级排序的视图顺序（默认值与 UI 默认一致）。
    let group_by = group_by.unwrap_or_else(|| "date".to_string());
    let sort_within_group = sort_within_group.unwrap_or_else(|| "datetime".to_string());
    let sort_order = sort_order.unwrap_or_else(|| "desc".to_string());
    let quick = quick.unwrap_or(false);

    // Get root path
    // 获取根目录路径（R1-3：读池查询走 read_blocking）
    let root_path = super::blocking::read_blocking(&state, move |conn| {
        Ok(q::get_scan_root(conn, root_id)?.path)
    })
    .await?;

    info!("start_scan: root_id={root_id} path={root_path} | 开始扫描: root_id={root_id} path={root_path}");

    // Clone the Arc so the closure owns an independent reference (no unsafe needed)
    // 克隆 Arc 以便闭包拥有独立的引用（不需要 unsafe）
    let state_arc = Arc::clone(&*state);
    let cancel_fast = cancel.clone();
    let root_path_clone = root_path.clone();
    // 取当前 exotic Catalog 快照，移入扫描闭包（walker 据此 common-first 分类、seed 任务）。
    let catalog_snap = state_arc.exotic_catalog.snapshot();
    // Same view order drives the background enrichment fill-in order.
    // 同一视图顺序也驱动后台 enrichment 的补全顺序。
    let (group_by_e, sort_within_e, sort_order_e) = (
        group_by.clone(),
        sort_within_group.clone(),
        sort_order.clone(),
    );

    // Run fast scan (spawn_blocking so we don't block the async runtime)
    // 运行快速扫描（使用 spawn_blocking，因此我们不会阻塞异步运行时）
    let fast_scan_result = tokio::task::spawn_blocking(move || {
        // T12：fast_scan 改流式入库、视图序让渡布局层，故不再需要 group_by/sort_* 入参；
        // 同名参数仍单独驱动后台 enrichment 的补全顺序（见上方 `*_e` 克隆）。
        run_fast_scan(
            &state_arc.db_writer,
            root_id,
            &root_path_clone,
            &catalog_snap,
            &on_progress,
            &cancel_fast,
            quick,
        )
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?;

    if let Err(e) = fast_scan_result {
        // 快速扫描阶段还没进入 enrichment 的统一收尾区，必须在这里清理 token；
        // 否则 AI 会误以为扫描仍在运行并永久让步。
        state.cancel_scan(root_id);
        return Err(e);
    }

    // 扫描事务已提交并 seed 了 exotic 任务（Part1 §1.5）→ 唤醒 Coordinator（幂等，每次扫描一次，
    // 非逐文件）。让步机制保证 exotic 在扫描/缩略图期间不抢占（Part2 §4.1）。
    state.wake_exotic(crate::exotic::coordinator::WakeReason::ScanCommitted);

    // Spawn background enrichment (fire-and-forget, emits events)
    // After enrichment completes, remove the scan token so the AI pipeline
    // can start without waiting forever (was the root cause of infinite yielding).
    // 生成后台内容丰富任务（触发后不管，发出事件）
    // Enrichment 完成后移除 scan token，AI pipeline 才能正常启动（否则会无限让步）。
    {
        let state_arc2 = Arc::clone(&*state);
        let app_clone = app.clone();
        let cancel_enrich = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let enrichment_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_enrichment(
                    &app_clone,
                    &state_arc2.db_writer,
                    root_id,
                    &group_by_e,
                    &sort_within_e,
                    &sort_order_e,
                    &cancel_enrich,
                )
            }));

            match enrichment_result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    // 取消属正常终态（用户主动停止），不算失败 → 仍发「正常完成」让 UI 停转圈；
                    // 真错误才带 error_code，前端据此弹 warning（区分「取消」与「失败」）。
                    let is_cancel = matches!(e, AppError::Cancelled);
                    if is_cancel {
                        tracing::info!(
                            "Enrichment cancelled for root_id={root_id} | 增量补全已取消"
                        );
                    } else {
                        tracing::error!("Enrichment error for root_id={root_id}: {e}");
                    }
                    // On cancel/error, run_enrichment doesn't emit its completion event.
                    // Emit a terminal signal anyway so the frontend progress UI stops.
                    // 取消/出错时 run_enrichment 不会发出完成事件，这里补发终止信号，
                    // 以便前端进度 UI 能够停止。
                    let payload = if is_cancel {
                        crate::scanner::enricher::EnrichmentCompletedPayload::ok(root_id, 0)
                    } else {
                        crate::scanner::enricher::EnrichmentCompletedPayload::failed(
                            root_id,
                            "enrich_failed",
                        )
                    };
                    let _ = app_clone.emit("enrichment:completed", payload);
                }
                Err(_) => {
                    tracing::error!("Enrichment panicked for root_id={root_id} | 增量补全崩溃");
                    let _ = app_clone.emit(
                        "enrichment:completed",
                        crate::scanner::enricher::EnrichmentCompletedPayload::failed(
                            root_id,
                            "enrich_panicked",
                        ),
                    );
                }
            }
            // Remove the token for this root — work is done (or was cancelled).
            // This prevents should_yield_to_higher_priority() from returning true forever.
            // 移除此根目录的 token — 工作已完成（或已取消）。
            // 这可以防止 should_yield_to_higher_priority() 永远返回 true。
            state_arc2.scan_tokens.lock().unwrap().remove(&root_id);
            tracing::info!(
                "Scan token cleared for root_id={root_id} | 已清除扫描 token: root_id={root_id}"
            );
        });
    }

    Ok(())
}

/// Stop (cancel) an in-progress scan.
/// 停止（取消）正在进行的扫描。
#[tauri::command]
pub async fn stop_scan(root_id: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    info!(
        "User action: Stopping scan for root ID: {} | 用户操作：停止扫描根目录 ID: {}",
        root_id, root_id
    );
    state.cancel_scan(root_id);
    info!("stop_scan: root_id={root_id} | 停止扫描: root_id={root_id}");
    Ok(())
}

#[tauri::command]
pub async fn clear_database(state: State<'_, Arc<AppState>>, app: AppHandle) -> Result<()> {
    info!("User action: Clearing database | 用户操作：正在清除数据库");
    // Cancel all running scans first
    // 首先取消所有正在运行的扫描
    state.cancel_all_scans();

    let cache_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::System(e.to_string()))?
        .join("cache")
        .join("thumbnails");

    // R1-3：全表 DELETE + VACUUM（可达秒级）+ 缩略图目录递归删除（可达数万文件）全是重阻塞，
    // 整段下沉 blocking；事务需要 `&mut Connection`，write_blocking 只给 `&`，故手写 spawn_blocking。
    let state_arc = Arc::clone(&*state);
    tokio::task::spawn_blocking(move || -> Result<()> {
        // Wipe all DB tables
        // 擦除所有数据库表
        {
            let mut conn = state_arc
                .db_writer
                .lock()
                .map_err(|e| AppError::System(e.to_string()))?;
            let tx = conn.transaction()?;
            tx.execute_batch(
                "DELETE FROM image_meta;
                 DELETE FROM media_items;
                 DELETE FROM directories;
                 DELETE FROM scan_roots;",
            )?;
            tx.commit()?;

            // VACUUM must be run outside of a transaction
            // VACUUM 必须在事务外运行
            conn.execute("VACUUM", [])?;
        }

        // Drop the thumbnail cache directory
        // 删除缩略图缓存目录
        if cache_dir.exists() {
            std::fs::remove_dir_all(&cache_dir).map_err(AppError::Io)?;
        }

        // Reset the layout cache in memory
        // 重置内存中的布局缓存
        *state_arc.layout_cache.write().unwrap() = None;
        Ok(())
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))??;

    info!("clear_database: all media data wiped | 清除数据库：所有媒体数据已擦除");
    Ok(())
}

#[tauri::command]
pub async fn clear_settings(state: State<'_, Arc<AppState>>) -> Result<()> {
    info!("User action: Clearing settings | 用户操作：正在清除设置");
    // R1-3：写走 write_blocking。
    super::blocking::write_blocking(&state, |conn| {
        // ONLY delete user settings, preserve system keys like schema_version
        // 仅删除用户设置，保留如 schema_version 等系统键值，避免重启时重复执行数据库迁移导致崩溃
        conn.execute("DELETE FROM app_config WHERE key != 'schema_version'", [])?;
        Ok(())
    })
    .await?;
    info!("clear_settings: settings wiped | 清除设置：设置项已擦除");
    Ok(())
}
