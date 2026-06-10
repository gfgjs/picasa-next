// src/stores/aiStore.ts
// AI store — manages engine status, semantic search state, and analysis progress.
// AI store — 管理引擎状态、语义搜索状态和分析进度。

import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useMediaStore } from './mediaStore'
import { useUiStore } from './uiStore'
import type { AiStatusSummary, SemanticSearchResult, SearchMode } from '../types/ai'

export const useAiStore = defineStore('ai', () => {
  // ── State ─────────────────────────────────────────────────────────────────
  const status = ref<AiStatusSummary>({
    provider: '',
    gpuName: '',
    vramGb: 0,
    batchSize: 0,
    clipLoaded: false,
    totalItems: 0,
    analyzedItems: 0,
    pendingItems: 0,
    isAnalyzing: false,
  })

  const searchMode = ref<SearchMode>('mixed')
  const activeMixedQueryType = ref<'semantic' | 'normal' | 'none'>('none')
  const semanticQuery = ref('')
  const matchCount = ref(0)
  const similarityThreshold = ref(0.20)
  const isSearching = ref(false)
  const searchError = ref<string | null>(null)
  const previousGroupBy = ref<'date' | 'folder' | 'none'>('date')

  // Auto-refresh using reactive watcher
  // 使用响应式侦听器自动刷新
  watch(() => status.value.isAnalyzing, (isAnalyzing, _, onCleanup) => {
    if (isAnalyzing) {
      const interval = setInterval(async () => {
        await fetchStatus()
      }, 2000)
      onCleanup(() => clearInterval(interval))
    }
  })
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

  const isSemanticMode = computed(() => searchMode.value === 'semantic' || (searchMode.value === 'mixed' && activeMixedQueryType.value === 'semantic'))

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
    const { useScanStore } = await import('./scanStore')
    const scan = useScanStore()
    if (!scan.hasScanRoots) {
      const ui = useUiStore()
      ui.addToast('warning', '请先添加需要扫描的文件夹')
      return
    }

    try {
      await invoke('start_ai_analysis')
      status.value.isAnalyzing = true
      // Embeddings are reset server-side — clear stale search results immediately
      // 服务端已重置嵌入向量，立即清除前端过时的搜索结果
      searchError.value = null
    } catch (e) {
      console.error('[AI] Start analysis error | 启动分析错误:', e)
    }
  }

  /** Stop the running analysis pipeline | 停止正在运行的分析流水线 */
  async function stopAnalysis() {
    try {
      await invoke('stop_ai_analysis')
      status.value.isAnalyzing = false
      await fetchStatus()
    } catch (e) {
      console.error('[AI] Stop analysis error | 停止分析错误:', e)
    }
  }

  /** Reset all embeddings and re-analyze | 重置所有嵌入向量并重新分析 */
  async function rebuildEmbeddings() {
    try {
      await invoke('rebuild_embeddings')
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
      const count = await invoke<number>('semantic_search_cmd', {
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
    stopAnalysis,
    rebuildEmbeddings,
    runSemanticSearch,
    setNormalSearchQueryInMixedMode,
    toggleSearchMode,
    setSearchMode,
    listAiModels,
    importAiModel,
    reloadAiEngine,
  }
})
