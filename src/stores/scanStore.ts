// src/stores/scanStore.ts
// Scan state management
// 扫描状态管理

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { ScanRoot } from '../types/media'
import type {
  ScanChannelPayload,
  MediaEnrichedPayload,
  EnrichmentCompletedPayload,
} from '../types/ipc'
import { IPC, EVENTS } from '../constants/ipc'
import i18n from '../i18n'
import { useMediaStore } from './mediaStore'
import { useUiStore } from './uiStore'

interface ScanProgress {
  scanned: number
  total: number
  currentDir: string
  isRunning: boolean
  status?: 'discovering' | 'scanning' | 'enriching'
}

export const useScanStore = defineStore('scan', () => {
  const scanRoots = ref<ScanRoot[]>([])
  const progressMap = ref<Record<number, ScanProgress>>({})
  const isLoadingRoots = ref(false)

  const hasScanRoots = computed(() => scanRoots.value.length > 0)
  const isAnyScanRunning = computed(() => Object.values(progressMap.value).some((p) => p.isRunning))

  async function loadScanRoots() {
    isLoadingRoots.value = true
    try {
      scanRoots.value = await invoke<ScanRoot[]>(IPC.LIST_SCAN_ROOTS)
    } finally {
      isLoadingRoots.value = false
    }
  }

  async function addScanRoot(path: string, alias?: string): Promise<ScanRoot> {
    const root = await invoke<ScanRoot>(IPC.ADD_SCAN_ROOT, { path, alias: alias ?? null })
    if (!scanRoots.value.some((r) => r.id === root.id)) {
      scanRoots.value.push(root)
    }
    return root
  }

  async function removeScanRoot(id: number) {
    await invoke(IPC.REMOVE_SCAN_ROOT, { id })
    scanRoots.value = scanRoots.value.filter((r) => r.id !== id)
    delete progressMap.value[id]
  }

  // Global enrichment listeners — attached once. Background enrichment (EXIF +
  // dimension backfill) is now the long pole, so its progress drives the bar
  // through the `enriching` phase, well after the (now near-instant) fast insert.
  // 全局 enrichment 监听 — 仅注册一次。后台 enrichment（EXIF + 尺寸补全）现在是
  // 耗时大头，其进度在 `enriching` 阶段驱动进度条，远在（如今近乎瞬时的）快速入库之后。
  let enrichListenersReady = false
  async function ensureEnrichmentListeners() {
    if (enrichListenersReady) return
    enrichListenersReady = true
    const media = useMediaStore()
    await listen<MediaEnrichedPayload>(EVENTS.MEDIA_ENRICHED, (e) => {
      const { rootId, enrichedCount, total } = e.payload
      progressMap.value[rootId] = {
        scanned: enrichedCount,
        total,
        currentDir: '',
        isRunning: true,
        status: 'enriching',
      }
    })
    await listen<EnrichmentCompletedPayload>(EVENTS.ENRICHMENT_COMPLETED, (e) => {
      const { rootId, errorCode } = e.payload
      if (progressMap.value[rootId]) {
        progressMap.value[rootId].isRunning = false
      }
      // 后台补全异常终止（T11 可观测）：携带稳定 errorCode → 弹 warning，
      // 让失败对用户可见，而非伪装成「正常完成」只进日志。取消不带 code，不打扰。
      if (errorCode) {
        const ui = useUiStore()
        ui.addToast('warning', i18n.global.t('statusbar.enrichIncomplete'), 6000)
      }
      // Final stats refresh so counts are exact once everything is enriched.
      // 最终刷新一次统计，使全部补全后的计数精确。
      media.loadStats()
    })
  }

  async function startScan(rootId: number, onComplete?: () => void) {
    await ensureEnrichmentListeners()

    progressMap.value[rootId] = {
      scanned: 0,
      total: 0,
      currentDir: '',
      isRunning: true,
      status: 'discovering',
    }

    // Throttle mid-scan gallery refreshes so already-inserted rows surface
    // progressively (newest first) instead of the grid staying blank until the
    // whole fast scan finishes. loadStats() bumps totalItems → the grid recomputes.
    // 节流扫描中的画廊刷新，让已入库的行（最新在前）渐进式显示，而不是等整段
    // 快速扫描结束才出图。loadStats() 触动 totalItems → 网格重算。
    const media = useMediaStore()
    const ui = useUiStore()
    let lastRefresh = 0

    const channel = new Channel<ScanChannelPayload>()
    channel.onmessage = (msg) => {
      if (msg.type === 'progress') {
        // 扁平联合后 msg 在此分支已 narrow 到 { type:'progress' } & ScanProgressPayload，
        // 顶层字段直接可读，无需 `as unknown as` 强制 cast（P1-5）。
        progressMap.value[rootId] = {
          scanned: msg.scanned,
          total: msg.total,
          currentDir: msg.currentDir,
          isRunning: true,
          status: msg.status,
        }
        if (msg.status === 'scanning') {
          const now = Date.now()
          if (now - lastRefresh > 1000) {
            lastRefresh = now
            media.loadStats()
          }
        }
      } else if (msg.type === 'completed') {
        // Fast insert done — but the scan isn't "finished": hand off to the
        // enriching phase (driven by the global listeners) and keep running.
        // 快速入库完成 — 但扫描并未"结束"：移交到 enriching 阶段（由全局监听驱动）并保持运行中。
        progressMap.value[rootId] = {
          scanned: 0,
          total: 0,
          currentDir: '',
          isRunning: true,
          status: 'enriching',
        }
        // 缺失检测可观测（Part2 §3.2）：本次差集标记了「缺失」项 → 提示用户（非删除、可自动恢复）。
        if (msg.markedMissing > 0) {
          ui.addToast(
            'info',
            i18n.global.t('statusbar.filesMarkedMissing', { count: msg.markedMissing }),
            5000,
          )
        }
        onComplete?.()
      } else if (msg.type === 'error') {
        progressMap.value[rootId].isRunning = false
        console.error('Scan error for root', rootId, msg.error)
        // 扫描失败对用户可见（S7）：此前仅 console.error，失败被静默吞掉。
        ui.addToast('error', i18n.global.t('statusbar.scanError', { error: msg.error }), 6000)
      }
    }

    try {
      await invoke(IPC.START_SCAN, {
        rootId,
        onProgress: channel,
        groupBy: ui.groupBy,
        sortWithinGroup: ui.sortWithinGroup,
        sortOrder: ui.sortOrder,
      })
    } catch (e) {
      if (progressMap.value[rootId]) {
        progressMap.value[rootId].isRunning = false
      }
      throw e
    }
  }

  async function stopScan(rootId: number) {
    await invoke(IPC.STOP_SCAN, { rootId })
    if (progressMap.value[rootId]) {
      progressMap.value[rootId].isRunning = false
    }
  }

  function getProgress(rootId: number): ScanProgress | null {
    return progressMap.value[rootId] ?? null
  }

  async function clearDatabase() {
    await invoke(IPC.CLEAR_DATABASE)
    scanRoots.value = []
    progressMap.value = {}
  }

  // ── Full Thumbnail Generation ─────────────────────────────────────────────

  interface ThumbGenProgress {
    generated: number
    total: number
    isRunning: boolean
    status: 'idle' | 'running' | 'completed' | 'cancelled' | 'error'
    currentItem?: string
    phase?: string
  }

  const thumbGenProgress = ref<ThumbGenProgress>({
    generated: 0,
    total: 0,
    isRunning: false,
    status: 'idle',
    currentItem: undefined,
    phase: undefined,
  })

  // State for automatic thumbnail generation (triggered by scrolling)
  const autoThumbQueueSize = ref(0)
  const autoThumbInFlight = ref(0)

  async function startFullThumbnailGeneration() {
    if (!hasScanRoots.value) {
      const { useUiStore } = await import('./uiStore')
      const ui = useUiStore()
      ui.addToast('warning', i18n.global.t('common.addScanFolderFirst'))
      return
    }

    thumbGenProgress.value = {
      generated: 0,
      total: 0,
      isRunning: true,
      status: 'running',
      currentItem: undefined,
      phase: undefined,
    }

    const channel = new Channel<{
      generated: number
      total: number
      status: 'error' | 'completed' | 'running' | 'cancelled' | 'idle'
      currentItem?: string
      phase?: string
    }>()
    channel.onmessage = (msg) => {
      thumbGenProgress.value = {
        generated: msg.generated,
        total: msg.total,
        isRunning: msg.status === 'running',
        status: msg.status,
        currentItem: msg.currentItem,
        phase: msg.phase,
      }
      // When generation finishes (completed/cancelled), invalidate layout
      // so the grid re-fetches fresh thumb_status/thumb_path from DB.
      // 当生成完成（完成/取消）时，使布局失效，
      // 以便网格从数据库重新获取最新的 thumb_status/thumb_path。
      if (msg.status === 'completed' || msg.status === 'cancelled') {
        const media = useMediaStore()
        media.invalidateLayout()
      }
    }

    try {
      await invoke(IPC.START_FULL_THUMBNAIL_GENERATION, { onProgress: channel })
    } catch (e) {
      thumbGenProgress.value.isRunning = false
      thumbGenProgress.value.status = 'error'
      throw e
    }
  }

  async function stopFullThumbnailGeneration() {
    await invoke(IPC.STOP_FULL_THUMBNAIL_GENERATION)
    thumbGenProgress.value.isRunning = false
    if (thumbGenProgress.value.status === 'running') {
      thumbGenProgress.value.status = 'cancelled'
    }
  }

  return {
    scanRoots,
    progressMap,
    isLoadingRoots,
    hasScanRoots,
    isAnyScanRunning,
    loadScanRoots,
    addScanRoot,
    removeScanRoot,
    startScan,
    stopScan,
    getProgress,
    clearDatabase,
    thumbGenProgress,
    startFullThumbnailGeneration,
    stopFullThumbnailGeneration,
    autoThumbQueueSize,
    autoThumbInFlight,
  }
})
