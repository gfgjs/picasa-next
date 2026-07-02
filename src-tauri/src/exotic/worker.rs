// src-tauri/src/exotic/worker.rs
//! 冷门格式插件 · Worker 进程规格、子进程创建与「传输无关」的连接抽象（v3 Part2 §3.4-3.7）。
//!
//! 分层（为可测性）：
//!   - [`WorkerSpec`] / [`spawn_worker_process`]：定位 + 以**低优先级、隐藏窗口、管道 stdio** 创建子进程。
//!     低优先级是 exotic 让步阶梯的 OS 软让步底层手段（R1：主进程线程 sleep 无法令子进程让出 CPU）。
//!   - [`WorkerConn`]：**只依赖 `Write` + frame `Receiver`** 的协议状态机（握手 / run_task / 输出验证）。
//!     不持 `Child`，因此可用内存管道 + mock worker 线程做确定性单测（协议/恶意 Worker/超时/错序）。
//!   - 真正的进程生命周期（kill/wait/join 线程）在 [`super::supervisor`]。
//!
//! Host **不信任** Worker 返回值（§3.7）：用独立解码器验证 WebP 实际尺寸、声明与实际一致、像素上限，
//! 并核对 request_id / item_id / fingerprint。任一不符 → terminal `invalid_worker_output`，丢弃 blob。

use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use exotic_protocol::{
    read_frame, write_frame, FailureBody, Frame, FrameType, HelloBody, ReadyBody, RequestBody,
    SuccessBody,
};

/// 在途任务取消轮询周期：`run_thumbnail` 等待响应期间每隔此间隔检查取消标志，
/// 使 stop/App 退出能及时让 Supervisor kill 在途 Worker（v3.1 §4.1）。
const CANCEL_POLL: Duration = Duration::from_millis(100);

/// 定位 PSD Worker 可执行文件（**仅测试**）。
///
/// Part2（dev/test）旧入口：经环境变量 `EXOTIC_PSD_WORKER_PATH` 注入已构建的 worker 二进制路径、**不验签**。
/// 生产路径已由 [`crate::exotic::installer::resolve_worker_path`]（验签 + hash 复核，§3.6）替代；coordinator 只调后者。
/// 🔒 本函数整体 `#[cfg(test)]`：Release/普通 debug app 构建中**不存在**，杜绝经环境变量加载未验签 worker
/// 的信任链击穿（SEC-02，对齐 D8「Release 不得有验签旁路」红线）。仅 cargo test 的真实 worker 冒烟用例编入。
#[cfg(test)]
pub fn resolve_psd_worker_path() -> Option<PathBuf> {
    std::env::var_os("EXOTIC_PSD_WORKER_PATH").map(PathBuf::from)
}

/// Worker 进程规格 + Host 对其的期望（握手校验）。
#[derive(Debug, Clone)]
pub struct WorkerSpec {
    /// 可执行文件路径。
    pub exe_path: PathBuf,
    /// 期望的 worker_id（握手时 ReadyBody.worker_id 必须匹配）。
    pub expected_worker_id: String,
    /// Host 需要的能力（握手时 ReadyBody.capabilities 必须包含全部）。
    pub required_capabilities: Vec<String>,
}

/// Supervisor/Conn 配置常量。
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// 握手超时。
    pub handshake_timeout: Duration,
    /// Host 语义版本（写入 Hello）。
    pub host_version: String,
    /// Host 能接收的最大 blob（写入 Hello；与协议 MAX_BLOB_LEN 取小）。
    pub max_blob_len: u32,
}

/// Host 对缩略图输出的硬上限（§3.7）。
#[derive(Debug, Clone)]
pub struct WorkerLimits {
    /// blob 字节上限。
    pub max_blob_len: u32,
    /// 输出总像素上限。
    pub max_output_pixels: u64,
    /// 请求档位之上允许的长边误差（缩放取整/比例换算的容差）。
    pub long_edge_tolerance: u32,
}

