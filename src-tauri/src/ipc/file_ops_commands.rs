// src-tauri/src/ipc/file_ops_commands.rs
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, State};
use tracing::info;

use crate::db::queries as q;
use crate::error::{AppError, Result};
use crate::ipc::scan_commands::add_scan_root;
use crate::state::AppState;
use crate::utils::hash::{cache_key_to_hex, compute_cache_key};
use crate::utils::path::{path_depth, resolve_media_path};

/// Valid thumbnail size tiers (see thumbnail/cache.rs). Used when relocating cache files.
/// 有效缩略图档位（见 thumbnail/cache.rs）。重定位缓存文件时使用。
const THUMB_TIERS: [u32; 4] = [120, 240, 480, 960];

#[tauri::command]
pub async fn create_physical_folder(
    app: AppHandle,
    base_path: String,
    folder_name: String,
    state: State<'_, Arc<AppState>>,
) -> Result<String> {
    let target_path = if base_path.is_empty() {
        PathBuf::from(&folder_name)
    } else {
        PathBuf::from(&base_path).join(&folder_name)
    };

    let path_str = target_path.to_string_lossy().to_string();
    info!("create_physical_folder: path={}", path_str);

    tokio::fs::create_dir_all(&target_path)
        .await
        .map_err(|e| AppError::CreateFolder(e.to_string()))?;

    let norm = crate::utils::path::normalize_db_path(&path_str);

    // R1-3：读池查询走 read_blocking。
    let is_within_existing = super::blocking::read_blocking(&state, move |conn| {
        let roots = q::list_scan_roots(conn)?;
        let target_norm = format!("{norm}/");
        Ok(roots.into_iter().any(|r| {
            let r_norm = format!("{}/", r.path);
            target_norm.starts_with(&r_norm)
        }))
    })
    .await?;

    if !is_within_existing {
        // 自动纳入 Scan Roots
        add_scan_root(app.clone(), path_str.clone(), None, state.clone()).await?;
    }

    Ok(path_str)
}

#[tauri::command]
pub async fn move_media_items(
    media_ids: Vec<i64>,
    target_dir: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<i64>> {
    // R1-3：逐条「读路径(SQL) → rename(fs) → 删行(SQL)」串行交织，且尾部 trash::delete 是
    // 回收站 syscall，整段下沉一个 blocking 任务（tokio::fs 内部本就逐调用 spawn_blocking，
    // 合并后反而少跳线程）；顺序与失败语义不变（中途失败即返回，已移动项的删行已生效）。
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let mut moved_ids = vec![];
        let mut dirs_to_check = std::collections::HashSet::new();

        for id in media_ids {
            let src_path = {
                let pool = state.db_read_pool.get().map_err(AppError::from)?;
                let (root, rel, name) = q::get_item_path_info(&pool, id)?;
                resolve_media_path(&root, &rel, &name)
            };

            let src_file_name = PathBuf::from(&src_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            if src_file_name.is_empty() {
                continue;
            }

            let target_path = PathBuf::from(&target_dir).join(&src_file_name);

            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::MoveFile(e.to_string()))?;
            }

            std::fs::rename(&src_path, &target_path)
                .map_err(|e| AppError::MoveFile(e.to_string()))?;

            if let Some(parent) = PathBuf::from(&src_path).parent() {
                dirs_to_check.insert(parent.to_path_buf());
            }

            {
                let conn = state
                    .db_writer
                    .lock()
                    .map_err(|e| AppError::System(e.to_string()))?;
                conn.execute(
                    "DELETE FROM media_items WHERE id = ?1",
                    rusqlite::params![id],
                )
                .map_err(AppError::Db)?;
            }
            moved_ids.push(id);
        }

        // 如果源文件夹为空，则移入系统回收站
        for dir in dirs_to_check {
            if let Ok(mut entries) = std::fs::read_dir(&dir) {
                // 与原 tokio 版语义一致：仅首个 entry 成功读取才算非空（读取出错视作空）。
                let is_empty = !matches!(entries.next(), Some(Ok(_)));
                if is_empty {
                    if let Err(e) = trash::delete(&dir) {
                        tracing::warn!("Failed to move empty folder to trash: {}", e);
                    } else {
                        tracing::info!("Moved empty folder to trash: {:?}", dir);
                    }
                }
            }
        }

        Ok(moved_ids)
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

#[tauri::command]
pub async fn copy_media_items(
    media_ids: Vec<i64>,
    target_dir: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<i64>> {
    // R1-3：同 move_media_items——SQL 与 fs 复制（可达 GB 级）交织，整段下沉 blocking。
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let mut copied_ids = vec![];

        for id in media_ids {
            let src_path = {
                let pool = state.db_read_pool.get().map_err(AppError::from)?;
                let (root, rel, name) = q::get_item_path_info(&pool, id)?;
                resolve_media_path(&root, &rel, &name)
            };

            let src_file_name = PathBuf::from(&src_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            if src_file_name.is_empty() {
                continue;
            }

            let target_path = PathBuf::from(&target_dir).join(&src_file_name);

            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::CopyFile(e.to_string()))?;
            }

            std::fs::copy(&src_path, &target_path)
                .map_err(|e| AppError::CopyFile(e.to_string()))?;
            copied_ids.push(id);
        }

        Ok(copied_ids)
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

/// One requested relocation: move media item `id` into directory `target_dir_id`.
/// 一次重定位请求：将媒体项 `id` 移动到目录 `target_dir_id`。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRelocation {
    pub id: i64,
    pub target_dir_id: i64,
}

/// Result of a relocation — carries the ORIGINAL directory so the caller can build an
/// exact inverse for undo/redo.
/// 重定位结果 — 带上原目录，使调用方能构造精确的逆操作以供撤销/重做。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRelocationResult {
    pub id: i64,
    pub from_dir_id: i64,
    pub target_dir_id: i64,
}

