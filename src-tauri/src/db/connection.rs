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
pub type DbPool = Pool<SqliteConnectionManager>;
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
    info!(
        "Opening write connection at {:?} | 正在 {:?} 建立数据库写连接",
        db_path, db_path
    );
    let conn = Connection::open(db_path)?;
    apply_pragmas(&conn)?;
    crate::db::register_custom_collations(&conn).map_err(AppError::from)?;
    debug!("Write connection PRAGMAs applied");
    Ok(Mutex::new(conn))
}

/// 启动期 WAL 截断（S3.6，S3.7 修正调用时机）：退出钩子只覆盖正常退出——dev Ctrl+C/崩溃/
/// 强杀会让 WAL 带着整个会话的管线写量（缩略图/富化/AI）跨会话累积，拖慢后续所有读。
/// **调用方须在 tracing 订阅器就绪后调用**（原挂在 create_write_connection 内，先于日志
/// 初始化,info!/warn! 被静默丢弃——S3.6 首轮真机看不到日志的原因）。setup 内 tracing init
/// 之后、管线拉起之前调用：读池连接已归还、无并发读者，TRUNCATE 可完整回收；WAL 越大本步
/// 越久（一次性清偿，日志可见），失败仅告警不阻断启动。
pub(crate) fn checkpoint_wal_at_boot(conn: &Connection, db_path: &Path) {
    // SQLite WAL 命名 = 数据库路径直接追加 "-wal"（不是替换扩展名）。
    let wal_path = {
        let mut p = db_path.as_os_str().to_owned();
        p.push("-wal");
        std::path::PathBuf::from(p)
    };
    let size_mb = |p: &Path| {
        std::fs::metadata(p)
            .map(|m| m.len() as f64 / 1_048_576.0)
            .unwrap_or(0.0)
    };
    let before = size_mb(&wal_path);
    // 返回 (busy, WAL 总页数, 已检查点页数)；非 WAL 库返回 (0, -1, -1)，无害。
    let result: rusqlite::Result<(i64, i64, i64)> =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        });
    match result {
        Ok((busy, log, ckpt)) => info!(
            "Boot WAL checkpoint: {:.1}MB → {:.1}MB (busy={}, pages {}/{}) | 启动期 WAL 截断",
            before,
            size_mb(&wal_path),
            busy,
            ckpt,
            log
        ),
        Err(e) => tracing::warn!("Boot WAL checkpoint failed | 启动期 WAL 截断失败: {}", e),
    }
}

// ── Read pool ───────────────────────────────────────────────────────────────
// ── 读取池 ───────────────────────────────────────────────────────────────

/// Customises each read-pool connection: read-only flags + PRAGMAs.
/// 自定义每个读取池连接：只读标志 + PRAGMA。
#[derive(Debug)]
struct ReadPoolCustomiser;

impl r2d2::CustomizeConnection<Connection, rusqlite::Error> for ReadPoolCustomiser {
    fn on_acquire(&self, conn: &mut Connection) -> std::result::Result<(), rusqlite::Error> {
        conn.execute_batch(PRAGMAS)?;
        crate::db::register_custom_collations(conn)?;
        Ok(())
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
        .map_err(AppError::Pool)?;

    debug!("Read pool created successfully");
    Ok(pool)
}
#[test]
fn test_collation() {
    // 共享缓存内存库：同名 + `cache=shared` → write 连接与 read 池共享同一内存库，
    // 免去文件残留 / 并发撞库 / 污染工作目录（原用 CWD 文件 `test_collation.db`）。
    // 进程号入名避免跨测试串库；写连接须在 read 之前建好并存活，以维持内存库不被回收。
    let uri = format!(
        "file:picasa_test_collation_{}?mode=memory&cache=shared",
        std::process::id()
    );
    // 写连接：直接以 URI 标志开（create_write_connection 走裸 open 不解析 URI，故此处自建），
    // 并经同一 `register_custom_collations` 注册排序——与生产写路径同源。
    let write_conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    crate::db::register_custom_collations(&write_conn).unwrap();
    write_conn
        .execute_batch("CREATE TABLE test(name TEXT); INSERT INTO test VALUES ('10'),('2'),('1');")
        .unwrap();

    // 读池经 create_read_pool 建立——同时验证 ReadPoolCustomiser 也在每个池连接上注册了 NATURAL_CMP
    //（生产查询实际在读池上执行 ORDER BY COLLATE，这层覆盖不能丢）。
    let read_pool = crate::db::create_read_pool(std::path::Path::new(&uri), 2).unwrap();
    let conn = read_pool.get().unwrap();
    let mut stmt = conn
        .prepare("SELECT name FROM test ORDER BY name COLLATE NATURAL_CMP ASC")
        .unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    // 自然序：2 在 10 之前。词典序会错排成 ["1","10","2"]，故此断言真正锁住数字感知排序。
    assert_eq!(rows, vec!["1", "2", "10"]);
}
