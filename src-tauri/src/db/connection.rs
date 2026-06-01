// src-tauri/src/db/connection.rs
//! Database connection management.
//! - Write path: `Mutex<Connection>` — serialises all writes.
//! - Read path: `r2d2::Pool<SqliteConnectionManager>` — concurrent reads under WAL.

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::Mutex;
use tracing::{debug, info};

use crate::error::{AppError, Result};

/// Type aliases for clarity.
pub type DbPool   = Pool<SqliteConnectionManager>;
pub type DbWriter = Mutex<Connection>;

/// PRAGMA statements applied to every connection (write + each read-pool connection).
const PRAGMAS: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous  = NORMAL;
PRAGMA cache_size   = -64000;
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
PRAGMA temp_store   = MEMORY;
PRAGMA mmap_size    = 268435456;
";

/// Apply performance PRAGMAs to a connection.
fn apply_pragmas(conn: &Connection) -> Result<()> {
    conn.execute_batch(PRAGMAS).map_err(AppError::from)
}

// ── Write connection ────────────────────────────────────────────────────────

/// Open the write connection (read-write, serialised access via `Mutex`).
pub fn create_write_connection(db_path: &Path) -> Result<DbWriter> {
    info!("Opening write connection at {:?}", db_path);
    let conn = Connection::open(db_path)?;
    apply_pragmas(&conn)?;
    debug!("Write connection PRAGMAs applied");
    Ok(Mutex::new(conn))
}

// ── Read pool ───────────────────────────────────────────────────────────────

/// Customises each read-pool connection: read-only flags + PRAGMAs.
#[derive(Debug)]
struct ReadPoolCustomiser;

impl r2d2::CustomizeConnection<Connection, rusqlite::Error> for ReadPoolCustomiser {
    fn on_acquire(&self, conn: &mut Connection) -> std::result::Result<(), rusqlite::Error> {
        conn.execute_batch(PRAGMAS)
    }
}

/// Create the read connection pool.
///
/// `pool_size`: 4 for desktop, 2 for mobile (caller decides).
pub fn create_read_pool(db_path: &Path, pool_size: u32) -> Result<DbPool> {
    info!(
        "Creating read pool (size={}) at {:?}",
        pool_size, db_path
    );

    let manager = SqliteConnectionManager::file(db_path).with_flags(
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    );

    let pool = Pool::builder()
        .max_size(pool_size)
        .connection_customizer(Box::new(ReadPoolCustomiser))
        .build(manager)
        .map_err(|e| AppError::Pool(e.to_string()))?;

    debug!("Read pool created successfully");
    Ok(pool)
}