/// 一次任务的结果。`Success` 已通过 Host 全部验证。
pub enum TaskOutcome {
    /// 验证通过的缩略图。
    Success {
        width: u32,
        height: u32,
        mime: String,
        blob: Vec<u8>,
    },
    /// Worker 显式失败（已核对 item/fingerprint）。
    Failure(FailureBody),
    /// 超时（Supervisor 应 kill）。
    TimedOut,
    /// 连接断开 / Worker 退出（Supervisor 应 wait 回收）。
    Disconnected,
    /// 协议违例或输出非法（错序 / 错 id / 非法 WebP / 尺寸不符）→ terminal invalid_worker_output。
    Protocol(String),
}

/// 以低优先级、隐藏窗口、管道 stdio 创建 Worker 子进程（§3.6 / R1）。
pub fn spawn_worker_process(spec: &WorkerSpec) -> std::io::Result<Child> {
    let mut cmd = Command::new(&spec.exe_path);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_low_priority(&mut cmd);
    cmd.spawn()
}

/// 平台相关的低优先级 + 隐藏窗口设置。**始终生效**的 OS 软让步（R1 第 1 层）。
#[cfg(windows)]
fn apply_low_priority(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    // BELOW_NORMAL_PRIORITY_CLASS(0x4000)：低于普通优先级；CREATE_NO_WINDOW(0x0800_0000)：无控制台窗口。
    const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(BELOW_NORMAL_PRIORITY_CLASS | CREATE_NO_WINDOW);
}

/// macOS/Linux：Part2 在 Windows 落地与验证；此处记录降级（未降优先级），
/// 由 Part4 跨平台发布按实测接入 `nice`/QoS utility（R1 要求记录创建失败与降级行为）。
#[cfg(not(windows))]
fn apply_low_priority(_cmd: &mut Command) {
    tracing::debug!(
        "非 Windows：Worker 低优先级未接入（Part4 实测 nice/QoS）；本次以普通优先级创建"
    );
}

/// 启动一个把 `r` 的协议帧持续读入 `tx` 的线程（Supervisor 与测试共用）。
/// 读到错误（含干净 EOF）后发送该错误并结束——下游据此判定断开/违例。
pub fn spawn_frame_reader<R: Read + Send + 'static>(
    r: R,
    tx: Sender<Result<Frame, exotic_protocol::ProtocolError>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(r);
        loop {
            match read_frame(&mut reader) {
                Ok(f) => {
                    if tx.send(Ok(f)).is_err() {
                        break; // 下游已走
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    break; // 读到错误/EOF 即终止本线程
                }
            }
        }
    })
}

/// 「传输无关」的 Worker 连接：握手后用 `run_thumbnail` 跑任务。不持 `Child`，便于单测。
pub struct WorkerConn {
    writer: Box<dyn Write + Send>,
    rx: Receiver<Result<Frame, exotic_protocol::ProtocolError>>,
    ready: ReadyBody,
    next_request_id: u64,
}

impl WorkerConn {
    /// 握手：写 Hello → 等 Ready（带超时）→ 校验 worker_id/protocol/capabilities。
    pub fn handshake(
        mut writer: Box<dyn Write + Send>,
        rx: Receiver<Result<Frame, exotic_protocol::ProtocolError>>,
        spec: &WorkerSpec,
        cfg: &WorkerConfig,
    ) -> Result<WorkerConn, String> {
        let hello = HelloBody {
            host_version: cfg.host_version.clone(),
            protocol_version: exotic_protocol::PROTOCOL_VERSION,
            max_blob_len: cfg.max_blob_len.min(exotic_protocol::MAX_BLOB_LEN),
        };
        let frame = Frame::control(FrameType::Hello, 0, &hello).map_err(|e| e.to_string())?;
        write_frame(&mut writer, &frame).map_err(|e| format!("写 Hello 失败：{e}"))?;
        writer
            .flush()
            .map_err(|e| format!("flush Hello 失败：{e}"))?;

        let ready_frame = match rx.recv_timeout(cfg.handshake_timeout) {
            Ok(Ok(f)) => f,
            Ok(Err(e)) => return Err(format!("握手读取失败：{e}")),
            Err(RecvTimeoutError::Timeout) => return Err("握手超时（未收到 Ready）".into()),
            Err(RecvTimeoutError::Disconnected) => return Err("握手时连接断开".into()),
        };
        if ready_frame.frame_type != FrameType::Ready {
            return Err(format!("握手期望 Ready，收到 {:?}", ready_frame.frame_type));
        }
        let ready: ReadyBody = ready_frame
            .parse_json()
            .map_err(|e| format!("Ready 解析失败：{e}"))?;

        if ready.protocol_version != exotic_protocol::PROTOCOL_VERSION {
            return Err(format!(
                "协议版本不兼容：worker {} != host {}",
                ready.protocol_version,
                exotic_protocol::PROTOCOL_VERSION
            ));
        }
        if ready.worker_id != spec.expected_worker_id {
            return Err(format!(
                "worker_id 不符：{} != {}",
                ready.worker_id, spec.expected_worker_id
            ));
        }
        for need in &spec.required_capabilities {
            if !ready.capabilities.iter().any(|c| c == need) {
                return Err(format!("缺少能力：{need}"));
            }
        }

        Ok(WorkerConn {
            writer,
            rx,
            ready,
            next_request_id: 1,
        })
    }