/// Relocate media items into target directories by reassigning `directory_id` and moving
/// the file on disk. Unlike `move_media_items` (delete + re-ingest via rescan), this keeps
/// the SAME item id, so thumbnails and AI embeddings stay valid, and it is REVERSIBLE:
/// the returned `from_dir_id` lets the caller record an exact inverse for undo (问题5).
/// Drives both drag-to-folder and undo/redo by passing different target dirs.
/// 通过重设 `directory_id` 并移动磁盘文件来重定位媒体项。不同于 move_media_items（删行 +
/// 重扫重导入），本命令保留同一 item id，缩略图与 AI 嵌入向量仍有效，且可逆：返回的
/// from_dir_id 让调用方记录精确逆操作以撤销（问题5）。传入不同目标目录即可驱动「拖到文件夹」
/// 与撤销/重做。
#[tauri::command]
pub async fn relocate_media_items(
    moves: Vec<MediaRelocation>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<MediaRelocationResult>> {
    // R1-3：同 move_media_items——逐条 SQL 与 fs rename 交织，整段下沉 blocking。
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();

        for mv in moves {
            // Resolve current path + current directory, and the target directory's abs path.
            // 解析当前路径 + 当前目录，以及目标目录的绝对路径。
            let (cur_abs, from_dir_id, target_abs_dir) = {
                let pool = state.db_read_pool.get().map_err(AppError::from)?;
                let from_dir_id: i64 = pool
                    .query_row(
                        "SELECT directory_id FROM media_items WHERE id=?1",
                        rusqlite::params![mv.id],
                        |r| r.get(0),
                    )
                    .map_err(|_| AppError::MediaNotFound(mv.id))?;
                let (root, rel, name) = q::get_item_path_info(&pool, mv.id)?;
                let cur_abs = resolve_media_path(&root, &rel, &name);
                let target_abs_dir = q::get_directory_abs_path(&pool, mv.target_dir_id)?;
                (cur_abs, from_dir_id, target_abs_dir)
            };

            if mv.target_dir_id == from_dir_id {
                continue; // already there — no-op | 已在目标 — 跳过
            }

            let file_name = PathBuf::from(&cur_abs)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if file_name.is_empty() {
                continue;
            }

            let target_path = PathBuf::from(&target_abs_dir).join(&file_name);
            if target_path == cur_abs {
                continue;
            }
            if target_path.try_exists().unwrap_or(false) {
                return Err(AppError::MoveFile(format!(
                    "目标已存在同名文件: {}",
                    file_name
                )));
            }

            std::fs::create_dir_all(&target_abs_dir)
                .map_err(|e| AppError::MoveFile(e.to_string()))?;
            std::fs::rename(&cur_abs, &target_path)
                .map_err(|e| AppError::MoveFile(e.to_string()))?;

            {
                let conn = state
                    .db_writer
                    .lock()
                    .map_err(|e| AppError::System(e.to_string()))?;
                conn.execute(
                    "UPDATE media_items SET directory_id=?1, updated_at=strftime('%s','now') WHERE id=?2",
                    rusqlite::params![mv.target_dir_id, mv.id],
                )
                .map_err(AppError::Db)?;
            }

            results.push(MediaRelocationResult {
                id: mv.id,
                from_dir_id,
                target_dir_id: mv.target_dir_id,
            });
        }

        // Invalidate the layout cache so the next compute_layout reflects the moved items.
        // Directory media_count is computed live by the tree query, so no count maintenance.
        // 使布局缓存失效，下次 compute_layout 反映移动后的项。目录 media_count 由树查询实时计算，
        // 无需维护计数列。
        if !results.is_empty() {
            *state
                .layout_cache
                .write()
                .map_err(|e| AppError::System(e.to_string()))? = None;
        }

        Ok(results)
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

/// Result of a media copy — the new row id lets the caller record an exact inverse
/// (delete the copy) for undo/redo.
/// 媒体复制结果 — 新行 id 让调用方记录精确逆操作（删除副本）以撤销/重做。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCopyResult {
    pub src_id: i64,
    pub new_id: i64,
}

/// Copy media items INTO target directories (DB-aware): copy the file on disk and INSERT a
/// new media_items row duplicating the source (new directory_id, same cache_key so the
/// existing thumbnail is reused — no re-generation). Returns the new row ids so the caller
/// can undo precisely by deleting them. Same `moves` shape as relocate for symmetry (问题2).
/// 把媒体项复制到目标目录（DB 感知）：复制磁盘文件并 INSERT 一条复制自源的新 media_items 行
/// （新 directory_id、相同 cache_key 以复用现有缩略图，无需重新生成）。返回新行 id，使调用方
/// 通过删除它们精确撤销。与 relocate 相同的 moves 形状以对称（问题2）。
#[tauri::command]
pub async fn copy_media_items_db(
    moves: Vec<MediaRelocation>,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<MediaCopyResult>> {
    // R1-3：同 copy_media_items——逐条 SQL 与 fs copy（可达 GB 级）交织，整段下沉 blocking。
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();

        for mv in moves {
            let (cur_abs, target_abs_dir) = {
                let pool = state.db_read_pool.get().map_err(AppError::from)?;
                let (root, rel, name) = q::get_item_path_info(&pool, mv.id)?;
                (
                    resolve_media_path(&root, &rel, &name),
                    q::get_directory_abs_path(&pool, mv.target_dir_id)?,
                )
            };

            let file_name = PathBuf::from(&cur_abs)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if file_name.is_empty() {
                continue;
            }

            let dest = PathBuf::from(&target_abs_dir).join(&file_name);
            if dest == cur_abs {
                continue; // copy onto self — skip | 复制到自身 — 跳过
            }
            if dest.try_exists().unwrap_or(false) {
                return Err(AppError::CopyFile(format!(
                    "目标已存在同名文件: {}",
                    file_name
                )));
            }

            std::fs::create_dir_all(&target_abs_dir)
                .map_err(|e| AppError::CopyFile(e.to_string()))?;
            std::fs::copy(&cur_abs, &dest).map_err(|e| AppError::CopyFile(e.to_string()))?;

            let new_id = {
                let conn = state
                    .db_writer
                    .lock()
                    .map_err(|e| AppError::System(e.to_string()))?;
                q::duplicate_media_item_into_dir(&conn, mv.id, mv.target_dir_id)?
            };

            results.push(MediaCopyResult {
                src_id: mv.id,
                new_id,
            });
        }

        if !results.is_empty() {
            *state
                .layout_cache
                .write()
                .map_err(|e| AppError::System(e.to_string()))? = None;
        }

        Ok(results)
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

/// Hard-remove media items: move each file to the OS trash and delete its DB row. Used to
/// UNDO a drag-copy (问题2). Missing files / rows are tolerated. Invalidates the layout cache.
/// 硬删除媒体项：把每个文件移入系统回收站并删除其 DB 行。用于撤销拖拽复制（问题2）。
/// 容忍文件/行缺失。使布局缓存失效。
#[tauri::command]
pub async fn remove_media_items_hard(ids: Vec<i64>, state: State<'_, Arc<AppState>>) -> Result<()> {
    // R1-3：回收站 syscall + 逐条 SQL + 派生缓存文件删除全是阻塞，整段下沉 blocking。
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        // 缓存目录（删除后按 cache_key 清派生孤儿，§3.3.2 / Q7）。
        let cache_dir = state
            .thumb_config
            .read()
            .map_err(|e| AppError::System(e.to_string()))?
            .cache_dir
            .clone();
        for id in ids {
            // 删除前取路径 + cache_key（行还在；cache_key 决定其全部派生缓存路径）。
            let (abs, cache_key) = {
                let pool = state.db_read_pool.get().map_err(AppError::from)?;
                let abs = q::get_item_path_info(&pool, id)
                    .ok()
                    .map(|(root, rel, name)| resolve_media_path(&root, &rel, &name));
                let ck: Option<i64> = pool
                    .query_row(
                        "SELECT cache_key FROM media_items WHERE id=?1",
                        rusqlite::params![id],
                        |r| r.get(0),
                    )
                    .ok();
                (abs, ck)
            };
            if let Some(p) = abs {
                let pb = PathBuf::from(&p);
                if pb.exists() {
                    if let Err(e) = trash::delete(&pb) {
                        tracing::warn!("Failed to trash copied file {:?}: {}", pb, e);
                    }
                }
            }
            {
                let conn = state
                    .db_writer
                    .lock()
                    .map_err(|e| AppError::System(e.to_string()))?;
                conn.execute("DELETE FROM media_items WHERE id=?1", rusqlite::params![id])
                    .map_err(AppError::Db)?;
            }
            // 硬删后即时清理派生缓存孤儿（§3.3.2 / Q7）：按 cache_key 删 4 档缩略图 + ai_thumb +
            // sprite + motion。在 DB 锁之外、best-effort、失败不阻塞删除。软删（is_deleted）不走此路径
            // （保留缓存供恢复，与「离线≠删除」同理）。
            if let Some(ck) = cache_key {
                crate::thumbnail::cache::remove_cache_files_for_key(&cache_dir, ck);
            }
        }

        *state
            .layout_cache
            .write()
            .map_err(|e| AppError::System(e.to_string()))? = None;
        Ok(())
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

// ════════════════════════════════════════════════════════════════════════════
// Folder (directory) move / copy / undo-delete
// 文件夹（目录）移动 / 复制 / 撤销删除
// ════════════════════════════════════════════════════════════════════════════

/// Result of a folder move — enough for the frontend to relocate + record undo.
/// 文件夹移动结果 — 足以让前端定位并记录撤销。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveDirResult {
    pub dir_id: i64,
    pub root_id: i64,
    pub new_rel_path: String,
    pub affected_dirs: usize,
    pub affected_media: usize,
}

/// Result of a folder copy — identifies the newly-created subtree for undo.
/// 文件夹复制结果 — 标识新创建的子树以供撤销。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyDirResult {
    pub created_root_id: i64,
    pub created_rel_path: String,
    pub created_abs_path: String,
}

/// Build an absolute directory path from a scan-root path + a forward-slash rel_path.
/// 根据扫描根路径 + 正斜杠 rel_path 构建绝对目录路径。
fn abs_dir_path(root_path: &str, rel_path: &str) -> PathBuf {
    let mut pb = PathBuf::from(root_path);
    if !rel_path.is_empty() {
        pb.push(rel_path);
    }
    pb
}

/// Remap a (descendant) rel_path from an old prefix to a new prefix.
/// 将（后代）rel_path 从旧前缀重映射到新前缀。
fn remap_rel(rel: &str, old_prefix: &str, new_prefix: &str) -> String {
    if rel == old_prefix {
        new_prefix.to_string()
    } else if let Some(suffix) = rel.strip_prefix(&format!("{old_prefix}/")) {
        format!("{new_prefix}/{suffix}")
    } else {
        // Not actually inside the subtree — leave unchanged (defensive).
        // 实际上不在子树内 — 保持不变（防御性）。
        rel.to_string()
    }
}

/// Rebuild a stored thumb_path ("{size}/{prefix}/{hex}.webp") for a new cache_key,
/// preserving the size tier segment.
/// 为新的 cache_key 重建存储的 thumb_path（保留尺寸档位段）。
fn remap_thumb_path(old: &str, new_key: i64) -> String {
    let size = old.split('/').next().unwrap_or("");
    let hex = cache_key_to_hex(new_key);
    let prefix = &hex[..2];
    format!("{size}/{prefix}/{hex}.webp")
}

/// Best-effort: relocate cached thumbnail + motion-video files to match new cache_keys.
/// A failed rename is non-fatal — the asset will simply regenerate on demand.
/// 尽力而为：重定位已缓存的缩略图 + 动态视频文件以匹配新 cache_key。
/// 重命名失败不致命 — 资产会按需重新生成。
fn relocate_cache_files(cache_dir: &Path, renames: &[(i64, i64)]) {
    let thumbs = cache_dir.join("thumbnails");
    let motion = cache_dir.join("motion_videos");
    for &(old_key, new_key) in renames {
        let old_hex = cache_key_to_hex(old_key);
        let new_hex = cache_key_to_hex(new_key);
        // Thumbnails live in per-size buckets — try every tier.
        // 缩略图按尺寸分桶存储 — 逐档位尝试。
        for tier in THUMB_TIERS {
            let old_p = thumbs
                .join(tier.to_string())
                .join(&old_hex[..2])
                .join(format!("{old_hex}.webp"));
            if old_p.exists() {
                let new_p = thumbs
                    .join(tier.to_string())
                    .join(&new_hex[..2])
                    .join(format!("{new_hex}.webp"));
                if let Some(parent) = new_p.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::rename(&old_p, &new_p) {
                    tracing::warn!(
                        "thumb relocate failed {:?} -> {:?}: {} | 缩略图重定位失败",
                        old_p,
                        new_p,
                        e
                    );
                }
            }
        }
        // Motion video (Live Photo) cache.
        // 动态视频（实况照片）缓存。
        let old_mv = motion.join(&old_hex[..2]).join(format!("{old_hex}.mp4"));
        if old_mv.exists() {
            let new_mv = motion.join(&new_hex[..2]).join(format!("{new_hex}.mp4"));
            if let Some(parent) = new_mv.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::rename(&old_mv, &new_mv);
        }
    }
}

/// Recursively copy a directory tree (blocking; run inside spawn_blocking).
/// 递归复制目录树（阻塞；在 spawn_blocking 中运行）。
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst).map_err(|e| AppError::CopyFile(e.to_string()))?;
    for entry in std::fs::read_dir(src).map_err(|e| AppError::CopyFile(e.to_string()))? {
        let entry = entry.map_err(|e| AppError::CopyFile(e.to_string()))?;
        let ft = entry
            .file_type()
            .map_err(|e| AppError::CopyFile(e.to_string()))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| AppError::CopyFile(e.to_string()))?;
        }
    }
    Ok(())
}

