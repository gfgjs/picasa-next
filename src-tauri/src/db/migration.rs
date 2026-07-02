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

use crate::db::schema::{
    SCHEMA_V1, SCHEMA_V10, SCHEMA_V11, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6,
    SCHEMA_V7, SCHEMA_V8, SCHEMA_V9,
};
use crate::error::Result;

/// Latest schema version supported by this binary.
/// 此二进制文件支持的最新模式版本。
const CURRENT_VERSION: u32 = 11;

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

/// 单个版本块：DDL + 版本号写入在**同一事务**内原子提交。
/// 失败整块回滚 → `schema_version` 不前进 → 下次启动安全重跑（杜绝半迁移 / `duplicate column` 死循环式启动失败）。
///
/// 用 `unchecked_transaction`（与 queries/scanner/enricher 全仓一致）而非把 `run_migrations`
/// 改 `&mut Connection`：迁移在启动期独占运行、无并发写，DEFERRED 事务足够，且免去波及 7 处
/// 调用方的破坏性签名变更。
fn migrate_step(conn: &Connection, version: u32, sql: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(sql)?; // 该版本全部 DDL
    write_version(&tx, version)?; // 版本号与 DDL 同事务（&Transaction 经 Deref 强转 &Connection）
    tx.commit()?; // 原子提交：失败整块回滚、版本号不前进
    Ok(())
}

