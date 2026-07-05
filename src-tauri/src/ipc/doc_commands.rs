// src-tauri/src/ipc/doc_commands.rs
//! Document IPC commands (P4, §3.4/§3.5). 文档相关 IPC 命令（P4）。
//!
//! 首批：文档缩略图的前端离屏渲染回环（§3.4 Lite 路径）。
//!  - `list_pending_doc_thumbs`：前端领取待渲染的 pdf/svg 文档。
//!  - `store_doc_thumbnail`：前端回传渲染好的 PNG 字节 → 落盘缩略图缓存 + 回填 media_items/布局缓存。
//!
//! 后续阶段（5.x）的替换/版本/校对/阅读进度命令也归入本模块。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use std::path::{Path, PathBuf};

use serde::Deserialize;
use similar::{ChangeTag, TextDiff};
use xxhash_rust::xxh3::xxh3_64;

use super::blocking::{read_blocking, write_blocking};
use crate::db::models::{DiffOp, DocumentVersion, PendingDocThumb, ReplacementRule, ThumbResult};
use crate::db::queries as q;
use crate::error::{AppError, Result};
use crate::scanner::enricher::MediaEnrichedPayload;
use crate::state::AppState;
use crate::thumbnail::generator::{encode_media_step, snap_to_tier, ThumbConfig};
use crate::utils::path::resolve_media_path;

/// Frontend payload for creating/updating a replacement rule (§5.2). `id=None` → insert.
/// 前端创建/更新替换规则的载荷（§5.2）。`id=None` → 插入。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacementInput {
    pub id: Option<i64>,
    pub scope_kind: String,
    pub scope_id: Option<i64>,
    pub find: String,
    pub replace: String,
    pub is_regex: bool,
    pub enabled: bool,
    pub sort_order: i64,
}

