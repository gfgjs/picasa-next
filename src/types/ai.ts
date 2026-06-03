// src/types/ai.ts
// AI module type definitions | AI 模块类型定义

/** Execution provider returned from backend | 后端返回的执行提供者 */
export type AiProvider = 'directml' | 'cuda' | 'coreml' | 'openvino' | 'cpu'

/** AI status summary from get_ai_status IPC | 来自 get_ai_status IPC 的 AI 状态摘要 */
export interface AiStatusSummary {
  provider: string
  gpuName: string
  clipLoaded: boolean
  totalItems: number
  analyzedItems: number
  pendingItems: number
  isAnalyzing: boolean
}

/** Semantic search result with similarity score | 带相似度分数的语义搜索结果 */
export interface SemanticSearchResult {
  id: number
  fileName: string
  mediaType: string
  thumbPath: string | null
  thumbhash: number[] | null
  thumbStatus: number
  width: number
  height: number
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
export type SearchMode = 'filename' | 'semantic'
