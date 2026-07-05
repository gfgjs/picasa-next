// src-tauri/src/ai/worker_client.rs
//! AI worker 句柄(Part4-T17「AiEnginePool→worker 句柄」)。
//!
//! 主进程控制面持有 ai-worker 子进程的生命周期:spawn(死亡重建)→ ensure_session
//! (按活跃 profile 比对快照,不符先 close 再 init,D3 §4②)→ 批请求(EmbedBatch /
//! EncodeText)→ `exotic::worker::validate_*` 输出校验(「不信任 worker」纪律延续)。
//!
//! 过渡双活(Part4 §3.2):默认走进程内 ort(`ai_backend` 缺省 inproc),配置
//! `ai_backend=worker` 才启用本路径;worker e2e 验收后 T16 删 ort、本路径转正。
//!
//! 错误恢复契约(硬止损=重试一次):
//!   - 进程级异常(超时/断开/协议违例/输出校验失败)→ Supervisor 已 kill(或本端弃用
//!     实例)→ 重建 worker + 重建会话 + 重发一次;再败即向上返错。
//!   - `SessionExpired`(worker 端会话丢失,如其自杀重启)→ close(清 host 快照)后
//!     重 init 重发一次。
//!   - terminal Failure(EmbedDimMismatch/ModelLoadFailed 等)→ 不重试,直接返错。
//!
//! 人脸批(FaceDetectEmbed)已随 face_pipeline 接线波补齐(`face_detect_embed`);
//! 会话匹配采用**超集放宽**:不需要人脸的请求可复用带人脸的合并会话(见 `matches`)。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use exotic_protocol::{
    capability, EmbedItem, FaceItem, ModelDescriptor, ModelHandle, ModelProfileSnapshot, ModelRole,
    RequestBody, MAX_BLOB_LEN,
};
use tracing::{info, warn};

use crate::ai::face_profile::FaceProfile;
use crate::ai::profile::ModelProfile;
use crate::error::{AppError, Result};
use crate::exotic::coordinator::op_timeouts;
use crate::exotic::pipeline::EmbedWorker;
use crate::exotic::supervisor::{SessionDescriptor, WorkerSupervisor};
use crate::exotic::worker::{
    validate_embed_batch_output, validate_encode_text_output, validate_face_batch_output,
    EmbedItemOutcome, FaceItemOutcome, RawOutcome, WorkerConfig, WorkerSpec,
};

/// worker 进程握手超时(与 exotic 各插件一致:模型加载不在握手,恒快,D3 §2)。
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// 弃用实例时给 worker 的体面退出宽限(Shutdown 帧后等待;超时 kill)。
const SHUTDOWN_GRACE: Duration = Duration::from_millis(500);
/// 批请求硬止损:同一请求至多尝试次数(首发 + 重建后重发一次)。
const MAX_ATTEMPTS: usize = 2;

/// 期望会话规格:host 侧真相(活跃 profile + 运行参数),ensure_session 据此与
/// worker 快照比对。由 [`build_session_spec`] 从 AppState/配置组装。
#[derive(Clone)]
pub struct SessionSpec {
    pub profile: ModelProfile,
    /// None = 本会话不载人脸角色(CLIP 管线/搜索只需图文双塔;face 接线波传 Some)。
    pub face_profile: Option<FaceProfile>,
    /// 模型目录(= SessionInit.models_root,worker 侧归属校验根)。
    pub models_dir: PathBuf,
    /// AI 缓存根(= `{cache_dir}/ai_thumbs`;worker 只在其下按白名单 key 读图)。
    pub ai_cache_dir: PathBuf,
    /// 图像塔 EP("auto"/"directml"/"cpu";文本塔 worker 内恒 CPU,§8.6)。
    pub image_provider: String,
    /// EmbedBatch 单批上限(进 SessionInit 快照;worker 超限即拒)。
    pub batch_size: u32,
}

impl SessionSpec {
    /// 会话快照是否与本规格匹配(不符 = 需切换:先 close 再 init)。
    ///
    /// **超集放宽**(face 波裁决):spec 不需要人脸(`face_profile=None`)时,带人脸的
    /// 合并会话照样可服务——CLIP 双塔独立于 face 角色,face 边际 VRAM 仅 ~96MB(T9.5
    /// 实测)。否则 face 运行期间的语义搜索会把会话抖成 close→init 循环(每次 ~1.4s)。
    /// 需要人脸(Some)时仍须精确匹配。
    fn matches(&self, desc: &SessionDescriptor) -> bool {
        let clip_ok = desc.arch_id == self.profile.id && desc.image_file == self.profile.image_file;
        let face_ok = match &self.face_profile {
            Some(fp) => desc.face_profile_id.as_deref() == Some(fp.id.as_str()),
            None => true,
        };
        clip_ok && face_ok
    }
}

