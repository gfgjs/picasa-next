// src-tauri/src/ipc/ai_commands.rs
//! IPC commands for AI inference engine management and semantic search.
//! AI 推理引擎管理和语义搜索的 IPC 命令。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;
use tracing::{info, warn};

use crate::ai::clip::MODEL_NAME;
use crate::ai::engine::AiEnginePool;
use crate::ai::pipeline::start_ai_pipeline;
use crate::ai::search::semantic_search;
use crate::db::models::AiStatusSummary;
use crate::db::queries::{
    count_analyzed_ai_items, count_total_ai_items, get_config, reset_ai_embeddings, set_config,
};
use crate::error::{AppError, Result};
use crate::state::AppState;

// ── Helpers ───────────────────────────────────────────────────────────────────
// ── 辅助函数 ──────────────────────────────────────────────────────────────────

/// Get the models directory from app data.
/// 从应用数据获取模型目录。
fn models_dir(state: &AppState) -> PathBuf {
    // We derive models_dir from the log_dir parent (= app_data_dir)
    // 我们从 log_dir 的父目录（= app_data_dir）推导模型目录
    let app_data_dir = state.log_dir.parent().unwrap_or(&state.log_dir);
    app_data_dir.join("models")
}

/// Ensure the AI engine is initialised (lazy init on first call).
/// 确保 AI 引擎已初始化（首次调用时懒加载初始化）。
fn ensure_engine_initialised(state: &AppState) -> Result<()> {
    // Fast path: already initialised
    // 快速路径：已初始化
    {
        let guard = state.ai_engine.read().unwrap();
        if guard.is_some() {
            return Ok(());
        }
    }

    // Slow path: initialise under write lock
    // 慢速路径：在写锁下初始化
    let mut guard = state.ai_engine.write().unwrap();
    if guard.is_some() {
        return Ok(());  // Race check | 竞争检查
    }

    // [方案 5 准备] 指定加载系统自带的 onnxruntime.dll，避免打包官方 DLL 导致体积膨胀。
    // 若要测试方案 5，请在 Cargo.toml 中切换 ort 依赖，并取消下面这行代码的注释：
    // std::env::set_var("ORT_DYLIB_PATH", "C:\\Windows\\System32\\onnxruntime.dll");

    // Initialise ORT runtime once, lazily (avoids blocking Tauri setup() and the
    // white-screen delay caused by loading the 160 MB onnxruntime.dll at startup).
    // 惰性初始化 ORT runtime（避免在 Tauri setup() 中阻塞并导致白屏）。
    let ort_init_res = ort::init()
        .with_name("PicasaNext")
        .commit();
    info!("ORT initialization result: {:?}", ort_init_res);

    let models = models_dir(state);
    std::fs::create_dir_all(&models)
        .map_err(|e| AppError::Io(e))?;

    info!("Initialising AI engine | 正在初始化 AI 引擎...");
    let conn = state.db_read_pool.get()?;
    let image_model_name = get_config(&conn, "ai_image_model")
        .unwrap_or(None)
        // FP16 external-data format: 3.77 MB header + 172 MB weights loaded via memory-map.
        // MUCH faster than the monolithic fp32 format (330 MB Protobuf blob takes 5+ min to parse).
        // FP16 外部数据格式：3.77 MB 主文件 + 172 MB 权重通过内存映射加载，比单体 fp32 格式快数十倍。
        .unwrap_or_else(|| "vit-b-16.img.fp16.onnx".to_string());
    let text_model_name = get_config(&conn, "ai_text_model")
        .unwrap_or(None)
        // FP16 external-data format: 2.18 MB header + 194.7 MB weights.
        // FP16 外部数据格式：2.18 MB 主文件 + 194.7 MB 权重。
        .unwrap_or_else(|| "vit-b-16.txt.fp16.onnx".to_string());

    let provider_override = get_config(&conn, "ai_provider_override")
        .unwrap_or(None)
        .unwrap_or_else(|| "auto".to_string());
        
    let mut pool = AiEnginePool::init(&models, &image_model_name, &text_model_name, &provider_override)?;

    // Load tokenizer eagerly and cache it in the pool.
    // Avoids reading vocab.txt on every semantic_search_cmd call.
    // 立即加载分词器并缓存到池中。
    // 避免每次调用 semantic_search_cmd 都重读 vocab.txt。
    let vocab_path = models.join("vocab.txt");
    if vocab_path.exists() {
        match crate::ai::clip::ClipTokenizer::from_vocab(&vocab_path) {
            Ok(tokenizer) => {
                pool.clip_tokenizer = Some(tokenizer);
                info!("CLIP tokenizer cached in engine pool | CLIP 分词器已缓存到引擎池");
            }
            Err(e) => warn!("Failed to load tokenizer | 分词器加载失败: {}", e),
        }
    } else {
        warn!("vocab.txt not found at {:?} | vocab.txt 未找到: {:?}", vocab_path, vocab_path);
    }

    // Persist detected provider to app_config
    // 将探测到的提供者持久化到 app_config
    {
        let conn = state.db_writer.lock().unwrap();
        let _ = set_config(&conn, "ai_provider", pool.provider.as_str());
        let _ = set_config(&conn, "ai_gpu_name", &pool.gpu_name);
    }

    *guard = Some(pool);
    info!("AI engine initialised | AI 引擎初始化完成");
    Ok(())
}