/// Move a folder on disk; falls back to copy-tree + delete-source across volumes
/// (Windows rename fails with ERROR_NOT_SAME_DEVICE between drives).
/// 在磁盘上移动文件夹；跨卷时回退为「拷贝整树 + 删源」
/// （Windows 跨盘 rename 会因 ERROR_NOT_SAME_DEVICE 失败）。
async fn move_path_with_fallback(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::MoveFile(e.to_string()))?;
    }
    if tokio::fs::rename(src, dst).await.is_ok() {
        return Ok(());
    }
    // Cross-volume or other rename failure → copy then remove source.
    // 跨盘或其它 rename 失败 → 拷贝后删除源。
    let src_buf = src.to_path_buf();
    let dst_buf = dst.to_path_buf();
    tokio::task::spawn_blocking(move || copy_dir_recursive(&src_buf, &dst_buf))
        .await
        .map_err(|e| AppError::MoveFile(e.to_string()))??;
    tokio::fs::remove_dir_all(src)
        .await
        .map_err(|e| AppError::MoveFile(e.to_string()))?;
    Ok(())
}

/// Move a folder (and its whole subtree) into another folder, **preserving metadata**.
/// In-place DB update: rewrites rel_path/parent_id/depth/root_id of every descendant
/// directory, recomputes each affected media item's cache_key, and relocates cached
/// thumbnail files. Favorites / ratings / AI embeddings (keyed by item id) are kept.
///
/// 将文件夹（及其整个子树）移动到另一个文件夹，**保留元数据**。
/// 原地更新数据库：重写每个后代目录的 rel_path/parent_id/depth/root_id，
/// 重算每个受影响媒体项的 cache_key，并重定位已缓存的缩略图文件。
/// 收藏 / 评分 / AI 嵌入（按 item id 关联）均保留。
#[tauri::command]
pub async fn move_directory(
    source_dir_id: i64,
    target_dir_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<MoveDirResult> {
    // ── 1. Read source/target metadata ── 读取源/目标元数据 ──
    // R1-3：读池查询走 read_blocking（校验的 early-return 以 Err 形式穿出闭包，语义不变）。
    let (source, target, src_root_path, tgt_root_path) =
        super::blocking::read_blocking(&state, move |pool| {
            let source = q::get_directory(pool, source_dir_id)?;
            let target = q::get_directory(pool, target_dir_id)?;
            let src_root_path = q::get_scan_root(pool, source.root_id)?.path;
            let tgt_root_path = q::get_scan_root(pool, target.root_id)?.path;

            // ── 2. Validate ── 校验 ──
            if source_dir_id == target_dir_id {
                return Err(AppError::InvalidMove(
                    "不能移动到自身 | cannot move into itself".into(),
                ));
            }
            if source.parent_id.is_none() {
                return Err(AppError::InvalidMove(
                    "不能移动扫描根目录 | cannot move a scan root".into(),
                ));
            }
            if source.parent_id == Some(target_dir_id) {
                return Err(AppError::InvalidMove(
                    "已位于目标目录中 | already inside the target".into(),
                ));
            }
            // Target must not live inside the source subtree (would create a cycle).
            // 目标不能位于源子树内（会成环）。
            let ancestors = q::get_directory_ancestors(pool, target_dir_id)?;
            if ancestors.contains(&source_dir_id) {
                return Err(AppError::InvalidMove(
                    "不能移动到自身的子目录 | cannot move into own descendant".into(),
                ));
            }
            // Name collision under target — merge is not supported in v1.
            // 目标下同名冲突 — v1 暂不支持合并。
            if q::dir_has_child_named(pool, target_dir_id, &source.name)? {
                return Err(AppError::DirectoryExists(source.name.clone()));
            }
            Ok((source, target, src_root_path, tgt_root_path))
        })
        .await?;

    // Guard: refuse while a scan is running on either root (state would corrupt).
    // 守卫：任一根目录正在扫描时拒绝（否则状态会损坏）。
    {
        let tokens = state
            .scan_tokens
            .lock()
            .map_err(|e| AppError::System(e.to_string()))?;
        if tokens.contains_key(&source.root_id) || tokens.contains_key(&target.root_id) {
            return Err(AppError::InvalidMove(
                "扫描进行中，请稍后再试 | a scan is running, try again later".into(),
            ));
        }
    }

    // ── 3. Compute new prefixes + absolute paths ── 计算新前缀 + 绝对路径 ──
    let old_prefix = source.rel_path.clone();
    let new_prefix = if target.rel_path.is_empty() {
        source.name.clone()
    } else {
        format!("{}/{}", target.rel_path, source.name)
    };
    let src_abs = abs_dir_path(&src_root_path, &old_prefix);
    let dst_abs = abs_dir_path(&tgt_root_path, &new_prefix);
    let new_root_id = target.root_id;

    // ── 4. Physical move ── 物理移动 ──
    move_path_with_fallback(&src_abs, &dst_abs).await?;

    // ── 5. DB transaction: rewrite subtree + recompute cache_keys ──
    // ── 数据库事务：重写子树 + 重算 cache_key ──
    // R1-3：子树重写事务（行数与子树规模同阶）+ 第 6 步缓存文件重定位（逐媒体 fs rename）
    // 一并下沉 blocking；事务要 `&mut Connection`，故手写 spawn_blocking 而非 write_blocking。
    let state_arc = Arc::clone(&*state);
    let new_prefix_ret = new_prefix.clone();
    let (affected_dirs, affected_media) = tokio::task::spawn_blocking(move || -> Result<(usize, usize)> {
    let mut thumb_renames: Vec<(i64, i64)> = Vec::new();
    let (affected_dirs, affected_media) = {
        let mut conn = state_arc
            .db_writer
            .lock()
            .map_err(|e| AppError::System(e.to_string()))?;
        let tx = conn.transaction().map_err(AppError::from)?;

        let dirs = q::get_directory_subtree(&tx, source_dir_id)?;
        let mut new_rel_by_dir: HashMap<i64, String> = HashMap::with_capacity(dirs.len());
        for d in &dirs {
            let new_rel = remap_rel(&d.rel_path, &old_prefix, &new_prefix);
            let new_depth = path_depth(&new_rel);
            if d.id == source_dir_id {
                tx.execute(
                    "UPDATE directories SET rel_path=?1, depth=?2, root_id=?3, parent_id=?4 WHERE id=?5",
                    rusqlite::params![new_rel, new_depth, new_root_id, target_dir_id, d.id],
                ).map_err(AppError::Db)?;
            } else {
                tx.execute(
                    "UPDATE directories SET rel_path=?1, depth=?2, root_id=?3 WHERE id=?4",
                    rusqlite::params![new_rel, new_depth, new_root_id, d.id],
                )
                .map_err(AppError::Db)?;
            }
            new_rel_by_dir.insert(d.id, new_rel);
        }

        let media = q::get_media_in_subtree(&tx, source_dir_id)?;
        for m in &media {
            let dir_rel = new_rel_by_dir
                .get(&m.directory_id)
                .cloned()
                .unwrap_or_default();
            let new_key = compute_cache_key(&dir_rel, &m.file_name, m.file_mtime);
            if new_key == m.cache_key {
                continue;
            }
            // thumb_status == 3 (source-direct): thumb_path is the ABSOLUTE source path,
            // which moved with the folder → recompute it. Otherwise it's the relative
            // cache path "{size}/{prefix}/{hex}.webp" keyed by cache_key → remap the hex.
            // thumb_status == 3（直接使用源文件）：thumb_path 是绝对源路径，随文件夹一起移动
            // → 重新计算。否则它是按 cache_key 命名的相对缓存路径 → 重映射 hex。
            let new_thumb = match (m.thumb_status, m.thumb_path.as_ref()) {
                (3, Some(_)) => Some(resolve_media_path(&tgt_root_path, &dir_rel, &m.file_name)),
                (_, Some(p)) => Some(remap_thumb_path(p, new_key)),
                (_, None) => None,
            };
            tx.execute(
                "UPDATE media_items SET cache_key=?1, thumb_path=?2, updated_at=strftime('%s','now') WHERE id=?3",
                rusqlite::params![new_key, new_thumb, m.id],
            ).map_err(AppError::Db)?;
            thumb_renames.push((m.cache_key, new_key));
        }

        tx.commit().map_err(AppError::Db)?;
        (dirs.len(), media.len())
    };

    // ── 6. Relocate cache files (best-effort, outside the DB lock) ──
    // ── 重定位缓存文件（尽力而为，在数据库锁之外） ──
    let cache_dir = state_arc
        .thumb_config
        .read()
        .map_err(|e| AppError::System(e.to_string()))?
        .cache_dir
        .clone();
    relocate_cache_files(&cache_dir, &thumb_renames);

    Ok((affected_dirs, affected_media))
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))??;

    info!(
        "move_directory: dir={source_dir_id} -> parent={target_dir_id} ({affected_dirs} dirs, {affected_media} media) | 文件夹已移动"
    );

    Ok(MoveDirResult {
        dir_id: source_dir_id,
        root_id: new_root_id,
        new_rel_path: new_prefix_ret,
        affected_dirs,
        affected_media,
    })
}

/// Copy a folder (and its whole subtree) into another folder on disk. The new files
/// are ingested as fresh assets via a subsequent re-scan triggered by the frontend.
/// Returns the created subtree's location so an undo can remove exactly it.
///
/// 将文件夹（及其整个子树）复制到磁盘上的另一个文件夹。新文件经前端随后触发的
/// 重扫作为全新资产引入。返回新建子树的位置，以便撤销时精确移除。
#[tauri::command]
pub async fn copy_directory(
    source_dir_id: i64,
    target_dir_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<CopyDirResult> {
    // R1-3：读池查询走 read_blocking（校验 early-return 以 Err 穿出闭包）。
    let (source, target, src_root_path, tgt_root_path) =
        super::blocking::read_blocking(&state, move |pool| {
            let source = q::get_directory(pool, source_dir_id)?;
            let target = q::get_directory(pool, target_dir_id)?;
            let src_root_path = q::get_scan_root(pool, source.root_id)?.path;
            let tgt_root_path = q::get_scan_root(pool, target.root_id)?.path;

            if source.parent_id.is_none() {
                return Err(AppError::InvalidMove(
                    "不能复制扫描根目录 | cannot copy a scan root".into(),
                ));
            }
            if target_dir_id == source_dir_id {
                return Err(AppError::InvalidMove(
                    "不能复制到自身 | cannot copy into itself".into(),
                ));
            }
            let ancestors = q::get_directory_ancestors(pool, target_dir_id)?;
            if ancestors.contains(&source_dir_id) {
                return Err(AppError::InvalidMove(
                    "不能复制到自身的子目录 | cannot copy into own descendant".into(),
                ));
            }
            if q::dir_has_child_named(pool, target_dir_id, &source.name)? {
                return Err(AppError::DirectoryExists(source.name.clone()));
            }
            Ok((source, target, src_root_path, tgt_root_path))
        })
        .await?;

    let new_rel = if target.rel_path.is_empty() {
        source.name.clone()
    } else {
        format!("{}/{}", target.rel_path, source.name)
    };
    let src_abs = abs_dir_path(&src_root_path, &source.rel_path);
    let dst_abs = abs_dir_path(&tgt_root_path, &new_rel);

    {
        let src_buf = src_abs.clone();
        let dst_buf = dst_abs.clone();
        tokio::task::spawn_blocking(move || copy_dir_recursive(&src_buf, &dst_buf))
            .await
            .map_err(|e| AppError::CopyFile(e.to_string()))??;
    }

    info!("copy_directory: dir={source_dir_id} -> parent={target_dir_id} | 文件夹已复制");

    Ok(CopyDirResult {
        created_root_id: target.root_id,
        created_rel_path: new_rel,
        created_abs_path: dst_abs.to_string_lossy().replace('\\', "/"),
    })
}

/// Undo a folder copy: move the copied folder to the system trash and purge the
/// ingested DB rows (CASCADE). Cancels any running scan on the affected root first
/// to avoid a race with re-ingestion.
///
/// 撤销文件夹复制：将复制出的文件夹移入系统回收站，并清除已登记的数据库行
/// （CASCADE）。会先取消受影响根目录上正在运行的扫描，避免与重新引入竞争。
#[tauri::command]
pub async fn delete_directory_to_trash(
    abs_path: String,
    root_id: i64,
    rel_path: String,
    state: State<'_, Arc<AppState>>,
) -> Result<()> {
    // Stop any in-flight scan on this root so it cannot re-create the rows we delete.
    // 停止该根目录上进行中的扫描，避免它重新创建我们要删除的行。
    state.cancel_scan(root_id);

    // R1-3：回收站 syscall（整树入回收站，可达秒级）+ 级联删除 SQL 一并下沉 blocking。
    let state = Arc::clone(&state);
    let abs_path_c = abs_path.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        // 1. Physical: move folder to system trash (recoverable, safer than hard delete).
        // 1. 物理：将文件夹移入系统回收站（可恢复，比硬删除更安全）。
        let pb = PathBuf::from(&abs_path_c);
        if pb.exists() {
            trash::delete(&pb).map_err(|e| AppError::System(e.to_string()))?;
        }

        // 2. DB: delete the directory row → CASCADE removes descendants + their media.
        // 2. 数据库：删除目录行 → CASCADE 级联移除后代及其媒体。
        {
            let conn = state
                .db_writer
                .lock()
                .map_err(|e| AppError::System(e.to_string()))?;
            if let Some(dir_id) = q::find_directory_id(&conn, root_id, &rel_path)? {
                q::delete_directory_by_id(&conn, dir_id)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))??;

    info!("delete_directory_to_trash: {abs_path} | 已将复制的文件夹移入回收站并清理数据库行");
    Ok(())
}
