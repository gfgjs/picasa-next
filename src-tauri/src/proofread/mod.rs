// src-tauri/src/proofread/mod.rs
//! Remote AI proofreading (§5.4). 远程 AI 校对（§5.4）。
//!
//! 文本 LLM 与 CLIP 是两套独立机制。本模块是「可插拔接口」的远程实现：走 OpenAI 兼容的
//! `/chat/completions`（覆盖 OpenAI / 多数网关 / Ollama / LM Studio 等）。本地实现
//! （Ollama 进程 / 进程内模型）按决策 D5 待用户调研后再补 —— 届时新增一个同形函数/实现即可，
//! IPC 层按配置选择，无需改动调用方。
//!
//! Text-LLM proofreading is separate from CLIP. This is the *remote* impl of a pluggable seam:
//! an OpenAI-compatible `/chat/completions` call. A `LocalProofreader` (Ollama / in-process) is
//! deferred per decision D5; adding it later means another function the IPC layer selects by config.
//!
//! 安全：API key 存系统凭据库（keyring），不落明文 DB；base_url 仅允许 http(s)。

use serde_json::json;

use crate::error::{AppError, Result};

/// Remote endpoint config (key is fetched separately from keyring, never stored here).
/// 远程端点配置（key 另从 keyring 取，绝不存于此）。
pub struct ProofreadConfig {
    /// e.g. `https://api.openai.com/v1` (no trailing `/chat/completions`).
    pub base_url: String,
    pub model: String,
}

/// Chinese-first proofreading instruction: fix typos/punctuation/grammar, keep meaning + format,
/// return ONLY the corrected full text (no explanations / headings / code fences).
/// 中文优先的校对指令：修正错别字/标点/语病，保持原意与格式，仅返回修正后的完整文本。
const SYSTEM_PROMPT: &str = "你是专业的中文校对助手。请修正文本中的错别字、标点符号和语病，\
保持原意、写作风格与段落格式不变。只输出修正后的完整文本，不要添加任何解释、标题或代码块标记。";

/// Proofread one text chunk via an OpenAI-compatible chat completion. Returns the corrected text.
/// 经 OpenAI 兼容 chat completion 校对一段文本，返回修正后的文本。
pub async fn proofread_remote(cfg: &ProofreadConfig, api_key: &str, text: &str) -> Result<String> {
    // base_url 校验：仅允许 http(s)（桌面端由用户自配端点，含 localhost Ollama，故不做白名单）。
    let base = cfg.base_url.trim().trim_end_matches('/');
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(AppError::Internal(
            "proofread base_url must be http(s) | 校对 base_url 必须为 http(s)".into(),
        ));
    }
    if cfg.model.trim().is_empty() {
        return Err(AppError::Internal(
            "proofread model not set | 未配置校对模型".into(),
        ));
    }

    let endpoint = format!("{base}/chat/completions");
    let body = json!({
        "model": cfg.model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user",   "content": text }
        ],
        "temperature": 0.2,
        "stream": false
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&endpoint)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("proofread request failed | 校对请求失败: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default();
        let snippet: String = detail.chars().take(300).collect();
        return Err(AppError::Internal(format!(
            "proofread API {status} | 校对接口返回错误: {snippet}"
        )));
    }

    let v: serde_json::Value = resp.json().await.map_err(|e| {
        AppError::Internal(format!("proofread parse failed | 校对响应解析失败: {e}"))
    })?;

    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| AppError::Internal("proofread: empty response | 校对响应无内容".into()))?;

    Ok(content.trim().to_string())
}
