use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_TARGET: i64 = 500_000;
const DEFAULT_ROOT_PATH: &str = "C:/MockPhotos";
const DEFAULT_ALIAS: &str = "Mock Stress";
const DEFAULT_FILE_SIZE: i64 = 5 * 1024 * 1024;

#[derive(Clone, Copy)]
enum ThumbSeedStatus {
    Pending,
    Failed,
}

impl ThumbSeedStatus {
    fn as_db_value(self) -> i64 {
        match self {
            Self::Pending => 0,
            Self::Failed => 2,
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "pending" => Ok(Self::Pending),
            "failed" => Ok(Self::Failed),
            other => Err(format!(
                "不支持的 --thumb-status: {other}，可选 pending/failed"
            )),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending(0)",
            Self::Failed => "failed(2)",
        }
    }
}

struct Options {
    db_path: PathBuf,
    root_path: String,
    alias: String,
    target: i64,
    file_size: i64,
    thumb_status: ThumbSeedStatus,
    reset_thumbs: bool,
}

impl Options {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut opts = Self {
            db_path: default_db_path()?,
            root_path: DEFAULT_ROOT_PATH.to_string(),
            alias: DEFAULT_ALIAS.to_string(),
            target: DEFAULT_TARGET,
            file_size: DEFAULT_FILE_SIZE,
            thumb_status: ThumbSeedStatus::Pending,
            reset_thumbs: true,
        };

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                "--db" => {
                    opts.db_path = PathBuf::from(next_value(&mut args, "--db")?);
                }
                "--root" => {
                    opts.root_path = next_value(&mut args, "--root")?;
                }
                "--alias" => {
                    opts.alias = next_value(&mut args, "--alias")?;
                }
                "--target" => {
                    opts.target = next_value(&mut args, "--target")?.parse()?;
                }
                "--file-size" => {
                    opts.file_size = next_value(&mut args, "--file-size")?.parse()?;
                }
                "--thumb-status" => {
                    let value = next_value(&mut args, "--thumb-status")?;
                    opts.thumb_status = ThumbSeedStatus::parse(&value)?;
                }
                "--reset-thumbs" => {
                    opts.reset_thumbs = true;
                }
                "--no-reset-thumbs" => {
                    opts.reset_thumbs = false;
                }
                other => {
                    return Err(format!("未知参数: {other}，使用 --help 查看用法").into());
                }
            }
        }

        if opts.target <= 0 {
            return Err("--target 必须大于 0".into());
        }
        if opts.file_size <= 0 {
            return Err("--file-size 必须大于 0".into());
        }

        Ok(opts)
    }
}

fn default_db_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let app_data = std::env::var("APPDATA")?;
    Ok(PathBuf::from(app_data)
        .join("com.picasanext.app")
        .join("picasa_next.db"))
}

fn next_value(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    args.next()
        .ok_or_else(|| format!("{name} 缺少参数值").into())
}

