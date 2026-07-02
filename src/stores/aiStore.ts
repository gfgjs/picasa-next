// src/stores/aiStore.ts
// AI store — manages engine status, semantic search state, and analysis progress.
// AI store — 管理引擎状态、语义搜索状态和分析进度。

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { Channel } from '@tauri-apps/api/core'
import { invokeIpc } from '../utils/ipc'
import { IPC } from '../constants/ipc'
import { useMediaStore } from './mediaStore'
import { useUiStore } from './uiStore'
import { useAnalysisController } from '../composables/useAnalysisController'
import type { AiStatusSummary, SearchMode, ModelRegistry, ModelDownloadProgress } from '../types/ai'

export const useAiStore = defineStore('ai', () => {
  // ── State ─────────────────────────────────────────────────────────────────
  const status = ref<AiStatusSummary>({
    provider: '',
    gpuName: '',
    vramGb: 0,
    batchSize: 0,
    activeFixedBatch: null,
    clipLoaded: false,
    totalItems: 0,
    analyzedItems: 0,
    pendingItems: 0,
    isAnalyzing: false,
    analysisActive: false,
  })

  const searchMode = ref<SearchMode>('mixed')
  const activeMixedQueryType = ref<'semantic' | 'normal' | 'none'>('none')
  const semanticQuery = ref('')
  const matchCount = ref(0)
  const similarityThreshold = ref(0.2)
  const isSearching = ref(false)
  const searchError = ref<string | null>(null)
  const previousGroupBy = ref<'date' | 'folder' | 'none'>('date')

  // 分析管理半部（轮询 / start·pause·restart·stop / 自动续传 / 进度 / providerLabel）委托共享
  // 控制器（S6 去重，与 faceStore 共用 useAnalysisController）。AI 专属:onStarted 清 searchError、
  // onError 走 console；analyzedCount 取 analyzedItems。
  const analysis = useAnalysisController<AiStatusSummary>({
    status,
    commands: {
      getStatus: IPC.GET_AI_STATUS,
      start: IPC.START_AI_ANALYSIS,
      pause: IPC.PAUSE_AI_ANALYSIS,
      restart: IPC.RESTART_AI_ANALYSIS,
      stop: IPC.STOP_AI_ANALYSIS,
    },
    analyzedCount: () => status.value.analyzedItems,
    logTag: '[AI]',
    onError: (action, e) => console.error(`[AI] ${action} 分析出错 | analysis error:`, e),
    onStarted: () => {
      searchError.value = null
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

  // ── Computed ──────────────────────────────────────────────────────────────
  const isSemanticMode = computed(
    () =>
      searchMode.value === 'semantic' ||
      (searchMode.value === 'mixed' && activeMixedQueryType.value === 'semantic'),
  )

  // ── Actions ───────────────────────────────────────────────────────────────

  /** Initialise AI engine on demand (lazy) | 按需初始化 AI 引擎（懒加载） */
  async function initEngine() {
    try {
      await invokeIpc(IPC.DETECT_AI_PROVIDER)
      await fetchStatus()
    } catch (e) {
      console.error('[AI] Init engine error | 初始化引擎错误:', e)
    }
  }

  /** Reset all embeddings and re-analyze | 重置所有嵌入向量并重新分析 */
  async function rebuildEmbeddings() {
    try {
      await invokeIpc(IPC.REBUILD_EMBEDDINGS)
      await fetchStatus()
    } catch (e) {
      console.error('[AI] Rebuild embeddings error | 重建嵌入向量错误:', e)
    }
  }

  /** Run a semantic search query | 运行语义搜索查询 */
  async function runSemanticSearch(query: string, limit = 1000) {
    if (!query.trim()) {
      semanticQuery.value = ''
      if (searchMode.value === 'mixed') {
        activeMixedQueryType.value = 'none'
        const ui = useUiStore()
        if (ui.sortWithinGroup === 'similarity') ui.setSortWithinGroup('datetime')
        if (ui.groupBy === 'none') ui.setGroupBy(previousGroupBy.value)
      }
      useMediaStore().invalidateLayout()
      return
    }

    isSearching.value = true
    searchError.value = null
    semanticQuery.value = query

    if (searchMode.value === 'mixed' && activeMixedQueryType.value !== 'semantic') {
      activeMixedQueryType.value = 'semantic'
      const ui = useUiStore()
      ui.searchQuery = ''
      if (ui.sortWithinGroup !== 'similarity') ui.setSortWithinGroup('similarity')
      if (ui.groupBy !== 'none') {
        previousGroupBy.value = ui.groupBy
        ui.setGroupBy('none')
      }
    }

    try {
      const count = await invokeIpc<number>(IPC.SEMANTIC_SEARCH_CMD, {
        query,
        limit,
      })
      matchCount.value = count
      // The results are stored in the ai_search_results table in DB.
      // We just need to invalidate the layout so MediaGrid reloads.
      useMediaStore().invalidateLayout()
    } catch (e) {
      searchError.value = String(e)
    } finally {
      isSearching.value = false
    }
  }

  function setNormalSearchQueryInMixedMode(query: string) {
    const ui = useUiStore()
    ui.searchQuery = query
    if (!query.trim()) {
      activeMixedQueryType.value = 'none'
    } else {
      activeMixedQueryType.value = 'normal'
      semanticQuery.value = ''
      if (ui.sortWithinGroup === 'similarity') ui.setSortWithinGroup('datetime')
      if (ui.groupBy === 'none') ui.setGroupBy(previousGroupBy.value)
    }
  }

  function toggleSearchMode() {
    if (searchMode.value === 'mixed') {
      setSearchMode('semantic')
    } else if (searchMode.value === 'semantic') {
      setSearchMode('normal')
    } else {
      setSearchMode('mixed')
    }
  }

  function setSearchMode(mode: SearchMode) {
    searchMode.value = mode
    const ui = useUiStore()

    // Reset queries and types on mode switch | 切换模式时重置查询和类型
    ui.searchQuery = ''
    semanticQuery.value = ''
    activeMixedQueryType.value = 'none'
    searchError.value = null

    if (mode === 'semantic') {
      if (ui.sortWithinGroup !== 'similarity') {
        ui.setSortWithinGroup('similarity')
      }
      if (ui.groupBy !== 'none') {
        previousGroupBy.value = ui.groupBy
        ui.setGroupBy('none')
      }
    } else {
      // For both 'normal' and 'mixed' (initial state), we want regular sorting/grouping
      if (ui.sortWithinGroup === 'similarity') {
        ui.setSortWithinGroup('datetime')
      }
      if (ui.groupBy === 'none') {
        ui.setGroupBy(previousGroupBy.value)
      }
    }

    useMediaStore().invalidateLayout()
  }

  /** Reload the AI engine | 重新加载 AI 引擎 */
  async function reloadAiEngine(): Promise<void> {
    try {
      await invokeIpc(IPC.RELOAD_AI_ENGINE)
      await fetchStatus()
    } catch (e) {
      console.error('[AI] Reload engine error | 重载引擎错误:', e)
      throw e
    }
  }

  // ── Model registry / library (Layer B) | 模型注册表 / 模型库 ──────────────────

  /** List the built-in model registry with install/active status | 列出内置模型注册表（含安装/激活状态） */
  async function listModelRegistry(): Promise<ModelRegistry> {
    return await invokeIpc<ModelRegistry>(IPC.LIST_MODEL_REGISTRY)
  }

  /** Switch the active model to a specific batch variant (validates installed; re-syncs status;
   *  reloads engine). `imageFile` = the variant's image onnx filename.
   *  切换激活模型到某 batch 变体（校验已安装；重同步状态；重载引擎）。`imageFile` = 该变体图像 onnx 文件名。 */
  async function setActiveModel(imageFile: string): Promise<void> {
    await invokeIpc(IPC.SET_ACTIVE_MODEL, { imageFile })
    await fetchStatus()
  }

  /** Download a specific variant's assets (image+extra+shared text+vocab), streaming progress.
   *  下载某变体的资产（图像+extra+共享文本塔+vocab），经 Channel 流式回传进度。 */
  function downloadModel(
    imageFile: string,
    onProgress: (p: ModelDownloadProgress) => void,
  ): Promise<void> {
    const ch = new Channel<ModelDownloadProgress>()
    ch.onmessage = onProgress
    return invokeIpc(IPC.DOWNLOAD_MODEL, { imageFile, onProgress: ch })
  }

  return {
    // state
    status,
    searchMode,
    activeMixedQueryType,
    semanticQuery,
    similarityThreshold,
    isSearching,
    searchError,
    matchCount,
    // computed
    analyzeProgress,
    providerLabel,
    isSemanticMode,
    // actions
    fetchStatus,
    initEngine,
    startAnalysis,
    pauseAnalysis,
    restartAnalysis,
    stopAnalysis,
    maybeAutoResume,
    rebuildEmbeddings,
    runSemanticSearch,
    setNormalSearchQueryInMixedMode,
    toggleSearchMode,
    setSearchMode,
    reloadAiEngine,
    listModelRegistry,
    setActiveModel,
    downloadModel,
  }
})
