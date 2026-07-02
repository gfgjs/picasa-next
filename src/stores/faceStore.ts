// src/stores/faceStore.ts
// Face store (F5) — face-recognition analysis status + pipeline controls.
// 人脸 store（F5）——人脸识别分析状态 + 流水线控制。
//
// Mirrors aiStore's analysis-management half (status poll + start/pause/stop/restart +
// auto-resume). Search/people-wall UI is deferred to F6.
// 镜像 aiStore 的分析管理部分（状态轮询 + 开始/暂停/停止/重启 + 自动续传）。
// 搜索/人物墙 UI 留待 F6。

import { defineStore } from 'pinia'
import { ref } from 'vue'
import { Channel } from '@tauri-apps/api/core'
import { invokeIpc } from '../utils/ipc'
import { IPC } from '../constants/ipc'
import { useUiStore } from './uiStore'
import { useAnalysisController } from '../composables/useAnalysisController'
import type { FaceStatusSummary, FaceModelInfo, FaceModelDownloadProgress } from '../types/face'

export const useFaceStore = defineStore('face', () => {
  // ── State ─────────────────────────────────────────────────────────────────
  const status = ref<FaceStatusSummary>({
    provider: '',
    gpuName: '',
    faceLoaded: false,
    totalItems: 0,
    processedItems: 0,
    pendingItems: 0,
    personCount: 0,
    faceCount: 0,
    isAnalyzing: false,
    analysisActive: false,
  })

  // 分析管理半部委托共享控制器（S6 去重，与 aiStore 共用 useAnalysisController）。face 专属:
  // start/restart 出错走 toast（模型未下载 / 与 CLIP 互斥被拒需用户可见）、pause/stop 走 console；
  // analyzedCount 取 processedItems（含失败项，故进度条仍到 100%）；无 onStarted。
  const analysis = useAnalysisController<FaceStatusSummary>({
    status,
    commands: {
      getStatus: IPC.GET_FACE_STATUS,
      start: IPC.START_FACE_ANALYSIS,
      pause: IPC.PAUSE_FACE_ANALYSIS,
      restart: IPC.RESTART_FACE_ANALYSIS,
      stop: IPC.STOP_FACE_ANALYSIS,
    },
    analyzedCount: () => status.value.processedItems,
    logTag: '[Face]',
    onError: (action, e) => {
      // start/restart 的后端错误（模型未下载 / 与 CLIP 互斥）须用户可见 → toast；其余记 console。
      if (action === 'start' || action === 'restart') {
        useUiStore().addToast('error', String(e))
      } else {
        console.error(`[Face] ${action} 分析出错 | analysis error:`, e)
      }
    },
  })
  const {
    fetchStatus,
    analyzeProgress,
    providerLabel,
    startAnalysis,
    pauseAnalysis,
    restartAnalysis,
    stopAnalysis,
    maybeAutoResume,
  } = analysis

  /** List the built-in face-model tracks + install status (F7, read-only). | 列出内置人脸模型轨+安装状态（F7 只读）。 */
  async function listFaceModels(): Promise<FaceModelInfo[]> {
    try {
      return await invokeIpc<FaceModelInfo[]>(IPC.LIST_FACE_MODEL_REGISTRY)
    } catch (e) {
      console.error('[Face] listFaceModels failed:', e)
      return []
    }
  }

  /** Download a face-model track's onnx files (verified size+sha256, resume), progress over a Channel.
   *  Only downloadable tracks (default YuNet+SFace) succeed; SCRFD/ArcFace is manual-import only.
   *  下载某人脸模型轨的 onnx（size+sha256 校验、断点续传），进度经 Channel 回传。仅可下载轨（默认
   *  YuNet+SFace）成功；SCRFD/ArcFace 仅手动导入。 */
  function downloadFaceModel(
    profileId: string,
    onProgress: (p: FaceModelDownloadProgress) => void,
  ): Promise<void> {
    const ch = new Channel<FaceModelDownloadProgress>()
    ch.onmessage = onProgress
    return invokeIpc(IPC.DOWNLOAD_FACE_MODEL, { profileId, onProgress: ch })
  }

  return {
    // state
    status,
    // computed
    analyzeProgress,
    providerLabel,
    // actions
    fetchStatus,
    startAnalysis,
    pauseAnalysis,
    restartAnalysis,
    stopAnalysis,
    maybeAutoResume,
    listFaceModels,
    downloadFaceModel,
  }
})
