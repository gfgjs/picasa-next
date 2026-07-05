// crates/exotic-protocol/src/message.rs
//! 协议消息体（v3 Part2 §3.3）。两端共享同一份定义，JSON 只放控制字段；缩略图等二进制走同帧 blob。

use serde::{Deserialize, Serialize};

/// Host→Worker 握手开场（Hello 帧）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloBody {
    /// Host 语义版本（展示/兼容用，不承担回滚防护，R11）。
    pub host_version: String,
    /// Host 期望的协议版本；与 Worker `PROTOCOL_VERSION` 不一致即握手失败。
    pub protocol_version: u16,
    /// Host 能接收的最大 blob；Worker 据此自限输出。
    pub max_blob_len: u32,
}

/// Worker→Host 握手应答（Ready 帧）。Host 校验 worker_id/version/protocol/capabilities。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadyBody {
    pub worker_id: String,
    pub worker_version: String,
    pub protocol_version: u16,
    /// Worker 实测支持的能力（如 `["thumbnail"]`）；只声明 probe 通过范围。
    pub capabilities: Vec<String>,
    pub max_blob_len: u32,
}

/// Host→Worker 处理请求（Request 帧）。按 `op` 分流；首发只有 thumbnail。
///
/// `target_long_edge` 必须是**吸附后档位**（R5）——与指纹里的 `target_tier` 一致，
/// 否则同档不同请求 size 会算出不同指纹、反复重做。
// v2 起含 f32 字段(FaceDetectEmbed.det_score_thresh),不再派生 Eq。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RequestBody {
    Thumbnail {
        item_id: i64,
        source_path: String,
        target_long_edge: u32,
        input_fingerprint: String,
    },
    Metadata {
        item_id: i64,
        source_path: String,
        input_fingerprint: String,
    },
    /// 模型加载 = 显式会话请求(v2;Part4 D3 §2:不进进程握手、不加帧类型;
    /// 其 300s 超时由 host 侧 per-op timeout 表配置)。响应 = Success 帧 +
    /// `SuccessBody.session`(SessionReady 不是新帧)。
    SessionInit {
        /// host 侧单调分配;SessionClose/诊断日志引用。
        session_id: u64,
        /// 多角色模型载荷(Part6 §3.2.1a:合并单 ai-worker 一次声明 CLIP 图/文 +
        /// YuNet + SFace;worker 按 role 取各自 handle)。
        models: Vec<ModelDescriptor>,
        model_profile: ModelProfileSnapshot,
        /// `ModelHandle::Path` 的归属校验根:worker 侧 canonicalize 后须以此为
        /// 前缀,越界回 `ModelLoadFailed`(D1 §3,防宿主被劫持后诱导任意读)。
        models_root: String,
        /// 受限缓存根:worker 只读 `{ai_cache_dir}/{key[..2]}/{key}.webp`,拒越界
        /// (Part6 §3.2.1a ②,补「worker 只收 cache_keys、不知缓存根」缺口)。
        ai_cache_dir: String,
        /// 图像塔 EP:"directml"|"cpu";文本塔由 worker 内硬编码 CPU(Part4 §8.6)。
        image_provider: String,
    },
    /// 显式卸载会话(host 主导生命周期:空闲计时到期/切换模型前;D3 §4)。
    SessionClose { session_id: u64 },
    /// CLIP 批量嵌入,一 Request = 一批(§8.1 决策1/方案B:传 cache_keys 而非
    /// tensor,blob≈0 无 MAX_BLOB 压力)。响应 = Success 帧 + `SuccessBody.embed`,
    /// 嵌入本体在同帧 blob(布局见 [`EmbedBatchSuccess`])。
    EmbedBatch { items: Vec<EmbedItem> },
    /// 人脸检测+嵌入批(与 EmbedBatch 同构逐项化;几何走 JSON、嵌入走 blob)。
    FaceDetectEmbed {
        items: Vec<FaceItem>,
        /// 检测置信度阈值快照(行为参数,进指纹):同图不同阈值产出不同结果。
        /// 取值随 host 侧 face profile(YuNet 0.9 / SCRFD 0.5)。
        det_score_thresh: f32,
    },
    /// CLIP 文本编码(v2 additive,T17):语义搜索查询向量。T16 删主进程 ort+tokenizers
    /// 后,文本塔只存在于 worker(其 EP 恒 CPU,Part4 §8.6),故此 op 是搜索链路的必经
    /// 载体——T10 三源合并时漏列,T15 发现缺口、T17 补齐(additive,不动 PROTOCOL_VERSION)。
    /// 响应 = Success 帧 + `SuccessBody.text_embed`,向量本体在同帧 blob(按 texts 顺序连续,
    /// 每项 `embed_dim × f32(LE)`)。**全批原子**:文本编码无逐项 IO 失败模式(不读盘、
    /// tokenizer 接受任意字符串),任一失败即整批 Failure,不做逐项 Ok/Err。
    /// 不新增 capability:文本塔与 CLIP 图像塔同属 `embedding` 会话,凡会话就绪即可服务。
    EncodeText { texts: Vec<String> },
}

