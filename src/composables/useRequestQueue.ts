// src/composables/useRequestQueue.ts
// Batched thumbnail request queue (§8.3)
// 批量缩略图请求队列 (§8.3)
// Collects item IDs and flushes in batches of THUMB_BATCH_SIZE.
// 收集项目 ID 并以 THUMB_BATCH_SIZE 为批次进行刷新。

import { ref } from 'vue'
import { invoke, Channel } from '@tauri-apps/api/core'
import type { ThumbResult } from '../types/media'
import { IPC } from '../constants/ipc'
import { DEFAULTS, THUMB_SIZE_TIERS } from '../constants/defaults'
import { useScanStore } from '../stores/scanStore'
import { useUiStore } from '../stores/uiStore'

function getOptimalThumbTier(rowHeight: number): number {
  for (const tier of THUMB_SIZE_TIERS) {
    if (tier >= rowHeight) return tier
  }
  return THUMB_SIZE_TIERS[THUMB_SIZE_TIERS.length - 1]
}

type Resolver = (result: ThumbResult) => void

interface QueueEntry {
  id:       number
  resolve:  Resolver
  reject:   (err: unknown) => void
}

export function useRequestQueue() {
  const queue = ref<number[]>([])
  let flushTimer: ReturnType<typeof setTimeout> | null = null
  const inFlight = new Set<number>()
  const resolvers = new Map<number, { resolve: Resolver, reject: (err: unknown) => void }[]>()

  let isFlushing = false

  function flush() {
    flushTimer = null
    if (queue.value.length === 0) return

    isFlushing = true
    const batch = queue.value.splice(0, DEFAULTS.THUMB_BATCH_SIZE)
    batch.forEach(id => inFlight.add(id))

    const scan = useScanStore()
    scan.autoThumbQueueSize = queue.value.length
    scan.autoThumbInFlight = inFlight.size
    
    console.log(`[useRequestQueue] flush: starting batch of ${batch.length}, inFlight=${inFlight.size}, queue=${queue.value.length}`)

    const onResult = new Channel<ThumbResult>()
    onResult.onmessage = (r) => {
      inFlight.delete(r.itemId)
      const cbs = resolvers.get(r.itemId)
      if (cbs) {
        cbs.forEach(cb => cb.resolve(r))
        resolvers.delete(r.itemId)
      }
    }

    const ui = useUiStore()
    const targetSize = getOptimalThumbTier(ui.gridRowHeight)

    invoke(IPC.BATCH_REQUEST_THUMBNAILS, { itemIds: batch, targetSize, onResult })
      .then(() => {
        console.log(`[useRequestQueue] batch finished`)
      })
      .catch(err => {
        console.error(`[useRequestQueue] batch failed: ${err}`, batch)
        batch.forEach(id => {
          inFlight.delete(id)
          const cbs = resolvers.get(id)
          if (cbs) {
            cbs.forEach(cb => cb.reject(err))
            resolvers.delete(id)
          }
        })
      })
      .finally(() => {
        batch.forEach(id => {
          inFlight.delete(id)
          const cbs = resolvers.get(id)
          if (cbs) {
            cbs.forEach(cb => cb.reject(new Error('Batch finished without result')))
            resolvers.delete(id)
          }
        })
        isFlushing = false
        scan.autoThumbInFlight = inFlight.size
        if (queue.value.length > 0) scheduleFlush()
      })
  }

  function scheduleFlush() {
    if (flushTimer !== null) return
    if (isFlushing) return
    flushTimer = setTimeout(flush, 50)
  }

  function request(id: number): Promise<ThumbResult> {
    return new Promise((resolve, reject) => {
      if (!resolvers.has(id)) {
        resolvers.set(id, [])
      }
      resolvers.get(id)!.push({ resolve, reject })

      if (inFlight.has(id)) {
        return // Already being processed by backend, just wait for resolve
               // 后端已经在处理中，只需等待 resolve
      }

      if (!queue.value.includes(id)) {
        queue.value.push(id)
        useScanStore().autoThumbQueueSize = queue.value.length
        scheduleFlush()
      }
    })
  }

  function cancel(id: number) {
    const idx = queue.value.indexOf(id)
    if (idx >= 0) {
      queue.value.splice(idx, 1)
      useScanStore().autoThumbQueueSize = queue.value.length
      const cbs = resolvers.get(id)
      if (cbs) {
        cbs.forEach(cb => cb.reject(new Error('cancelled')))
        resolvers.delete(id)
      }
    } else if (inFlight.has(id)) {
      // If it's already sent to the backend but the user scrolled away, abort the heavy decode.
      invoke(IPC.CANCEL_THUMBNAIL_REQUEST, { id }).catch(console.error)
    }
  }

  return { request, cancel }
}
