// src-tauri/src/exotic/supervisor.rs
//! 冷门格式插件 · WorkerSupervisor（v3 Part2 §3.6）。
//!
//! 持有真实子进程，并把进程生命周期与 [`super::worker::WorkerConn`] 的协议状态机绑在一起：
//!
//! ```text
//! Supervisor（control）── 持 Child + stdin（经 WorkerConn 写）
//!        ├── stdout reader 线程 ── FrameResult channel ──> WorkerConn.rx
//!        └── stderr drain 线程  ── 有界环形缓冲（最近 64 KiB）
//! ```
//!
//! 关键边界：
//!   - 每 Supervisor **同一时刻只一个请求**；并发由进程池实现（Pipeline 控制）。
//!   - 超时 / 断开 / 协议违例 → `kill → wait`，标实例死亡；池补新实例前由 Pipeline 做崩溃退避。
//!   - stderr 持续排空（独立线程），防管道写满死锁；只保留最近 64 KiB 诊断。
//!   - Drop 兜底 kill + wait + join，绝不留孤儿进程或泄漏线程。

use std::collections::VecDeque;
use std::io::Read;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use super::worker::{
    spawn_frame_reader, spawn_worker_process, TaskOutcome, WorkerConfig, WorkerConn, WorkerLimits,
    WorkerSpec,
};
use exotic_protocol::RequestBody;

/// 子进程句柄抽象(R2-5 测试缝):生产实现为 [`Child`] 的 1:1 机械委托。
/// ExitStatus 被整体擦除——本模块所有调用点本就丢弃它(`let _ = wait()`、try_wait 只
/// match `Ok(Some(_))`),擦除不损失任何决策信息;kill/wait/try_wait 的调用语义与顺序不变。
/// 真实 Child 路径仍由 env-gated 冒烟测试(real_worker_thumbnail_and_shutdown)端到端覆盖。
trait ChildHandle: Send {
    fn kill(&mut self) -> std::io::Result<()>;
    fn wait(&mut self) -> std::io::Result<()>;
    fn try_wait(&mut self) -> std::io::Result<Option<()>>;
}

impl ChildHandle for Child {
    fn kill(&mut self) -> std::io::Result<()> {
        Child::kill(self)
    }
    fn wait(&mut self) -> std::io::Result<()> {
        Child::wait(self).map(|_| ())
    }
    fn try_wait(&mut self) -> std::io::Result<Option<()>> {
        Child::try_wait(self).map(|o| o.map(|_| ()))
    }
}

/// stderr 环形缓冲上限：只保留最近 64 KiB 诊断，不无限累积内存（§3.4）。
const STDERR_RING_CAP: usize = 64 * 1024;

/// 长驻 Worker 的监督者。
pub struct WorkerSupervisor {
    child: Box<dyn ChildHandle>,
    conn: WorkerConn,
    stderr_ring: Arc<Mutex<VecDeque<u8>>>,
    reader_handle: Option<JoinHandle<()>>,
    stderr_handle: Option<JoinHandle<()>>,
    /// 实例死亡（超时/断开/协议违例后置位）；池据此回收并补新实例。
    alive: bool,
    worker_version: String,
}

