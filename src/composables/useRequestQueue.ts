// src/composables/useRequestQueue.ts
// Batched thumbnail request queue (§8.3)
// Collects item IDs and flushes in batches of THUMB_BATCH_SIZE.

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ThumbResult } from '../types/media'
import { IPC } from '../constants/ipc'
import { DEFAULTS } from '../constants/defaults'

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

  function flush() {
    flushTimer = null
    if (queue.value.length === 0) return

    const batch = queue.value.splice(0, DEFAULTS.THUMB_BATCH_SIZE)
    batch.forEach(id => inFlight.add(id))

    invoke<ThumbResult[]>(IPC.BATCH_REQUEST_THUMBNAILS, { itemIds: batch })
      .then(results => {
        results.forEach((r, i) => {
          const id = batch[i]
          inFlight.delete(id)
          const cbs = resolvers.get(id)
          if (cbs) {
            cbs.forEach(cb => cb.resolve(r))
            resolvers.delete(id)
          }
        })
      })
      .catch(err => {
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
        if (queue.value.length > 0) scheduleFlush()
      })
  }

  function scheduleFlush() {
    if (flushTimer !== null) return
    flushTimer = setTimeout(flush, 16)
  }

  function request(id: number): Promise<ThumbResult> {
    return new Promise((resolve, reject) => {
      if (!resolvers.has(id)) {
        resolvers.set(id, [])
      }
      resolvers.get(id)!.push({ resolve, reject })

      if (inFlight.has(id)) {
        return // Already being processed by backend, just wait for resolve
      }

      if (!queue.value.includes(id)) {
        queue.value.push(id)
        scheduleFlush()
      }
    })
  }

  function cancel(id: number) {
    const idx = queue.value.indexOf(id)
    if (idx >= 0) {
      queue.value.splice(idx, 1)
      const cbs = resolvers.get(id)
      if (cbs) {
        cbs.forEach(cb => cb.reject(new Error('cancelled')))
        resolvers.delete(id)
      }
    }
  }

  return { request, cancel }
}