impl RequestBody {
    /// 取请求关联的单一 item_id(响应核对用)。会话/批量 op 无单值语义,返回 None
    /// ——批量的逐项核对走 [`EmbedBatchSuccess`]/[`FaceBatchSuccess`] 的 per-item 字段。
    pub fn item_id(&self) -> Option<i64> {
        match self {
            RequestBody::Thumbnail { item_id, .. } | RequestBody::Metadata { item_id, .. } => {
                Some(*item_id)
            }
            RequestBody::SessionInit { .. }
            | RequestBody::SessionClose { .. }
            | RequestBody::EmbedBatch { .. }
            | RequestBody::FaceDetectEmbed { .. }
            | RequestBody::EncodeText { .. } => None,
        }
    }

    /// 取请求的单一输入指纹(响应核对用);会话/批量 op 返回 None,理由同 [`Self::item_id`]。
    pub fn input_fingerprint(&self) -> Option<&str> {
        match self {
            RequestBody::Thumbnail {
                input_fingerprint, ..
            }
            | RequestBody::Metadata {
                input_fingerprint, ..
            } => Some(input_fingerprint),
            RequestBody::SessionInit { .. }
            | RequestBody::SessionClose { .. }
            | RequestBody::EmbedBatch { .. }
            | RequestBody::FaceDetectEmbed { .. }
            | RequestBody::EncodeText { .. } => None,
        }
    }
}

/// 模型载荷句柄(Part4 D1 两级通道;v2 一次定型,避免二次破坏性升版)。
///
/// - `Path`:明文权重(P2 首期全部模型)——models 目录内文件的**绝对路径**,worker
///   `commit_from_file` 直接加载;须先做 models_root 归属校验(见 SessionInit 字段注)。
/// - `Named`:AES 加密权重(④ 变现后)——主进程解密后写具名共享内存,worker 按名
///   map 后 `commit_from_memory`;名称格式 `pn-{8B hex}-{16B CSPRNG hex}`(D1 §4)。
/// - 刻意不设 `Fd`/`Win32Handle` 变体:匿名句柄跨 exec 进程无效(Part4 §3.7.3),
///   具名方案三平台闭合;除非实测具名开销不可接受才另启(D1 裁决②)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ModelHandle {
    Path(String),
    Named(String),
}

/// 模型角色(Part6 §3.2.1a):一个 session 载多模型,worker 按角色寻址。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    ImageEncoder,
    TextEncoder,
    FaceDetect,
    FaceRecog,
}

/// 单个模型载荷描述:role→handle 寻址(§3.2.1a)+ 逐模型完整性字段(D1 §3)。
/// worker 加载前校验 len/sha256(均对**明文**;Named 通道 map 后校验),不符回
/// `ModelLoadFailed`(terminal)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub role: ModelRole,
    pub handle: ModelHandle,
    /// 明文字节数(Named 通道即共享内存映射长度)。
    pub len: u64,
    /// 明文 sha256(64 位小写 hex,不带算法前缀)。
    pub sha256: String,
}

