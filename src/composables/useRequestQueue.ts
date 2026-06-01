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
  const queue = ref<QueueEntry[]>([])
  let flushTimer: ReturnType<typeof setTimeout> | null = null
  const inFlight = new Set<number>()

  function flush() {
    flushTimer = null
    if (queue.value.length === 0) return

    const batch = queue.value.splice(0, DEFAULTS.THUMB_BATCH_SIZE)
    const ids   = batch.map(e => e.id)

    // Remove from in-flight when done
    invoke<ThumbResult[]>(IPC.BATCH_REQUEST_THUMBNAILS, { itemIds: ids })
      .then(results => {
        results.forEach((r, i) => {
          batch[i]?.resolve(r)
          inFlight.delete(ids[i])
        })
      })
      .catch(err => {
        batch.forEach(e => {
          e.reject(err)
          inFlight.delete(e.id)
        })
      })
      .finally(() => {
        if (queue.value.length > 0) scheduleFlush()
      })
  }

  function scheduleFlush() {
    if (flushTimer !== null) return
    flushTimer = setTimeout(flush, 16) // next animation frame-ish
  }

  /**
   * Request a thumbnail for an item. Returns a promise that resolves with ThumbResult.
   * Automatically batches with other pending requests.
   */
  function request(id: number): Promise<ThumbResult> {
    if (inFlight.has(id)) {
      // Already in queue — find existing entry
      const existing = queue.value.find(e => e.id === id)
      if (existing) {
        return new Promise((resolve, reject) => {
          const orig = existing.resolve
          existing.resolve = (r) => { orig(r); resolve(r) }
          const origR = existing.reject
          existing.reject = (e) => { origR(e); reject(e) }
        })
      }
    }

    inFlight.add(id)
    return new Promise<ThumbResult>((resolve, reject) => {
      queue.value.push({ id, resolve, reject })
      scheduleFlush()
    })
  }

  /** Cancel a pending request (e.g. item scrolled out of view) */
  function cancel(id: number) {
    const idx = queue.value.findIndex(e => e.id === id)
    if (idx >= 0) {
      const [entry] = queue.value.splice(idx, 1)
      inFlight.delete(id)
      entry.reject(new Error('cancelled'))
    }
  }

  return { request, cancel }
}
