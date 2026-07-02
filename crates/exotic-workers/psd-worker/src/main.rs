// crates/exotic-workers/psd-worker/src/main.rs
//! PSD 缩略图 Worker 主循环（v3 Part2 §3.4-3.5）。
//!
//! 契约：
//!   - **stdout 只走协议帧**；任何日志只写 stderr（Host 视 stdout 前导非帧字节为协议损坏）。
//!   - stdin 收 Hello → 回 Ready（声明 probe 通过范围）→ 循环收 Request/Shutdown。
//!   - stdin EOF / Shutdown / Host 消失 → 立即退出。
//!   - 每任务顶层 `catch_unwind`：panic 后回 `internal_error` 并**主动退出进程**，不带病继续服务。

mod decode;

use std::io::{BufReader, BufWriter, Write};

use exotic_protocol::{
    read_frame, write_frame, FailureBody, Frame, FrameType, ProtocolError, ReadyBody, RequestBody,
    SuccessBody, WorkerErrorCode, MAX_BLOB_LEN, PROTOCOL_VERSION,
};

/// Worker 稳定标识（Host 握手校验）。
const WORKER_ID: &str = "psd-worker";
/// Worker 版本（进入指纹/展示/兼容，R11）。
const WORKER_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 源文件字节上限：读盘前用 metadata 拦截，避免巨文件吃满内存（→ resource_limit）。
const MAX_SOURCE_FILE_BYTES: u64 = 512 << 20;

fn log(msg: &str) {
    eprintln!("[psd-worker] {msg}");
}

fn main() {
    // 锁定 stdin/stdout 原始字节流。BufWriter 后每帧 flush，保证 Host 及时收到。
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    // ── 握手：等 Hello → 回 Ready ────────────────────────────────────────────────
    match read_frame(&mut reader) {
        Ok(f) if f.frame_type == FrameType::Hello => {
            // 解析 Hello 仅为记录；协议版本不一致由 Host 在收到 Ready 后判定（本端如实声明自己的版本）。
            if let Ok(hello) = f.parse_json::<exotic_protocol::HelloBody>() {
                if hello.protocol_version != PROTOCOL_VERSION {
                    log(&format!(
                        "Hello 协议版本 {} != 本端 {}（仍回 Ready，由 Host 决定）",
                        hello.protocol_version, PROTOCOL_VERSION
                    ));
                }
            }
        }
        Ok(f) => {
            log(&format!("握手期望 Hello，收到 {:?} → 退出", f.frame_type));
            std::process::exit(2);
        }
        Err(e) if e.is_clean_eof() => std::process::exit(0),
        Err(e) => {
            log(&format!("握手读取失败：{e} → 退出"));
            std::process::exit(2);
        }
    }

    let ready = ReadyBody {
        worker_id: WORKER_ID.to_string(),
        worker_version: WORKER_VERSION.to_string(),
        protocol_version: PROTOCOL_VERSION,
        capabilities: vec!["thumbnail".to_string()],
        max_blob_len: MAX_BLOB_LEN,
    };
    if let Err(e) = send(
        &mut writer,
        &Frame::control(FrameType::Ready, 0, &ready).unwrap(),
    ) {
        log(&format!("发送 Ready 失败：{e} → 退出"));
        std::process::exit(2);
    }

    // ── 主循环：每帧一个请求 ─────────────────────────────────────────────────────
    loop {
        let frame = match read_frame(&mut reader) {
            Ok(f) => f,
            // stdin EOF（Host 关闭管道/消失）→ 正常退出。
            Err(e) if e.is_clean_eof() => {
                log("stdin EOF → 退出");
                std::process::exit(0);
            }
            Err(e) => {
                log(&format!("读取帧失败（协议损坏）：{e} → 退出"));
                std::process::exit(3);
            }
        };

        match frame.frame_type {
            FrameType::Shutdown => {
                log("收到 Shutdown → 退出");
                std::process::exit(0);
            }
            FrameType::Request => {
                // 每任务顶层 catch_unwind：panic → 回 internal_error 后主动退出进程（§3.5）。
                let req_id = frame.request_id;
                let handled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handle_request(&frame)
                }));
                match handled {
                    Ok(out) => {
                        if send(&mut writer, &out).is_err() {
                            std::process::exit(0); // Host 消失
                        }
                    }
                    Err(_) => {
                        // 已知 item_id/fingerprint 才能回 Failure；panic 时尽力解析请求体。
                        let (item_id, fp) = frame
                            .parse_json::<RequestBody>()
                            .map(|r| (r.item_id(), r.input_fingerprint().to_string()))
                            .unwrap_or((0, String::new()));
                        let fail = FailureBody {
                            item_id,
                            input_fingerprint: fp,
                            code: WorkerErrorCode::InternalError,
                            retryable: true,
                            message: "worker panic".to_string(),
                        };
                        let _ = send(
                            &mut writer,
                            &Frame::control(FrameType::Failure, req_id, &fail).unwrap(),
                        );
                        log("任务 panic → 已回 internal_error，主动退出进程");
                        std::process::exit(4);
                    }
                }
            }
            other => {
                log(&format!("意外帧类型 {other:?} → 忽略"));
            }
        }
    }
}