fn print_usage() {
    println!(
        r#"用法:
  cargo run --bin mock_data -- [选项]

常用压测:
  cargo run --bin mock_data -- --target 500000 --reset-thumbs --thumb-status pending

选项:
  --db <PATH>                 指定数据库路径，默认使用 APPDATA/com.picasanext.app/picasa_next.db
  --root <PATH>               mock 扫描根路径，默认 C:/MockPhotos
  --alias <TEXT>              mock 扫描根别名，默认 Mock Stress
  --target <N>                mock 媒体条目目标数量，默认 500000
  --file-size <BYTES>         写入的模拟文件大小，默认 5242880
  --thumb-status pending|failed
                              新增/重置条目的缩略图状态，默认 pending
  --reset-thumbs              将已有 mock 条目重置为指定缩略图状态，默认开启
  --no-reset-thumbs           只补齐数量，不重置已有条目
"#
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = Options::parse()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

    println!("数据库路径: {:?}", opts.db_path);
    println!(
        "压测参数: target={} root={} file_size={} thumb_status={} reset_thumbs={}",
        opts.target,
        opts.root_path,
        opts.file_size,
        opts.thumb_status.label(),
        opts.reset_thumbs
    );

    if let Some(parent) = opts.db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut conn = Connection::open(&opts.db_path)?;
    let root_id = ensure_root(&conn, &opts, now)?;
    let directory_id = ensure_root_directory(&conn, root_id, now)?;

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE directory_id = ?1",
        params![directory_id],
        |r| r.get(0),
    )?;
    println!("当前 mock 条目数: {count}");

    if count < opts.target {
        insert_missing_items(&mut conn, &opts, directory_id, count, now)?;
    } else {
        println!("mock 条目数量已达标，无需新增。");
    }

    if opts.reset_thumbs {
        // 压测关键：把 mock 条目保持在 pending，前端滚动时会持续触发视口缩略图队列。
        let changed = conn.execute(
            "UPDATE media_items
             SET thumb_status = ?2, thumb_path = NULL, thumbhash = NULL, updated_at = ?3
             WHERE directory_id = ?1",
            params![directory_id, opts.thumb_status.as_db_value(), now],
        )?;
        println!("已重置 mock 缩略图状态: {changed} 条");
    }

    let final_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM media_items WHERE directory_id = ?1",
        params![directory_id],
        |r| r.get(0),
    )?;

    conn.execute(
        "UPDATE directories SET media_count = ?2 WHERE id = ?1",
        params![directory_id, final_count],
    )?;
    conn.execute(
        "UPDATE scan_roots SET total_files = ?2, updated_at = ?3 WHERE id = ?1",
        params![root_id, final_count, now],
    )?;

    println!("完成。mock 条目总数: {final_count}");
    println!("最小验证: 启动应用，打开 Mock Stress 根目录，快速拖动图库滚动条；状态栏不应长期停在“视口缩略图: 处理中”。");

    Ok(())
}

fn ensure_root(
    conn: &Connection,
    opts: &Options,
    now: i64,
) -> Result<i64, Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT OR IGNORE INTO scan_roots (path, alias, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3)",
        params![opts.root_path, opts.alias, now],
    )?;
    conn.execute(
        "UPDATE scan_roots SET alias = ?2, updated_at = ?3 WHERE path = ?1",
        params![opts.root_path, opts.alias, now],
    )?;
    let root_id = conn.query_row(
        "SELECT id FROM scan_roots WHERE path = ?1",
        params![opts.root_path],
        |r| r.get(0),
    )?;
    Ok(root_id)
}

fn ensure_root_directory(
    conn: &Connection,
    root_id: i64,
    now: i64,
) -> Result<i64, Box<dyn std::error::Error>> {
    conn.execute(
        "INSERT OR IGNORE INTO directories (root_id, rel_path, name, depth, media_count, created_at)
         VALUES (?1, '', 'root', 0, 0, ?2)",
        params![root_id, now],
    )?;
    let directory_id = conn.query_row(
        "SELECT id FROM directories WHERE root_id = ?1 AND rel_path = ''",
        params![root_id],
        |r| r.get(0),
    )?;
    Ok(directory_id)
}

fn insert_missing_items(
    conn: &mut Connection,
    opts: &Options,
    directory_id: i64,
    existing_count: i64,
    now: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let to_insert = opts.target - existing_count;
    println!("开始新增 mock 条目: {to_insert}");

    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO media_items (
                directory_id, file_name, file_size, file_mtime, file_format,
                media_type, width, height, sort_datetime, cache_key, thumb_status,
                is_favorited, is_deleted, rating, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, 'jpg', 'image', ?5, ?6, ?7, ?8, ?9, 0, 0, 0, ?10, ?10
            )",
        )?;

        let start_time = now - 100_000_000;
        for i in 0..to_insert {
            let seq = existing_count + i;
            let filename = format!("mock_stress_{seq:07}.jpg");
            let sort_time = start_time + seq;
            let (width, height) = match seq % 6 {
                0 => (1080, 1920),
                1 | 2 => (1920, 1080),
                3 => (1920, 1200),
                4 => (3000, 2000),
                _ => (1200, 1200),
            };

            stmt.execute(params![
                directory_id,
                filename,
                opts.file_size,
                sort_time,
                width,
                height,
                sort_time,
                sort_time,
                opts.thumb_status.as_db_value(),
                now
            ])?;

            if i > 0 && i % 50_000 == 0 {
                println!("已新增 {i} 条");
            }
        }
    }
    tx.commit()?;

    Ok(())
}