    /// 测试用：以现成部件构造连接（跳过握手）。
    #[cfg(test)]
    pub fn from_parts(
        writer: Box<dyn Write + Send>,
        rx: Receiver<Result<Frame, exotic_protocol::ProtocolError>>,
        ready: ReadyBody,
    ) -> WorkerConn {
        WorkerConn {
            writer,
            rx,
            ready,
            next_request_id: 1,
        }
    }

    pub fn worker_version(&self) -> &str {
        &self.ready.worker_version
    }

    /// 分配单调递增 request_id。
    fn alloc_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }

    /// 发送一个缩略图请求并等待响应，验证后返回 [`TaskOutcome`]。
    /// `req` 必须是 `RequestBody::Thumbnail`；`target_long_edge` 须为吸附后档位（R5）。
    /// `cancelled` 在等待期间被周期轮询；返回 true 即放弃等待返回 `Disconnected`（上层 kill 在途 Worker）。
    pub fn run_thumbnail(
        &mut self,
        req: &RequestBody,
        limits: &WorkerLimits,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> TaskOutcome {
        let request_id = self.alloc_request_id();
        let frame = match Frame::control(FrameType::Request, request_id, req) {
            Ok(f) => f,
            Err(e) => return TaskOutcome::Protocol(format!("构造 Request 失败：{e}")),
        };
        if write_frame(&mut self.writer, &frame).is_err() || self.writer.flush().is_err() {
            return TaskOutcome::Disconnected;
        }

        // 可取消等待：每 CANCEL_POLL 检查一次取消标志，使 stop/App 退出能及时让 Supervisor kill
        // 在途 Worker（v3.1 §4.1：停止按取消协议终止在途，不等其自然完成；返回 Disconnected → kill）。
        let deadline = Instant::now() + timeout;
        let resp = loop {
            if cancelled() {
                return TaskOutcome::Disconnected;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return TaskOutcome::TimedOut;
            }
            match self.rx.recv_timeout(remaining.min(CANCEL_POLL)) {
                Ok(Ok(f)) => break f,
                Ok(Err(e)) => {
                    return if e.is_clean_eof() {
                        TaskOutcome::Disconnected
                    } else {
                        // Worker 发出损坏帧 → 协议违例（Supervisor 会 kill）。
                        TaskOutcome::Protocol(format!("响应帧损坏：{e}"))
                    };
                }
                Err(RecvTimeoutError::Timeout) => continue, // 轮询：再查取消 / 总超时
                Err(RecvTimeoutError::Disconnected) => return TaskOutcome::Disconnected,
            }
        };

        // request_id 必须匹配当前在途请求（每 Supervisor 同时只一个请求）。
        if resp.request_id != request_id {
            return TaskOutcome::Protocol(format!(
                "request_id 错配：{} != {}",
                resp.request_id, request_id
            ));
        }

        match resp.frame_type {
            FrameType::Success => {
                let body: SuccessBody = match resp.parse_json() {
                    Ok(b) => b,
                    Err(e) => return TaskOutcome::Protocol(format!("Success 解析失败：{e}")),
                };
                match validate_thumbnail_output(req, &body, &resp.blob, limits) {
                    Ok((w, h, mime)) => TaskOutcome::Success {
                        width: w,
                        height: h,
                        mime,
                        blob: resp.blob,
                    },
                    Err(reason) => TaskOutcome::Protocol(reason),
                }
            }
            FrameType::Failure => {
                let body: FailureBody = match resp.parse_json() {
                    Ok(b) => b,
                    Err(e) => return TaskOutcome::Protocol(format!("Failure 解析失败：{e}")),
                };
                // 即便是失败响应也要核对 id/fingerprint，防错序串扰。
                if body.item_id != req.item_id()
                    || body.input_fingerprint != req.input_fingerprint()
                {
                    return TaskOutcome::Protocol("Failure 的 item/fingerprint 错配".into());
                }
                TaskOutcome::Failure(body)
            }
            other => TaskOutcome::Protocol(format!("意外帧类型：{other:?}")),
        }
    }

    /// 尽力发送 Shutdown（关闭流程用；失败忽略）。
    pub fn send_shutdown(&mut self) {
        if let Ok(f) = Frame::control(FrameType::Shutdown, 0, &serde_json::json!({})) {
            let _ = write_frame(&mut self.writer, &f);
            let _ = self.writer.flush();
        }
    }
}

