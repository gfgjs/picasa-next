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
    read_frame, write_frame, EmbedItem, EmbedResult, FaceDet, FaceItem, FaceItemResult,
    FailureBody, Frame, FrameType, HelloBody, ReadyBody, RequestBody, SuccessBody, WorkerErrorCode,
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

/// op 无关的原始请求结果(T15,D3 §4①)。`Success` **未经任何 op 特定校验**——
/// thumbnail 的 WebP 复核在 [`WorkerConn::run_thumbnail`],embed/face 批的
/// 维度×数量一致性在 [`validate_embed_batch_output`]/[`validate_face_batch_output`],
/// 由调用方按 op 分派。进程级三态(TimedOut/Disconnected/Protocol)语义与
/// [`TaskOutcome`] 一致(Supervisor 据此 kill 回收)。
// SessionReady 回声字段(T16)使 Success 变体略超 clippy 阈值;本枚举按值单次传递、
// 不进集合,变体大小差异无实际内存代价,故豁免而非 Box(免全链解引用噪音)。
#[allow(clippy::large_enum_variant)]
pub enum RawOutcome {
    /// 收到 Success 帧(body+blob 原样交出,尚未按 op 校验)。
    Success { body: SuccessBody, blob: Vec<u8> },
    /// Worker 显式失败(item/fingerprint 核对属 op 语义,由调用方做)。
    Failure(FailureBody),
    /// 超时(Supervisor 应 kill)。
    TimedOut,
    /// 连接断开 / Worker 退出。
    Disconnected,
    /// 协议违例(错序 / 损坏帧 / 意外帧类型)。
    Protocol(String),
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