/// Run all pending migrations against the **write** connection.
/// 针对 **写入** 连接运行所有挂起的迁移。
/// This MUST be called at startup before any other DB operations.
/// 这必须在启动时在任何其他数据库操作之前调用。
pub fn run_migrations(conn: &Connection) -> Result<()> {
    let version = read_version(conn);
    info!(
        "DB schema version = {}, target = {} | 数据库结构版本 = {}, 目标版本 = {}",
        version, CURRENT_VERSION, version, CURRENT_VERSION
    );

    // 版本块顺序表：每块「DDL + 写版本号」由 `migrate_step` 在**单一事务**内原子提交。
    // 第三元素为日志描述（保留原有中英双语上下文）。
    const STEPS: &[(u32, &str, &str)] = &[
        (1, SCHEMA_V1, "v1 base | 基础结构"),
        (2, SCHEMA_V2, "v2 (AI embeddings) | v2（AI 嵌入向量）"),
        (3, SCHEMA_V3, "v3 (AI search results) | v3（AI 搜索结果）"),
        (
            4,
            SCHEMA_V4,
            "v4 (derivations + meta columns + reading progress) | v4（派生任务 + 元数据扩列 + 阅读进度）",
        ),
        (5, SCHEMA_V5, "v5 (collections / favorites) | v5（收藏夹）"),
        (
            6,
            SCHEMA_V6,
            "v6 (doc replacements + document versions) | v6（文档替换规则 + 版本管理）",
        ),
        (
            7,
            SCHEMA_V7,
            "v7 (storage backends / network drives) | v7（存储后端 / 网络盘）",
        ),
        (
            8,
            SCHEMA_V8,
            "v8 (face recognition: persons + faces + face_status) | v8（人脸识别：人物 + 人脸 + 检测状态）",
        ),
        (
            9,
            SCHEMA_V9,
            "v9 (exotic format plugin: catalog + plugins + tasks) | v9（冷门格式插件：能力目录 + 已装插件 + 任务表）",
        ),
        (
            10,
            SCHEMA_V10,
            "v10 (volume availability: volumes + volume_id/availability + color_label + content_identifier + face_rejections + backfill) | v10（卷可用性：卷表 + 卷id/可用性 + 颜色标签 + Live Photo 标识 + 人脸负样本 + 回填）",
        ),
        (
            11,
            SCHEMA_V11,
            "v11 (keyset pagination: composite idx_media_sort + idx_media_trash) | v11（keyset 分页：复合排序索引 + 回收站 seek 索引）",
        ),
    ];
    for &(v, sql, desc) in STEPS {
        if version < v {
            info!(
                "Applying migration → {} | 正在应用数据库迁移 → {}",
                desc, desc
            );
            migrate_step(conn, v, sql)?; // DDL + 版本号同事务原子提交（杜绝半迁移）
            info!("Migration v{} complete | v{} 数据库迁移完成", v, v);
        }
    }

    // 未来迁移：在 STEPS 末尾追加 `(12, SCHEMA_V12, "...")` 即可，事务化由 migrate_step 统一保证。

    let final_version = read_version(conn);
    if final_version == CURRENT_VERSION {
        info!(
            "DB schema is up-to-date (v{}) | 数据库结构已是最新 (v{})",
            CURRENT_VERSION, CURRENT_VERSION
        );
    } else {
        warn!("Post-migration version check: expected {CURRENT_VERSION}, got {final_version}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全新内存库跑全部迁移，验证版本号到 CURRENT_VERSION 且 exotic 三表 + 索引 + 配置就绪（端到端到最新版本）。
    #[test]
    fn migrates_fresh_db_to_current_version_with_exotic_tables() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        run_migrations(&conn).expect("run_migrations");

        assert_eq!(read_version(&conn), CURRENT_VERSION);
        assert_eq!(CURRENT_VERSION, 11);

        // 三表存在。
        for table in ["exotic_catalog_formats", "exotic_plugins", "exotic_tasks"] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺表 {table}");
        }

        // 领取/门控索引存在。
        for idx in ["idx_exotic_tasks_ready", "idx_exotic_tasks_item"] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [idx],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺索引 {idx}");
        }

        // 配置默认值就绪（含 max_workers），且**不存在** exotic_dev_mode。
        for (k, v) in [
            ("exotic_enabled", "true"),
            ("exotic_auto_process", "true"),
            ("exotic_paused", "false"),
            ("exotic_max_workers", "0"),
        ] {
            let got: String = conn
                .query_row("SELECT value FROM app_config WHERE key=?1", [k], |r| {
                    r.get(0)
                })
                .unwrap_or_else(|_| panic!("缺配置 {k}"));
            assert_eq!(got, v, "配置 {k} 值不符");
        }
        let dev: i64 = conn
            .query_row(
                "SELECT count(*) FROM app_config WHERE key='exotic_dev_mode'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dev, 0, "Release 不得有 exotic_dev_mode 授权旁路");

        // 唯一约束：同 (item, plugin, capability) 不可重复。
        // 迁移启用了 FK；本子测试只验 UNIQUE，临时关 FK 免去构造完整 media_items 行。
        conn.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
        conn.execute(
            "INSERT INTO exotic_tasks (item_id, plugin_id, capability) VALUES (1, 'exotic-image-psd', 'thumbnail')",
            [],
        )
        .expect("first task insert");
        let dup = conn.execute(
            "INSERT INTO exotic_tasks (item_id, plugin_id, capability) VALUES (1, 'exotic-image-psd', 'thumbnail')",
            [],
        );
        assert!(
            dup.is_err(),
            "UNIQUE(item_id,plugin_id,capability) 应拒绝重复"
        );
    }

    /// 事务化迁移的幂等性：已迁移库再跑一次必须是 no-op（不得 `duplicate column` / 不得改版本）。
    /// 锁住「半迁移死循环式启动失败」的根治效果——这是开机 panic 的来源之一。
    #[test]
    fn migrations_are_idempotent_on_rerun() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        run_migrations(&conn).expect("first run");
        // 版本已是 CURRENT_VERSION，第二次全部跳过；若有未事务化/非幂等块，这里会 duplicate column 报错。
        run_migrations(&conn).expect("second run must be a safe no-op");
        assert_eq!(read_version(&conn), CURRENT_VERSION);
    }

    /// 取某表的列名集合（pragma_table_info；表名为测试内字面量，format! 安全）。
    fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
        let sql = format!("SELECT name FROM pragma_table_info('{table}')");
        let mut stmt = conn.prepare(&sql).unwrap();
        let cols = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        cols
    }

    /// 全新库到 V10：卷可用性模型的表 / 列 / 索引齐备（结构层端到端）。
    #[test]
    fn v10_volume_schema_present() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        run_migrations(&conn).expect("run_migrations");
        assert_eq!(read_version(&conn), CURRENT_VERSION);

        // 新表存在。
        for table in ["volumes", "face_rejections"] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺表 {table}");
        }

        // media_items 新列齐备。
        let mi = table_columns(&conn, "media_items");
        for col in [
            "volume_id",
            "volume_relative_path",
            "availability",
            "color_label",
            "content_identifier",
        ] {
            assert!(mi.contains(&col.to_string()), "media_items 缺列 {col}");
        }
        // scan_roots / persons 新列。
        let sr = table_columns(&conn, "scan_roots");
        assert!(
            sr.contains(&"volume_id".to_string()) && sr.contains(&"volume_subpath".to_string()),
            "scan_roots 缺卷列"
        );
        let p = table_columns(&conn, "persons");
        assert!(
            p.contains(&"model_name".to_string()),
            "persons 缺 model_name"
        );

        // 部分索引存在。
        for idx in [
            "idx_media_avail",
            "idx_media_volume",
            "idx_media_content_id",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='index' AND name=?1",
                    [idx],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺索引 {idx}");
        }
    }

    /// V9→V10 升级 + 卷回填：模拟既有 V9 用户（有 scan_root/目录/媒体项但无卷列）。升级后必须：
    /// 建出 volumes 占位行、scan_roots 与 media_items 的 volume_id 经 directory→scan_root→volume
    /// 链正确回填、availability 默认 online。锁住「既有用户零数据丢失 + 正确联卷」（迁移最高危处）。
    #[test]
    fn v10_backfill_links_existing_data_to_volumes() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        // 1) 逐版本 DDL 搭一个 V9 库（停在 9，不含卷列）。
        for sql in [
            SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V7, SCHEMA_V8,
            SCHEMA_V9,
        ] {
            conn.execute_batch(sql).unwrap();
        }
        write_version(&conn, 9).unwrap();

        // 2) 既有数据：scan_root → directory → media_item（media_item 列对齐 fast_scan INSERT）。
        conn.execute(
            "INSERT INTO scan_roots (id, path, alias) VALUES (1, '/test/root', 'Test')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO directories (id, root_id, rel_path, name) VALUES (1, 1, 'sub', 'sub')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO media_items
                (id, directory_id, file_name, file_size, file_mtime, file_format,
                 media_type, width, height, sort_datetime, cache_key)
             VALUES (1, 1, 'a.jpg', 100, 0, 'jpg', 'image', 0, 0, 0, 0)",
            [],
        )
        .unwrap();

        // 3) 升级到 V10（version=9 → 只跑 V10 步：ALTER + 回填）。
        run_migrations(&conn).expect("V9→V10 migration");
        assert_eq!(read_version(&conn), CURRENT_VERSION);

        // 4) volumes 占位行建出（stable_id='pending:1'，last_mount_path=根路径）。
        let vol_id: i64 = conn
            .query_row(
                "SELECT id FROM volumes WHERE stable_id='pending:1' AND last_mount_path='/test/root'",
                [],
                |r| r.get(0),
            )
            .expect("缺卷占位行");
        // 5) scan_roots.volume_id 回填到该卷。
        let sr_vol: i64 = conn
            .query_row("SELECT volume_id FROM scan_roots WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(sr_vol, vol_id, "scan_root 未联卷");
        // 6) media_items.volume_id 经 directory→scan_root→volume 链回填。
        let mi_vol: i64 = conn
            .query_row("SELECT volume_id FROM media_items WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(mi_vol, vol_id, "media_item 未经链回填联卷");
        // 7) availability 默认 online（零成本迁移：既有行自动在线）。
        let avail: String = conn
            .query_row("SELECT availability FROM media_items WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(avail, "online", "既有行 availability 应默认 online");
    }

    /// V11：idx_media_sort 升为复合键（2 列），idx_media_trash 新建。锁住 keyset 分页的索引地基。
    #[test]
    fn v11_keyset_indexes_present() {
        let conn = Connection::open_in_memory().expect("open in-memory");
        run_migrations(&conn).expect("run_migrations");
        assert_eq!(read_version(&conn), 11);

        // idx_media_sort 现为复合键：pragma_index_info 应有 2 列（sort_datetime + id）。
        let sort_cols: i64 = conn
            .query_row(
                "SELECT count(*) FROM pragma_index_info('idx_media_sort')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sort_cols, 2, "idx_media_sort 应为 2 列复合键");

        // idx_media_trash 存在。
        let trash: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='idx_media_trash'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(trash, 1, "缺 idx_media_trash");
    }
}
