// src/composables/useRequestQueue.ts
// Batched thumbnail request queue (§8.3)
// 批量缩略图请求队列 (§8.3)
// Collects item IDs and flushes in batches of THUMB_BATCH_SIZE.
// 收集项目 ID 并以 THUMB_BATCH_SIZE 为批次进行刷新。

import { invoke, Channel } from '@tauri-apps/api/core'
import type { ThumbResult } from '../types/media'
import { IPC } from '../constants/ipc'
import { DEFAULTS, THUMB_SIZE_TIERS } from '../constants/defaults'
import { useScanStore } from '../stores/scanStore'
import { useUiStore } from '../stores/uiStore'

/// No-progress timeout for a viewport thumbnail batch. If the backend delivers no
/// result for this long we assume a worker hung (corrupt/unsupported file) and release
/// the batch so the status indicator can't stick forever (问题9). Generous so a slow
/// disk / large RAW batch that is still making progress never trips it.
/// 视口缩略图批处理的「无进展」超时。若后端在此时长内无任何结果，判定为 worker 卡住
/// （损坏/不支持的文件）并释放该批，使状态指示不会永久卡住（问题9）。取值宽松，
/// 避免仍在推进的慢盘/大 RAW 批次误触发。
const STALL_MS = 30000

function getOptimalThumbTier(rowHeight: number): number {
  for (const tier of THUMB_SIZE_TIERS) {
    if (tier >= rowHeight) return tier
  }
  return THUMB_SIZE_TIERS[THUMB_SIZE_TIERS.length - 1]
}

type Resolver = (result: ThumbResult) => void

interface RequestWaiter {
  resolve: Resolver
  reject: (err: unknown) => void
}

interface RequestSlot {
  id: number
  state: 'queued' | 'inFlight'
  waiters: RequestWaiter[]
}

