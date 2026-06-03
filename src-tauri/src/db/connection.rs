// src-tauri/src/db/connection.rs
// src-tauri/src/db/connection.rs
//! Database connection management.
//! 数据库连接管理。
//! - Write path: `Mutex<Connection>` — serialises all writes.
//! - 写入路径：`Mutex<Connection>` — 序列化所有写入操作。
//! - Read path: `r2d2::Pool<SqliteConnectionManager>` — concurrent reads under WAL.
//! - 读取路径：`r2d2::Pool<SqliteConnectionManager>` — WAL 模式下的并发读取。

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::Mutex;
use tracing::{debug, info};

use crate::error::{AppError, Result};

/// Type aliases for clarity.
/// 为清晰起见定义类型别名。
pub type DbPool   = Pool<SqliteConnectionManager>;
pub type DbWriter = Mutex<Connection>;

/// PRAGMA statements applied to every connection (write + each read-pool connection).
/// 适用于每个连接（写入 + 每个读池连接）的 PRAGMA 语句。
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
/// 将性能相关的 PRAGMA 应用于连接。
fn apply_pragmas(conn: &Connection) -> Result<()> {
    conn.execute_batch(PRAGMAS).map_err(AppError::from)
}

// ── Write connection ────────────────────────────────────────────────────────
// ── 写入连接 ────────────────────────────────────────────────────────

/// Open the write connection (read-write, serialised access via `Mutex`).
/// 打开写入连接（读写，通过 `Mutex` 串行化访问）。
pub fn create_write_connection(db_path: &Path) -> Result<DbWriter> {
    info!("Opening write connection at {:?} | 正在 {:?} 建立数据库写连接", db_path, db_path);
    let conn = Connection::open(db_path)?;
    apply_pragmas(&conn)?;
    debug!("Write connection PRAGMAs applied");
    Ok(Mutex::new(conn))
}

// ── Read pool ───────────────────────────────────────────────────────────────
// ── 读取池 ───────────────────────────────────────────────────────────────

/// Customises each read-pool connection: read-only flags + PRAGMAs.
/// 自定义每个读取池连接：只读标志 + PRAGMA。
#[derive(Debug)]
struct ReadPoolCustomiser;

impl r2d2::CustomizeConnection<Connection, rusqlite::Error> for ReadPoolCustomiser {
    fn on_acquire(&self, conn: &mut Connection) -> std::result::Result<(), rusqlite::Error> {
        conn.execute_batch(PRAGMAS)
    }
}

/// Create the read connection pool.
/// 创建读取连接池。
///
/// `pool_size`: 4 for desktop, 2 for mobile (caller decides).
/// `pool_size`：桌面端为 4，移动端为 2（由调用者决定）。
pub fn create_read_pool(db_path: &Path, pool_size: u32) -> Result<DbPool> {
    info!(
        "Opening read pool at {:?} with max_size={} | 正在 {:?} 建立读连接池，最大连接数={}",
        db_path, pool_size, db_path, pool_size
    );

    let manager = SqliteConnectionManager::file(db_path).with_flags(
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    );

    let pool = Pool::builder()
        .max_size(pool_size)
        // Defer connection creation to first use — avoids blocking setup() with
        // 4× SQLite open + PRAGMA batches during cold start.
        // 延迟连接创建到首次使用，避免在冷启动时阻塞 setup()（4 次 SQLite 打开 + PRAGMA 批次）
        .min_idle(Some(0))
        .connection_customizer(Box::new(ReadPoolCustomiser))
        .build(manager)
        .map_err(|e| AppError::Pool(e.to_string()))?;

    debug!("Read pool created successfully");
    Ok(pool)
}
