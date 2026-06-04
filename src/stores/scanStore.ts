// src/stores/scanStore.ts
// Scan state management
// 扫描状态管理

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import type { ScanRoot } from '../types/media'
import type { ScanChannelPayload, ScanProgressPayload } from '../types/ipc'
import { IPC } from '../constants/ipc'
import { useMediaStore } from './mediaStore'

interface ScanProgress {
  scanned:    number
  total:      number
  currentDir: string
  isRunning:  boolean
  status?:    'discovering' | 'scanning'
}

export const useScanStore = defineStore('scan', () => {
  const scanRoots   = ref<ScanRoot[]>([])
  const progressMap = ref<Record<number, ScanProgress>>({})
  const isLoadingRoots = ref(false)

  const hasScanRoots = computed(() => scanRoots.value.length > 0)
  const isAnyScanRunning = computed(() =>
    Object.values(progressMap.value).some(p => p.isRunning)
  )

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
    if (!scanRoots.value.some(r => r.id === root.id)) {
      scanRoots.value.push(root)
    }
    return root
  }

  async function checkFolderOverlap(path: string): Promise<ScanRoot[]> {
    return await invoke<ScanRoot[]>('check_folder_overlap', { path })
  }

  async function mergeScanRoots(newPath: string, alias: string | null, overlappingIds: number[]): Promise<ScanRoot> {
    const root = await invoke<ScanRoot>('merge_scan_roots', { newPath, alias, overlappingIds })
    scanRoots.value = scanRoots.value.filter(r => !overlappingIds.includes(r.id))
    if (!scanRoots.value.some(r => r.id === root.id)) {
      scanRoots.value.push(root)
    }
    return root
  }

  async function removeScanRoot(id: number) {
    await invoke(IPC.REMOVE_SCAN_ROOT, { id })
    scanRoots.value = scanRoots.value.filter(r => r.id !== id)
    delete progressMap.value[id]
  }

  async function startScan(rootId: number, onComplete?: () => void) {
    progressMap.value[rootId] = {
      scanned: 0, total: 0, currentDir: '', isRunning: true, status: 'discovering'
    }

    const channel = new Channel<ScanChannelPayload>()
    channel.onmessage = (msg) => {
      if (msg.type === 'progress') {
        const p = msg as unknown as ScanProgressPayload
        progressMap.value[rootId] = {
          scanned:    p.scanned,
          total:      p.total,
          currentDir: p.currentDir,
          isRunning:  true,
          status:     p.status,
        }
      } else if (msg.type === 'completed') {
        progressMap.value[rootId] = {
          ...progressMap.value[rootId],
          isRunning: false,
        }
        onComplete?.()
      } else if (msg.type === 'error') {
        progressMap.value[rootId].isRunning = false
        console.error('Scan error for root', rootId, (msg as any).error)
      }
    }

    try {
      await invoke(IPC.START_SCAN, { rootId, onProgress: channel })
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
    scanRoots.value   = []
    progressMap.value = {}
  }

  // ── Full Thumbnail Generation ─────────────────────────────────────────────
  
  interface ThumbGenProgress {
    generated: number
    total:     number
    isRunning: boolean
    status:    'idle' | 'running' | 'completed' | 'cancelled' | 'error'
    currentItem?: string
  }

  const thumbGenProgress = ref<ThumbGenProgress>({
    generated: 0,
    total: 0,
    isRunning: false,
    status: 'idle',
    currentItem: undefined
  })

  // State for automatic thumbnail generation (triggered by scrolling)
  const autoThumbQueueSize = ref(0)
  const autoThumbInFlight = ref(0)

  async function startFullThumbnailGeneration() {
    thumbGenProgress.value = {
      generated: 0,
      total: 0,
      isRunning: true,
      status: 'running',
      currentItem: undefined
    }

    const channel = new Channel<any>()
    channel.onmessage = (msg: any) => {
      thumbGenProgress.value = {
        generated: msg.generated,
        total: msg.total,
        isRunning: msg.status === 'running',
        status: msg.status,
        currentItem: msg.currentItem
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
    scanRoots, progressMap, isLoadingRoots,
    hasScanRoots, isAnyScanRunning,
    loadScanRoots, addScanRoot, checkFolderOverlap, mergeScanRoots, removeScanRoot,
    startScan, stopScan, getProgress, clearDatabase,
    thumbGenProgress, startFullThumbnailGeneration, stopFullThumbnailGeneration,
    autoThumbQueueSize, autoThumbInFlight
  }
})