export function useRequestQueue() {
  const queue: RequestSlot[] = []
  let flushTimer: ReturnType<typeof setTimeout> | null = null
  const inFlight = new Set<RequestSlot>()
  const activeSlots = new Map<number, RequestSlot>()

  let isFlushing = false

  function syncStats() {
    const scan = useScanStore()
    scan.autoThumbQueueSize = queue.length
    scan.autoThumbInFlight = inFlight.size
  }

  function detachSlot(slot: RequestSlot) {
    inFlight.delete(slot)
    if (activeSlots.get(slot.id) === slot) {
      activeSlots.delete(slot.id)
    }
  }

  function resolveSlot(slot: RequestSlot, result: ThumbResult) {
    const waiters = slot.waiters.splice(0)
    waiters.forEach((cb) => cb.resolve(result))
    detachSlot(slot)
  }

  function rejectSlot(slot: RequestSlot, err: unknown) {
    const waiters = slot.waiters.splice(0)
    waiters.forEach((cb) => cb.reject(err))
    detachSlot(slot)
  }

  function flush() {
    flushTimer = null
    if (queue.length === 0) return

    isFlushing = true
    const batch = queue.splice(0, DEFAULTS.THUMB_BATCH_SIZE)
    const batchIds = batch.map((slot) => slot.id)
    const batchSlots = new Map<number, RequestSlot>(batch.map((slot) => [slot.id, slot] as const))
    const pending = new Set(batch)
    batch.forEach((slot) => {
      slot.state = 'inFlight'
      inFlight.add(slot)
    })

    syncStats()

    // ── Stall watchdog (问题9) ───────────────────────────────────────────────
    // If a backend worker hangs on a corrupt/unsupported file, the batch invoke never
    // resolves and `.finally` never runs, leaving the "处理中 N 项" indicator stuck
    // forever. Arm a no-progress timer (reset on each delivered result); if no result
    // arrives for STALL_MS, release this batch's still-in-flight ids so the UI recovers
    // regardless of the backend. `released` makes the first of {stall, finally} win, so
    // a late-resolving hung invoke can't clobber a subsequently-started batch.
    // ── 停滞看门狗（问题9） ───────────────────────────────────────────────────
    // 若后端 worker 卡在损坏/不支持的文件上，批处理 invoke 永不 resolve、`.finally` 永不
    // 执行，「处理中 N 项」会永久卡住。装一个「无进展」计时器（每送达一个结果就重置）；
    // 若 STALL_MS 内无任何结果，则释放本批仍在途的 id，使 UI 不依赖后端即可恢复。
    // `released` 让 {停滞, finally} 中先到者生效，避免迟到的卡死 invoke 冲掉后续新批。
    let released = false
    let stallTimer: ReturnType<typeof setTimeout> | null = null

    const releaseBatch = (reason: string, rejectErr?: unknown) => {
      if (released) return
      released = true
      if (stallTimer !== null) {
        clearTimeout(stallTimer)
        stallTimer = null
      }
      if (rejectErr !== undefined) {
        for (const slot of Array.from(pending)) {
          pending.delete(slot)
          rejectSlot(slot, rejectErr)
        }
        console.warn(`[useRequestQueue] ${reason}`)
      } else {
        console.debug(`[useRequestQueue] ${reason}`)
      }
      isFlushing = false
      syncStats()
      if (queue.length > 0) scheduleFlush()
    }

    const armStall = () => {
      if (released) return
      if (stallTimer !== null) clearTimeout(stallTimer)
      stallTimer = setTimeout(() => {
        releaseBatch(
          `batch stalled ${STALL_MS}ms with no progress — releasing ${batch.length} id(s)`,
          new Error('thumb batch stalled'),
        )
      }, STALL_MS)
    }

    const onResult = new Channel<ThumbResult>()
    onResult.onmessage = (r) => {
      const slot = batchSlots.get(r.itemId)
      if (!slot || !pending.has(slot)) return
      armStall() // progress made — reset the no-progress timer | 有进展 — 重置无进展计时器

      // 关键点：按本批次的 slot 收尾，而不是按 id 清全局 Map。
      // 这样同一 id 在旧批次结果返回后重新排队时，新 Promise 不会被旧批次误删或误拒绝。
      pending.delete(slot)
      resolveSlot(slot, r)
      syncStats()
      if (pending.size === 0) {
        releaseBatch('batch drained by item results')
      }
    }

    const ui = useUiStore()
    const targetSize = getOptimalThumbTier(ui.gridRowHeight)
    armStall()

    invoke(IPC.BATCH_REQUEST_THUMBNAILS, { itemIds: batchIds, targetSize, onResult })
      .catch((err) => {
        console.error(`[useRequestQueue] batch failed: ${err}`, batchIds)
      })
      .finally(() => {
        // Normal completion: clear the watchdog and release anything still pending
        // (e.g. ids the backend skipped). No-op if the stall watchdog already fired.
        // 正常完成：关掉看门狗并释放仍挂起的项（如后端跳过的 id）。若停滞看门狗已触发则空操作。
        releaseBatch('batch finished', new Error('Batch finished without result'))
      })
  }

  function scheduleFlush() {
    if (flushTimer !== null) return
    if (isFlushing) return
    flushTimer = setTimeout(flush, 50)
  }

  function request(id: number): Promise<ThumbResult> {
    return new Promise((resolve, reject) => {
      const existing = activeSlots.get(id)
      if (existing) {
        existing.waiters.push({ resolve, reject })
        return // Already queued or being processed by backend, share the same slot.
        // 已排队或已在后端处理中：复用同一个 slot，确保重复 Promise 一起收尾。
      }

      const slot: RequestSlot = {
        id,
        state: 'queued',
        waiters: [{ resolve, reject }],
      }
      activeSlots.set(id, slot)
      queue.push(slot)
      syncStats()
      scheduleFlush()
    })
  }

  function cancel(id: number) {
    const slot = activeSlots.get(id)
    if (!slot) return

    if (slot.state === 'queued') {
      const idx = queue.indexOf(slot)
      if (idx >= 0) {
        queue.splice(idx, 1)
      }
      rejectSlot(slot, new Error('cancelled'))
      syncStats()
    } else {
      // in-flight 请求不能安全地按 id 取消：同一 id 很可能马上重新进入视口。
      // 这里只取消当前前端等待者，保留后端 single-flight；新 Promise 会挂回同一 slot 并随结果 resolve。
      slot.waiters.splice(0).forEach((cb) => cb.reject(new Error('cancelled')))
    }
  }

  return { request, cancel }
}