impl WorkerSupervisor {
    /// 启动并完成握手。失败则 kill 子进程后返回 Err（不留孤儿）。
    ///
    /// 注：§3.6 启动顺序第一步「验证安装记录与当前文件 hash」在 Part3 接入；Part2 仅校验路径存在。
    pub fn spawn(spec: &WorkerSpec, cfg: &WorkerConfig) -> Result<Self, String> {
        let mut child =
            spawn_worker_process(spec).map_err(|e| format!("创建 Worker 进程失败：{e}"))?;

        let stdin = child.stdin.take().ok_or("无法获取 Worker stdin")?;
        let stdout = child.stdout.take().ok_or("无法获取 Worker stdout")?;
        let stderr = child.stderr.take().ok_or("无法获取 Worker stderr")?;

        // stdout → 协议帧 channel。
        let (tx, rx) = crossbeam_channel::unbounded();
        let reader_handle = spawn_frame_reader(stdout, tx);

        // stderr → 有界环形缓冲（持续排空，防管道写满死锁）。
        let stderr_ring = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_RING_CAP)));
        let stderr_handle = spawn_stderr_drain(stderr, Arc::clone(&stderr_ring));

        // 握手（写 Hello → 等 Ready → 校验）。失败要 kill 兜底。
        let conn = match WorkerConn::handshake(Box::new(stdin), rx, spec, cfg) {
            Ok(c) => c,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader_handle.join();
                let _ = stderr_handle.join();
                return Err(format!("握手失败：{e}"));
            }
        };

        let worker_version = conn.worker_version().to_string();
        Ok(WorkerSupervisor {
            child: Box::new(child),
            conn,
            stderr_ring,
            reader_handle: Some(reader_handle),
            stderr_handle: Some(stderr_handle),
            alive: true,
            worker_version,
        })
    }

    pub fn worker_version(&self) -> &str {
        &self.worker_version
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// 跑一个缩略图任务。超时/断开/协议违例时 `kill → wait` 并标死亡（不再复用本实例）。
    /// `cancelled` 由上层注入（stop/退出）；命中时等待中止并 `kill` 在途 Worker（v3.1 §4.1）。
    pub fn run_thumbnail(
        &mut self,
        req: &RequestBody,
        limits: &WorkerLimits,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> TaskOutcome {
        if !self.alive {
            return TaskOutcome::Disconnected;
        }
        let outcome = self.conn.run_thumbnail(req, limits, timeout, cancelled);
        match &outcome {
            TaskOutcome::Success { .. } | TaskOutcome::Failure(_) => {}
            TaskOutcome::TimedOut | TaskOutcome::Disconnected | TaskOutcome::Protocol(_) => {
                warn!(
                    "Worker 实例异常（{}）→ kill 回收；stderr 尾部：{}",
                    outcome_label(&outcome),
                    self.stderr_tail_lossy()
                );
                self.kill_and_reap();
            }
        }
        outcome
    }

    /// 取 stderr 环形缓冲的有损字符串（诊断用）。
    pub fn stderr_tail_lossy(&self) -> String {
        let ring = self.stderr_ring.lock().unwrap_or_else(|e| e.into_inner());
        String::from_utf8_lossy(&ring.iter().copied().collect::<Vec<u8>>()).into_owned()
    }

    /// kill → wait → join 读取线程；标实例死亡。幂等。
    fn kill_and_reap(&mut self) {
        if !self.alive {
            return;
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.alive = false;
        self.join_threads();
    }

    fn join_threads(&mut self) {
        if let Some(h) = self.reader_handle.take() {
            let _ = h.join();
        }
        if let Some(h) = self.stderr_handle.take() {
            let _ = h.join();
        }
    }

    /// 优雅关闭：发 Shutdown → 等宽限期 → 超时 kill → wait → join。消费 self。
    pub fn shutdown(mut self, grace: Duration) {
        if !self.alive {
            self.join_threads();
            return;
        }
        self.conn.send_shutdown();
        let deadline = Instant::now() + grace;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.alive = false;
                    break;
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        debug!("Worker 未在宽限期内退出 → kill");
                        let _ = self.child.kill();
                        let _ = self.child.wait();
                        self.alive = false;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    self.alive = false;
                    break;
                }
            }
        }
        self.join_threads();
    }
}

impl Drop for WorkerSupervisor {
    fn drop(&mut self) {
        // 兜底：绝不留孤儿进程。已死则只 join。
        if self.alive {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.alive = false;
        }
        self.join_threads();
    }
}

fn outcome_label(o: &TaskOutcome) -> &'static str {
    match o {
        TaskOutcome::Success { .. } => "success",
        TaskOutcome::Failure(_) => "failure",
        TaskOutcome::TimedOut => "timeout",
        TaskOutcome::Disconnected => "disconnected",
        TaskOutcome::Protocol(_) => "protocol_violation",
    }
}

