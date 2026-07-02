// src-tauri/src/ipc/proofread_commands.rs
//! IPC for remote AI proofreading (§5.4). 远程 AI 校对的 IPC（§5.4）。
//!
//! 配置（base_url / model）存 app_config；API key 存系统凭据库（keyring），不落明文 DB。
//! 校对按文本分块由前端逐块调用 `proofread_chunk`，结果以 track-changes 呈现、接受后存为新版本（接 §5.3）。

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::db::queries::{get_config, set_config};
use crate::proofread::{proofread_remote, ProofreadConfig};
use crate::state::AppState;

/// keyring 服务名 / 账户名 —— API key 的存放坐标。
const KEYRING_SERVICE: &str = "picasa-next";
const KEYRING_ACCOUNT: &str = "proofread_api_key";

fn keyring_entry() -> std::result::Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(|e| e.to_string())
}

/// Proofread config surfaced to the UI. The key itself is never returned — only whether it's set.
/// 暴露给 UI 的校对配置。key 本身绝不返回 —— 仅返回是否已设置。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofreadConfigDto {
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
}

/// Read proofread config (base_url / model / whether a key is stored) (§5.4).
/// 读取校对配置（base_url / model / 是否已存 key）（§5.4）。
#[tauri::command]
pub async fn get_proofread_config(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<ProofreadConfigDto, String> {
    // R1-3：DB 读 + keyring 探测（同步系统调用）一并离开 tokio worker。
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> std::result::Result<ProofreadConfigDto, String> {
        let pool = s.db_read_pool.get().map_err(|e| e.to_string())?;
        let base_url = get_config(&pool, "proofread_base_url")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let model = get_config(&pool, "proofread_model")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        // key 是否存在：能取到密码即视为已设置（NoEntry → 未设置）。
        let has_key = keyring_entry()
            .ok()
            .and_then(|e| e.get_password().ok())
            .is_some();
        Ok(ProofreadConfigDto {
            base_url,
            model,
            has_key,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Persist proofread endpoint config (base_url / model) (§5.4).
/// 持久化校对端点配置（base_url / model）（§5.4）。
#[tauri::command]
pub async fn set_proofread_config(
    base_url: String,
    model: String,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    let s = Arc::clone(&state);
    tokio::task::spawn_blocking(move || -> std::result::Result<(), String> {
        let conn = s.db_writer.lock().unwrap_or_else(|e| e.into_inner());
        set_config(&conn, "proofread_base_url", &base_url).map_err(|e| e.to_string())?;
        set_config(&conn, "proofread_model", &model).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Store the API key in the OS credential store (never in the DB) (§5.4).
/// 把 API key 存入系统凭据库（绝不入 DB）（§5.4）。
#[tauri::command]
pub async fn set_proofread_key(key: String) -> std::result::Result<(), String> {
    keyring_entry()?
        .set_password(&key)
        .map_err(|e| e.to_string())
}

/// Remove the stored API key (§5.4).
/// 删除已存的 API key（§5.4）。
#[tauri::command]
pub async fn clear_proofread_key() -> std::result::Result<(), String> {
    match keyring_entry()?.delete_credential() {
        Ok(()) => Ok(()),
        // 未设置时删除视为成功（幂等）。
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Proofread one text chunk via the configured remote LLM (§5.4). The frontend chunks the
/// document and calls this per chunk, then renders a track-changes diff before accepting.
/// 经配置的远程 LLM 校对一段文本（§5.4）。前端分块后逐块调用，再以 track-changes 呈现差异供接受。
#[tauri::command]
pub async fn proofread_chunk(
    text: String,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<String, String> {
    // R1-3：DB 读 + keyring 取 key 离开 tokio worker（其后的远程调用本就是 async IO）。
    let s = Arc::clone(&state);
    let (base_url, model, key) = tokio::task::spawn_blocking(
        move || -> std::result::Result<(String, String, String), String> {
            let pool = s.db_read_pool.get().map_err(|e| e.to_string())?;
            let b = get_config(&pool, "proofread_base_url")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            let m = get_config(&pool, "proofread_model")
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            if b.trim().is_empty() {
                return Err("未配置校对服务地址 | proofread base_url not set".into());
            }
            let key = keyring_entry()?
                .get_password()
                .map_err(|_| "未设置 API Key | proofread API key not set".to_string())?;
            Ok((b, m, key))
        },
    )
    .await
    .map_err(|e| e.to_string())??;

    let cfg = ProofreadConfig { base_url, model };
    proofread_remote(&cfg, &key, &text)
        .await
        .map_err(|e| e.to_string())
}