/// 验证缩略图 Success（§3.7）：core/request 核对 + 独立解码器验真尺寸 + 上限。返回 (w,h,mime)。
///
/// 用 `image` crate 解码 WebP（独立于 Worker 声明）得到**真实**尺寸——既验证 WebP 自洽，
/// 又拿到与声明对照的实际宽高（Worker 声明不可信）。缩略图体积小，解码开销可忽略。
pub fn validate_thumbnail_output(
    req: &RequestBody,
    body: &SuccessBody,
    blob: &[u8],
    limits: &WorkerLimits,
) -> Result<(u32, u32, String), String> {
    // 核对 item / fingerprint（防错序串扰）。
    if body.item_id != req.item_id() {
        return Err(format!(
            "item_id 错配：{} != {}",
            body.item_id,
            req.item_id()
        ));
    }
    if body.input_fingerprint != req.input_fingerprint() {
        return Err("fingerprint 错配".into());
    }
    // mime 必须 image/webp。
    let mime = body.mime.clone().unwrap_or_default();
    if mime != "image/webp" {
        return Err(format!("mime 非 image/webp：{mime}"));
    }
    // blob 非空且不超上限。
    if blob.is_empty() {
        return Err("blob 为空".into());
    }
    if blob.len() as u64 > limits.max_blob_len as u64 {
        return Err(format!(
            "blob 超限：{} > {}",
            blob.len(),
            limits.max_blob_len
        ));
    }
    // WebP 魔数（RIFF....WEBP）。
    if blob.len() < 12 || &blob[0..4] != b"RIFF" || &blob[8..12] != b"WEBP" {
        return Err("WebP 魔数非法".into());
    }
    // 独立解码取真实尺寸（同时验证 WebP 自洽）。
    let img = image::load_from_memory_with_format(blob, image::ImageFormat::WebP)
        .map_err(|e| format!("WebP 独立解码失败：{e}"))?;
    use image::GenericImageView;
    let (aw, ah) = img.dimensions();
    // 声明尺寸（若有）必须与实际一致。
    if let Some(dw) = body.width {
        if dw != aw {
            return Err(format!("声明宽 {dw} != 实际 {aw}"));
        }
    }
    if let Some(dh) = body.height {
        if dh != ah {
            return Err(format!("声明高 {dh} != 实际 {ah}"));
        }
    }
    // 长边不超过请求档位 + 容差。
    if let RequestBody::Thumbnail {
        target_long_edge, ..
    } = req
    {
        let long = aw.max(ah);
        if long > target_long_edge.saturating_add(limits.long_edge_tolerance) {
            return Err(format!(
                "长边 {long} 超过档位 {target_long_edge}+容差 {}",
                limits.long_edge_tolerance
            ));
        }
    }
    // 总像素不超上限。
    let pixels = (aw as u64).saturating_mul(ah as u64);
    if pixels > limits.max_output_pixels {
        return Err(format!("像素 {pixels} 超上限 {}", limits.max_output_pixels));
    }
    Ok((aw, ah, mime))
}

