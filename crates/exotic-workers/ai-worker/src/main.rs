// crates/exotic-workers/ai-worker/src/main.rs
//! AI 推理 Worker 主循环(Part4-T15;合并单 worker:CLIP 嵌入 + 人脸检测/嵌入,
//! T9.5 VRAM 实测支持合并,正式拍板随 T20)。
//!
//! 契约(承接 psd-worker 样板 + v2 会话族 op):
//!   - **stdout 只走协议帧**;日志只写 stderr。
//!   - 进程握手快而恒定(Hello→Ready,5s 档):**不承载模型加载**(D3 §2)。模型加载 =
//!     显式 SessionInit 请求(host 侧对其配 300s 档),SessionReady = 其 Success 应答。
//!   - host 主导会话生命周期(SessionClose 显式卸载);本端兜底**空闲自杀 timer**:
//!     收到最后一帧后 300s 无活动 `exit(0)`,防 host 失联留 VRAM 僵尸(D3 §4④——
//!     读线程 + channel `recv_timeout` 实现,阻塞式 stdin 读无法带超时)。
//!   - 严格串行:一次一请求(host Supervisor 不变量),无并发状态。
//!   - 每请求顶层 `catch_unwind`:panic 回 internal_error 并主动退出,不带病服务。

mod batch;
mod session;

use std::io::{BufReader, BufWriter, Write};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use exotic_protocol::{
    capability, read_frame, write_frame, FailureBody, Frame, FrameType, ProtocolError, ReadyBody,
    RequestBody, SessionReadyBody, SuccessBody, WorkerErrorCode, MAX_BLOB_LEN, PROTOCOL_VERSION,
};

use session::SessionState;

/// Worker 稳定标识(Host 握手校验;installer manifest 的 worker_id 与此一致)。
const WORKER_ID: &str = "ai-worker";
/// Worker 版本(进指纹/展示/升级失效,R11)。
const WORKER_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 空闲自杀阈值:最后一帧后无活动即退出(D3 §4④ 兜底,host 失联不留 VRAM 僵尸)。
/// 计时仅覆盖「等下一帧」——在途推理(SessionInit 可达分钟级)不在等待态,不会误杀。
const IDLE_SELF_EXIT: Duration = Duration::from_secs(300);

fn log(msg: &str) {
    eprintln!("[ai-worker] {msg}");
}

type FrameResult = Result<Frame, ProtocolError>;