/// 模型 profile 快照(Part4 §8.6 / Part6 §8.3):worker 侧预/后处理按 `arch_id` 从
/// 其内建注册表取几何/归一化/tokenizer 等参数,host 不逐字段下发。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelProfileSnapshot {
    /// CLIP 架构族 id(worker 内建注册表键)。
    pub arch_id: String,
    pub image_file: String,
    pub text_file: String,
    /// CLIP 图像批容量(静态 batch;EmbedBatch 单批 items 数不得超过)。
    pub batch_size: u32,
    /// 人脸 profile id(= `faces.model_name`);None = 本 session 不载人脸角色。
    /// (T10 补充:合并 session 需同时声明 CLIP 与人脸两套 profile,§3.2.1a 原稿仅 CLIP 形。)
    pub face_profile_id: Option<String>,
}

/// EmbedBatch 单项(Part6 §3.2.1a 逐项化:每项独立 fingerprint,陈旧/错位防护到项)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbedItem {
    pub item_id: i64,
    /// ai_cache 键;worker 拼 `{ai_cache_dir}/{key[..2]}/{key}.webp` 读图自解码。
    pub cache_key: String,
    pub fingerprint: String,
}

/// FaceDetectEmbed 单项:`cache_key`/`source_path` 至少给一,cache 缺失或分辨率不足时
/// host 以 source_path 派活(人脸检测对分辨率敏感;信任语义同 Thumbnail.source_path)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FaceItem {
    pub item_id: i64,
    pub cache_key: Option<String>,
    pub source_path: Option<String>,
    pub fingerprint: String,
}

/// 能力名标准字符串:Ready.capabilities / SessionReadyBody.caps / DB
/// `exotic_tasks.capability` 共用,与 host 侧 `catalog::Capability::as_str` 一一对应,
/// 防两端字面量漂移(G5:embedding/face_detect_embed 为 v2 新增)。
pub mod capability {
    pub const THUMBNAIL: &str = "thumbnail";
    pub const METADATA: &str = "metadata";
    pub const EMBEDDING: &str = "embedding";
    pub const FACE_DETECT_EMBED: &str = "face_detect_embed";
}

/// Worker→Host 成功（Success 帧 + 同帧 blob）。
///
/// v2 起 `item_id`/`input_fingerprint` 为 Option:会话/批量 op 的 Success 无单项语义
/// (批量逐项核对字段在 `embed`/`face` 内)。thumbnail/metadata 的线上形状不变:
/// Some 序列化为原样数值/字符串,新增三字段 None 时不序列化。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SuccessBody {
    pub item_id: Option<i64>,
    pub input_fingerprint: Option<String>,
    /// 缩略图固定 `image/webp`；Host 二次校验。
    pub mime: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// metadata 能力的结构化结果（thumbnail 时为 None）。
    pub metadata: Option<serde_json::Value>,
    /// SessionInit 的就绪应答(D3 §2:SessionReady 不是新帧)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionReadyBody>,
    /// EmbedBatch 的逐项结果(嵌入本体在同帧 blob)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed: Option<EmbedBatchSuccess>,
    /// FaceDetectEmbed 的逐项结果(嵌入本体在同帧 blob)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<FaceBatchSuccess>,
    /// EncodeText 的应答(向量本体在同帧 blob;v2 additive,T17)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_embed: Option<TextEmbedSuccess>,
}

/// SessionInit 成功应答体(经 `SuccessBody.session` 携带)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionReadyBody {
    /// CLIP 嵌入维度;EmbedBatch 响应 blob 每项长度 = embed_dim × 4 字节。
    pub embed_dim: u32,
    /// 人脸嵌入维度(载了 FaceRecog 角色才有)。
    pub face_embed_dim: Option<u32>,
    /// 本会话实际可服务的能力(如 ["embedding","face_detect_embed"]),host 据此派活。
    pub caps: Vec<String>,
    /// 实际选用的执行提供器回声(如 "directml"/"cpu";T16 additive:host 删进程内引擎后
    /// provider 探测只发生在 worker 侧,host 借此写回 `ai_provider` 配置供状态栏显示)。
    /// serde(default) 容旧 worker 帧;None = 旧帧/未回报,host 保留既有配置值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// GPU 显示名回声(CPU 时为空串;语义同 provider,写回 `ai_gpu_name`)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_name: Option<String>,
}