/// sha256 备忘条目:模型文件 GB 级,SessionInit 重建时不重算(len+mtime 未变即命中)。
struct ShaEntry {
    len: u64,
    mtime: SystemTime,
    hex: String,
}

/// 生成 EmbedWorker 实例的工厂闭包(测试注入 mock;运行时为 [`spawn_ai_worker`])。
type Spawner = Box<dyn Fn() -> std::result::Result<Box<dyn EmbedWorker>, String> + Send>;

/// AI worker 句柄:worker 实例 + 会话簿记 + 模型 sha 备忘。放 `AppState.ai_worker`
/// (std Mutex,按调用粒度持锁——批与批之间可插入搜索请求,worker 本身严格串行)。
pub struct AiWorkerClient {
    worker: Option<Box<dyn EmbedWorker>>,
    spawner: Spawner,
    sha_cache: HashMap<PathBuf, ShaEntry>,
    next_session_id: u64,
}

impl AiWorkerClient {
    /// 运行时构造:spawner = 真实子进程(exe 按 [`ai_worker_exe`] 解析)。
    pub fn new() -> Self {
        Self::with_spawner(Box::new(spawn_ai_worker))
    }

    /// 测试构造:注入 mock spawner。
    pub fn with_spawner(spawner: Spawner) -> Self {
        AiWorkerClient {
            worker: None,
            spawner,
            sha_cache: HashMap::new(),
            next_session_id: 1,
        }
    }

    /// 当前在载会话快照(T16:provider 回声落库/状态命令「已加载」判定消费);
    /// 无存活实例或无会话为 None。
    pub fn session(&self) -> Option<&SessionDescriptor> {
        self.worker
            .as_ref()
            .filter(|w| w.is_alive())
            .and_then(|w| w.session())
    }

    /// 弃用当前实例(体面退出;已死实例内部直接回收)。
    fn drop_worker(&mut self) {
        if let Some(w) = self.worker.take() {
            w.shutdown(SHUTDOWN_GRACE);
        }
    }

    /// 确保有存活 worker 实例(死亡/缺失即重建)。
    fn ensure_worker(&mut self) -> Result<()> {
        if self.worker.as_ref().is_some_and(|w| w.is_alive()) {
            return Ok(());
        }
        self.drop_worker();
        let w =
            (self.spawner)().map_err(|e| AppError::System(format!("AI worker 启动失败:{e}")))?;
        info!("AI worker 已启动(version={})", w.worker_version());
        self.worker = Some(w);
        Ok(())
    }