// ── Commands ──────────────────────────────────────────────────────────────────
// ── 命令 ──────────────────────────────────────────────────────────────────────

/// Detect and initialise the best available AI provider.
/// 探测并初始化最优的 AI 提供者。
///
/// Returns: `{ provider: string, gpu_name: string }`
#[tauri::command]
pub async fn detect_ai_provider(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<serde_json::Value, String> {
    let state = Arc::clone(&state);

    tokio::task::spawn_blocking(move || {
        ensure_engine_initialised(&state)
            .map_err(|e| e.to_string())?;

        let guard = state.ai_engine.read().unwrap();
        let pool = guard.as_ref().unwrap();

        Ok(serde_json::json!({
            "provider": pool.provider.as_str(),
            "gpuName":  pool.gpu_name.clone(),
            "clipLoaded": pool.clip_ready(),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get comprehensive AI status for the UI status bar.
/// 获取 UI 状态栏所需的综合 AI 状态。
#[tauri::command]
pub async fn get_ai_status(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<AiStatusSummary, String> {
    let state = Arc::clone(&state);

    tokio::task::spawn_blocking(move || -> std::result::Result<AiStatusSummary, String> {
        let conn = state.db_read_pool.get().map_err(|e| e.to_string())?;

        let provider = get_config(&conn, "ai_provider")
            .unwrap_or_default()
            .unwrap_or_default();
        let gpu_name = get_config(&conn, "ai_gpu_name")
            .unwrap_or_default()
            .unwrap_or_default();

        let total_items    = count_total_ai_items(&conn).unwrap_or(0);
        let analyzed_items = count_analyzed_ai_items(&conn).unwrap_or(0);
        let pending_items  = total_items.saturating_sub(analyzed_items);

        let clip_loaded = {
            let guard = state.ai_engine.read().unwrap();
            guard.as_ref().map(|e| e.clip_ready()).unwrap_or(false)
        };

        let is_analyzing = state.ai_analysis_token.lock().unwrap().is_some();

        let vram_bytes = crate::ai::provider::detect_vram_bytes();
        let vram_gb = vram_bytes.map(|b| (b / (1024 * 1024 * 1024)) as i64);

        let batch_size_str = get_config(&conn, "ai_batch_size").unwrap_or_default();
        let batch_size = if let Some(s) = batch_size_str {
            s.parse::<i64>().unwrap_or(8)
        } else {
            let default_batch = if let Some(gb) = vram_gb {
                if gb >= 8 { 64 } else if gb >= 4 { 32 } else if gb >= 2 { 16 } else { 8 }
            } else {
                8
            };
            let w_conn = state.db_writer.lock().unwrap();
            let _ = set_config(&w_conn, "ai_batch_size", &default_batch.to_string());
            default_batch
        };

        Ok(AiStatusSummary {
            provider,
            gpu_name,
            vram_gb,
            batch_size,
            clip_loaded,
            total_items,
            analyzed_items,
            pending_items,
            is_analyzing,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Perform semantic search using Chinese-CLIP text encoder.
/// 使用 Chinese-CLIP 文本编码器执行语义搜索。
#[tauri::command]
pub async fn semantic_search_cmd(
    query: String,
    limit: Option<usize>,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<usize, String> {
    let state = Arc::clone(&state);
    let top_k = limit.unwrap_or(50).min(1000);

    tokio::task::spawn_blocking(move || -> std::result::Result<usize, String> {
        // Ensure engine is ready
        // 确保引擎就绪
        ensure_engine_initialised(&state).map_err(|e| e.to_string())?;

        let engine_guard = state.ai_engine.read().unwrap();
        let engine = engine_guard.as_ref().unwrap();

        let text_session = match engine.clip_text_session.as_ref() {
            Some(s) => s,
            None => return Err("Text encoder not loaded | 文本编码器未加载".to_string()),
        };

        // semantic_search manages its own connections: it loads the resident embedding
        // cache from the READ pool and only takes the write lock briefly to persist
        // results — so scoring no longer blocks all DB writes.
        // semantic_search 自行管理连接：从读连接池加载常驻嵌入缓存，仅在持久化结果时
        // 短暂持有写锁 —— 打分阶段不再阻塞所有数据库写入。
        if let Some(tokenizer) = engine.clip_tokenizer.as_ref() {
            semantic_search(&state, text_session, tokenizer, &query, top_k)
                .map_err(|e| e.to_string())
        } else {
            // Fallback: load tokenizer from disk (happens if vocab.txt wasn't present at init time)
            // 回退：从磁盘加载分词器（vocab.txt 初始化时不存在的情况）
            let models = models_dir(&state);
            let vocab_path = models.join("vocab.txt");
            let tokenizer = crate::ai::clip::ClipTokenizer::from_vocab(&vocab_path)
                .map_err(|e| e.to_string())?;
            semantic_search(&state, text_session, &tokenizer, &query, top_k)
                .map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Start the background AI analysis pipeline.
/// Always resets existing embeddings first for a full fresh analysis.
///
/// 启动后台 AI 分析流水线。
/// 始终先清除已有嵌入向量，保证每次都是全量重新分析。
#[tauri::command]
pub async fn start_ai_analysis(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    let state_arc = Arc::clone(&state);

    // Initialise engine first (idempotent)
    // 首先初始化引擎（幂等）
    tokio::task::spawn_blocking({
        let s = Arc::clone(&state_arc);
        move || ensure_engine_initialised(&s)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // Cancel any existing analysis before starting a new one
    // 开始新的分析前取消任何现有的分析
    state_arc.cancel_ai_analysis();

    // Always clear previous embeddings so every click is a complete fresh run.
    // The button is "全量 AI 分析" — "全量" means full/complete, not incremental.
    // 始终清除之前的嵌入向量，保证每次点击都是全量重新分析。
    tokio::task::spawn_blocking({
        let s = Arc::clone(&state_arc);
        move || {
            let conn = s.db_writer.lock().unwrap();
            reset_ai_embeddings(&conn, MODEL_NAME)
                .map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // Embeddings were just wiped — drop the resident cache so search reflects the reset.
    // 嵌入向量刚被清空 —— 丢弃常驻缓存，使搜索反映重置结果。
    state_arc.invalidate_embedding_cache();

    let token = state_arc.new_ai_analysis_token();
    info!("Starting AI analysis pipeline (full reset) | 启动 AI 分析流水线（全量重置）");
    start_ai_pipeline(Arc::clone(&state_arc), token);

    Ok(())
}


/// Stop the running AI analysis pipeline.
/// 停止正在运行的 AI 分析流水线。
#[tauri::command]
pub async fn stop_ai_analysis(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    info!("Stopping AI analysis pipeline | 停止 AI 分析流水线");
    state.cancel_ai_analysis();
    Ok(())
}

/// List all AI models in the models directory.
#[tauri::command]
pub async fn list_ai_models(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<Vec<String>, String> {
    let models = models_dir(&state);
    let mut files = Vec::new();
    if models.exists() && models.is_dir() {
        if let Ok(entries) = std::fs::read_dir(models) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        if let Some(ext) = entry.path().extension() {
                            if ext == "onnx" {
                                if let Some(name) = entry.file_name().to_str() {
                                    files.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Import an AI model into the models directory.
#[tauri::command]
pub async fn import_ai_model(
    source_path: String,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    let models = models_dir(&state);
    if !models.exists() {
        std::fs::create_dir_all(&models).map_err(|e| e.to_string())?;
    }
    
    let source = std::path::Path::new(&source_path);
    let file_name = source.file_name().ok_or("Invalid file name")?;
    let dest = models.join(file_name);
    
    std::fs::copy(source, &dest).map_err(|e| format!("Failed to copy file: {}", e))?;
    Ok(())
}

/// Reload the AI engine with new models
#[tauri::command]
pub async fn reload_ai_engine(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    info!("Reloading AI engine | 重新加载 AI 引擎");
    state.cancel_ai_analysis();
    
    {
        let mut guard = state.ai_engine.write().unwrap();
        *guard = None; // Drop the current engine
    }
    
    ensure_engine_initialised(&state).map_err(|e| e.to_string())?;
    Ok(())
}

/// Reset all embeddings and re-queue all images for analysis.
/// 重置所有嵌入向量，将所有图像重新排入分析队列。
#[tauri::command]
pub async fn rebuild_embeddings(
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), String> {
    let state_arc = Arc::clone(&state);

    // Stop any running pipeline first
    // 首先停止任何正在运行的流水线
    state_arc.cancel_ai_analysis();

    tokio::task::spawn_blocking(move || -> std::result::Result<(), String> {
        {
            let conn = state_arc.db_writer.lock().unwrap();
            reset_ai_embeddings(&conn, MODEL_NAME).map_err(|e| e.to_string())?;
        }
        state_arc.invalidate_embedding_cache();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    info!("Embeddings reset, re-queuing all images | 嵌入向量已重置，重新排队所有图像");
    Ok(())
}
