// src-tauri/src/ipc/media_commands.rs
//! Tauri IPC commands for media item operations (§ 6.1 — media queries).
//! 用于媒体项操作的 Tauri IPC 命令（§ 6.1 — 媒体查询）。

use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use tauri::State;

use crate::db::models::{AppStats, DirFile, DirNode, MediaDetail, MediaItem, SelectionDescriptor};
use crate::db::queries as q;
use crate::error::{AppError, Result};
use crate::scanner::metadata::read_image_dimensions;
use crate::state::AppState;
use crate::utils::path::resolve_media_path;

/// Extract real pixel dimensions on demand for the given items that are still
/// 0×0 placeholders (typically the just-scrolled-to viewport). Header-only read
/// (+ JPEG orientation), in parallel; updates the DB. Returns how many were
/// measured. The frontend recomputes the layout afterwards so the squares snap
/// to their correct aspect — ahead of the sequential background enrichment.
/// 按需为给定的、仍是 0×0 占位的项（通常是刚滚动到的可视窗口）提取真实像素尺寸。
/// 仅读文件头（+ JPEG 方向），并行执行并更新数据库；返回成功测量的数量。前端随后
/// 重算布局，使方块贴回正确比例 —— 抢在自上而下的后台 enrichment 之前。
#[tauri::command]
pub async fn prioritize_dimensions(
    item_ids: Vec<i64>,
    state: State<'_, Arc<AppState>>,
) -> Result<usize> {
    if item_ids.is_empty() {
        return Ok(0);
    }

    // R1-3：DB 读 + rayon 并行文件头读取（par_iter 会阻塞当前线程）+ 批量写，整段离开 tokio worker。
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<usize> {
        // Resolve paths for the placeholder items only (skip already-measured ones).
        // 仅解析占位项的路径（跳过已测量的项）。
        let targets: Vec<(i64, String, String)> = {
            let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
            item_ids
                .iter()
                .filter_map(|&id| {
                    q::get_placeholder_item_path(&pool, id)
                        .ok()
                        .flatten()
                        .map(|(path, ext)| (id, path, ext))
                })
                .collect()
        };
        if targets.is_empty() {
            return Ok(0);
        }

        // Read real dimensions in parallel (header read + JPEG orientation).
        // 并行读取真实尺寸（文件头 + JPEG 方向）。
        let results: Vec<(i64, i64, i64)> = targets
            .par_iter()
            .filter_map(|(id, path, ext)| {
                let (w, h) = read_image_dimensions(Path::new(path), ext);
                if w > 0 && h > 0 {
                    Some((*id, w, h))
                } else {
                    None
                }
            })
            .collect();

        let n = results.len();
        if n > 0 {
            let conn = state_arc
                .db_writer
                .lock()
                .map_err(|e| AppError::System(e.to_string()))?;
            let tx = conn.unchecked_transaction()?;
            for (id, w, h) in &results {
                q::update_media_dimensions(&tx, *id, *w, *h)?;
            }
            tx.commit()?;
        }
        Ok(n)
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

// R1-3 助手已提升为全 ipc/ 共享模块（本文件首建，随全量清扫上移）。
use super::blocking::{read_blocking, write_blocking};

/// Get full detail for a single media item.
/// 获取单个媒体项的完整详细信息。
#[tauri::command]
pub async fn get_media_detail(id: i64, state: State<'_, Arc<AppState>>) -> Result<MediaDetail> {
    read_blocking(&state, move |c| q::get_media_detail(c, id)).await
}

/// Fetch heavy per-item metadata (file name, dir path, EXIF, GPS) for the visible
/// viewport only — stripped from the resident layout cache for million-item memory.
///
/// 仅为可视区批量获取重型逐项元数据（文件名、目录路径、EXIF、GPS）——
/// 这些字段已从常驻布局缓存剥离，以支撑百万项内存目标。
#[tauri::command]
pub async fn get_meta_for_viewport(
    ids: Vec<i64>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::db::models::MediaMeta>> {
    read_blocking(&state, move |c| q::get_media_meta_batch(c, &ids)).await
}

/// Get the adjacent media item detail.
/// 获取相邻的媒体项详细信息。
#[tauri::command]
pub async fn get_adjacent_media(
    current_id: i64,
    offset: isize,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<MediaDetail>> {
    let adj_id = crate::layout::cache::get_adjacent_item(&state.layout_cache, current_id, offset);
    if let Some(id) = adj_id {
        let detail = get_media_detail(id, state).await?;
        Ok(Some(detail))
    } else {
        Ok(None)
    }
}

/// Get the playable video URL for a Live Photo companion.
/// 获取实况照片（Live Photo）关联文件的可播放视频 URL。
/// Returns the absolute file path (caller wraps with convertFileSrc).
/// 返回绝对文件路径（调用者使用 convertFileSrc 进行包装）。
#[tauri::command]
pub async fn get_companion_video_url(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<String> {
    // R1-3：DB 读 + 全文件读取提取嵌入 MP4（可达数十 MB）整段离开 tokio worker。
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<String> {
        let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;

        // 卷离线守门（T13 §3.7）：所在卷离线 → 直接返 VolumeOffline（携卷标签），
        // 前端弹「请插入设备 <label>」而非任由后续路径解析失败给破图/泛化错误。
        if let Some(label) = q::get_item_volume_offline_label(&pool, item_id)? {
            return Err(AppError::VolumeOffline(label));
        }

        // Check if the item has a companion MOV (Apple Live Photo)
        // 检查项目是否有配套的 MOV 文件（Apple 实况照片）
        let companion_id = q::get_companion_item_id(&pool, item_id);

        if let Ok(Some(comp_id)) = companion_id {
            let (root, rel, name) = q::get_item_path_info(&pool, comp_id)?;
            return Ok(resolve_media_path(&root, &rel, &name));
        }

        // Check for embedded video (Google/Samsung Motion Photo)
        // 检查是否有嵌入式视频（Google/Samsung 动态照片）
        let conn = state_arc
            .db_writer
            .lock()
            .map_err(|e| AppError::System(e.to_string()))?;
        let (has_embedded, cache_key): (bool, i64) = conn.query_row(
            "SELECT has_embedded_video, cache_key FROM media_items WHERE id=?1",
            rusqlite::params![item_id],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get(1)?)),
        )?;
        drop(conn);

        if has_embedded {
            let (root, rel, name) = q::get_item_path_info(&pool, item_id)?;
            let abs_path = resolve_media_path(&root, &rel, &name);

            // Check motion video cache
            // 检查动态视频缓存
            let cache_path = {
                let config = state_arc.thumb_config.read().unwrap();
                crate::thumbnail::cache::motion_video_cache_path(&config.cache_dir, cache_key)
            };

            if cache_path.exists() {
                return Ok(cache_path.to_string_lossy().replace('\\', "/"));
            }

            // Extract from JPEG (read trailing bytes)
            // 从 JPEG 中提取（读取尾部字节）
            let video_bytes = extract_embedded_mp4(&abs_path)?;
            if let Some(parent) = cache_path.parent() {
                std::fs::create_dir_all(parent).map_err(AppError::from)?;
            }
            std::fs::write(&cache_path, &video_bytes).map_err(AppError::from)?;
            return Ok(cache_path.to_string_lossy().replace('\\', "/"));
        }

        Err(AppError::MediaNotFound(item_id))
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

/// Get the keyframe sprite URL for a video, if generated (§3.3). Returns the absolute path
/// (caller wraps with `convertFileSrc`) or `None` if no sprite exists yet. Backs the hover
/// scrub fallback for oversized / network videos (§3.1).
/// 获取视频的关键帧雪碧图 URL（若已生成，§3.3）。返回绝对路径（调用者用 `convertFileSrc` 包装），
/// 尚无雪碧图则返回 `None`。支撑超大 / 网络盘视频的悬停 scrub 降级（§3.1）。
#[tauri::command]
pub async fn get_keyframe_sprite(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<String>> {
    let Some(rel) =
        read_blocking(&state, move |c| q::get_keyframe_sprite_payload(c, item_id)).await?
    else {
        return Ok(None);
    };
    let cache_dir = { state.thumb_config.read().unwrap().cache_dir.clone() };
    let abs = cache_dir.join(&rel);
    Ok(Some(abs.to_string_lossy().replace('\\', "/")))
}

/// Toggle the favorite status of a media item.
/// 切换媒体项的收藏状态。
#[tauri::command]
pub async fn toggle_favorite(item_id: i64, state: State<'_, Arc<AppState>>) -> Result<bool> {
    // R1-3：写锁等待与执行都在 blocking 线程；布局缓存同步回到 async 侧（锁已释放）。
    let new_val = write_blocking(&state, move |c| q::toggle_favorite(c, item_id)).await?;
    // Keep the resident layout cache consistent so the star doesn't revert on
    // scroll-out/scroll-in (D3).
    // 同步常驻布局缓存，避免滚出再滚回时收藏标记回退（D3）。
    crate::layout::cache::set_favorite_in_cache(&state.layout_cache, &[item_id], new_val);
    Ok(new_val)
}

/// R1-2（S4 收尾）：解析 `SelectionDescriptor`（读池）→ 在同一 blocking 线程上执行写闭包。
///
/// - 批量命令的入参从 `Vec<i64>` 迁为描述符：全选百万项时 IPC payload 只含视图描述 + 排除集，
///   与选区大小无关（T18 D4）；id 物化收敛到后端 SQL 层。
/// - resolve 与写全程在 `spawn_blocking` 内（CLAUDE.md 硬化：async command 内 rusqlite 调用
///   一律离开 tokio worker；模式同 `compute_layout`）。
/// - 空选区由写闭包侧自然短路（queries 批量助手对空集返回 0 / Ok）。
async fn resolve_then_write<T, F>(
    state: &State<'_, Arc<AppState>>,
    selection: SelectionDescriptor,
    write: F,
) -> Result<(Vec<i64>, T)>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection, &[i64]) -> Result<T> + Send + 'static,
{
    let version = current_layout_version(state);
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<(Vec<i64>, T)> {
        // 读连接仅在解析期间持有，写前释放（不占读池名额跨越写锁等待）。
        let ids = {
            let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
            q::resolve_selection(&pool, &selection, version)?
        };
        let conn = state_arc
            .db_writer
            .lock()
            .map_err(|e| AppError::System(e.to_string()))?;
        let out = write(&conn, &ids)?;
        Ok((ids, out))
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

/// Batch set favorite status for the resolved selection.
/// 批量设置选区收藏状态（R1-2/S4：入参迁 SelectionDescriptor，全选不整包传 id）。
#[tauri::command]
pub async fn batch_toggle_favorite(
    state: State<'_, Arc<AppState>>,
    selection: SelectionDescriptor,
    value: bool,
) -> Result<u64> {
    let (ids, affected) = resolve_then_write(&state, selection, move |conn, ids| {
        q::batch_set_favorite(conn, ids, value)
    })
    .await?;

    // Sync the resident layout cache so favorites survive scroll-out/scroll-in (D3).
    // 同步常驻布局缓存，使收藏在滚出再滚回后仍保持（D3）。
    crate::layout::cache::set_favorite_in_cache(&state.layout_cache, &ids, value);

    // S4 验收证据：payload 与选区大小无关——解析出的实际规模只在此后端日志可见。
    tracing::info!(
        "Batch favorite(S4): resolved {} ids, affected {}, value {} | 批量收藏：解析 {} 项，影响 {} 行，值 {}",
        ids.len(),
        affected,
        value,
        ids.len(),
        affected,
        value
    );

    Ok(affected)
}

/// Set the rating for a media item (0-5).
/// 设置媒体项的评分（0-5）。
#[tauri::command]
pub async fn set_rating(item_id: i64, rating: i64, state: State<'_, Arc<AppState>>) -> Result<()> {
    // R1-3：写走 write_blocking（断言测试补抓的漏网点）。
    write_blocking(&state, move |conn| {
        q::set_rating(conn, item_id, rating.clamp(0, 5))
    })
    .await
}

/// Batch-set rating (0-5) for many items in a single UPDATE + IN. Mirrors `batch_toggle_favorite`;
/// backs the gallery's keyboard 1-5 quick-rating over the current selection (avoids N round-trips
/// that a per-item loop would incur on large selections). Returns the affected row count.
/// 批量设置评分（0-5），单条 UPDATE + IN 完成。镜像 `batch_toggle_favorite`,支撑画廊键盘 1-5 对
/// 当前选区快捷评分（避免逐项 loop 在大选区上的 N 次 IPC 往返）。返回受影响行数。
/// 注:rating 不在布局缓存(布局行不携带该列)，故与 set_favorite 不同，此处无需同步布局缓存。
#[tauri::command]
pub async fn batch_set_rating(
    state: State<'_, Arc<AppState>>,
    selection: SelectionDescriptor,
    rating: i64,
) -> Result<u64> {
    let (ids, affected) = resolve_then_write(&state, selection, move |conn, ids| {
        q::batch_set_rating(conn, ids, rating) // 0-5 钳制在 db 层
    })
    .await?;

    tracing::info!(
        "Batch rating(S4): resolved {} ids, affected {}, rating {} | 批量评分：解析 {} 项，影响 {} 行，评分 {}",
        ids.len(),
        affected,
        rating.clamp(0, 5),
        ids.len(),
        affected,
        rating.clamp(0, 5)
    );

    Ok(affected)
}

/// Set the color label for a media item (0=none, 1-7).
/// 设置媒体项的颜色标签（0=无，1-7 色档）。
#[tauri::command]
pub async fn set_color_label(
    item_id: i64,
    color_label: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    // R1-3：写走 write_blocking（断言测试补抓的漏网点）。
    write_blocking(&state, move |conn| {
        q::set_color_label(conn, item_id, color_label.clamp(0, 7))
    })
    .await
}

/// Batch-set color label (0-7) for many items in a single UPDATE + IN. Mirrors `batch_set_rating`;
/// backs the gallery's batch color-tagging over the current selection. Returns the affected row count.
/// 批量设置颜色标签（0-7），单条 UPDATE + IN 完成。镜像 `batch_set_rating`，支撑画廊对当前选区批量
/// 打色签（避免逐项 loop 在大选区上的 N 次 IPC）。返回受影响行数。
#[tauri::command]
pub async fn batch_set_color_label(
    state: State<'_, Arc<AppState>>,
    selection: SelectionDescriptor,
    color_label: i64,
) -> Result<u64> {
    let (ids, affected) = resolve_then_write(&state, selection, move |conn, ids| {
        q::batch_set_color_label(conn, ids, color_label) // 0-7 钳制在 db 层
    })
    .await?;

    tracing::info!(
        "Batch color label(S4): resolved {} ids, affected {} | 批量色签：解析 {} 项，影响 {} 行",
        ids.len(),
        affected,
        ids.len(),
        affected
    );

    Ok(affected)
}

/// Soft-delete the resolved selection (mark is_deleted=1; Live Photo companion 连带在 db 层)。
/// 软删除选区（R1-2/S4：入参迁 SelectionDescriptor）。
#[tauri::command]
pub async fn soft_delete_items(
    selection: SelectionDescriptor,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    let (ids, ()) = resolve_then_write(&state, selection, |conn, ids| {
        q::soft_delete_items(conn, ids)
    })
    .await?;
    tracing::info!(
        "Soft delete(S4): resolved {} ids | 软删除：解析 {} 项",
        ids.len(),
        ids.len()
    );
    Ok(())
}

/// Restore soft-deleted items.
/// 恢复软删除的项目（R1-2/S4：入参迁 SelectionDescriptor；撤销路径传 Explicit）。
#[tauri::command]
pub async fn restore_items(
    selection: SelectionDescriptor,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    let (ids, ()) = resolve_then_write(&state, selection, q::restore_items).await?;
    tracing::info!(
        "Restore(S4): resolved {} ids | 恢复：解析 {} 项",
        ids.len(),
        ids.len()
    );
    Ok(())
}

/// 读当前布局版本（无布局 → 0）。SelectAll 解析以此对 `view.layout_version` 守门;
/// Explicit 解析忽略此值，故无布局时传 0 安全（不会误判 Explicit）。
fn current_layout_version(state: &AppState) -> u64 {
    crate::layout::cache::get_summary(&state.layout_cache)
        .map(|s| s.layout_version)
        .unwrap_or(0)
}

/// 把 `SelectionDescriptor` 解析为实际 id 列表（按视图布局序）。Part5 S4：暴露为 IPC（纯新增）。
///
/// - `Explicit{ids}`：上限校验后原样返回。
/// - `SelectAll{view, excludedIds}`：`view.layoutVersion` 与当前布局不一致 → `ViewStale`；
///   否则经 `view_to_sql` 取全集 − 排除集。百万级全选在后端 SQL 解析，不经前端整包传 id。
///
/// R1-2（T4c）已落地：批量命令（favorite/rating/color/soft_delete/restore）直接收
/// SelectionDescriptor 在后端解析；本命令保留为通用「描述符 → id 列表」入口（前端仅在
/// 确需 id 的操作——移动/复制/加收藏夹等——使用物化路径）。
#[tauri::command]
pub async fn resolve_selection(
    selection: SelectionDescriptor,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<i64>> {
    let version = current_layout_version(&state);
    read_blocking(&state, move |c| {
        q::resolve_selection(c, &selection, version)
    })
    .await
}

/// 仅计数 `SelectionDescriptor`（UI「将操作 N 项」）。Part5 S4：暴露为 IPC（纯新增）。
/// SelectAll 走 `COUNT(*) − (excluded ∩ view)`，不取全 id（精确计数，T18 D3）。
#[tauri::command]
pub async fn count_selection(
    selection: SelectionDescriptor,
    state: State<'_, Arc<AppState>>,
) -> Result<u64> {
    let version = current_layout_version(&state);
    read_blocking(&state, move |c| q::count_selection(c, &selection, version)).await
}

/// Get items in the trash (paginated).
/// 获取垃圾桶中的项目（分页）。
#[tauri::command]
pub async fn get_trash(
    offset: i64,
    limit: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<MediaItem>> {
    read_blocking(&state, move |c| q::get_trash(c, offset, limit.min(200))).await
}

/// Get overall app statistics.
/// 获取整体应用统计信息。
#[tauri::command]
pub async fn get_stats(state: State<'_, Arc<AppState>>) -> Result<AppStats> {
    read_blocking(&state, q::get_app_stats).await
}

/// Get the full directory tree for a scan root.
/// 获取扫描根目录的完整目录树。
#[tauri::command]
pub async fn get_directory_tree(
    root_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<DirNode>> {
    read_blocking(&state, move |c| q::get_directory_tree(c, root_id)).await
}

/// Get direct children of a directory node (lazy loading).
/// 获取目录节点的直接子节点（延迟加载）。
#[tauri::command]
pub async fn get_directory_children(
    parent_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<DirNode>> {
    read_blocking(&state, move |c| q::get_directory_children(c, parent_id)).await
}

#[tauri::command]
pub async fn get_directory_ancestors(id: i64, state: State<'_, Arc<AppState>>) -> Result<Vec<i64>> {
    read_blocking(&state, move |c| q::get_directory_ancestors(c, id)).await
}

/// List the direct media files of a directory for the sidebar tree's file list
/// (lazy-loaded when a folder is expanded).
/// 列出某目录的直接媒体文件，供侧边栏树的文件列表使用（展开文件夹时懒加载）。
#[tauri::command]
pub async fn list_directory_files(
    directory_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<DirFile>> {
    read_blocking(&state, move |c| q::list_directory_files(c, directory_id)).await
}
// ── Helpers ───────────────────────────────────────────────────────────────────
// ── 助手函数 ───────────────────────────────────────────────────────────────────

/// Extract embedded MP4 from a Google/Samsung Motion Photo JPEG.
/// 从 Google/Samsung 动态照片 JPEG 中提取嵌入的 MP4。
fn extract_embedded_mp4(abs_path: &str) -> Result<Vec<u8>> {
    let data = std::fs::read(abs_path).map_err(AppError::from)?;
    let ftyp_marker = b"ftyp";
    for i in (4..data.len().saturating_sub(4)).rev() {
        if &data[i..i + 4] == ftyp_marker {
            let mp4_start = i - 4;
            if mp4_start + 8 < data.len() {
                return Ok(data[mp4_start..].to_vec());
            }
        }
    }
    Err(AppError::Internal(
        "No embedded MP4 found in Motion Photo".into(),
    ))
}
