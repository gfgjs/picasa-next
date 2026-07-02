// src-tauri/src/ipc/blocking.rs
//! R1-3 · rusqlite 下沉 blocking 线程池的共享助手（全 ipc/ 命令族复用）。
//!
//! CLAUDE.md 硬化条款：async command 内的任何 rusqlite 调用——包括「看起来很快」的读——
//! 一律 `spawn_blocking`，不做逐条估时豁免（SQLite 同步跑在 tokio worker 上会拖垮并发 IPC）。
//! 闭包收 `&rusqlite::Connection`（读池连接经 Deref 强转），既有 `q::*` 查询零改动直接复用；
//! 复杂命令（多段读写混排 / 文件 IO 交织）不强套本助手，可自建 `spawn_blocking` 块（同规则）。

use std::sync::Arc;

use tauri::State;

use crate::error::{AppError, Result};
use crate::state::AppState;

/// 只读查询下沉 blocking 线程池（读池连接在闭包期间持有、返回即还池）。
pub async fn read_blocking<T, F>(state: &State<'_, Arc<AppState>>, f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> Result<T> + Send + 'static,
{
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let pool = state_arc.db_read_pool.get().map_err(AppError::from)?;
        f(&pool)
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

/// 写查询下沉 blocking 线程池（db_writer 互斥锁在 blocking 线程上等待/持有）。
pub async fn write_blocking<T, F>(state: &State<'_, Arc<AppState>>, f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> Result<T> + Send + 'static,
{
    let state_arc = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let conn = state_arc
            .db_writer
            .lock()
            .map_err(|e| AppError::System(e.to_string()))?;
        f(&conn)
    })
    .await
    .map_err(|e| AppError::System(e.to_string()))?
}

#[cfg(test)]
mod tests {
    /// 「最近标记」启发式的三态：见测试正文说明。
    #[derive(PartialEq, Clone, Copy, Debug)]
    enum Marker {
        /// 尚无标记 / 位于 async fn 正文（在此处直查 = 违规）。
        AsyncBody,
        /// 位于 spawn_blocking / read_blocking / write_blocking / thread::spawn 之后。
        Blocking,
        /// 位于同步 fn 正文（同步助手的调用方自证 blocking 上下文，如 active_profile）。
        SyncFn,
    }

    /// R1-3 回归门（CLAUDE.md 硬化条款的 tripwire）：ipc/ 全部源文件中，任何
    /// `db_read_pool` / `db_writer` 访问都必须出现在 spawn_blocking / read_blocking /
    /// write_blocking / thread::spawn 标记**之后**；若「最近的上游标记」是 `async fn`
    /// 签名，即判为「SQL 直跑 tokio worker」违规。
    ///
    /// 这是逐行扫描的启发式而非完备的语法证明：闭包结束后回到 async 正文的直查可能漏报,
    /// 但最常见的回归形态——新命令在 async 正文顶部直接拿连接查询——必然立刻红。
    #[test]
    fn ipc_commands_keep_rusqlite_off_async_workers() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ipc");
        let mut violations: Vec<String> = Vec::new();

        for entry in std::fs::read_dir(&dir).expect("read src/ipc") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("read ipc source");
            let file = path.file_name().unwrap().to_string_lossy().to_string();

            let mut marker = Marker::AsyncBody;
            for (idx, raw) in src.lines().enumerate() {
                // 去掉行注释（`//` 之后），避免注释里提到 db_writer 造成误报；
                // 行内 `https://` 也会被截断,但截断只影响其后文本,不影响本判定。
                let line = raw.split("//").next().unwrap_or("");

                // 标记优先级：blocking 入口 > async fn 签名 > 同步 fn 签名。
                if line.contains("spawn_blocking")
                    || line.contains("read_blocking(")
                    || line.contains("write_blocking(")
                    || line.contains("write_blocking_str(")
                    || line.contains("thread::spawn")
                {
                    marker = Marker::Blocking;
                } else if line.contains("async fn ") {
                    marker = Marker::AsyncBody;
                } else if line.contains("fn ")
                    && (line.trim_start().starts_with("fn ")
                        || line.trim_start().starts_with("pub fn ")
                        || line.trim_start().starts_with("pub(crate) fn ")
                        || line.trim_start().starts_with("pub(super) fn "))
                {
                    marker = Marker::SyncFn;
                }

                // concat! 拆分字面量：避免本测试自身源码命中扫描（自匹配误报）。
                if (line.contains(concat!("db_", "read_pool"))
                    || line.contains(concat!("db_", "writer")))
                    && marker == Marker::AsyncBody
                {
                    violations.push(format!("{}:{} → {}", file, idx + 1, raw.trim()));
                }
            }
        }

        assert!(
            violations.is_empty(),
            "以下 ipc/ 位置疑似在 tokio worker 上直跑 rusqlite（R1-3 硬化条款）。\n\
             若为误报（如闭包结束后的合法用法），请重构使 SQL 进入 blocking 闭包，\n\
             或调整本启发式并说明理由：\n{}",
            violations.join("\n")
        );
    }
}