/// stderr 排空线程：持续读，追加到有界环形缓冲（超过 64 KiB 从头丢弃）。
fn spawn_stderr_drain<R: Read + Send + 'static>(
    mut r: R,
    ring: Arc<Mutex<VecDeque<u8>>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match r.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let mut g = ring.lock().unwrap_or_else(|e| e.into_inner());
                    g.extend(&buf[..n]);
                    // 截断到最近 STDERR_RING_CAP 字节。
                    while g.len() > STDERR_RING_CAP {
                        g.pop_front();
                    }
                }
                Err(_) => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exotic::worker::{default_thumbnail_limits, resolve_psd_worker_path};

    /// 合成最小合法 RGB 8-bit raw PSD（与 worker 解码单测同结构）。
    fn make_rgb_psd(w: u32, h: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"8BPS");
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&[0u8; 6]);
        b.extend_from_slice(&3u16.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&8u16.to_be_bytes());
        b.extend_from_slice(&3u16.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u16.to_be_bytes());
        for ch in 0..3u32 {
            for y in 0..h {
                for x in 0..w {
                    b.push(match ch {
                        0 => {
                            if w > 1 {
                                (x * 255 / (w - 1)) as u8
                            } else {
                                200
                            }
                        }
                        1 => {
                            if h > 1 {
                                (y * 255 / (h - 1)) as u8
                            } else {
                                120
                            }
                        }
                        _ => 128,
                    });
                }
            }
        }
        b
    }

    fn test_spec(path: std::path::PathBuf) -> WorkerSpec {
        WorkerSpec {
            exe_path: path,
            expected_worker_id: "psd-worker".into(),
            required_capabilities: vec!["thumbnail".into()],
        }
    }

    fn test_cfg() -> WorkerConfig {
        WorkerConfig {
            handshake_timeout: Duration::from_secs(5),
            host_version: "0.1.0".into(),
            max_blob_len: exotic_protocol::MAX_BLOB_LEN,
        }
    }

    // ── R2-5 确定性单测:kill/reap/Drop/shutdown 簿记(经 ChildHandle 假件,零真进程) ──

    use exotic_protocol::{
        FailureBody, Frame, FrameType, ProtocolError, ReadyBody, WorkerErrorCode,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 记录 kill/wait 次数的假子进程;try_wait 行为由闭包脚本化。
    struct FakeChild {
        kills: Arc<AtomicUsize>,
        waits: Arc<AtomicUsize>,
        try_wait_fn: Box<dyn FnMut() -> std::io::Result<Option<()>> + Send>,
    }
    impl ChildHandle for FakeChild {
        fn kill(&mut self) -> std::io::Result<()> {
            self.kills.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn wait(&mut self) -> std::io::Result<()> {
            self.waits.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn try_wait(&mut self) -> std::io::Result<Option<()>> {
            (self.try_wait_fn)()
        }
    }

    /// 共享捕获写端:conn 写出的帧字节落入 Vec,供断言「是否发过 Shutdown/Request」。
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct SupParts {
        kills: Arc<AtomicUsize>,
        waits: Arc<AtomicUsize>,
        written: Arc<Mutex<Vec<u8>>>,
        /// 持有发送端防 channel 变 clean-EOF;测试可预灌响应帧。
        tx: crossbeam_channel::Sender<Result<Frame, ProtocolError>>,
    }

    /// 以字面量组装含私有字段的 supervisor(tests 是子模块,可直构)。
    /// 线程句柄给立即退出的空线程,join_threads 不会挂起。
    fn make_sup(
        alive: bool,
        try_wait_fn: Box<dyn FnMut() -> std::io::Result<Option<()>> + Send>,
    ) -> (WorkerSupervisor, SupParts) {
        let kills = Arc::new(AtomicUsize::new(0));
        let waits = Arc::new(AtomicUsize::new(0));
        let written = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = crossbeam_channel::unbounded();
        let ready = ReadyBody {
            worker_id: "psd-worker".into(),
            worker_version: "0.0-test".into(),
            protocol_version: 0, // from_parts 跳过握手,不校验
            capabilities: vec!["thumbnail".into()],
            max_blob_len: exotic_protocol::MAX_BLOB_LEN,
        };
        let sup = WorkerSupervisor {
            child: Box::new(FakeChild {
                kills: Arc::clone(&kills),
                waits: Arc::clone(&waits),
                try_wait_fn,
            }),
            conn: WorkerConn::from_parts(Box::new(SharedWriter(Arc::clone(&written))), rx, ready),
            stderr_ring: Arc::new(Mutex::new(VecDeque::new())),
            reader_handle: Some(std::thread::spawn(|| {})),
            stderr_handle: Some(std::thread::spawn(|| {})),
            alive,
            worker_version: "0.0-test".into(),
        };
        (
            sup,
            SupParts {
                kills,
                waits,
                written,
                tx,
            },
        )
    }

    fn thumb_req() -> RequestBody {
        RequestBody::Thumbnail {
            item_id: 7,
            source_path: "x.psd".into(),
            target_long_edge: 480,
            input_fingerprint: "fp".into(),
        }
    }

    fn never_exits() -> Box<dyn FnMut() -> std::io::Result<Option<()>> + Send> {
        Box::new(|| Ok(None))
    }

    #[test]
    fn dead_instance_guard_skips_conn_and_child() {
        let (mut sup, parts) = make_sup(false, never_exits());
        let out = sup.run_thumbnail(
            &thumb_req(),
            &default_thumbnail_limits(),
            Duration::from_secs(1),
            &|| false,
        );
        assert!(matches!(out, TaskOutcome::Disconnected));
        assert!(
            parts.written.lock().unwrap().is_empty(),
            "不得向坏管道写请求"
        );
        assert_eq!(parts.kills.load(Ordering::SeqCst), 0);
        assert_eq!(parts.waits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn cancelled_disconnect_kills_once_and_stays_dead() {
        let (mut sup, parts) = make_sup(true, never_exits());
        // cancelled=||true 在任何等待前命中(worker.rs 取消轮询),零 sleep 确定性触发 kill 路径。
        let out = sup.run_thumbnail(
            &thumb_req(),
            &default_thumbnail_limits(),
            Duration::from_secs(30),
            &|| true,
        );
        assert!(matches!(out, TaskOutcome::Disconnected));
        assert_eq!(parts.kills.load(Ordering::SeqCst), 1);
        assert_eq!(parts.waits.load(Ordering::SeqCst), 1);
        assert!(!sup.is_alive());
        assert!(
            sup.reader_handle.is_none(),
            "kill_and_reap 应已 join 读取线程"
        );
        assert!(sup.stderr_handle.is_none());

        // 幂等:再跑直接 Disconnected、不二次 kill;Drop 因 alive=false 也只 join 不复杀。
        let out2 = sup.run_thumbnail(
            &thumb_req(),
            &default_thumbnail_limits(),
            Duration::from_secs(1),
            &|| false,
        );
        assert!(matches!(out2, TaskOutcome::Disconnected));
        drop(sup);
        assert_eq!(
            parts.kills.load(Ordering::SeqCst),
            1,
            "全生命周期恰好 1 次 kill"
        );
        assert_eq!(parts.waits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn timeout_kills_and_marks_dead() {
        let (mut sup, parts) = make_sup(true, never_exits());
        let out = sup.run_thumbnail(
            &thumb_req(),
            &default_thumbnail_limits(),
            Duration::from_millis(50),
            &|| false,
        );
        assert!(matches!(out, TaskOutcome::TimedOut));
        assert_eq!(parts.kills.load(Ordering::SeqCst), 1);
        assert!(!sup.is_alive());
        drop(parts.tx); // 显式:发送端存活至此,超时非 EOF 所致
    }

    #[test]
    fn failure_outcome_keeps_instance_alive() {
        // Failure 与 Success 走同一「数据类结果不杀进程」匹配臂;Success 需真 WebP 过
        // Host 校验(worker.rs 单测已覆盖协议侧),此处以 Failure 锁 supervisor 的保活语义。
        let (mut sup, parts) = make_sup(true, never_exits());
        let body = FailureBody {
            item_id: 7,
            input_fingerprint: "fp".into(),
            code: WorkerErrorCode::MalformedInput,
            retryable: false,
            message: "synthetic".into(),
        };
        // 预灌响应帧(request_id=1:from_parts 后首个分配值),recv 立即命中。
        parts
            .tx
            .send(Ok(Frame::control(FrameType::Failure, 1, &body).unwrap()))
            .unwrap();
        let out = sup.run_thumbnail(
            &thumb_req(),
            &default_thumbnail_limits(),
            Duration::from_secs(1),
            &|| false,
        );
        assert!(matches!(out, TaskOutcome::Failure(_)));
        assert_eq!(
            parts.kills.load(Ordering::SeqCst),
            0,
            "数据类失败不得杀进程"
        );
        assert!(sup.is_alive(), "实例应可复用");
    }

    #[test]
    fn shutdown_graceful_exit_sends_frame_and_never_kills() {
        let (sup, parts) = make_sup(true, Box::new(|| Ok(Some(()))));
        sup.shutdown(Duration::from_secs(5));
        // shutdown 消费 self,返回时 Drop 已跑完——断言全生命周期总数。
        assert!(
            !parts.written.lock().unwrap().is_empty(),
            "应已发送 Shutdown 帧"
        );
        assert_eq!(
            parts.kills.load(Ordering::SeqCst),
            0,
            "优雅退出不得 kill(含 Drop)"
        );
        assert_eq!(parts.waits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn shutdown_grace_expired_kills_once() {
        // grace=0:首轮 try_wait Ok(None) 即命中 deadline 分支,零 sleep 确定性。
        let (sup, parts) = make_sup(true, never_exits());
        sup.shutdown(Duration::ZERO);
        assert_eq!(parts.kills.load(Ordering::SeqCst), 1);
        assert_eq!(parts.waits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn shutdown_try_wait_error_kills_immediately() {
        let (sup, parts) = make_sup(
            true,
            Box::new(|| Err(std::io::Error::other("try_wait failed"))),
        );
        sup.shutdown(Duration::from_secs(5));
        assert_eq!(
            parts.kills.load(Ordering::SeqCst),
            1,
            "try_wait Err 不等宽限期"
        );
        assert_eq!(parts.waits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn shutdown_on_dead_instance_only_joins() {
        let (mut sup, parts) = make_sup(true, never_exits());
        let _ = sup.run_thumbnail(
            &thumb_req(),
            &default_thumbnail_limits(),
            Duration::from_secs(30),
            &|| true,
        ); // 先经 kill 路径致死
        parts.written.lock().unwrap().clear();
        sup.shutdown(Duration::from_secs(3));
        assert!(
            parts.written.lock().unwrap().is_empty(),
            "已死实例不得再发 Shutdown 帧"
        );
        assert_eq!(parts.kills.load(Ordering::SeqCst), 1, "kill 计数不变");
    }

    #[test]
    fn drop_alive_kills_once_dead_only_joins() {
        let (sup, parts) = make_sup(true, never_exits());
        drop(sup);
        assert_eq!(
            parts.kills.load(Ordering::SeqCst),
            1,
            "活实例 Drop 兜底 kill"
        );
        assert_eq!(parts.waits.load(Ordering::SeqCst), 1);

        let (sup2, parts2) = make_sup(false, never_exits());
        drop(sup2);
        assert_eq!(
            parts2.kills.load(Ordering::SeqCst),
            0,
            "死实例 Drop 只 join"
        );
    }

    #[test]
    fn stderr_ring_keeps_last_64k() {
        let data: Vec<u8> = (0..100_000usize).map(|i| (i % 251) as u8).collect();
        let ring = Arc::new(Mutex::new(VecDeque::new()));
        spawn_stderr_drain(std::io::Cursor::new(data.clone()), Arc::clone(&ring))
            .join()
            .unwrap();
        let g = ring.lock().unwrap();
        assert_eq!(g.len(), STDERR_RING_CAP);
        let kept: Vec<u8> = g.iter().copied().collect();
        assert_eq!(
            &kept[..],
            &data[100_000 - STDERR_RING_CAP..],
            "保留的是最后 64 KiB"
        );
        drop(g);

        let small = b"short stderr".to_vec();
        let ring2 = Arc::new(Mutex::new(VecDeque::new()));
        spawn_stderr_drain(std::io::Cursor::new(small.clone()), Arc::clone(&ring2))
            .join()
            .unwrap();
        assert_eq!(
            ring2.lock().unwrap().iter().copied().collect::<Vec<u8>>(),
            small
        );
    }

    #[test]
    fn outcome_label_maps_all_variants() {
        assert_eq!(
            outcome_label(&TaskOutcome::Success {
                width: 1,
                height: 1,
                mime: "image/webp".into(),
                blob: Vec::new(),
            }),
            "success"
        );
        assert_eq!(
            outcome_label(&TaskOutcome::Failure(FailureBody {
                item_id: 1,
                input_fingerprint: "f".into(),
                code: WorkerErrorCode::IoError,
                retryable: true,
                message: String::new(),
            })),
            "failure"
        );
        assert_eq!(outcome_label(&TaskOutcome::TimedOut), "timeout");
        assert_eq!(outcome_label(&TaskOutcome::Disconnected), "disconnected");
        assert_eq!(
            outcome_label(&TaskOutcome::Protocol(String::new())),
            "protocol_violation"
        );
    }

    /// 真实子进程冒烟测试:仅当设置 `EXOTIC_PSD_WORKER_PATH` 指向已构建的 psd-worker 时运行。
    /// 未设置则跳过（src-tauri 的 cargo test 不会自动构建独立 worker crate）。
    ///
    /// 构建并运行：
    ///   cargo build --release --manifest-path crates/exotic-workers/psd-worker/Cargo.toml
    ///   EXOTIC_PSD_WORKER_PATH=crates/exotic-workers/psd-worker/target/release/psd-worker.exe \
    ///     cargo test -p picasa-next exotic::supervisor::tests::real_worker -- --nocapture
    #[test]
    fn real_worker_thumbnail_and_shutdown() {
        let Some(path) = resolve_psd_worker_path() else {
            eprintln!("[skip] 未设 EXOTIC_PSD_WORKER_PATH，跳过真实 Worker 冒烟测试");
            return;
        };
        // 写一张合成 PSD 到临时文件。
        let dir = std::env::temp_dir().join(format!("exotic-sup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let psd_path = dir.join("synthetic.psd");
        std::fs::write(&psd_path, make_rgb_psd(300, 200)).unwrap();

        let mut sup = WorkerSupervisor::spawn(&test_spec(path), &test_cfg()).expect("spawn+握手");
        assert!(sup.is_alive());
        assert!(
            !sup.worker_version().is_empty(),
            "握手应拿到 worker_version"
        );

        let req = RequestBody::Thumbnail {
            item_id: 42,
            source_path: psd_path.to_string_lossy().into_owned(),
            target_long_edge: 480,
            input_fingerprint: "fp-real".into(),
        };
        let out = sup.run_thumbnail(
            &req,
            &default_thumbnail_limits(),
            Duration::from_secs(15),
            &|| false,
        );
        match out {
            TaskOutcome::Success { width, height, .. } => {
                assert_eq!((width, height), (300, 200));
            }
            other => panic!("期望 Success，得到 {}", outcome_label(&other)),
        }

        sup.shutdown(Duration::from_secs(3));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
