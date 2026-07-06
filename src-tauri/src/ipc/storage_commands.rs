// src-tauri/src/ipc/storage_commands.rs
//! Storage-backend IPC (network drives, P5 8B, §3.8). 存储后端 IPC（网络盘，P5 8B，§3.8）。
//!
//! CRUD + connectivity test for storage backends. Passwords live in the OS keyring (account
//! `storage_backend_<id>`), never in the DB — only a `cred_ref` is persisted (mirrors the
//! proofread-key pattern). `test_backend` builds the backend and lists its base dir; under Lite
//! (no `netfs`) a WebDAV test returns a clear "needs perf variant" message (degrade to 8A).
//!
//! 存储后端的 CRUD + 连通性测试。密码存系统 keyring（账户 `storage_backend_<id>`），绝不入 DB ——
//! 仅持久化 `cred_ref`（与校对 key 同模式）。`test_backend` 构建后端并列其 base 目录；轻量版
//! （无 `netfs`）测试 WebDAV 返回清晰的「需性能版」提示（降级到 8A）。

use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use crate::db::models::StorageBackendInfo;
use crate::db::queries as q;
use crate::error::AppError;
use crate::state::AppState;
use crate::storage::{build_backend, BackendConfig};

use scrollery_plugin_api::KEYRING_SERVICE;

fn cred_account(id: i64) -> String {
    format!("storage_backend_{id}")
}

fn keyring_entry(account: &str) -> std::result::Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())
}

/// Connection params from the add/test form. Password is in-memory only (→ keyring on save).
/// 来自添加/测试表单的连接参数。密码仅在内存（保存时 → keyring）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendInput {
    pub kind: String, // 'local' | 'smb' | 'webdav'
    pub name: Option<String>,
    pub host: Option<String>,
    pub base_path: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl BackendInput {
    fn to_config(&self) -> BackendConfig {
        BackendConfig {
            kind: self.kind.clone(),
            host: self.host.clone(),
            base_path: self.base_path.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
        }
    }
}

/// List all configured storage backends (§3.8). Passwords are never returned.
/// 列出所有已配置的存储后端（§3.8）。密码绝不返回。
#[tauri::command]
pub async fn list_backends(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<StorageBackendInfo>, String> {
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(
        move || -> std::result::Result<Vec<StorageBackendInfo>, String> {
            let pool = s.db_read_pool.get().map_err(|e| e.to_string())?;
            q::list_storage_backends(&pool).map_err(|e| e.to_string())
        },
    )
    .await
    .map_err(|e| e.to_string())?
}

/// Test connectivity/credentials for a backend BEFORE saving (§3.8). Returns the number of
/// entries listed at the base path on success. Runs in `spawn_blocking` (the WebDAV backend
/// block_on's internally and must not nest inside the async runtime).
/// 保存前测试后端的连通性/凭据（§3.8）。成功时返回 base 路径下的项数。在 `spawn_blocking` 运行
/// （WebDAV 后端内部 block_on，不能套在异步运行时内）。
#[tauri::command]
pub async fn test_backend(input: BackendInput) -> std::result::Result<usize, String> {
    tokio::task::spawn_blocking(move || -> std::result::Result<usize, String> {
        let backend = build_backend(&input.to_config()).map_err(|e: AppError| e.to_string())?;
        let entries = backend.list_dir("").map_err(|e| e.to_string())?;
        Ok(entries.len())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Add a storage backend (§3.8): persist the row, then store the password in the keyring under
/// `storage_backend_<id>` and record that account as `cred_ref`. Returns the saved row.
/// 添加存储后端（§3.8）：持久化行，再把密码存入 keyring（账户 `storage_backend_<id>`）并把该账户
/// 记为 `cred_ref`。返回已保存的行。
#[tauri::command]
pub async fn add_backend(
    input: BackendInput,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<StorageBackendInfo, String> {
    let name = input
        .name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| input.host.clone())
        .unwrap_or_else(|| input.kind.clone());

    // R1-3：DB 写 + keyring 存密（同步系统调用）+ 回读，整段离开 tokio worker。
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> std::result::Result<StorageBackendInfo, String> {
        let id = {
            let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            q::insert_storage_backend(
                &conn,
                &input.kind,
                &name,
                input.host.as_deref(),
                input.base_path.as_deref(),
                input.username.as_deref(),
                None, // cred_ref filled in below once we know the id
                None,
            )
            .map_err(|e| e.to_string())?
        };

        // Store the password in the keyring and link it via cred_ref (never in the DB).
        // 把密码存入 keyring 并经 cred_ref 关联（绝不入 DB）。
        if let Some(pw) = input.password.as_deref().filter(|p| !p.is_empty()) {
            let account = cred_account(id);
            keyring_entry(&account)?
                .set_password(pw)
                .map_err(|e| e.to_string())?;
            let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            conn.execute(
                "UPDATE storage_backends SET cred_ref = ?1 WHERE id = ?2",
                rusqlite::params![account, id],
            )
            .map_err(|e| e.to_string())?;
        }

        let backends = {
            let pool = s.db_read_pool.get().map_err(|e| e.to_string())?;
            q::list_storage_backends(&pool).map_err(|e| e.to_string())?
        };
        backends
            .into_iter()
            .find(|b| b.id == id)
            .ok_or_else(|| "backend not found after insert | 插入后未找到后端".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Remove a storage backend (§3.8): delete the row and purge its keyring credential.
/// 移除存储后端（§3.8）：删除行并清理其 keyring 凭据。
#[tauri::command]
pub async fn remove_backend(
    id: i64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    // R1-3：DB 写 + keyring 清理整段离开 tokio worker。
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> std::result::Result<(), String> {
        let cred_ref = {
            let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
            q::delete_storage_backend(&conn, id).map_err(|e| e.to_string())?
        };
        if let Some(account) = cred_ref {
            // Purge the credential; NoEntry is fine (idempotent).
            // 清理凭据；NoEntry 视为成功（幂等）。
            if let Ok(entry) = keyring_entry(&account) {
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