/// EncodeText 成功应答体(经 `SuccessBody.text_embed` 携带)。全批原子,无逐项结构;
/// `count` 供 host 与请求 `texts.len()` 双向核对(「不信任 worker」:blob 长度校验之外
/// 再锁一道数量,op 错配/漏项在协议边界即违例)。blob 布局见 `RequestBody::EncodeText`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextEmbedSuccess {
    /// 已编码文本数;必须等于请求 texts 数(host 校验,不符即协议违例)。
    pub count: u32,
}

/// EmbedBatch 逐项结果(§3.2.1a ③)。`results` 与请求 `items` **严格同序同长**:
/// host 收到先断言长度(不符整批判 InternalError 重试),再逐项比对 item_id +
/// fingerprint,不符即丢弃该项(陈旧/错位防护);逐项 Err 不连坐整批。
///
/// 嵌入本体不进 JSON——128 项 × 768d 的 JSON 文本会撞 MAX_JSON_LEN(1MiB),且违反
/// 「JSON 只放控制字段、二进制走同帧 blob」天条(T10 对 §3.2.1a `Ok{embedding}` 的
/// 修正)。blob 布局:按 `results` 中 **Ok 项的顺序**连续排布,每项 `embed_dim ×
/// f32(LE)`;host 校验 `blob.len() == ok_count × embed_dim × 4`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbedBatchSuccess {
    pub results: Vec<EmbedResult>,
}

/// 单项嵌入结果。回带 item_id+fingerprint 供 host 核对(延续单项 input_fingerprint
/// 核对语义到批量)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EmbedResult {
    /// 成功:嵌入在同帧 blob 中(布局见 [`EmbedBatchSuccess`])。
    Ok { item_id: i64, fingerprint: String },
    /// 该项失败(如缓存缺失/解码失败),整批其余项不受影响。
    Err {
        item_id: i64,
        fingerprint: String,
        code: WorkerErrorCode,
    },
}

/// 单张脸的几何输出(原图坐标系)。嵌入不在此(在同帧 blob,见 [`FaceBatchSuccess`]);
/// 质量分由 host 从 bbox+score 派生,不进协议。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceDet {
    /// x, y, w, h。
    pub bbox: [f32; 4],
    /// 5 关键点(双眼/鼻尖/双嘴角),对齐模板用。
    pub landmarks: [[f32; 2]; 5],
    /// 检测置信度。
    pub score: f32,
}

/// FaceDetectEmbed 逐项结果,同序同长/逐项核对语义同 [`EmbedBatchSuccess`]。
/// blob 布局:按 results 中 Ok 项顺序、项内按 `faces` 顺序,每脸 `face_embed_dim ×
/// f32(LE)`;host 校验总长 = 全部 Ok 项脸数之和 × face_embed_dim × 4。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceBatchSuccess {
    pub results: Vec<FaceItemResult>,
}

/// 单项人脸结果(0 张脸也是 Ok,faces 为空)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FaceItemResult {
    Ok {
        item_id: i64,
        fingerprint: String,
        faces: Vec<FaceDet>,
        /// 解码图实际宽高(face 波 additive 补,不动 PROTOCOL_VERSION):FaceDet 几何是
        /// 解码图像素坐标,而解码发生在 worker(源可能是缩略图档位)——host 归一化落库与
        /// quality 派生必须用**实际**解码尺寸,预测尺寸有舍入误差。`default` 容旧帧,
        /// host 校验将 0 视作协议违例(两端同仓同步分发,正常不会出现)。
        #[serde(default)]
        width: u32,
        #[serde(default)]
        height: u32,
    },
    Err {
        item_id: i64,
        fingerprint: String,
        code: WorkerErrorCode,
    },
}

