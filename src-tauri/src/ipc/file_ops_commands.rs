// src-tauri/src/ipc/file_ops_commands.rs
use std::sync::Arc;
use std::path::PathBuf;
use tauri::{AppHandle, State};
use tracing::info;

use crate::error::{AppError, Result};
use crate::state::AppState;
use crate::db::queries as q;
use crate::utils::path::resolve_media_path;
use crate::ipc::scan_commands::add_scan_root;

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

    tokio::fs::create_dir_all(&target_path).await.map_err(|e| AppError::CreateFolder(e.to_string()))?;
    
    let norm = crate::utils::path::normalize_db_path(&path_str);
    
    let is_within_existing = {
        let pool = state.db_read_pool.get().map_err(AppError::from)?;
        let roots = q::list_scan_roots(&pool)?;
        let target_norm = format!("{norm}/");
        roots.into_iter().any(|r| {
            let r_norm = format!("{}/", r.path);
            target_norm.starts_with(&r_norm)
        })
    };
    
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
            tokio::fs::create_dir_all(parent).await.map_err(|e| AppError::MoveFile(e.to_string()))?;
        }
        
        tokio::fs::rename(&src_path, &target_path).await.map_err(|e| AppError::MoveFile(e.to_string()))?;
        
        if let Some(parent) = PathBuf::from(&src_path).parent() {
            dirs_to_check.insert(parent.to_path_buf());
        }
        
        {
            let conn = state.db_writer.lock().map_err(|e| AppError::System(e.to_string()))?;
            conn.execute("DELETE FROM media_items WHERE id = ?1", rusqlite::params![id]).map_err(|e| AppError::Db(e))?;
        }
        moved_ids.push(id);
    }
    
    // 如果源文件夹为空，则移入系统回收站
    for dir in dirs_to_check {
        if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
            let mut is_empty = true;
            while let Ok(Some(_)) = entries.next_entry().await {
                is_empty = false;
                break;
            }
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
}

#[tauri::command]
pub async fn copy_media_items(
    media_ids: Vec<i64>,
    target_dir: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<i64>> {
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
            tokio::fs::create_dir_all(parent).await.map_err(|e| AppError::CopyFile(e.to_string()))?;
        }
        
        tokio::fs::copy(&src_path, &target_path).await.map_err(|e| AppError::CopyFile(e.to_string()))?;
        copied_ids.push(id);
    }
    
    Ok(copied_ids)
}