fn main() {
    let stdout = std::io::stdout();
    let mut writer = BufWriter::new(stdout.lock());

    // stdin → 帧 channel 的读线程:主循环由此获得 recv_timeout(空闲自杀的实现前提)。
    let (tx, rx) = std::sync::mpsc::channel::<FrameResult>();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(std::io::stdin().lock());
        loop {
            match read_frame(&mut reader) {
                Ok(f) => {
                    if tx.send(Ok(f)).is_err() {
                        break; // 主线程已退出
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    break; // 错误/EOF 即终止读线程
                }
            }
        }
    });

    // ── 握手:等 Hello → 回 Ready(空闲上限同样约束握手:host 失联即自杀)──────────────
    match recv_frame(&rx) {
        Some(f) if f.frame_type == FrameType::Hello => {
            if let Ok(hello) = f.parse_json::<exotic_protocol::HelloBody>() {
                if hello.protocol_version != PROTOCOL_VERSION {
                    log(&format!(
                        "Hello 协议版本 {} != 本端 {}(仍回 Ready,由 Host 决定)",
                        hello.protocol_version, PROTOCOL_VERSION
                    ));
                }
            }
        }
        Some(f) => {
            log(&format!("握手期望 Hello,收到 {:?} → 退出", f.frame_type));
            std::process::exit(2);
        }
        None => std::process::exit(0), // EOF/超时/损坏,recv_frame 已写日志
    }

    let ready = ReadyBody {
        worker_id: WORKER_ID.to_string(),
        worker_version: WORKER_VERSION.to_string(),
        protocol_version: PROTOCOL_VERSION,
        capabilities: vec![
            capability::EMBEDDING.to_string(),
            capability::FACE_DETECT_EMBED.to_string(),
        ],
        max_blob_len: MAX_BLOB_LEN,
    };
    if let Err(e) = send(
        &mut writer,
        &Frame::control(FrameType::Ready, 0, &ready).unwrap(),
    ) {
        log(&format!("发送 Ready 失败:{e} → 退出"));
        std::process::exit(2);
    }

    // ── 主循环:严格串行,一帧一请求 ─────────────────────────────────────────────────
    let mut sess: Option<SessionState> = None;
    loop {
        let Some(frame) = recv_frame(&rx) else {
            std::process::exit(0);
        };
        match frame.frame_type {
            FrameType::Shutdown => {
                log("收到 Shutdown → 退出(会话随进程释放)");
                std::process::exit(0);
            }
            FrameType::Request => {
                let req_id = frame.request_id;
                // 每请求顶层 catch_unwind:panic → 回 internal_error 后主动退出(§3.5)。
                let handled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handle_request(&frame, &mut sess)
                }));
                match handled {
                    Ok(out) => {
                        if send(&mut writer, &out).is_err() {
                            std::process::exit(0); // Host 消失
                        }
                    }
                    Err(_) => {
                        let fail = FailureBody {
                            item_id: None,
                            input_fingerprint: None,
                            code: WorkerErrorCode::InternalError,
                            retryable: true,
                            message: "worker panic".to_string(),
                        };
                        let _ = send(
                            &mut writer,
                            &Frame::control(FrameType::Failure, req_id, &fail).unwrap(),
                        );
                        log("请求 panic → 已回 internal_error,主动退出进程");
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

/// 从帧 channel 取下一帧;EOF/损坏/空闲超时/读线程消失均返回 None(日志已写,调用方退出)。
/// 协议损坏用非零码退出使 host 可诊断,其余路径正常退出。
fn recv_frame(rx: &Receiver<FrameResult>) -> Option<Frame> {
    match rx.recv_timeout(IDLE_SELF_EXIT) {
        Ok(Ok(f)) => Some(f),
        Ok(Err(e)) if e.is_clean_eof() => {
            log("stdin EOF → 退出");
            None
        }
        Ok(Err(e)) => {
            log(&format!("读取帧失败(协议损坏):{e} → 退出"));
            std::process::exit(3);
        }
        Err(RecvTimeoutError::Timeout) => {
            log(&format!(
                "空闲 {}s 无帧 → 自杀兜底(host 失联不留 VRAM 僵尸,D3 §4)",
                IDLE_SELF_EXIT.as_secs()
            ));
            None
        }
        Err(RecvTimeoutError::Disconnected) => {
            log("读线程已终止 → 退出");
            None
        }
    }
}

/// 处理一个 Request 帧 → 返回应发送的 Success/Failure 帧。会话状态经 `sess` 串行流转。
fn handle_request(frame: &Frame, sess: &mut Option<SessionState>) -> Frame {
    let req: RequestBody = match frame.parse_json() {
        Ok(r) => r,
        Err(e) => {
            log(&format!("Request JSON 解析失败:{e}"));
            let fail = FailureBody {
                item_id: None,
                input_fingerprint: None,
                code: WorkerErrorCode::InternalError,
                retryable: false,
                message: "bad request json".to_string(),
            };
            return Frame::control(FrameType::Failure, frame.request_id, &fail).unwrap();
        }
    };

    match req {
        RequestBody::SessionInit {
            session_id,
            models,
            model_profile,
            models_root,
            ai_cache_dir,
            image_provider,
        } => {
            // host 主导切换语义(先 Close 再 Init);未 Close 即 Init 按切换处理,旧会话先卸。
            if let Some(old) = sess.take() {
                log(&format!(
                    "SessionInit 前存在旧会话 {} → 先卸载(切换语义)",
                    old.session_id
                ));
                drop(old);
            }
            let init = session::validate_and_resolve(
                session_id,
                &models,
                &model_profile,
                &models_root,
                &ai_cache_dir,
                &image_provider,
            )
            .and_then(session::load);
            match init {
                Ok(state) => {
                    let body = SuccessBody {
                        session: Some(SessionReadyBody {
                            embed_dim: state.profile.embed_dim as u32,
                            face_embed_dim: state
                                .face_profile
                                .as_ref()
                                .map(|fp| fp.embed_dim as u32),
                            caps: session_caps(&state),
                            // provider 回声(T16 additive):探测/回退结果只有本端知道,
                            // host 借此写回 ai_provider/ai_gpu_name 配置(状态栏显示)。
                            provider: Some(state.pool.provider.as_str().to_string()),
                            gpu_name: Some(state.pool.gpu_name.clone()),
                        }),
                        ..Default::default()
                    };
                    log(&format!(
                        "会话 {} 就绪:arch={} face={:?} provider={}",
                        session_id,
                        state.profile.id,
                        state.face_profile.as_ref().map(|f| f.id.clone()),
                        state.pool.provider.as_str()
                    ));
                    *sess = Some(state);
                    Frame::control(FrameType::Success, frame.request_id, &body).unwrap()
                }
                Err((code, message)) => {
                    log(&format!("SessionInit 失败[{}]:{message}", code.as_str()));
                    let fail = FailureBody {
                        item_id: None,
                        input_fingerprint: None,
                        code,
                        retryable: code.default_retryable(),
                        message,
                    };
                    Frame::control(FrameType::Failure, frame.request_id, &fail).unwrap()
                }
            }
        }
        RequestBody::SessionClose { session_id } => {
            // 幂等:无会话/错 id 也回 Success(host 只关心「之后没有会话」这一后置条件)。
            match sess.take() {
                Some(s) if s.session_id == session_id => {
                    log(&format!("会话 {session_id} 已卸载"));
                }
                Some(s) => {
                    log(&format!(
                        "SessionClose id 不符:{} != 当前 {} → 仍卸载当前会话",
                        session_id, s.session_id
                    ));
                }
                None => log(&format!("SessionClose {session_id}:无在载会话(幂等)")),
            }
            Frame::control(
                FrameType::Success,
                frame.request_id,
                &SuccessBody::default(),
            )
            .unwrap()
        }
        RequestBody::EmbedBatch { items } => match sess.as_ref() {
            Some(s) => batch::handle_embed(s, frame.request_id, &items),
            None => session_expired(frame.request_id),
        },
        RequestBody::FaceDetectEmbed {
            items,
            det_score_thresh,
        } => match sess.as_ref() {
            Some(s) => batch::handle_face(s, frame.request_id, &items, det_score_thresh),
            None => session_expired(frame.request_id),
        },
        RequestBody::EncodeText { texts } => match sess.as_ref() {
            Some(s) => batch::handle_encode_text(s, frame.request_id, &texts),
            None => session_expired(frame.request_id),
        },
        // 本 worker 不做缩略图/元数据;host 按能力路由不会派发,防御性兜底稳定错误码。
        other @ (RequestBody::Thumbnail { .. } | RequestBody::Metadata { .. }) => {
            let fail = FailureBody {
                item_id: other.item_id(),
                input_fingerprint: other.input_fingerprint().map(str::to_string),
                code: WorkerErrorCode::UnsupportedVariant,
                retryable: false,
                message: "该 op 未实现(ai-worker 仅 embedding/face_detect_embed)".to_string(),
            };
            Frame::control(FrameType::Failure, frame.request_id, &fail).unwrap()
        }
    }
}

/// 会话未加载 → SessionExpired(retryable:host 重发 SessionInit 后重派,G6)。
fn session_expired(request_id: u64) -> Frame {
    let fail = FailureBody {
        item_id: None,
        input_fingerprint: None,
        code: WorkerErrorCode::SessionExpired,
        retryable: true,
        message: "会话未加载或已卸载".to_string(),
    };
    Frame::control(FrameType::Failure, request_id, &fail).unwrap()
}

/// 本会话实际可服务的能力(host 据此派活;与 Ready.capabilities 的「静态支持范围」区分)。
fn session_caps(state: &SessionState) -> Vec<String> {
    let mut caps = vec![capability::EMBEDDING.to_string()];
    if state.face_profile.is_some() {
        caps.push(capability::FACE_DETECT_EMBED.to_string());
    }
    caps
}

/// 写一帧并 flush(保证 Host 立即可读)。
fn send<W: Write>(w: &mut W, frame: &Frame) -> Result<(), ProtocolError> {
    write_frame(w, frame)?;
    w.flush()?;
    Ok(())
}
