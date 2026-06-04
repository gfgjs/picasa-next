// src/stores/aiStore.ts
// AI store — manages engine status, semantic search state, and analysis progress.
// AI store — 管理引擎状态、语义搜索状态和分析进度。

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { AiStatusSummary, SemanticSearchResult, SearchMode } from '../types/ai'

export const useAiStore = defineStore('ai', () => {
  // ── State ─────────────────────────────────────────────────────────────────
  const status = ref<AiStatusSummary>({
    provider: '',
    gpuName: '',
    clipLoaded: false,
    totalItems: 0,
    analyzedItems: 0,
    pendingItems: 0,
    isAnalyzing: false,
  })

  const searchMode = ref<SearchMode>('filename')
  const semanticQuery = ref('')
  const semanticResults = ref<SemanticSearchResult[]>([])
  const standardResults = ref<import('../types/media').SearchResult[]>([])
  const isSearching = ref(false)
  const searchError = ref<string | null>(null)

  // Auto-refresh interval handle
  // 自动刷新间隔句柄
  let _statusInterval: ReturnType<typeof setInterval> | null = null

  // ── Computed ──────────────────────────────────────────────────────────────
  const analyzeProgress = computed(() => {
    if (status.value.totalItems === 0) return 0
    return Math.round((status.value.analyzedItems / status.value.totalItems) * 100)
  })

  const providerLabel = computed(() => {
    const p = status.value.provider
    if (p === 'directml') return 'DirectML'
    if (p === 'cuda') return 'CUDA'
    if (p === 'coreml') return 'CoreML'
    if (p === 'openvino') return 'OpenVINO'
    if (p === 'cpu') return 'CPU'
    return '未初始化'
  })

  const isSemanticMode = computed(() => searchMode.value === 'semantic')

  // ── Actions ───────────────────────────────────────────────────────────────

  /** Fetch latest AI status from backend | 从后端获取最新 AI 状态 */
  async function fetchStatus() {
    try {
      const s = await invoke<AiStatusSummary>('get_ai_status')
      status.value = s
    } catch {
      // Silently ignore — status bar shows last known state | 静默忽略 — 状态栏显示最后已知状态
    }
  }

  /** Initialise AI engine on demand (lazy) | 按需初始化 AI 引擎（懒加载） */
  async function initEngine() {
    try {
      await invoke('detect_ai_provider')
      await fetchStatus()
    } catch (e) {
      console.error('[AI] Init engine error | 初始化引擎错误:', e)
    }
  }

  /** Start background embedding analysis | 启动后台嵌入向量分析 */
  async function startAnalysis() {
    try {
      await invoke('start_ai_analysis')
      status.value.isAnalyzing = true
      // Embeddings are reset server-side — clear stale search results immediately
      // 服务端已重置嵌入向量，立即清除前端过时的搜索结果
      semanticResults.value = []
      searchError.value = null
      startStatusPolling()
    } catch (e) {
      console.error('[AI] Start analysis error | 启动分析错误:', e)
    }
  }

  /** Stop the running analysis pipeline | 停止正在运行的分析流水线 */
  async function stopAnalysis() {
    try {
      await invoke('stop_ai_analysis')
      status.value.isAnalyzing = false
      stopStatusPolling()
      await fetchStatus()
    } catch (e) {
      console.error('[AI] Stop analysis error | 停止分析错误:', e)
    }
  }

  /** Reset all embeddings and re-analyze | 重置所有嵌入向量并重新分析 */
  async function rebuildEmbeddings() {
    try {
      await invoke('rebuild_embeddings')
      semanticResults.value = []
      await fetchStatus()
    } catch (e) {
      console.error('[AI] Rebuild embeddings error | 重建嵌入向量错误:', e)
    }
  }

  /** Run a semantic search query | 运行语义搜索查询 */
  async function runSemanticSearch(query: string, limit = 50) {
    if (!query.trim()) {
      semanticResults.value = []
      return
    }

    isSearching.value = true
    searchError.value = null
    semanticQuery.value = query

    try {
      const results = await invoke<SemanticSearchResult[]>('semantic_search_cmd', {
        query,
        limit,
      })
      semanticResults.value = results
    } catch (e) {
      searchError.value = String(e)
      semanticResults.value = []
    } finally {
      isSearching.value = false
    }
  }

  /** Run a standard filename search query | 运行标准文件名搜索查询 */
  async function runStandardSearch(query: string, limit = 100) {
    if (!query.trim()) {
      standardResults.value = []
      return
    }

    isSearching.value = true
    searchError.value = null

    try {
      const results = await invoke<import('../types/media').SearchResult[]>('search_media', {
        query,
        filter: {},
        limit,
      })
      standardResults.value = results
    } catch (e) {
      searchError.value = String(e)
      standardResults.value = []
    } finally {
      isSearching.value = false
    }
  }

  /** Toggle between filename and semantic search modes | 在文件名和语义搜索模式之间切换 */
  function toggleSearchMode() {
    searchMode.value = searchMode.value === 'filename' ? 'semantic' : 'filename'
    semanticResults.value = []
    standardResults.value = []
    searchError.value = null
  }

  function setSearchMode(mode: SearchMode) {
    searchMode.value = mode
    if (mode === 'filename') {
      semanticResults.value = []
    } else {
      standardResults.value = []
    }
  }

  /** Poll AI status every 2 seconds during analysis | 分析期间每 2 秒轮询 AI 状态 */
  function startStatusPolling() {
    stopStatusPolling()
    _statusInterval = setInterval(async () => {
      await fetchStatus()
      if (!status.value.isAnalyzing) {
        stopStatusPolling()
      }
    }, 2000)
  }

  function stopStatusPolling() {
    if (_statusInterval !== null) {
      clearInterval(_statusInterval)
      _statusInterval = null
    }
  }

  /** List available AI models | 列出可用的 AI 模型 */
  async function listAiModels(): Promise<string[]> {
    try {
      return await invoke<string[]>('list_ai_models')
    } catch (e) {
      console.error('[AI] List models error | 列出模型错误:', e)
      return []
    }
  }

  /** Import an AI model | 导入 AI 模型 */
  async function importAiModel(sourcePath: string): Promise<void> {
    await invoke('import_ai_model', { sourcePath })
  }

  /** Reload the AI engine | 重新加载 AI 引擎 */
  async function reloadAiEngine(): Promise<void> {
    try {
      await invoke('reload_ai_engine')
      await fetchStatus()
    } catch (e) {
      console.error('[AI] Reload engine error | 重载引擎错误:', e)
      throw e
    }
  }

  return {
    // state
    status,
    searchMode,
    semanticQuery,
    semanticResults,
    standardResults,
    isSearching,
    searchError,
    // computed
    analyzeProgress,
    providerLabel,
    isSemanticMode,
    // actions
    fetchStatus,
    initEngine,
    startAnalysis,
    stopAnalysis,
    rebuildEmbeddings,
    runSemanticSearch,
    runStandardSearch,
    toggleSearchMode,
    setSearchMode,
    startStatusPolling,
    stopStatusPolling,
    listAiModels,
    importAiModel,
    reloadAiEngine,
  }
})
