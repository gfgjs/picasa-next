// src-tauri/src/db/migration.rs
//! Versioned schema migration.
//!
//! Strategy: read `app_config.schema_version`, execute each `if version < N` block in order.
//! Adding a new migration: increment `CURRENT_VERSION` and add a new block.
//! Safe to re-run: all DDL uses `CREATE TABLE IF NOT EXISTS`.

use rusqlite::Connection;
use tracing::{info, warn};

use crate::db::schema::SCHEMA_V1;
use crate::error::{AppError, Result};

/// Latest schema version supported by this binary.
const CURRENT_VERSION: u32 = 1;

/// Read the current schema version from the database.
/// Returns 0 if the table or key does not yet exist (fresh DB).
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
fn write_version(conn: &Connection, version: u32) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![version.to_string()],
    )?;
    Ok(())
}

/// Run all pending migrations against the **write** connection.
/// This MUST be called at startup before any other DB operations.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    let version = read_version(conn);
    info!("DB schema version = {}, target = {}", version, CURRENT_VERSION);

    if version < 1 {
        info!("Applying migration → v1");
        conn.execute_batch(SCHEMA_V1)
            .map_err(|e| AppError::Db(format!("Migration v1 failed: {e}")))?;
        write_version(conn, 1)?;
        info!("Migration v1 complete");
    }

    // Future migrations follow the same pattern:
    // if version < 2 {
    //     conn.execute_batch(SCHEMA_V2)?;
    //     write_version(conn, 2)?;
    // }

    if version == CURRENT_VERSION {
        info!("DB schema is up-to-date (v{})", CURRENT_VERSION);
    } else {
        warn!("Post-migration version check: expected {CURRENT_VERSION}");
    }

    Ok(())
}