    /// 发送任意 op 的请求并等待响应(T15 泛化,D3 §4①)。返回**未经 op 校验**的
    /// [`RawOutcome`];`cancelled` 在等待期间被周期轮询,返回 true 即放弃等待返回
    /// `Disconnected`(上层 kill 在途 Worker,v3.1 §4.1)。
    pub fn run_request(
        &mut self,
        req: &RequestBody,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> RawOutcome {
        let request_id = self.alloc_request_id();
        let frame = match Frame::control(FrameType::Request, request_id, req) {
            Ok(f) => f,
            Err(e) => return RawOutcome::Protocol(format!("构造 Request 失败：{e}")),
        };
        if write_frame(&mut self.writer, &frame).is_err() || self.writer.flush().is_err() {
            return RawOutcome::Disconnected;
        }

        // 可取消等待：每 CANCEL_POLL 检查一次取消标志，使 stop/App 退出能及时让 Supervisor kill
        // 在途 Worker（v3.1 §4.1：停止按取消协议终止在途，不等其自然完成；返回 Disconnected → kill）。
        let deadline = Instant::now() + timeout;
        let resp = loop {
            if cancelled() {
                return RawOutcome::Disconnected;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return RawOutcome::TimedOut;
            }
            match self.rx.recv_timeout(remaining.min(CANCEL_POLL)) {
                Ok(Ok(f)) => break f,
                Ok(Err(e)) => {
                    return if e.is_clean_eof() {
                        RawOutcome::Disconnected
                    } else {
                        // Worker 发出损坏帧 → 协议违例（Supervisor 会 kill）。
                        RawOutcome::Protocol(format!("响应帧损坏：{e}"))
                    };
                }
                Err(RecvTimeoutError::Timeout) => continue, // 轮询：再查取消 / 总超时
                Err(RecvTimeoutError::Disconnected) => return RawOutcome::Disconnected,
            }
        };

        // request_id 必须匹配当前在途请求（每 Supervisor 同时只一个请求）。
        if resp.request_id != request_id {
            return RawOutcome::Protocol(format!(
                "request_id 错配：{} != {}",
                resp.request_id, request_id
            ));
        }

        match resp.frame_type {
            FrameType::Success => match resp.parse_json::<SuccessBody>() {
                Ok(body) => RawOutcome::Success {
                    body,
                    blob: resp.blob,
                },
                Err(e) => RawOutcome::Protocol(format!("Success 解析失败：{e}")),
            },
            FrameType::Failure => match resp.parse_json::<FailureBody>() {
                Ok(body) => RawOutcome::Failure(body),
                Err(e) => RawOutcome::Protocol(format!("Failure 解析失败：{e}")),
            },
            other => RawOutcome::Protocol(format!("意外帧类型：{other:?}")),
        }
    }

    /// 发送一个缩略图请求并等待响应，验证后返回 [`TaskOutcome`]。
    /// `req` 必须是 `RequestBody::Thumbnail`；`target_long_edge` 须为吸附后档位（R5）。
    /// T15 起 = [`Self::run_request`] + thumbnail 专属输出校验(WebP 复核/尺寸/上限)。
    pub fn run_thumbnail(
        &mut self,
        req: &RequestBody,
        limits: &WorkerLimits,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> TaskOutcome {
        match self.run_request(req, timeout, cancelled) {
            RawOutcome::Success { body, blob } => {
                match validate_thumbnail_output(req, &body, &blob, limits) {
                    Ok((w, h, mime)) => TaskOutcome::Success {
                        width: w,
                        height: h,
                        mime,
                        blob,
                    },
                    Err(reason) => TaskOutcome::Protocol(reason),
                }
            }
            RawOutcome::Failure(body) => {
                // 即便是失败响应也要核对 id/fingerprint，防错序串扰。
                if body.item_id != req.item_id()
                    || body.input_fingerprint.as_deref() != req.input_fingerprint()
                {
                    return TaskOutcome::Protocol("Failure 的 item/fingerprint 错配".into());
                }
                TaskOutcome::Failure(body)
            }
            RawOutcome::TimedOut => TaskOutcome::TimedOut,
            RawOutcome::Disconnected => TaskOutcome::Disconnected,
            RawOutcome::Protocol(p) => TaskOutcome::Protocol(p),
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
            "item_id 错配：{:?} != {:?}",
            body.item_id,
            req.item_id()
        ));
    }
    if body.input_fingerprint.as_deref() != req.input_fingerprint() {
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

// ── v2 批量输出校验(T15,D3 §4①:「embed 批的输出校验 = 维度×数量一致性」)────────────
// Host 不信任 Worker(§3.7)在 v2 上的延伸:results 严格同序同长、逐项 item/fingerprint
// 核对、blob 长度精确等于 Ok 项载荷之和。任一不符 → Err(协议违例,调用方 kill 回收);
// 逐项 Err 是数据结果、不判违例。T17 派发器直接消费这两个纯函数。

/// EmbedBatch 单项的校验后结果(与请求 items 同序对齐)。
#[derive(Debug)]
pub enum EmbedItemOutcome {
    /// 该项嵌入(已按 embed_dim 从 blob 切出,f32 LE)。
    Ok(Vec<f32>),
    /// 该项失败(worker 逐项报错,不连坐)。
    Err(WorkerErrorCode),
}

/// 校验 EmbedBatch 的 Success 输出并切出各项嵌入。`embed_dim` 取自 SessionReady。
pub fn validate_embed_batch_output(
    items: &[EmbedItem],
    body: &SuccessBody,
    blob: &[u8],
    embed_dim: usize,
) -> Result<Vec<EmbedItemOutcome>, String> {
    let batch = body
        .embed
        .as_ref()
        .ok_or("EmbedBatch Success 缺 embed 应答体")?;
    if batch.results.len() != items.len() {
        return Err(format!(
            "results 长度错配：{} != items {}",
            batch.results.len(),
            items.len()
        ));
    }
    if embed_dim == 0 {
        return Err("embed_dim 为 0".into());
    }
    let item_bytes = embed_dim * 4;
    let ok_count = batch
        .results
        .iter()
        .filter(|r| matches!(r, EmbedResult::Ok { .. }))
        .count();
    if blob.len() != ok_count * item_bytes {
        return Err(format!(
            "blob 长度错配：{} != {}×{}",
            blob.len(),
            ok_count,
            item_bytes
        ));
    }

    let mut out = Vec::with_capacity(items.len());
    let mut off = 0usize;
    for (i, r) in batch.results.iter().enumerate() {
        let (rid, rfp) = match r {
            EmbedResult::Ok {
                item_id,
                fingerprint,
            }
            | EmbedResult::Err {
                item_id,
                fingerprint,
                ..
            } => (*item_id, fingerprint.as_str()),
        };
        // 同序核对:错序/陈旧结果即违例(延续单项 input_fingerprint 核对语义到批量)。
        if rid != items[i].item_id || rfp != items[i].fingerprint {
            return Err(format!("第 {i} 项 item/fingerprint 错配"));
        }
        match r {
            EmbedResult::Ok { .. } => {
                let emb: Vec<f32> = blob[off..off + item_bytes]
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
                off += item_bytes;
                out.push(EmbedItemOutcome::Ok(emb));
            }
            EmbedResult::Err { code, .. } => out.push(EmbedItemOutcome::Err(*code)),
        }
    }
    Ok(out)
}

/// FaceDetectEmbed 单项的校验后结果(与请求 items 同序对齐)。
#[derive(Debug)]
pub enum FaceItemOutcome {
    /// 几何 + 逐脸嵌入(faces 与 embeddings 同序同长;0 脸也是 Ok)。
    Ok {
        faces: Vec<FaceDet>,
        embeddings: Vec<Vec<f32>>,
        /// worker 实际解码尺寸(几何为该图像素坐标;归一化/quality 派生用)。
        width: u32,
        height: u32,
    },
    /// 该项失败(不连坐)。
    Err(WorkerErrorCode),
}

/// 校验 FaceDetectEmbed 的 Success 输出并按「Ok 项序 × 项内脸序」切出嵌入。
/// `face_embed_dim` 取自 SessionReady(未载人脸角色时本函数不应被调用)。
pub fn validate_face_batch_output(
    items: &[FaceItem],
    body: &SuccessBody,
    blob: &[u8],
    face_embed_dim: usize,
) -> Result<Vec<FaceItemOutcome>, String> {
    let batch = body
        .face
        .as_ref()
        .ok_or("FaceDetectEmbed Success 缺 face 应答体")?;
    if batch.results.len() != items.len() {
        return Err(format!(
            "results 长度错配：{} != items {}",
            batch.results.len(),
            items.len()
        ));
    }
    if face_embed_dim == 0 {
        return Err("face_embed_dim 为 0".into());
    }
    let face_bytes = face_embed_dim * 4;
    let total_faces: usize = batch
        .results
        .iter()
        .map(|r| match r {
            FaceItemResult::Ok { faces, .. } => faces.len(),
            FaceItemResult::Err { .. } => 0,
        })
        .sum();
    if blob.len() != total_faces * face_bytes {
        return Err(format!(
            "blob 长度错配：{} != {}×{}",
            blob.len(),
            total_faces,
            face_bytes
        ));
    }

    let mut out = Vec::with_capacity(items.len());
    let mut off = 0usize;
    for (i, r) in batch.results.iter().enumerate() {
        let (rid, rfp) = match r {
            FaceItemResult::Ok {
                item_id,
                fingerprint,
                ..
            }
            | FaceItemResult::Err {
                item_id,
                fingerprint,
                ..
            } => (*item_id, fingerprint.as_str()),
        };
        if rid != items[i].item_id || rfp != items[i].fingerprint {
            return Err(format!("第 {i} 项 item/fingerprint 错配"));
        }
        match r {
            FaceItemResult::Ok {
                faces,
                width,
                height,
                ..
            } => {
                // 解码尺寸为 0 = 旧帧缺字段或 worker bug——归一化会除坏,按协议违例回收。
                if *width == 0 || *height == 0 {
                    return Err(format!("第 {i} 项解码尺寸为 0({width}×{height})"));
                }
                let mut embeddings = Vec::with_capacity(faces.len());
                for _ in 0..faces.len() {
                    let emb: Vec<f32> = blob[off..off + face_bytes]
                        .chunks_exact(4)
                        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                        .collect();
                    off += face_bytes;
                    embeddings.push(emb);
                }
                out.push(FaceItemOutcome::Ok {
                    faces: faces.clone(),
                    embeddings,
                    width: *width,
                    height: *height,
                });
            }
            FaceItemResult::Err { code, .. } => out.push(FaceItemOutcome::Err(*code)),
        }
    }
    Ok(out)
}

/// 校验 EncodeText 的 Success 输出并切出各文本向量(T17)。全批原子(无逐项结构),
/// 校验 = 应答体 count 与请求 texts 数一致 + blob 长度精确等于 count×embed_dim×4;
/// 任一不符即协议违例(调用方 kill 回收),与 embed/face 批的「不信任 worker」同纪律。
pub fn validate_encode_text_output(
    text_count: usize,
    body: &SuccessBody,
    blob: &[u8],
    embed_dim: usize,
) -> Result<Vec<Vec<f32>>, String> {
    let te = body
        .text_embed
        .as_ref()
        .ok_or("EncodeText Success 缺 text_embed 应答体")?;
    if te.count as usize != text_count {
        return Err(format!("count 错配:{} != texts {}", te.count, text_count));
    }
    if embed_dim == 0 {
        return Err("embed_dim 为 0".into());
    }
    let item_bytes = embed_dim * 4;
    if blob.len() != text_count * item_bytes {
        return Err(format!(
            "blob 长度错配:{} != {}×{}",
            blob.len(),
            text_count,
            item_bytes
        ));
    }
    Ok(blob
        .chunks_exact(item_bytes)
        .map(|chunk| {
            chunk
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        })
        .collect())
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
            item_id: Some(7),
            input_fingerprint: Some("fp".into()),
            mime: Some("image/webp".into()),
            width: Some(480),
            height: Some(240),
            ..Default::default()
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
            item_id: Some(999),
            input_fingerprint: Some("fp".into()),
            mime: Some("image/webp".into()),
            width: Some(100),
            height: Some(100),
            ..Default::default()
        };
        assert!(validate_thumbnail_output(&req, &body, &webp, &limits()).is_err());
    }

    #[test]
    fn validate_rejects_declared_dims_mismatch() {
        let req = thumb_req(7, "fp", 480);
        let webp = make_webp(100, 100);
        let body = SuccessBody {
            item_id: Some(7),
            input_fingerprint: Some("fp".into()),
            mime: Some("image/webp".into()),
            width: Some(480), // 谎报
            height: Some(100),
            ..Default::default()
        };
        assert!(validate_thumbnail_output(&req, &body, &webp, &limits()).is_err());
    }

    #[test]
    fn validate_rejects_oversized_long_edge() {
        let req = thumb_req(7, "fp", 120);
        let webp = make_webp(960, 100); // 长边 960 >> 120+容差
        let body = SuccessBody {
            item_id: Some(7),
            input_fingerprint: Some("fp".into()),
            mime: Some("image/webp".into()),
            width: Some(960),
            height: Some(100),
            ..Default::default()
        };
        assert!(validate_thumbnail_output(&req, &body, &webp, &limits()).is_err());
    }

    #[test]
    fn validate_rejects_non_webp_blob() {
        let req = thumb_req(7, "fp", 480);
        let body = SuccessBody {
            item_id: Some(7),
            input_fingerprint: Some("fp".into()),
            mime: Some("image/webp".into()),
            width: None,
            height: None,
            ..Default::default()
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
                input_fingerprint: req.input_fingerprint().map(String::from),
                mime: Some("image/webp".into()),
                width: Some(480),
                height: Some(240),
                ..Default::default()
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
                input_fingerprint: req.input_fingerprint().map(String::from),
                mime: Some("image/webp".into()),
                width: Some(10),
                height: Some(10),
                ..Default::default()
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
                input_fingerprint: req.input_fingerprint().map(String::from),
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
                input_fingerprint: req.input_fingerprint().map(String::from),
                mime: Some("image/webp".into()),
                width: Some(10),
                height: Some(10),
                ..Default::default()
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

    // ── v2 批量输出校验(T15)────────────────────────────────────────────────────────

    fn embed_items(n: usize) -> Vec<EmbedItem> {
        (0..n)
            .map(|i| EmbedItem {
                item_id: i as i64 + 1,
                cache_key: format!("k{i}"),
                fingerprint: format!("fp{i}"),
            })
            .collect()
    }

    fn le_blob(embs: &[&[f32]]) -> Vec<u8> {
        let mut b = Vec::new();
        for e in embs {
            for f in e.iter() {
                b.extend_from_slice(&f.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn validate_embed_batch_happy_path_with_per_item_err() {
        let items = embed_items(3);
        let body = SuccessBody {
            embed: Some(exotic_protocol::EmbedBatchSuccess {
                results: vec![
                    EmbedResult::Ok {
                        item_id: 1,
                        fingerprint: "fp0".into(),
                    },
                    EmbedResult::Err {
                        item_id: 2,
                        fingerprint: "fp1".into(),
                        code: WorkerErrorCode::IoError,
                    },
                    EmbedResult::Ok {
                        item_id: 3,
                        fingerprint: "fp2".into(),
                    },
                ],
            }),
            ..Default::default()
        };
        // blob 只含两个 Ok 项(dim=2),按 Ok 项序连续。
        let blob = le_blob(&[&[1.0, 2.0], &[3.0, 4.0]]);
        let out = validate_embed_batch_output(&items, &body, &blob, 2).unwrap();
        assert_eq!(out.len(), 3);
        assert!(matches!(&out[0], EmbedItemOutcome::Ok(v) if v == &vec![1.0, 2.0]));
        assert!(matches!(
            &out[1],
            EmbedItemOutcome::Err(WorkerErrorCode::IoError)
        ));
        assert!(matches!(&out[2], EmbedItemOutcome::Ok(v) if v == &vec![3.0, 4.0]));
    }

    #[test]
    fn validate_embed_batch_rejects_length_and_order_violations() {
        let items = embed_items(2);
        // ① results 少一项 → 违例。
        let short = SuccessBody {
            embed: Some(exotic_protocol::EmbedBatchSuccess {
                results: vec![EmbedResult::Ok {
                    item_id: 1,
                    fingerprint: "fp0".into(),
                }],
            }),
            ..Default::default()
        };
        assert!(validate_embed_batch_output(&items, &short, &le_blob(&[&[0.0, 0.0]]), 2).is_err());

        // ② 错序(item_id 对调)→ 违例(陈旧/错位防护)。
        let swapped = SuccessBody {
            embed: Some(exotic_protocol::EmbedBatchSuccess {
                results: vec![
                    EmbedResult::Ok {
                        item_id: 2,
                        fingerprint: "fp1".into(),
                    },
                    EmbedResult::Ok {
                        item_id: 1,
                        fingerprint: "fp0".into(),
                    },
                ],
            }),
            ..Default::default()
        };
        let blob = le_blob(&[&[0.0, 0.0], &[0.0, 0.0]]);
        assert!(validate_embed_batch_output(&items, &swapped, &blob, 2).is_err());

        // ③ blob 长度与 Ok 项数不符 → 违例。
        let good = SuccessBody {
            embed: Some(exotic_protocol::EmbedBatchSuccess {
                results: vec![
                    EmbedResult::Ok {
                        item_id: 1,
                        fingerprint: "fp0".into(),
                    },
                    EmbedResult::Ok {
                        item_id: 2,
                        fingerprint: "fp1".into(),
                    },
                ],
            }),
            ..Default::default()
        };
        assert!(validate_embed_batch_output(&items, &good, &le_blob(&[&[0.0, 0.0]]), 2).is_err());
        // ④ 缺 embed 应答体 → 违例。
        assert!(validate_embed_batch_output(&items, &SuccessBody::default(), &[], 2).is_err());
    }

    #[test]
    fn validate_encode_text_happy_path_and_violations() {
        // 合法:count=2、blob=2×dim×4,按顺序切出两个向量。
        let good = SuccessBody {
            text_embed: Some(exotic_protocol::TextEmbedSuccess { count: 2 }),
            ..Default::default()
        };
        let blob = le_blob(&[&[1.0, -2.0], &[0.5, 0.25]]);
        let out = validate_encode_text_output(2, &good, &blob, 2).unwrap();
        assert_eq!(out, vec![vec![1.0, -2.0], vec![0.5, 0.25]]);

        // ① count 与请求 texts 数不符 → 违例。
        assert!(validate_encode_text_output(1, &good, &blob, 2).is_err());
        // ② blob 长度错配 → 违例。
        assert!(validate_encode_text_output(2, &good, &le_blob(&[&[1.0, -2.0]]), 2).is_err());
        // ③ 缺 text_embed 应答体(op 错配)→ 违例。
        assert!(validate_encode_text_output(2, &SuccessBody::default(), &blob, 2).is_err());
        // ④ embed_dim=0 → 违例(除零/空契约防御)。
        assert!(validate_encode_text_output(2, &good, &blob, 0).is_err());
    }

    #[test]
    fn validate_face_batch_happy_path_zero_and_multi_faces() {
        let items = vec![
            FaceItem {
                item_id: 10,
                cache_key: Some("aaa".into()),
                source_path: None,
                fingerprint: "f10".into(),
            },
            FaceItem {
                item_id: 11,
                cache_key: None,
                source_path: Some("x.jpg".into()),
                fingerprint: "f11".into(),
            },
        ];
        let det = FaceDet {
            bbox: [1.0, 2.0, 3.0, 4.0],
            landmarks: [[0.0; 2]; 5],
            score: 0.95,
        };
        let body = SuccessBody {
            face: Some(exotic_protocol::FaceBatchSuccess {
                results: vec![
                    FaceItemResult::Ok {
                        item_id: 10,
                        fingerprint: "f10".into(),
                        faces: vec![det.clone(), det.clone()],
                        width: 640,
                        height: 480,
                    },
                    // 0 张脸也是 Ok(协议明文)。
                    FaceItemResult::Ok {
                        item_id: 11,
                        fingerprint: "f11".into(),
                        faces: vec![],
                        width: 320,
                        height: 240,
                    },
                ],
            }),
            ..Default::default()
        };
        let blob = le_blob(&[&[0.5, 0.6], &[0.7, 0.8]]); // 2 脸 × dim 2
        let out = validate_face_batch_output(&items, &body, &blob, 2).unwrap();
        assert_eq!(out.len(), 2);
        match &out[0] {
            FaceItemOutcome::Ok {
                faces,
                embeddings,
                width,
                height,
            } => {
                assert_eq!(faces.len(), 2);
                assert_eq!(embeddings, &vec![vec![0.5, 0.6], vec![0.7, 0.8]]);
                assert_eq!((*width, *height), (640, 480));
            }
            _ => panic!("期望 Ok"),
        }
        match &out[1] {
            FaceItemOutcome::Ok {
                faces, embeddings, ..
            } => {
                assert!(faces.is_empty() && embeddings.is_empty());
            }
            _ => panic!("期望 0 脸 Ok"),
        }
    }

    #[test]
    fn validate_face_batch_rejects_zero_dims() {
        // 旧帧缺 width/height 经 serde default 落 0——host 必须拒收(归一化会除坏),
        // 该测试锁死「additive 字段的缺省值不可被静默接受」的契约。
        let items = vec![FaceItem {
            item_id: 10,
            cache_key: Some("aaa".into()),
            source_path: None,
            fingerprint: "f10".into(),
        }];
        let body = SuccessBody {
            face: Some(exotic_protocol::FaceBatchSuccess {
                results: vec![FaceItemResult::Ok {
                    item_id: 10,
                    fingerprint: "f10".into(),
                    faces: vec![],
                    width: 0,
                    height: 0,
                }],
            }),
            ..Default::default()
        };
        assert!(validate_face_batch_output(&items, &body, &[], 2).is_err());
    }

    #[test]
    fn validate_face_batch_rejects_blob_mismatch() {
        let items = vec![FaceItem {
            item_id: 10,
            cache_key: Some("aaa".into()),
            source_path: None,
            fingerprint: "f10".into(),
        }];
        let body = SuccessBody {
            face: Some(exotic_protocol::FaceBatchSuccess {
                results: vec![FaceItemResult::Ok {
                    item_id: 10,
                    fingerprint: "f10".into(),
                    faces: vec![FaceDet {
                        bbox: [0.0; 4],
                        landmarks: [[0.0; 2]; 5],
                        score: 1.0,
                    }],
                    width: 640,
                    height: 480,
                }],
            }),
            ..Default::default()
        };
        // 1 脸 × dim 2 应为 8 字节,给 4 字节 → 违例。
        assert!(validate_face_batch_output(&items, &body, &le_blob(&[&[0.5]]), 2).is_err());
        // fingerprint 错配 → 违例。
        let bad_fp = SuccessBody {
            face: Some(exotic_protocol::FaceBatchSuccess {
                results: vec![FaceItemResult::Err {
                    item_id: 10,
                    fingerprint: "WRONG".into(),
                    code: WorkerErrorCode::IoError,
                }],
            }),
            ..Default::default()
        };
        assert!(validate_face_batch_output(&items, &bad_fp, &[], 2).is_err());
    }
}