/// List replacement rules for a scope (rule editor). `scope_id=None` → global rules (§5.2).
/// 列出某作用域的替换规则（规则编辑器）。`scope_id=None` → 全局规则（§5.2）。
#[tauri::command]
pub async fn list_replacements(
    scope_kind: String,
    scope_id: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<ReplacementRule>, String> {
    read_blocking(&state, move |c| {
        q::list_replacements(c, &scope_kind, scope_id)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Effective rules to apply for an item = enabled global + item-scoped, ordered (§5.2).
/// 对某项实际生效的规则 = 启用的 global + item 作用域，按序（§5.2）。
#[tauri::command]
pub async fn get_effective_replacements(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<ReplacementRule>, String> {
    read_blocking(&state, move |c| q::get_effective_replacements(c, item_id))
        .await
        .map_err(|e| e.to_string())
}

/// Insert/update a replacement rule (§5.2). Returns the row id.
/// 插入/更新一条替换规则（§5.2）。返回行 id。
#[tauri::command]
pub async fn upsert_replacement(
    rule: ReplacementInput,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<i64, String> {
    write_blocking(&state, move |c| {
        q::upsert_replacement(
            c,
            rule.id,
            &rule.scope_kind,
            rule.scope_id,
            &rule.find,
            &rule.replace,
            rule.is_regex,
            rule.enabled,
            rule.sort_order,
        )
    })
    .await
    .map_err(|e| e.to_string())
}

/// Delete a replacement rule by id (§5.2).
/// 按 id 删除替换规则（§5.2）。
#[tauri::command]
pub async fn delete_replacement(
    id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking(&state, move |c| q::delete_replacement(c, id))
        .await
        .map_err(|e| e.to_string())
}

// ── Document versions (§5.3) ──────────────────────────────────────────────────

/// App-data versions dir for an item: `<appData>/documents/<item_id>/`.
/// 某项的版本存储目录：`<appData>/documents/<item_id>/`。
fn documents_dir(state: &AppState, item_id: i64) -> PathBuf {
    let app_data = state.log_dir.parent().unwrap_or(&state.log_dir);
    app_data.join("documents").join(item_id.to_string())
}

/// Read the text of a version ref: `None` = source baseline; `Some(id)` = a stored version.
/// 读取某版本引用的文本：`None` = 源文件基线；`Some(id)` = 已存版本。
fn read_ref_text(state: &AppState, item_id: i64, r: Option<i64>) -> Result<String> {
    match r {
        None => {
            let pool = state.db_read_pool.get()?;
            let (root, rel, name) = q::get_item_path_info(&pool, item_id)?;
            std::fs::read_to_string(resolve_media_path(&root, &rel, &name)).map_err(AppError::from)
        }
        Some(vid) => {
            let pool = state.db_read_pool.get()?;
            let v = q::get_version(&pool, vid)?.ok_or_else(|| {
                AppError::Internal(format!("version {vid} not found | 版本不存在"))
            })?;
            std::fs::read_to_string(&v.abs_path).map_err(AppError::from)
        }
    }
}

/// List all versions of a document, oldest first (§5.3).
/// 列出文档的所有版本，最旧在前（§5.3）。
#[tauri::command]
pub async fn list_versions(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<DocumentVersion>, String> {
    read_blocking(&state, move |c| q::list_versions(c, item_id))
        .await
        .map_err(|e| e.to_string())
}

/// Current version of a document, if one is marked (§5.3). `None` → source is current.
/// 文档当前版本（若有，§5.3）。`None` → 以源文件为当前。
#[tauri::command]
pub async fn get_current_version(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Option<DocumentVersion>, String> {
    read_blocking(&state, move |c| q::get_current_version(c, item_id))
        .await
        .map_err(|e| e.to_string())
}

/// Effective document text = current version's content if set, else the source file (§5.3).
/// Used by the viewer/editor so a "set current" version is what's read/edited.
/// 文档生效文本 = 已设当前版本的内容，否则源文件（§5.3）。供查看器/编辑器读取与编辑。
#[tauri::command]
pub async fn get_document_text(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<String, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> Result<String> {
        let cur = {
            let pool = state.db_read_pool.get()?;
            q::get_current_version(&pool, item_id)?
        };
        read_ref_text(&state, item_id, cur.map(|v| v.id))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Read a specific version's text content (§5.3).
/// 读取某版本的文本内容（§5.3）。
#[tauri::command]
pub async fn get_version_content(
    version_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<String, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> Result<String> {
        read_ref_text(&state, 0, Some(version_id))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Save edited text. `target`:
///   - `"version"` (default) → new snapshot in appData (does NOT enter the gallery, §5.3 D2).
///   - `"overwrite"` → overwrite the SOURCE file, after auto-backing-up the old source as a version
///     (advanced; the frontend gates this behind a confirm). Returns the new/backup version id.
/// 保存编辑后的文本。`target`：`"version"`（默认，appData 新快照，不进画廊）；
/// `"overwrite"`（覆盖源文件，先把旧源自动备份为一个版本；高级，前端二次确认）。返回版本 id。
#[tauri::command]
pub async fn save_version(
    item_id: i64,
    content: String,
    label: Option<String>,
    parent_id: Option<i64>,
    target: String,
    // 版本来源：'user'（默认）| 'ai-remote' | 'ai-local'（§5.4 AI 校对接受后存为新版本）。
    source: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<i64, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> Result<i64> {
        let (root, rel, name) = {
            let pool = state.db_read_pool.get()?;
            q::get_item_path_info(&pool, item_id)?
        };
        let src_path = resolve_media_path(&root, &rel, &name);
        let ext = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("txt")
            .to_string();
        let dir = documents_dir(&state, item_id);
        std::fs::create_dir_all(&dir).map_err(AppError::from)?;

        // Two-step (insert → write file at id-derived path → set path).
        let write_version =
            |label: Option<&str>, parent: Option<i64>, src: &str, text: &str| -> Result<i64> {
                let hash = format!("{:016x}", xxh3_64(text.as_bytes()));
                let id = {
                    let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
                    q::insert_version(
                        &conn,
                        item_id,
                        parent,
                        label,
                        "appdata",
                        "",
                        src,
                        Some(&hash),
                    )?
                };
                let path = dir.join(format!("{id}.{ext}"));
                std::fs::write(&path, text.as_bytes()).map_err(AppError::from)?;
                let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
                q::update_version_path(&conn, id, &path.to_string_lossy())?;
                Ok(id)
            };

        if target == "overwrite" {
            // Back up the existing SOURCE content first (always 'user'), then overwrite the source file.
            let src_text = std::fs::read_to_string(&src_path).unwrap_or_default();
            let backup_id = write_version(
                Some("覆盖前自动备份 | auto-backup"),
                None,
                "user",
                &src_text,
            )?;
            std::fs::write(&src_path, content.as_bytes()).map_err(AppError::from)?;
            Ok(backup_id)
        } else {
            write_version(
                label.as_deref(),
                parent_id,
                source.as_deref().unwrap_or("user"),
                &content,
            )
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Mark a version as current (or pass `None` to revert to the source baseline) (§5.3).
/// 将某版本设为当前（传 `None` 回到源文件基线）（§5.3）。
#[tauri::command]
pub async fn set_current_version(
    item_id: i64,
    version_id: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking(&state, move |c| {
        q::set_current_version(c, item_id, version_id)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Delete a version (row + file) (§5.3).
/// 删除一个版本（行 + 文件）（§5.3）。
#[tauri::command]
pub async fn delete_version(
    version_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    // DB 删行 + 文件删除同段下沉（文件删除也是阻塞 IO）。
    write_blocking(&state, move |c| {
        let path = q::delete_version(c, version_id)?;
        if let Some(p) = path {
            let _ = std::fs::remove_file(p);
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
}

/// Line-level diff between two version refs (`None` = source baseline) (§5.3).
/// 两个版本引用间的行级 diff（`None` = 源文件基线）（§5.3）。
#[tauri::command]
pub async fn diff_versions(
    item_id: i64,
    a: Option<i64>,
    b: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<DiffOp>, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> Result<Vec<DiffOp>> {
        let ta = read_ref_text(&state, item_id, a)?;
        let tb = read_ref_text(&state, item_id, b)?;
        let diff = TextDiff::from_lines(&ta, &tb);
        let ops = diff
            .iter_all_changes()
            .map(|c| {
                let tag = match c.tag() {
                    ChangeTag::Equal => "equal",
                    ChangeTag::Insert => "insert",
                    ChangeTag::Delete => "delete",
                };
                DiffOp {
                    tag: tag.to_string(),
                    value: c.value().trim_end_matches('\n').to_string(),
                }
            })
            .collect();
        Ok(ops)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Line-level diff between two arbitrary texts (§5.4 track-changes preview): original vs the
/// AI-corrected text, before the user accepts. Pure compute (no DB / files).
/// 两段任意文本的行级 diff（§5.4 track-changes 预览）：原文 vs AI 修订文，接受前预览。纯计算。
#[tauri::command]
pub async fn diff_texts(a: String, b: String) -> std::result::Result<Vec<DiffOp>, String> {
    tokio::task::spawn_blocking(move || {
        TextDiff::from_lines(&a, &b)
            .iter_all_changes()
            .map(|c| {
                let tag = match c.tag() {
                    ChangeTag::Equal => "equal",
                    ChangeTag::Insert => "insert",
                    ChangeTag::Delete => "delete",
                };
                DiffOp {
                    tag: tag.to_string(),
                    value: c.value().trim_end_matches('\n').to_string(),
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Get a document's saved reading position (§5.1). Returns `None` if never opened.
/// 获取文档已保存的阅读位置（§5.1）。从未打开过则返回 `None`。
#[tauri::command]
pub async fn get_reading_progress(
    item_id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Option<String>, String> {
    read_blocking(&state, move |c| q::get_reading_progress(c, item_id))
        .await
        .map_err(|e| e.to_string())
}

/// Persist a document's reading position (§5.1). Called debounced by the viewer.
/// 持久化文档阅读位置（§5.1）。由查看器去抖调用。
#[tauri::command]
pub async fn set_reading_progress(
    item_id: i64,
    position: String,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    write_blocking(&state, move |c| {
        q::set_reading_progress(c, item_id, &position)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Ensure `doc_thumb` rows exist for all pdf/svg documents (INSERT OR IGNORE backfill).
/// pdf/svg thumbnails are **frontend-driven** and must work without the user starting the
/// backend derivation pipeline, so the always-mounted renderer calls this once per pass to
/// seed the queue itself. Idempotent + cheap (indexed scan of the small `document` subset).
/// epub is intentionally left to the backend pipeline (same path as video covers).
/// 确保所有 pdf/svg 文档都有 `doc_thumb` 行（INSERT OR IGNORE 入队）。pdf/svg 缩略图是
/// 「前端驱动」，需在用户未启动后端派生流水线时也能工作，故常驻渲染器每轮自行调用本命令播种队列。
/// 幂等且廉价（仅扫描很小的 document 子集）。epub 有意留给后端流水线（与视频封面同路径）。
#[tauri::command]
pub async fn ensure_doc_thumb_queue(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<usize, String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> Result<usize> {
        let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        q::backfill_derivations(&conn, "doc_thumb", "document", Some(&["pdf", "svg"]))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// List pdf/svg documents awaiting a frontend-rendered thumbnail (§3.4). `limit` caps the
/// batch the renderer pulls per pass (default 8) so the main window stays responsive.
/// 列出等待前端渲染缩略图的 pdf/svg 文档（§3.4）。`limit` 限制渲染器每轮领取的批量（默认 8），
/// 以保持主窗口响应。
#[tauri::command]
pub async fn list_pending_doc_thumbs(
    limit: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<PendingDocThumb>, String> {
    let rows = read_blocking(&state, move |c| {
        q::list_pending_doc_thumbs(c, limit.unwrap_or(8))
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(item_id, abs_path, file_format)| PendingDocThumb {
            item_id,
            abs_path,
            file_format,
        })
        .collect())
}

/// Receive a frontend-rendered document thumbnail (PNG bytes) and persist it like any cover:
/// decode → reuse the thumbnail encoder (resize → WebP → write to cache by `cache_key`) →
/// mark the `doc_thumb` derivation done → mirror `thumb_status/path/hash` onto `media_items`
/// and the resident layout cache → nudge the gallery to refresh (invariant §1.3.4).
/// **Empty `png_bytes` = render failed** → mark the derivation error (status=3) so it is not
/// retried forever, and set `media_items.thumb_status=2` (no thumb).
///
/// 接收前端渲染好的文档缩略图（PNG 字节）并像封面一样持久化：解码 → 复用缩略图编码器
/// （缩放 → WebP → 按 `cache_key` 写缓存）→ 标记 `doc_thumb` 派生完成 → 回填
/// `thumb_status/path/hash` 到 `media_items` 与常驻布局缓存 → 通知画廊刷新（不变量 §1.3.4）。
/// **空字节 = 渲染失败** → 标记派生错误（status=3）避免无限重试，并置 `thumb_status=2`（无缩略图）。
#[tauri::command]
pub async fn store_doc_thumbnail(
    app: AppHandle,
    item_id: i64,
    png_bytes: Vec<u8>,
    // T10(§3.8.2):pdf 渲染时前端顺带取 pdf.js numPages 传入;svg 无页概念传 null。
    page_count: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    let state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> Result<()> {
        // ── 失败分支：空字节 → 标错，停止重试 ───────────────────────────────
        if png_bytes.is_empty() {
            let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            q::batch_finish_derivations(
                &conn,
                &[(
                    item_id,
                    "doc_thumb".to_string(),
                    3,
                    None,
                    Some("frontend render failed | 前端渲染失败".to_string()),
                    None,
                    None, // 元组第 7 项被写入器忽略;页数经 upsert_document_meta 直写(仅成功分支)
                )],
            )?;
            q::update_thumb_result(&conn, item_id, 2, None, None)?;
            return Ok(());
        }

        // ── 成功分支：解码 PNG → 编码为缩略图（复用图像编码器/缓存键）────────────
        let cache_key = {
            let pool = state.db_read_pool.get()?;
            q::get_item_cache_key(&pool, item_id)?.ok_or(AppError::MediaNotFound(item_id))?
        };
        let (cache_dir, thumb_size) = {
            let cfg = state.thumb_config.read().unwrap();
            (cfg.cache_dir.clone(), cfg.size)
        };

        let dynimg = image::load_from_memory(&png_bytes).map_err(|e| {
            AppError::Internal(format!("doc thumb decode failed | 文档缩略图解码失败: {e}"))
        })?;
        let rgba = dynimg.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let decoded = crate::engine::traits::DecodedImage {
            pixels: rgba.into_raw(),
            width: w,
            height: h,
        };

        let cfg = ThumbConfig {
            cache_dir,
            size: snap_to_tier(thumb_size),
            skip_max_bytes: 0,
            strategy: String::new(),
            gpu_engine: String::new(),
            ai_hq_cache: false, // 文档缩略图（前端驱动栅格化）非 CLIP 分析对象，不产 AI 缓存
        };
        let res = encode_media_step(item_id, cache_key, decoded, &cfg)?;

        // ── 持久化：派生完成 + 回填 media_items（同一写锁）────────────────────
        {
            let conn = state.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            q::batch_finish_derivations(
                &conn,
                &[(
                    item_id,
                    "doc_thumb".to_string(),
                    2,
                    res.thumb_path.clone(),
                    None,
                    None,
                    None, // 元组第 7 项被写入器忽略;页数走下方 upsert_document_meta 直写
                )],
            )?;
            // T10 页数回填(§3.8.2):pdf 传 pdf.js numPages,svg 传 None(仍写 doc_subtype
            // 激活该行)。doc_subtype 取 DB file_format 权威值,不信前端回传。
            let subtype = q::get_item_file_format(&conn, item_id)?;
            q::upsert_document_meta(&conn, item_id, page_count, subtype.as_deref())?;
            q::update_thumb_result(
                &conn,
                item_id,
                1,
                res.thumb_path.as_deref(),
                res.thumbhash.as_deref(),
            )?;
        }

        // ── 同步常驻双缓存（layout + S1 items，O(1) 按 id），使封面滚出再滚回无需整表重算 ──
        state.apply_thumb_results(&[ThumbResult {
            item_id,
            thumb_status: 1,
            thumb_path: res.thumb_path.clone(),
            thumbhash: res.thumbhash.clone(),
        }]);

        // 复用 enrichment 事件触发画廊防抖刷新（payload 被忽略）。
        let _ = app.emit(
            "db:media_enriched",
            MediaEnrichedPayload {
                root_id: 0,
                enriched_count: 0,
                total: 0,
            },
        );
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