/// 默认缩略图上限：64 MiB blob、4 兆像素（足够 960 档）、64px 长边容差。
pub fn default_thumbnail_limits() -> WorkerLimits {
    WorkerLimits {
        max_blob_len: exotic_protocol::MAX_BLOB_LEN,
        max_output_pixels: 4_000_000,
        long_edge_tolerance: 64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exotic_protocol::{WorkerErrorCode, PROTOCOL_VERSION};
    use std::io::Cursor;

    fn thumb_req(item_id: i64, fp: &str, tier: u32) -> RequestBody {
        RequestBody::Thumbnail {
            item_id,
            source_path: "x.psd".into(),
            target_long_edge: tier,
            input_fingerprint: fp.into(),
        }
    }

    /// 生成一张真实 WebP（用 image crate 编码一张纯色图）。
    fn make_webp(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        let mut buf = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(Cursor::new(&mut buf))
            .encode(img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
            .unwrap();
        buf
    }

    fn limits() -> WorkerLimits {
        default_thumbnail_limits()
    }

    #[test]
    fn validate_accepts_good_output() {
        let req = thumb_req(7, "fp", 480);
        let webp = make_webp(480, 240);
        let body = SuccessBody {
            item_id: 7,
            input_fingerprint: "fp".into(),
            mime: Some("image/webp".into()),
            width: Some(480),
            height: Some(240),
            metadata: None,
        };
        let (w, h, mime) = validate_thumbnail_output(&req, &body, &webp, &limits()).unwrap();
        assert_eq!((w, h), (480, 240));
        assert_eq!(mime, "image/webp");
    }

    #[test]
    fn validate_rejects_item_id_mismatch() {
        let req = thumb_req(7, "fp", 480);
        let webp = make_webp(100, 100);
        let body = SuccessBody {
            item_id: 999,
            input_fingerprint: "fp".into(),
            mime: Some("image/webp".into()),
            width: Some(100),
            height: Some(100),
            metadata: None,
        };
        assert!(validate_thumbnail_output(&req, &body, &webp, &limits()).is_err());
    }

    #[test]
    fn validate_rejects_declared_dims_mismatch() {
        let req = thumb_req(7, "fp", 480);
        let webp = make_webp(100, 100);
        let body = SuccessBody {
            item_id: 7,
            input_fingerprint: "fp".into(),
            mime: Some("image/webp".into()),
            width: Some(480), // 谎报
            height: Some(100),
            metadata: None,
        };
        assert!(validate_thumbnail_output(&req, &body, &webp, &limits()).is_err());
    }

    #[test]
    fn validate_rejects_oversized_long_edge() {
        let req = thumb_req(7, "fp", 120);
        let webp = make_webp(960, 100); // 长边 960 >> 120+容差
        let body = SuccessBody {
            item_id: 7,
            input_fingerprint: "fp".into(),
            mime: Some("image/webp".into()),
            width: Some(960),
            height: Some(100),
            metadata: None,
        };
        assert!(validate_thumbnail_output(&req, &body, &webp, &limits()).is_err());
    }

    #[test]
    fn validate_rejects_non_webp_blob() {
        let req = thumb_req(7, "fp", 480);
        let body = SuccessBody {
            item_id: 7,
            input_fingerprint: "fp".into(),
            mime: Some("image/webp".into()),
            width: None,
            height: None,
            metadata: None,
        };
        assert!(validate_thumbnail_output(&req, &body, b"not a webp at all!!", &limits()).is_err());
    }

    // ── WorkerConn 端到端（内存管道 + mock worker 线程，无真实子进程）──────────────────

    /// 一个内存单向管道：写端 + 读端共享有界缓冲（用 crossbeam 字节通道模拟）。
    /// 这里直接用 Vec→Cursor 不便于流式；改用 os 无关的简单实现：std::sync::mpsc 传字节块 + Read 适配。
    struct PipeWriter(Sender<Vec<u8>>);
    impl Write for PipeWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .send(buf.to_vec())
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "closed"))?;
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    struct PipeReader {
        rx: Receiver<Vec<u8>>,
        buf: std::collections::VecDeque<u8>,
    }
    impl Read for PipeReader {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            while self.buf.is_empty() {
                match self.rx.recv() {
                    Ok(chunk) => self.buf.extend(chunk),
                    Err(_) => return Ok(0), // 写端关闭 → EOF
                }
            }
            let n = out.len().min(self.buf.len());
            for slot in out.iter_mut().take(n) {
                *slot = self.buf.pop_front().unwrap();
            }
            Ok(n)
        }
    }
    fn unidir() -> (PipeWriter, PipeReader) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (
            PipeWriter(tx),
            PipeReader {
                rx,
                buf: std::collections::VecDeque::new(),
            },
        )
    }

    /// 搭一对连接：返回（host 侧 conn，worker 侧 reader/writer）。
    /// host 写 → worker 读；worker 写 → 经 frame_reader → host rx。
    fn wired_conn() -> (WorkerConn, PipeReader, PipeWriter) {
        let (host_w, worker_r) = unidir(); // host→worker
        let (worker_w, host_r) = unidir(); // worker→host
        let (tx, rx) = crossbeam_channel::unbounded();
        spawn_frame_reader(host_r, tx);
        let ready = ReadyBody {
            worker_id: "psd-worker".into(),
            worker_version: "1.0.0".into(),
            protocol_version: PROTOCOL_VERSION,
            capabilities: vec!["thumbnail".into()],
            max_blob_len: exotic_protocol::MAX_BLOB_LEN,
        };
        let conn = WorkerConn::from_parts(Box::new(host_w), rx, ready);
        (conn, worker_r, worker_w)
    }

    #[test]
    fn run_thumbnail_success_roundtrip() {
        let (mut conn, mut worker_r, mut worker_w) = wired_conn();
        // mock worker：读一个 Request，回 Success + 真 WebP。
        let handle = std::thread::spawn(move || {
            let frame = read_frame(&mut worker_r).unwrap();
            let req: RequestBody = frame.parse_json().unwrap();
            let webp = make_webp(480, 240);
            let body = SuccessBody {
                item_id: req.item_id(),
                input_fingerprint: req.input_fingerprint().into(),
                mime: Some("image/webp".into()),
                width: Some(480),
                height: Some(240),
                metadata: None,
            };
            let resp = Frame::with_blob(FrameType::Success, frame.request_id, &body, webp).unwrap();
            write_frame(&mut worker_w, &resp).unwrap();
            worker_w.flush().unwrap();
        });
        let req = thumb_req(7, "fp", 480);
        let out = conn.run_thumbnail(&req, &limits(), Duration::from_secs(5), &|| false);
        handle.join().unwrap();
        match out {
            TaskOutcome::Success { width, height, .. } => assert_eq!((width, height), (480, 240)),
            _ => panic!("期望 Success"),
        }
    }

    #[test]
    fn run_thumbnail_timeout_when_worker_silent() {
        let (mut conn, _worker_r, _worker_w) = wired_conn();
        // worker 不回复（持有读端但不读不写）。
        let req = thumb_req(7, "fp", 480);
        let out = conn.run_thumbnail(&req, &limits(), Duration::from_millis(150), &|| false);
        assert!(matches!(out, TaskOutcome::TimedOut));
    }

    #[test]
    fn run_thumbnail_cancelled_returns_disconnected_fast() {
        let (mut conn, _worker_r, _worker_w) = wired_conn();
        // worker 静默；cancelled 立即 true → 不等满 timeout，快速返回 Disconnected（stop 终止在途）。
        let req = thumb_req(7, "fp", 480);
        let start = std::time::Instant::now();
        let out = conn.run_thumbnail(&req, &limits(), Duration::from_secs(30), &|| true);
        assert!(matches!(out, TaskOutcome::Disconnected));
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "取消应快速返回，不等满 30s timeout"
        );
    }

    #[test]
    fn run_thumbnail_disconnect_when_worker_exits() {
        let (mut conn, worker_r, worker_w) = wired_conn();
        // worker 立即关闭两端 → host 读到 EOF。
        drop(worker_r);
        drop(worker_w);
        let req = thumb_req(7, "fp", 480);
        let out = conn.run_thumbnail(&req, &limits(), Duration::from_secs(2), &|| false);
        assert!(matches!(out, TaskOutcome::Disconnected));
    }

    #[test]
    fn run_thumbnail_request_id_mismatch_is_protocol() {
        let (mut conn, mut worker_r, mut worker_w) = wired_conn();
        let handle = std::thread::spawn(move || {
            let frame = read_frame(&mut worker_r).unwrap();
            let req: RequestBody = frame.parse_json().unwrap();
            let body = SuccessBody {
                item_id: req.item_id(),
                input_fingerprint: req.input_fingerprint().into(),
                mime: Some("image/webp".into()),
                width: Some(10),
                height: Some(10),
                metadata: None,
            };
            // 故意用错的 request_id。
            let resp = Frame::with_blob(
                FrameType::Success,
                frame.request_id + 99,
                &body,
                make_webp(10, 10),
            )
            .unwrap();
            write_frame(&mut worker_w, &resp).unwrap();
            worker_w.flush().unwrap();
        });
        let req = thumb_req(7, "fp", 480);
        let out = conn.run_thumbnail(&req, &limits(), Duration::from_secs(5), &|| false);
        handle.join().unwrap();
        assert!(matches!(out, TaskOutcome::Protocol(_)));
    }

    #[test]
    fn run_thumbnail_failure_passthrough() {
        let (mut conn, mut worker_r, mut worker_w) = wired_conn();
        let handle = std::thread::spawn(move || {
            let frame = read_frame(&mut worker_r).unwrap();
            let req: RequestBody = frame.parse_json().unwrap();
            let body = FailureBody {
                item_id: req.item_id(),
                input_fingerprint: req.input_fingerprint().into(),
                code: WorkerErrorCode::UnsupportedVariant,
                retryable: false,
                message: "cmyk".into(),
            };
            let resp = Frame::control(FrameType::Failure, frame.request_id, &body).unwrap();
            write_frame(&mut worker_w, &resp).unwrap();
            worker_w.flush().unwrap();
        });
        let req = thumb_req(7, "fp", 480);
        let out = conn.run_thumbnail(&req, &limits(), Duration::from_secs(5), &|| false);
        handle.join().unwrap();
        match out {
            TaskOutcome::Failure(b) => assert_eq!(b.code, WorkerErrorCode::UnsupportedVariant),
            _ => panic!("期望 Failure"),
        }
    }

    #[test]
    fn run_thumbnail_invalid_webp_output_is_protocol() {
        let (mut conn, mut worker_r, mut worker_w) = wired_conn();
        let handle = std::thread::spawn(move || {
            let frame = read_frame(&mut worker_r).unwrap();
            let req: RequestBody = frame.parse_json().unwrap();
            let body = SuccessBody {
                item_id: req.item_id(),
                input_fingerprint: req.input_fingerprint().into(),
                mime: Some("image/webp".into()),
                width: Some(10),
                height: Some(10),
                metadata: None,
            };
            // blob 不是合法 WebP。
            let resp = Frame::with_blob(FrameType::Success, frame.request_id, &body, vec![1, 2, 3])
                .unwrap();
            write_frame(&mut worker_w, &resp).unwrap();
            worker_w.flush().unwrap();
        });
        let req = thumb_req(7, "fp", 480);
        let out = conn.run_thumbnail(&req, &limits(), Duration::from_secs(5), &|| false);
        handle.join().unwrap();
        assert!(matches!(out, TaskOutcome::Protocol(_)));
    }
}
