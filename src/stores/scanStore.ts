// src/stores/scanStore.ts
// Scan state management

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import type { ScanRoot } from '../types/media'
import type { ScanChannelPayload, ScanProgressPayload } from '../types/ipc'
import { IPC } from '../constants/ipc'

interface ScanProgress {
  scanned:    number
  total:      number
  currentDir: string
  isRunning:  boolean
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

  async function removeScanRoot(id: number) {
    await invoke(IPC.REMOVE_SCAN_ROOT, { id })
    scanRoots.value = scanRoots.value.filter(r => r.id !== id)
    delete progressMap.value[id]
  }

  async function startScan(rootId: number, onComplete?: () => void) {
    progressMap.value[rootId] = {
      scanned: 0, total: 0, currentDir: '', isRunning: true
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

  return {
    scanRoots, progressMap, isLoadingRoots,
    hasScanRoots, isAnyScanRunning,
    loadScanRoots, addScanRoot, removeScanRoot,
    startScan, stopScan, getProgress, clearDatabase,
  }
})
