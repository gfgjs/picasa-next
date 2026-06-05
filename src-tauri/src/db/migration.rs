// src-tauri/src/db/migration.rs
// src-tauri/src/db/migration.rs
//! Versioned schema migration.
//! 带版本控制的模式迁移。
//!
//! Strategy: read `app_config.schema_version`, execute each `if version < N` block in order.
//! 策略：读取 `app_config.schema_version`，按顺序执行每个 `if version < N` 块。
//! Adding a new migration: increment `CURRENT_VERSION` and add a new block.
//! 添加新迁移：递增 `CURRENT_VERSION` 并添加一个新块。
//! Safe to re-run: all DDL uses `CREATE TABLE IF NOT EXISTS`.
//! 重新运行是安全的：所有 DDL 都使用 `CREATE TABLE IF NOT EXISTS`。

use rusqlite::Connection;
use tracing::{info, warn};

use crate::db::schema::{SCHEMA_V1, SCHEMA_V2};
use crate::error::{AppError, Result};

/// Latest schema version supported by this binary.
/// 此二进制文件支持的最新模式版本。
const CURRENT_VERSION: u32 = 3;

/// Read the current schema version from the database.
/// 从数据库读取当前的模式版本。
/// Returns 0 if the table or key does not yet exist (fresh DB).
/// 如果表或键尚不存在，则返回 0（全新数据库）。
fn read_version(conn: &Connection) -> u32 {
    conn.query_row(
        "SELECT value FROM app_config WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.parse::<u32>().ok())
    .unwrap_or(0)
}

/// Write the current schema version.
/// 写入当前的模式版本。
fn write_version(conn: &Connection, version: u32) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![version.to_string()],
    )?;
    Ok(())
}

/// Run all pending migrations against the **write** connection.
/// 针对 **写入** 连接运行所有挂起的迁移。
/// This MUST be called at startup before any other DB operations.
/// 这必须在启动时在任何其他数据库操作之前调用。
pub fn run_migrations(conn: &Connection) -> Result<()> {
    let version = read_version(conn);
    info!("DB schema version = {}, target = {} | 数据库结构版本 = {}, 目标版本 = {}", version, CURRENT_VERSION, version, CURRENT_VERSION);

    if version < 1 {
        info!("Applying migration → v1 | 正在应用数据库迁移 → v1");
        conn.execute_batch(SCHEMA_V1)
            .map_err(|e| AppError::Db(format!("Migration v1 failed: {e}")))?;
        write_version(conn, 1)?;
        info!("Migration v1 complete | v1 数据库迁移完成");
    }

    if version < 2 {
        info!("Applying migration → v2 (AI embeddings) | 正在应用数据库迁移 → v2（AI 嵌入向量）");
        conn.execute_batch(SCHEMA_V2)
            .map_err(|e| AppError::Db(format!("Migration v2 failed: {e}")))?;
        write_version(conn, 2)?;
        info!("Migration v2 complete | v2 数据库迁移完成");
    }

    if version < 3 {
        info!("Applying migration → v3 (AI search results) | 正在应用数据库迁移 → v3（AI 搜索结果）");
        conn.execute_batch(crate::db::schema::SCHEMA_V3)
            .map_err(|e| AppError::Db(format!("Migration v3 failed: {e}")))?;
        write_version(conn, 3)?;
        info!("Migration v3 complete | v3 数据库迁移完成");
    }

    // Future migrations follow the same pattern:
    // 未来的迁移遵循相同的模式：
    // if version < 4 {
    //     conn.execute_batch(SCHEMA_V3)?;
    //     write_version(conn, 3)?;
    // }

    let final_version = read_version(conn);
    if final_version == CURRENT_VERSION {
        info!("DB schema is up-to-date (v{}) | 数据库结构已是最新 (v{})", CURRENT_VERSION, CURRENT_VERSION);
    } else {
        warn!("Post-migration version check: expected {CURRENT_VERSION}, got {final_version}");
    }

    Ok(())
}