    /// 确保 worker 会话与 `spec` 一致,返回会话快照(embed_dim 等校验参数来源)。
    /// 快照匹配时零帧直接返回;不符先 close 再 init(切换语义)。
    fn ensure_session(
        &mut self,
        spec: &SessionSpec,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<SessionDescriptor> {
        self.ensure_worker()?;
        // 借用分离:先只读比对,需要切换时再取 &mut。
        let need_switch = {
            let w = self.worker.as_ref().expect("ensure_worker 后必有实例");
            match w.session() {
                Some(desc) if spec.matches(desc) => return Ok(desc.clone()),
                Some(_) => true,
                None => false,
            }
        };
        if need_switch {
            let w = self.worker.as_mut().expect("上方已确保实例");
            let _ = w.close_session(op_timeouts::SESSION_CLOSE, cancelled);
            // close 的进程级失败已由 Supervisor kill;重建后继续走 init。
            self.ensure_worker()?;
        }

        let session_id = self.next_session_id;
        self.next_session_id += 1;
        let req = build_session_init(session_id, spec, &mut self.sha_cache)?;

        let w = self.worker.as_mut().expect("上方已确保实例");
        match w.init_session(&req, op_timeouts::SESSION_INIT, cancelled) {
            RawOutcome::Success { .. } => w
                .session()
                .cloned()
                .ok_or_else(|| AppError::Internal("init_session 成功但无会话快照".into())),
            RawOutcome::Failure(fb) => Err(AppError::System(format!(
                "AI worker 会话初始化失败[{}]:{}",
                fb.code.as_str(),
                fb.message
            ))),
            RawOutcome::TimedOut => Err(AppError::System(
                "AI worker 会话初始化超时(实例已回收)".into(),
            )),
            RawOutcome::Disconnected => Err(AppError::System("AI worker 会话初始化中断开".into())),
            RawOutcome::Protocol(msg) => Err(AppError::System(format!(
                "AI worker 会话初始化协议违例:{msg}"
            ))),
        }
    }

    /// 跑一个批请求并按 op 校验输出。进程级异常/输出违例 → 重建重发一次(硬止损)。
    fn run_validated<T>(
        &mut self,
        spec: &SessionSpec,
        req: &RequestBody,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
        validate: impl Fn(
            &SessionDescriptor,
            &exotic_protocol::SuccessBody,
            &[u8],
        ) -> std::result::Result<T, String>,
    ) -> Result<T> {
        for attempt in 1..=MAX_ATTEMPTS {
            let desc = self.ensure_session(spec, cancelled)?;
            let w = self.worker.as_mut().expect("ensure_session 后必有实例");
            match w.run_batch(req, timeout, cancelled) {
                RawOutcome::Success { body, blob } => match validate(&desc, &body, &blob) {
                    Ok(out) => return Ok(out),
                    Err(msg) => {
                        // 输出校验失败 = 协议违例:弃用实例,重建后重发一次。
                        warn!("AI worker 输出校验失败(第 {attempt} 次):{msg} → 弃用实例");
                        self.drop_worker();
                    }
                },
                RawOutcome::Failure(fb)
                    if fb.code == exotic_protocol::WorkerErrorCode::SessionExpired =>
                {
                    // worker 端会话丢失:close 清 host 快照(幂等)后由下轮 ensure 重建。
                    warn!("AI worker 报会话失效(第 {attempt} 次)→ 重建会话");
                    if let Some(w) = self.worker.as_mut() {
                        let _ = w.close_session(op_timeouts::SESSION_CLOSE, cancelled);
                    }
                }
                RawOutcome::Failure(fb) => {
                    return Err(AppError::System(format!(
                        "AI worker 批请求失败[{}]:{}",
                        fb.code.as_str(),
                        fb.message
                    )))
                }
                RawOutcome::TimedOut => {
                    warn!("AI worker 批请求超时(第 {attempt} 次,实例已回收)");
                }
                RawOutcome::Disconnected => {
                    warn!("AI worker 断开(第 {attempt} 次)");
                }
                RawOutcome::Protocol(msg) => {
                    warn!("AI worker 协议违例(第 {attempt} 次):{msg}");
                }
            }
            if cancelled() {
                return Err(AppError::System("AI worker 批请求已取消".into()));
            }
        }
        Err(AppError::System(format!(
            "AI worker 批请求 {MAX_ATTEMPTS} 次尝试均失败(硬止损)"
        )))
    }

    /// CLIP 图像批嵌入:结果与 `items` 同序对齐(校验见 `validate_embed_batch_output`)。
    pub fn embed_batch(
        &mut self,
        spec: &SessionSpec,
        items: &[EmbedItem],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<EmbedItemOutcome>> {
        let req = RequestBody::EmbedBatch {
            items: items.to_vec(),
        };
        self.run_validated(
            spec,
            &req,
            op_timeouts::EMBED_BATCH,
            cancelled,
            |d, b, bl| validate_embed_batch_output(items, b, bl, d.embed_dim as usize),
        )
    }

    /// CLIP 文本编码(语义搜索查询向量;T17 补的 EncodeText op)。
    pub fn encode_text(
        &mut self,
        spec: &SessionSpec,
        texts: &[String],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Vec<f32>>> {
        let req = RequestBody::EncodeText {
            texts: texts.to_vec(),
        };
        self.run_validated(
            spec,
            &req,
            op_timeouts::ENCODE_TEXT,
            cancelled,
            |d, b, bl| validate_encode_text_output(texts.len(), b, bl, d.embed_dim as usize),
        )
    }

    /// 人脸检测+嵌入批(face 接线波):结果与 `items` 同序对齐。要求
    /// `spec.face_profile = Some`(ensure_session 据此载入合并会话);校验维度取
    /// SessionReady 回报的 `face_embed_dim`,缺失即协议违例(触发硬止损重建)。
    pub fn face_detect_embed(
        &mut self,
        spec: &SessionSpec,
        items: &[FaceItem],
        det_score_thresh: f32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<FaceItemOutcome>> {
        let req = RequestBody::FaceDetectEmbed {
            items: items.to_vec(),
            det_score_thresh,
        };
        self.run_validated(
            spec,
            &req,
            // 超时按批内项数缩放(2026-07-03 修复:固定 120s 对全尺寸原图批必然误杀)。
            op_timeouts::face_detect_embed(items.len()),
            cancelled,
            |d, b, bl| {
                let dim = d
                    .face_embed_dim
                    .ok_or_else(|| "会话未声明 face_embed_dim(未载人脸角色)".to_string())?;
                validate_face_batch_output(items, b, bl, dim as usize)
            },
        )
    }

    /// 显式卸载会话(管线自然完成/停止时调用,对齐进程内路径「结束即卸引擎释放 VRAM」;
    /// worker 进程留存,空闲 300s 自杀兜底,D3 §4④)。幂等。
    pub fn close_session(&mut self) {
        if let Some(w) = self.worker.as_mut() {
            if w.is_alive() {
                let _ = w.close_session(op_timeouts::SESSION_CLOSE, &|| false);
            }
        }
    }
}

impl Default for AiWorkerClient {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析 ai-worker 可执行文件:环境变量 `PICASA_AI_WORKER_PATH` 覆盖(开发/测试),
/// 缺省取主程序同目录(workspace 构建与打包分发的共同布局;签名产线随 Part7)。
pub fn ai_worker_exe() -> std::result::Result<PathBuf, String> {
    if let Ok(p) = std::env::var("PICASA_AI_WORKER_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!(
            "PICASA_AI_WORKER_PATH 指向的文件不存在:{}",
            p.display()
        ));
    }
    let exe = std::env::current_exe().map_err(|e| format!("current_exe 失败:{e}"))?;
    let dir = exe.parent().ok_or("current_exe 无父目录")?;
    let p = dir.join(format!("ai-worker{}", std::env::consts::EXE_SUFFIX));
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!("ai-worker 可执行文件不存在:{}", p.display()))
    }
}

/// 真实 spawner:解析 exe → WorkerSupervisor::spawn(握手校验 worker_id/能力)。
fn spawn_ai_worker() -> std::result::Result<Box<dyn EmbedWorker>, String> {
    let spec = WorkerSpec {
        exe_path: ai_worker_exe()?,
        expected_worker_id: "ai-worker".to_string(),
        required_capabilities: vec![capability::EMBEDDING.to_string()],
    };
    let cfg = WorkerConfig {
        handshake_timeout: HANDSHAKE_TIMEOUT,
        host_version: env!("CARGO_PKG_VERSION").to_string(),
        max_blob_len: MAX_BLOB_LEN,
    };
    WorkerSupervisor::spawn(&spec, &cfg).map(|s| Box::new(s) as Box<dyn EmbedWorker>)
}

/// 组装 SessionInit 请求:按 profile 契约列模型角色(CLIP 成对必备;face 按 spec 成对),
/// 逐模型带 len+sha256(D1 §3;sha 经 mtime+len 备忘,GB 级文件不重算)。
fn build_session_init(
    session_id: u64,
    spec: &SessionSpec,
    sha_cache: &mut HashMap<PathBuf, ShaEntry>,
) -> Result<RequestBody> {
    let mut models = vec![
        model_descriptor(
            ModelRole::ImageEncoder,
            &spec.models_dir.join(&spec.profile.image_file),
            sha_cache,
        )?,
        model_descriptor(
            ModelRole::TextEncoder,
            &spec.models_dir.join(&spec.profile.text_file),
            sha_cache,
        )?,
    ];
    if let Some(fp) = &spec.face_profile {
        models.push(model_descriptor(
            ModelRole::FaceDetect,
            &spec.models_dir.join(&fp.detect_file),
            sha_cache,
        )?);
        models.push(model_descriptor(
            ModelRole::FaceRecog,
            &spec.models_dir.join(&fp.embed_file),
            sha_cache,
        )?);
    }
    Ok(RequestBody::SessionInit {
        session_id,
        models,
        model_profile: ModelProfileSnapshot {
            arch_id: spec.profile.id.clone(),
            image_file: spec.profile.image_file.clone(),
            text_file: spec.profile.text_file.clone(),
            batch_size: spec.batch_size,
            face_profile_id: spec.face_profile.as_ref().map(|f| f.id.clone()),
        },
        models_root: spec.models_dir.to_string_lossy().into_owned(),
        ai_cache_dir: spec.ai_cache_dir.to_string_lossy().into_owned(),
        image_provider: spec.image_provider.clone(),
    })
}

/// 单模型描述:stat 取 len/mtime → sha 备忘命中即复用,否则流式重算。
/// 文件缺失 → AiModelNotLoaded(可操作:请先下载),与 set_active_model 语义一致。
fn model_descriptor(
    role: ModelRole,
    path: &Path,
    sha_cache: &mut HashMap<PathBuf, ShaEntry>,
) -> Result<ModelDescriptor> {
    let meta = std::fs::metadata(path).map_err(|_| {
        AppError::AiModelNotLoaded(format!(
            "模型文件缺失,请先下载:{}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
        ))
    })?;
    let len = meta.len();
    let mtime = meta.modified().map_err(AppError::Io)?;

    let hit = sha_cache
        .get(path)
        .is_some_and(|e| e.len == len && e.mtime == mtime);
    if !hit {
        let hex = crate::utils::hash::sha256_hex_of_file(path).map_err(AppError::Io)?;
        sha_cache.insert(path.to_path_buf(), ShaEntry { len, mtime, hex });
    }
    let sha256 = sha_cache
        .get(path)
        .map(|e| e.hex.clone())
        .expect("上方必已插入");

    Ok(ModelDescriptor {
        role,
        handle: ModelHandle::Path(path.to_string_lossy().into_owned()),
        len,
        sha256,
    })
}

/// 从应用状态组装会话规格(worker 派发与搜索共用):models_dir 沿用 ai_commands 推导,
/// ai_cache_dir = `{cache_dir}/ai_thumbs`(与 `thumbnail::cache::ai_cache_path` 同构),
/// provider 取 `ai_provider_override` 配置(缺省 auto),batch 取 pipeline 的统一解析。
pub fn build_session_spec(
    state: &crate::state::AppState,
    profile: ModelProfile,
    face_profile: Option<FaceProfile>,
) -> SessionSpec {
    let models_dir = crate::ipc::ai_commands::models_dir(state);
    let ai_cache_dir = state
        .thumb_config
        .read()
        .unwrap()
        .cache_dir
        .join("ai_thumbs");
    let image_provider = state
        .db_read_pool
        .get()
        .ok()
        .and_then(|c| {
            crate::db::queries::get_config(&c, "ai_provider_override")
                .ok()
                .flatten()
        })
        .unwrap_or_else(|| "auto".to_string());
    let batch_size = crate::ai::pipeline::resolve_batch_size(state, &profile) as u32;
    SessionSpec {
        profile,
        face_profile,
        models_dir,
        ai_cache_dir,
        image_provider,
        batch_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exotic_protocol::{
        EmbedBatchSuccess, EmbedResult, FaceBatchSuccess, FaceItemResult, FailureBody,
        SessionReadyBody, SuccessBody, WorkerErrorCode,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// 脚本化 mock worker:记录调用次数,按预设应答批请求。
    struct MockEmbedWorker {
        alive: bool,
        session: Option<SessionDescriptor>,
        /// 每次 run_batch 依序弹出一个应答;耗尽后回 Disconnected。
        batch_script: Vec<RawOutcome>,
        init_count: Arc<AtomicUsize>,
        close_count: Arc<AtomicUsize>,
    }

    impl crate::exotic::pipeline::WorkerTask for MockEmbedWorker {
        fn worker_version(&self) -> String {
            "mock".into()
        }
        fn is_alive(&self) -> bool {
            self.alive
        }
        fn shutdown(self: Box<Self>, _grace: Duration) {}
    }

    impl EmbedWorker for MockEmbedWorker {
        fn session(&self) -> Option<&SessionDescriptor> {
            self.session.as_ref()
        }
        fn init_session(
            &mut self,
            req: &RequestBody,
            _timeout: Duration,
            _cancelled: &dyn Fn() -> bool,
        ) -> RawOutcome {
            self.init_count.fetch_add(1, Ordering::SeqCst);
            let RequestBody::SessionInit {
                session_id,
                model_profile,
                ..
            } = req
            else {
                return RawOutcome::Protocol("非 SessionInit".into());
            };
            // 镜像真实 worker:载入 face 角色时回报 face_embed_dim(测试用 dim=2)。
            let face_dim = model_profile.face_profile_id.as_ref().map(|_| 2u32);
            self.session = Some(SessionDescriptor {
                session_id: *session_id,
                arch_id: model_profile.arch_id.clone(),
                image_file: model_profile.image_file.clone(),
                face_profile_id: model_profile.face_profile_id.clone(),
                embed_dim: 2,
                face_embed_dim: face_dim,
                caps: vec!["embedding".into()],
                provider: Some("mock".into()),
                gpu_name: Some(String::new()),
            });
            RawOutcome::Success {
                body: SuccessBody {
                    session: Some(SessionReadyBody {
                        embed_dim: 2,
                        face_embed_dim: face_dim,
                        caps: vec!["embedding".into()],
                        provider: Some("mock".into()),
                        gpu_name: Some(String::new()),
                    }),
                    ..Default::default()
                },
                blob: Vec::new(),
            }
        }
        fn close_session(
            &mut self,
            _timeout: Duration,
            _cancelled: &dyn Fn() -> bool,
        ) -> RawOutcome {
            self.close_count.fetch_add(1, Ordering::SeqCst);
            self.session = None;
            RawOutcome::Success {
                body: SuccessBody::default(),
                blob: Vec::new(),
            }
        }
        fn run_batch(
            &mut self,
            _req: &RequestBody,
            _timeout: Duration,
            _cancelled: &dyn Fn() -> bool,
        ) -> RawOutcome {
            if self.batch_script.is_empty() {
                self.alive = false;
                return RawOutcome::Disconnected;
            }
            let out = self.batch_script.remove(0);
            // 进程级异常语义:真实 Supervisor 会 kill 标死,mock 同步。
            if matches!(
                out,
                RawOutcome::TimedOut | RawOutcome::Disconnected | RawOutcome::Protocol(_)
            ) {
                self.alive = false;
                self.session = None;
            }
            out
        }
    }

    /// 一个 dim=2、双项全 Ok 的合法批应答。
    fn ok_batch() -> RawOutcome {
        let mut blob = Vec::new();
        for f in [1.0f32, 2.0, 3.0, 4.0] {
            blob.extend_from_slice(&f.to_le_bytes());
        }
        RawOutcome::Success {
            body: SuccessBody {
                embed: Some(EmbedBatchSuccess {
                    results: vec![
                        EmbedResult::Ok {
                            item_id: 1,
                            fingerprint: "f1".into(),
                        },
                        EmbedResult::Ok {
                            item_id: 2,
                            fingerprint: "f2".into(),
                        },
                    ],
                }),
                ..Default::default()
            },
            blob,
        }
    }

    fn items2() -> Vec<EmbedItem> {
        vec![
            EmbedItem {
                item_id: 1,
                cache_key: "aaa1".into(),
                fingerprint: "f1".into(),
            },
            EmbedItem {
                item_id: 2,
                cache_key: "aaa2".into(),
                fingerprint: "f2".into(),
            },
        ]
    }

    /// 测试规格:profile 用注册表默认,模型文件不落盘(mock 不校验载荷,
    /// build_session_init 也不会被 mock 路径拒——sha 计算需要真文件,因此测试用
    /// 临时目录铺两份契约文件)。
    fn test_spec(dir: &Path) -> SessionSpec {
        let profile = crate::ai::profile::default_profile();
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(&profile.image_file), b"img").unwrap();
        std::fs::write(dir.join(&profile.text_file), b"txt").unwrap();
        SessionSpec {
            profile,
            face_profile: None,
            models_dir: dir.to_path_buf(),
            ai_cache_dir: dir.join("ai_thumbs"),
            image_provider: "cpu".into(),
            batch_size: 16,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "ai-worker-client-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn client_with_script(
        scripts: Vec<Vec<RawOutcome>>,
        init_count: Arc<AtomicUsize>,
        close_count: Arc<AtomicUsize>,
        spawn_count: Arc<AtomicUsize>,
    ) -> AiWorkerClient {
        let scripts = std::sync::Mutex::new(scripts);
        AiWorkerClient::with_spawner(Box::new(move || {
            spawn_count.fetch_add(1, Ordering::SeqCst);
            let mut s = scripts.lock().unwrap();
            if s.is_empty() {
                return Err("mock spawner 脚本耗尽".into());
            }
            Ok(Box::new(MockEmbedWorker {
                alive: true,
                session: None,
                batch_script: s.remove(0),
                init_count: Arc::clone(&init_count),
                close_count: Arc::clone(&close_count),
            }) as Box<dyn EmbedWorker>)
        }))
    }

    #[test]
    fn embed_batch_happy_path_inits_session_once() {
        let dir = temp_dir("happy");
        let spec = test_spec(&dir);
        let init = Arc::new(AtomicUsize::new(0));
        let close = Arc::new(AtomicUsize::new(0));
        let spawn = Arc::new(AtomicUsize::new(0));
        let mut c = client_with_script(
            vec![vec![ok_batch(), ok_batch()]],
            Arc::clone(&init),
            Arc::clone(&close),
            Arc::clone(&spawn),
        );

        let out = c.embed_batch(&spec, &items2(), &|| false).unwrap();
        assert_eq!(out.len(), 2);
        assert!(matches!(&out[0], EmbedItemOutcome::Ok(v) if v == &vec![1.0, 2.0]));
        // 第二批复用同一会话:init 不再发生。
        let _ = c.embed_batch(&spec, &items2(), &|| false).unwrap();
        assert_eq!(init.load(Ordering::SeqCst), 1, "会话快照匹配应零帧复用");
        assert_eq!(spawn.load(Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn embed_batch_respawns_after_timeout_then_succeeds() {
        let dir = temp_dir("timeout-retry");
        let spec = test_spec(&dir);
        let init = Arc::new(AtomicUsize::new(0));
        let close = Arc::new(AtomicUsize::new(0));
        let spawn = Arc::new(AtomicUsize::new(0));
        // 第一个实例:超时(死);第二个实例:成功。
        let mut c = client_with_script(
            vec![vec![RawOutcome::TimedOut], vec![ok_batch()]],
            Arc::clone(&init),
            Arc::clone(&close),
            Arc::clone(&spawn),
        );

        let out = c.embed_batch(&spec, &items2(), &|| false).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(spawn.load(Ordering::SeqCst), 2, "超时后应重建实例");
        assert_eq!(init.load(Ordering::SeqCst), 2, "重建后应重建会话");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn embed_batch_hard_stop_after_two_failures() {
        let dir = temp_dir("hard-stop");
        let spec = test_spec(&dir);
        let init = Arc::new(AtomicUsize::new(0));
        let close = Arc::new(AtomicUsize::new(0));
        let spawn = Arc::new(AtomicUsize::new(0));
        // 两个实例都超时 → 硬止损返错;第三个实例不该被创建。
        let mut c = client_with_script(
            vec![
                vec![RawOutcome::TimedOut],
                vec![RawOutcome::TimedOut],
                vec![ok_batch()],
            ],
            Arc::clone(&init),
            Arc::clone(&close),
            Arc::clone(&spawn),
        );
        assert!(c.embed_batch(&spec, &items2(), &|| false).is_err());
        assert_eq!(spawn.load(Ordering::SeqCst), 2, "硬止损:不应第三次重建");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_expired_reinits_same_instance() {
        let dir = temp_dir("expired");
        let spec = test_spec(&dir);
        let init = Arc::new(AtomicUsize::new(0));
        let close = Arc::new(AtomicUsize::new(0));
        let spawn = Arc::new(AtomicUsize::new(0));
        let expired = RawOutcome::Failure(FailureBody {
            item_id: None,
            input_fingerprint: None,
            code: WorkerErrorCode::SessionExpired,
            retryable: true,
            message: "会话未加载".into(),
        });
        let mut c = client_with_script(
            vec![vec![expired, ok_batch()]],
            Arc::clone(&init),
            Arc::clone(&close),
            Arc::clone(&spawn),
        );
        let out = c.embed_batch(&spec, &items2(), &|| false).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(spawn.load(Ordering::SeqCst), 1, "实例存活,不重建进程");
        assert_eq!(init.load(Ordering::SeqCst), 2, "会话失效应重 init");
        assert!(
            close.load(Ordering::SeqCst) >= 1,
            "重 init 前应先 close 清快照"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn terminal_failure_does_not_retry() {
        let dir = temp_dir("terminal");
        let spec = test_spec(&dir);
        let init = Arc::new(AtomicUsize::new(0));
        let close = Arc::new(AtomicUsize::new(0));
        let spawn = Arc::new(AtomicUsize::new(0));
        let dim_mismatch = RawOutcome::Failure(FailureBody {
            item_id: None,
            input_fingerprint: None,
            code: WorkerErrorCode::EmbedDimMismatch,
            retryable: false,
            message: "维度不符".into(),
        });
        let mut c = client_with_script(
            vec![vec![dim_mismatch, ok_batch()]],
            Arc::clone(&init),
            Arc::clone(&close),
            Arc::clone(&spawn),
        );
        let e = c.embed_batch(&spec, &items2(), &|| false).unwrap_err();
        assert!(e.to_string().contains("embed_dim_mismatch"), "err: {e}");
        assert_eq!(init.load(Ordering::SeqCst), 1, "terminal 失败不得重试");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_session_init_lists_clip_pair_with_integrity() {
        let dir = temp_dir("build-init");
        let spec = test_spec(&dir);
        let mut cache = HashMap::new();
        let req = build_session_init(7, &spec, &mut cache).unwrap();
        let RequestBody::SessionInit {
            session_id,
            models,
            model_profile,
            models_root,
            ..
        } = req
        else {
            panic!("应为 SessionInit");
        };
        assert_eq!(session_id, 7);
        assert_eq!(models.len(), 2, "无 face profile 时只列 CLIP 成对");
        let img = models
            .iter()
            .find(|m| m.role == ModelRole::ImageEncoder)
            .unwrap();
        assert_eq!(img.len, 3);
        assert_eq!(img.sha256, crate::utils::hash::sha256_hex(b"img"));
        assert!(
            matches!(&img.handle, ModelHandle::Path(p) if p.ends_with(&spec.profile.image_file))
        );
        assert_eq!(model_profile.arch_id, spec.profile.id);
        assert_eq!(model_profile.face_profile_id, None);
        assert_eq!(models_root, spec.models_dir.to_string_lossy());
        // sha 备忘:同文件未变更,二次组装命中缓存(条目仍在且值不变)。
        let req2 = build_session_init(8, &spec, &mut cache).unwrap();
        let RequestBody::SessionInit { models: m2, .. } = req2 else {
            panic!()
        };
        assert_eq!(m2[0].sha256, models[0].sha256);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_model_file_maps_to_model_not_loaded() {
        let dir = temp_dir("missing-model");
        let spec = test_spec(&dir);
        std::fs::remove_file(dir.join(&spec.profile.image_file)).unwrap();
        let mut cache = HashMap::new();
        let e = build_session_init(1, &spec, &mut cache).unwrap_err();
        assert!(matches!(e, AppError::AiModelNotLoaded(_)), "err: {e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// face 接线波用规格:在 test_spec 之上再铺 face 双 onnx 契约文件。
    fn test_spec_with_face(dir: &Path) -> SessionSpec {
        let mut spec = test_spec(dir);
        let face = crate::ai::face_profile::default_face_profile();
        std::fs::write(dir.join(&face.detect_file), b"det").unwrap();
        std::fs::write(dir.join(&face.embed_file), b"emb").unwrap();
        spec.face_profile = Some(face);
        spec
    }

    #[test]
    fn session_spec_superset_match() {
        let dir = temp_dir("superset");
        let plain = test_spec(&dir);
        let with_face = test_spec_with_face(&dir);
        let face_id = with_face.face_profile.as_ref().unwrap().id.clone();
        let desc = |face: Option<String>| SessionDescriptor {
            session_id: 1,
            arch_id: plain.profile.id.clone(),
            image_file: plain.profile.image_file.clone(),
            face_profile_id: face,
            embed_dim: 2,
            face_embed_dim: None,
            caps: vec![],
            provider: None,
            gpu_name: None,
        };
        // 超集放宽:不需要人脸的 spec 可复用带人脸的合并会话。
        assert!(plain.matches(&desc(Some(face_id.clone()))));
        assert!(plain.matches(&desc(None)));
        // 需要人脸的 spec 必须精确匹配:CLIP-only 会话不满足 → 切换。
        assert!(!with_face.matches(&desc(None)));
        assert!(with_face.matches(&desc(Some(face_id))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn face_detect_embed_switches_session_and_validates_dims() {
        let dir = temp_dir("face-batch");
        let plain = test_spec(&dir);
        let with_face = test_spec_with_face(&dir);
        let init = Arc::new(AtomicUsize::new(0));
        let close = Arc::new(AtomicUsize::new(0));
        let spawn = Arc::new(AtomicUsize::new(0));

        let fitem = FaceItem {
            item_id: 1,
            cache_key: None,
            source_path: Some("x.webp".into()),
            fingerprint: "ff1".into(),
        };
        // 应答:1 项 Ok、1 张脸(dim=2 → blob 8 字节),带实际解码尺寸。
        let mut blob = Vec::new();
        for f in [0.1f32, 0.2] {
            blob.extend_from_slice(&f.to_le_bytes());
        }
        let face_ok = RawOutcome::Success {
            body: SuccessBody {
                face: Some(FaceBatchSuccess {
                    results: vec![FaceItemResult::Ok {
                        item_id: 1,
                        fingerprint: "ff1".into(),
                        faces: vec![exotic_protocol::FaceDet {
                            bbox: [1.0, 2.0, 3.0, 4.0],
                            landmarks: [[0.0; 2]; 5],
                            score: 0.9,
                        }],
                        width: 640,
                        height: 480,
                    }],
                }),
                ..Default::default()
            },
            blob,
        };
        let mut c = client_with_script(
            vec![vec![ok_batch(), face_ok]],
            Arc::clone(&init),
            Arc::clone(&close),
            Arc::clone(&spawn),
        );

        // 先建 CLIP-only 会话,再发 face 批 → 必须切换(close + 重 init 合并会话)。
        let _ = c.embed_batch(&plain, &items2(), &|| false).unwrap();
        let out = c
            .face_detect_embed(&with_face, &[fitem], 0.9, &|| false)
            .unwrap();
        assert_eq!(init.load(Ordering::SeqCst), 2, "face spec 应触发会话切换");
        assert!(close.load(Ordering::SeqCst) >= 1, "切换前应先 close 旧会话");
        assert_eq!(spawn.load(Ordering::SeqCst), 1, "切换不重建进程");
        match &out[0] {
            FaceItemOutcome::Ok {
                faces,
                embeddings,
                width,
                height,
            } => {
                assert_eq!(faces.len(), 1);
                assert_eq!(embeddings[0], vec![0.1, 0.2]);
                assert_eq!((*width, *height), (640, 480));
            }
            _ => panic!("期望 Ok"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