/// 稳定错误码（v3 Part2 §3.3）。整数语义跨版本固定，serde 用 snake_case 字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerErrorCode {
    /// 该格式变体不被支持（CMYK/16-bit/PSB/无 merged image 等）→ terminal，等源/版本变化再失效。
    UnsupportedVariant,
    /// 输入畸形（截断/非法字节）→ terminal。
    MalformedInput,
    /// 触及资源上限（尺寸/像素/内存/输出字节）→ terminal。
    ResourceLimit,
    /// 暂时 IO/文件占用 → retryable。
    IoError,
    /// Worker 内部错误（含 panic 兜底）→ retryable。
    InternalError,
    /// GPU EP 不可用/显存不足(v2,G6)→ retryable:让步/降 CPU EP 由 host 决策。
    GpuUnavailable,
    /// 会话未加载或已卸载(v2,G6)→ retryable:host 重发 SessionInit 后重派。
    SessionExpired,
    /// 模型加载失败:文件缺失/校验不符/路径越界/ort 构建失败(v2,G6)→ terminal,
    /// 重试同一 handle 无意义,host 标记该模型待重下载校验。
    ModelLoadFailed,
    /// 嵌入维度与 profile 声明不符(v2,G6)→ terminal:数据完整性红线。
    EmbedDimMismatch,
}

impl WorkerErrorCode {
    /// 稳定字符串标识（与 DB `exotic_tasks.last_error_code`、序列化形态一致）。
    pub fn as_str(self) -> &'static str {
        match self {
            WorkerErrorCode::UnsupportedVariant => "unsupported_variant",
            WorkerErrorCode::MalformedInput => "malformed_input",
            WorkerErrorCode::ResourceLimit => "resource_limit",
            WorkerErrorCode::IoError => "io_error",
            WorkerErrorCode::InternalError => "internal_error",
            WorkerErrorCode::GpuUnavailable => "gpu_unavailable",
            WorkerErrorCode::SessionExpired => "session_expired",
            WorkerErrorCode::ModelLoadFailed => "model_load_failed",
            WorkerErrorCode::EmbedDimMismatch => "embed_dim_mismatch",
        }
    }

    /// 该错误码的**默认** retryable 语义（Worker 也可在 FailureBody 显式覆盖）。
    pub fn default_retryable(self) -> bool {
        matches!(
            self,
            WorkerErrorCode::IoError
                | WorkerErrorCode::InternalError
                | WorkerErrorCode::GpuUnavailable
                | WorkerErrorCode::SessionExpired
        )
    }
}