/// 处理一个 Request 帧 → 返回应发送的 Success/Failure 帧。
fn handle_request(frame: &Frame) -> Frame {
    let req: RequestBody = match frame.parse_json() {
        Ok(r) => r,
        Err(e) => {
            // 请求体都解析不了：无法可靠取 item_id；回 internal_error（request_id 仍匹配）。
            log(&format!("Request JSON 解析失败：{e}"));
            let fail = FailureBody {
                item_id: 0,
                input_fingerprint: String::new(),
                code: WorkerErrorCode::InternalError,
                retryable: false,
                message: "bad request json".to_string(),
            };
            return Frame::control(FrameType::Failure, frame.request_id, &fail).unwrap();
        }
    };

    match req {
        RequestBody::Thumbnail {
            item_id,
            source_path,
            target_long_edge,
            input_fingerprint,
        } => handle_thumbnail(
            frame.request_id,
            item_id,
            &source_path,
            target_long_edge,
            input_fingerprint,
        ),
        RequestBody::Metadata {
            item_id,
            input_fingerprint,
            ..
        } => {
            // 首发不实现 metadata 能力 → 稳定 unsupported_variant。
            let fail = FailureBody {
                item_id,
                input_fingerprint,
                code: WorkerErrorCode::UnsupportedVariant,
                retryable: false,
                message: "metadata 能力未实现".to_string(),
            };
            Frame::control(FrameType::Failure, frame.request_id, &fail).unwrap()
        }
    }
}

fn handle_thumbnail(
    request_id: u64,
    item_id: i64,
    source_path: &str,
    target_long_edge: u32,
    input_fingerprint: String,
) -> Frame {
    let fail = |code: WorkerErrorCode, retryable: bool, message: String| {
        Frame::control(
            FrameType::Failure,
            request_id,
            &FailureBody {
                item_id,
                input_fingerprint: input_fingerprint.clone(),
                code,
                retryable,
                message,
            },
        )
        .unwrap()
    };

    // 读盘前先看大小，拦截巨文件（→ resource_limit）。
    match std::fs::metadata(source_path) {
        Ok(m) if m.len() > MAX_SOURCE_FILE_BYTES => {
            return fail(
                WorkerErrorCode::ResourceLimit,
                false,
                format!("源文件过大：{} 字节", m.len()),
            );
        }
        Ok(_) => {}
        Err(e) => {
            // 文件不存在/占用：IO 错误 → retryable。
            return fail(
                WorkerErrorCode::IoError,
                true,
                format!("stat 失败：{}", e.kind()),
            );
        }
    }

    let bytes = match std::fs::read(source_path) {
        Ok(b) => b,
        Err(e) => {
            return fail(
                WorkerErrorCode::IoError,
                true,
                format!("读取失败：{}", e.kind()),
            )
        }
    };

    match decode::decode_psd_to_webp(&bytes, target_long_edge) {
        Ok(out) => {
            let body = SuccessBody {
                item_id,
                input_fingerprint: input_fingerprint.clone(),
                mime: Some("image/webp".to_string()),
                width: Some(out.width),
                height: Some(out.height),
                metadata: None,
            };
            Frame::with_blob(FrameType::Success, request_id, &body, out.webp).unwrap()
        }
        Err(e) => fail(e.code, e.code.default_retryable(), e.message),
    }
}

/// 写一帧并 flush（保证 Host 立即可读）。
fn send<W: Write>(w: &mut W, frame: &Frame) -> Result<(), ProtocolError> {
    write_frame(w, frame)?;
    w.flush()?;
    Ok(())
}
