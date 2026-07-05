// src/types/face.ts
// Face-recognition module type definitions (F5) | 人脸识别模块类型定义（F5）

/** Face status summary from get_face_status IPC | 来自 get_face_status IPC 的人脸状态摘要 */
export interface FaceStatusSummary {
  provider: string
  gpuName: string
  /** Both face sessions (detector + embedder) loaded | 人脸双 session（检测器 + 嵌入器）均已加载 */
  faceLoaded: boolean
  totalItems: number
  /** Images whose detection finished (Done OR Error), not "images with faces" | 完成检测的图像数（完成或错误），非"有脸的图像数" */
  processedItems: number
  pendingItems: number
  /** Clustered persons count (people-wall roster size) | 已聚类人物数（人物墙名册规模） */
  personCount: number
  /** Total detected faces for the active model | 当前模型下检测到的人脸总数 */
  faceCount: number
  isAnalyzing: boolean
  /** Analysis is "desired" — running, or paused/interrupted with work left | 分析处于「期望运行」 */
  analysisActive: boolean
}

/** One face-model track in the read-only registry (F7) | 只读模型库的一条人脸模型轨（F7） */
export interface FaceModelInfo {
  id: string
  displayName: string
  description: string
  detector: string
  embedder: string
  embedDim: number
  /** Commercial use allowed (false = research-only track, e.g. InsightFace) | 是否允许商用 */
  commercialOk: boolean
  license: string
  sizeMb: number
  /** Both onnx files present on disk | 两个 onnx 文件均在磁盘上 */
  installed: boolean
  /** Currently-active track | 当前激活轨 */
  active: boolean
  /** Has a verified download manifest (one-click). false = manual import only | 有已校验清单（可一键下载）；false=仅手动导入 */
  downloadable: boolean
  /** Inference cross-checked vs upstream reference; false = activation refused | 已与上游参考对拍;false=拒绝激活(防静默算错) */
  verified: boolean
}

/** Download progress streamed over a Channel from download_face_model (camelCase, mirrors CLIP) | 来自 download_face_model 的下载进度（camelCase，镜像 CLIP） */
export interface FaceModelDownloadProgress {
  modelId: string
  currentFile: string
  fileIndex: number
  fileCount: number
  received: number
  total: number
  done: boolean
  error: string | null
}
