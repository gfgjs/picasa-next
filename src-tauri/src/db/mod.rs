// src-tauri/src/db/mod.rs
pub mod connection;
pub mod migration;
pub mod models;
pub mod queries;
pub mod schema;

pub use connection::{create_write_connection, create_read_pool, DbPool, DbWriter};

use rusqlite::Connection;

/// Register custom collations like NATURAL for natural sorting of strings with numbers.
/// 注册自定义排序规则（如 NATURAL），用于包含数字的字符串的自然排序。
pub fn register_custom_collations(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_collation("NATURAL_CMP", |s1, s2| {
        lexicmp::natural_cmp(s1, s2)
    })
}
