// src/types/ai.ts
// AI module type definitions | AI 模块类型定义

/** Execution provider returned from backend | 后端返回的执行提供者 */
export type AiProvider = 'directml' | 'cuda' | 'coreml' | 'openvino' | 'cpu'

/** AI status summary from get_ai_status IPC | 来自 get_ai_status IPC 的 AI 状态摘要 */
export interface AiStatusSummary {
  provider: string
  gpuName: string
  vramGb: number | null
  batchSize: number
  /** 当前图像变体的固定 batch k（>1）；动态/单批为 null。驱动设置页 batch 最小限制 */
  activeFixedBatch: number | null
  clipLoaded: boolean
  totalItems: number
  analyzedItems: number
  pendingItems: number
  isAnalyzing: boolean
  /** Analysis is "desired" — running, or paused/interrupted with work left (问题7) */
  analysisActive: boolean
}

/** Semantic search result with similarity score | 带相似度分数的语义搜索结果 */
export interface SemanticSearchResult {
  id: number
  fileName: string
  mediaType: string
  width: number
  height: number
  thumbPath: string | null
  thumbhash: number[] | null
  thumbStatus: number
  /** Cosine similarity in [0, 1] | [0, 1] 范围内的余弦相似度 */
  similarity: number
}

/** Provider detection result | 提供者探测结果 */
export interface AiProviderInfo {
  provider: string
  gpuName: string
  clipLoaded: boolean
}

/** Search mode toggle | 搜索模式切换 */
export type SearchMode = 'mixed' | 'semantic' | 'normal'

// ── Model library (architecture → batch variants) | 模型库（架构 → batch 变体）──────

/** Image-tower batch-axis kind of a variant | 变体图像塔 batch 轴类型 */
export type BatchKind = 'single' | 'dynamic' | 'fixed'

/** One downloadable image-encoder batch variant | 一个可下载的图像编码器 batch 变体 */
export interface ModelVariant {
  /** Image onnx filename — also the download/switch identity | 图像 onnx 文件名，亦为下载/切换标识 */
  imageFile: string
  batchKind: BatchKind
  /** Fixed batch k (only when batchKind==='fixed') | 固定 batch k（仅 fixed 时非空） */
  fixedBatch: number | null
  /** Total bytes for this variant (image+extra+text+vocab); 0 if unknown/offline */
  sizeBytes: number
  installed: boolean
  active: boolean
}

/** One architecture group (= one repo folder) | 一个架构分组（= 仓库一个文件夹） */
export interface ModelArch {
  /** Stable arch id = embedding model_name | 稳定架构 id = 向量空间主键 */
  id: string
  displayName: string
  description: string
  imageSize: number
  embedDim: number
  sizeMb: number
  fp16: boolean
  active: boolean
  variants: ModelVariant[]
}

/** Result of list_model_registry | list_model_registry 的返回 */
export interface ModelRegistry {
  archs: ModelArch[]
  activeArchId: string
  activeImageFile: string
  /** False when online discovery failed (offline fallback) | 在线发现失败（离线回退）时为 false */
  online: boolean
}

/** Streamed download progress from download_model | download_model 流式下载进度 */
export interface ModelDownloadProgress {
  modelId: string
  currentFile: string
  fileIndex: number
  fileCount: number
  received: number
  total: number
  done: boolean
  error: string | null
}