/// Worker→Host 失败（Failure 帧）。`retryable` 由 Worker 给出，Host 据错误码 + 该位决定重试/终态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailureBody {
    /// v2 起 Option:会话/批量 op 的整批失败无单项语义(逐项失败走 Success 侧 per-item Err)。
    pub item_id: Option<i64>,
    pub input_fingerprint: Option<String>,
    pub code: WorkerErrorCode,
    pub retryable: bool,
    /// 用户可见诊断信息；**不得**含完整绝对路径或 License token（v3 Part2 §3.3）。
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_tagged_roundtrip() {
        let req = RequestBody::Thumbnail {
            item_id: 42,
            source_path: "a.psd".into(),
            target_long_edge: 480,
            input_fingerprint: "fp".into(),
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""op":"thumbnail""#));
        let back: RequestBody = serde_json::from_str(&s).unwrap();
        assert_eq!(back, req);
        assert_eq!(back.item_id(), Some(42));
        assert_eq!(back.input_fingerprint(), Some("fp"));
    }

    #[test]
    fn error_code_serde_and_retryable() {
        assert_eq!(
            serde_json::to_string(&WorkerErrorCode::UnsupportedVariant).unwrap(),
            r#""unsupported_variant""#
        );
        assert!(!WorkerErrorCode::UnsupportedVariant.default_retryable());
        assert!(WorkerErrorCode::IoError.default_retryable());
        assert!(WorkerErrorCode::InternalError.default_retryable());
        assert!(!WorkerErrorCode::MalformedInput.default_retryable());
    }

    #[test]
    fn session_init_roundtrip_and_tags() {
        let req = RequestBody::SessionInit {
            session_id: 7,
            models: vec![
                ModelDescriptor {
                    role: ModelRole::ImageEncoder,
                    handle: ModelHandle::Path("C:/models/img.onnx".into()),
                    len: 10,
                    sha256: "ab".repeat(32),
                },
                ModelDescriptor {
                    role: ModelRole::FaceRecog,
                    handle: ModelHandle::Named("pn-0011223344556677-ff".into()),
                    len: 20,
                    sha256: "cd".repeat(32),
                },
            ],
            model_profile: ModelProfileSnapshot {
                arch_id: "clip-vit".into(),
                image_file: "img.onnx".into(),
                text_file: "txt.onnx".into(),
                batch_size: 32,
                face_profile_id: Some("yunet-sface".into()),
            },
            models_root: "C:/models".into(),
            ai_cache_dir: "C:/cache/ai".into(),
            image_provider: "directml".into(),
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""op":"session_init""#));
        assert!(s.contains(r#""kind":"path""#));
        assert!(s.contains(r#""kind":"named""#));
        assert!(s.contains(r#""role":"image_encoder""#));
        let back: RequestBody = serde_json::from_str(&s).unwrap();
        assert_eq!(back, req);
        // 会话 op 无单项语义。
        assert_eq!(back.item_id(), None);
        assert_eq!(back.input_fingerprint(), None);
    }

    #[test]
    fn batch_ops_roundtrip_and_tags() {
        let embed = RequestBody::EmbedBatch {
            items: vec![EmbedItem {
                item_id: 1,
                cache_key: "aabbcc".into(),
                fingerprint: "fp1".into(),
            }],
        };
        let s = serde_json::to_string(&embed).unwrap();
        assert!(s.contains(r#""op":"embed_batch""#));
        assert_eq!(serde_json::from_str::<RequestBody>(&s).unwrap(), embed);
        assert_eq!(embed.item_id(), None);

        let face = RequestBody::FaceDetectEmbed {
            items: vec![FaceItem {
                item_id: 2,
                cache_key: None,
                source_path: Some("D:/photos/a.jpg".into()),
                fingerprint: "fp2".into(),
            }],
            det_score_thresh: 0.9,
        };
        let s = serde_json::to_string(&face).unwrap();
        assert!(s.contains(r#""op":"face_detect_embed""#));
        assert_eq!(serde_json::from_str::<RequestBody>(&s).unwrap(), face);

        let close = RequestBody::SessionClose { session_id: 9 };
        let s = serde_json::to_string(&close).unwrap();
        assert!(s.contains(r#""op":"session_close""#));
        assert_eq!(serde_json::from_str::<RequestBody>(&s).unwrap(), close);

        // EncodeText(T17 additive):无单项语义;应答体 count 往返一致。
        let enc = RequestBody::EncodeText {
            texts: vec!["海边日落".into(), "cat".into()],
        };
        let s = serde_json::to_string(&enc).unwrap();
        assert!(s.contains(r#""op":"encode_text""#));
        assert_eq!(serde_json::from_str::<RequestBody>(&s).unwrap(), enc);
        assert_eq!(enc.item_id(), None);
        assert_eq!(enc.input_fingerprint(), None);
        let te = TextEmbedSuccess { count: 2 };
        let s = serde_json::to_string(&te).unwrap();
        assert_eq!(serde_json::from_str::<TextEmbedSuccess>(&s).unwrap(), te);
    }

    #[test]
    fn face_item_result_dims_roundtrip_and_legacy_default() {
        // face 波 additive:Ok 补 width/height,往返一致。
        let ok = FaceItemResult::Ok {
            item_id: 7,
            fingerprint: "fp7".into(),
            faces: vec![],
            width: 800,
            height: 600,
        };
        let s = serde_json::to_string(&ok).unwrap();
        assert_eq!(serde_json::from_str::<FaceItemResult>(&s).unwrap(), ok);
        // 旧帧(无 width/height)仍可解析,serde default 落 0——host 校验层负责拒收 0。
        let legacy = r#"{"status":"ok","item_id":7,"fingerprint":"fp7","faces":[]}"#;
        match serde_json::from_str::<FaceItemResult>(legacy).unwrap() {
            FaceItemResult::Ok { width, height, .. } => assert_eq!((width, height), (0, 0)),
            _ => panic!("期望 Ok"),
        }
    }

    #[test]
    fn success_body_thumbnail_wire_shape_unchanged() {
        // v1 时代 thumbnail Success 的线上形状在 v2 下不变:新增三字段 None 时不序列化。
        let body = SuccessBody {
            item_id: Some(1),
            input_fingerprint: Some("fp".into()),
            mime: Some("image/webp".into()),
            width: Some(10),
            height: Some(20),
            metadata: None,
            session: None,
            embed: None,
            face: None,
            text_embed: None,
        };
        let s = serde_json::to_string(&body).unwrap();
        assert!(!s.contains("session"));
        assert!(!s.contains("embed"));
        assert!(!s.contains("face"));
        assert!(!s.contains("text_embed"));
        assert!(s.contains(r#""item_id":1"#));
        // v1 形状 JSON(无新字段)仍可解析。
        let legacy = r#"{"item_id":2,"input_fingerprint":"f","mime":null,"width":null,"height":null,"metadata":null}"#;
        let back: SuccessBody = serde_json::from_str(legacy).unwrap();
        assert_eq!(back.item_id, Some(2));
        assert!(back.session.is_none() && back.embed.is_none() && back.face.is_none());
        assert!(back.text_embed.is_none());
    }

    #[test]
    fn embed_result_status_tags() {
        let ok = EmbedResult::Ok {
            item_id: 1,
            fingerprint: "f1".into(),
        };
        let err = EmbedResult::Err {
            item_id: 2,
            fingerprint: "f2".into(),
            code: WorkerErrorCode::GpuUnavailable,
        };
        let s = serde_json::to_string(&vec![ok.clone(), err.clone()]).unwrap();
        assert!(s.contains(r#""status":"ok""#));
        assert!(s.contains(r#""status":"err""#));
        assert!(s.contains(r#""code":"gpu_unavailable""#));
        let back: Vec<EmbedResult> = serde_json::from_str(&s).unwrap();
        assert_eq!(back, vec![ok, err]);
    }

    #[test]
    fn v2_error_codes_serde_and_retryable() {
        assert_eq!(WorkerErrorCode::GpuUnavailable.as_str(), "gpu_unavailable");
        assert_eq!(WorkerErrorCode::SessionExpired.as_str(), "session_expired");
        assert_eq!(
            WorkerErrorCode::ModelLoadFailed.as_str(),
            "model_load_failed"
        );
        assert_eq!(
            WorkerErrorCode::EmbedDimMismatch.as_str(),
            "embed_dim_mismatch"
        );
        assert!(WorkerErrorCode::GpuUnavailable.default_retryable());
        assert!(WorkerErrorCode::SessionExpired.default_retryable());
        assert!(!WorkerErrorCode::ModelLoadFailed.default_retryable());
        assert!(!WorkerErrorCode::EmbedDimMismatch.default_retryable());
        assert_eq!(
            serde_json::to_string(&WorkerErrorCode::SessionExpired).unwrap(),
            r#""session_expired""#
        );
    }

    #[test]
    fn failure_body_optional_item_fields() {
        // 整批失败:无单项字段;缺省字段可解析(Option 特化)。
        let legacy = r#"{"code":"session_expired","retryable":true,"message":"m"}"#;
        let back: FailureBody = serde_json::from_str(legacy).unwrap();
        assert_eq!(back.item_id, None);
        assert_eq!(back.input_fingerprint, None);
        assert_eq!(back.code, WorkerErrorCode::SessionExpired);
    }

    #[test]
    fn capability_names_stable() {
        assert_eq!(capability::THUMBNAIL, "thumbnail");
        assert_eq!(capability::METADATA, "metadata");
        assert_eq!(capability::EMBEDDING, "embedding");
        assert_eq!(capability::FACE_DETECT_EMBED, "face_detect_embed");
    }
}
