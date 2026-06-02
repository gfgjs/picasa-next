// src/composables/useVirtualScroll.ts
// Row-level virtual scrolling composable (§10.3)
//
// Key design constraint:
//   scrollHeight must equal totalHeight at all times.
//   If scrollHeight > totalHeight, the browser clamps scrollTop when
//   the user reaches the bottom, fires a new scroll event, which triggers
//   another fetch → infinite request loop.
//
//   The caller must ensure the DOM representation of rows does NOT add extra
//   height beyond what the layout reports (e.g. no marginBottom on row wrappers;
//   use CSS gap on the flex container instead, since flex gap does not add
//   trailing space after the last child).
//
// Concurrency model:
//   A boolean `busy` flag ensures only one fetch is in-flight at a time.
//   While busy, incoming updateVisible calls schedule themselves via rAF so
//   the LAST position wins when the current fetch completes.

import { ref, onMounted, onBeforeUnmount } from 'vue'
import { DEFAULTS } from '../constants/defaults'
import type { LayoutRow } from '../types/layout'

const LOG = '[VirtualScroll]'

interface UseVirtualScrollOptions {
  totalHeight:  () => number
  totalRows:    () => number
  fetchRowsByY: (topY: number, bottomY: number) => Promise<LayoutRow[]>
  containerRef: () => HTMLElement | null
}

export function useVirtualScroll(opts: UseVirtualScrollOptions) {
  const containerHeight = ref(0)
  const visibleRows     = ref<LayoutRow[]>([])
  const startIndex      = ref(0)
  const paddingTop      = ref(0)
  const paddingBottom   = ref(0)
  const isFetching      = ref(false)

  let rafId:          number | null = null
  let resizeObserver: ResizeObserver | null = null

  let currentFetchId = 0
  let lastFetchedTop    = -1
  let lastFetchedBottom = -1
  let ticking = false

  // ── Scroll handler (called by host @scroll) ────────────────────────────

  let pendingUpdate = false

  function onScroll() {
    scheduleUpdate()
  }

  function scheduleUpdate(force = false) {
    if (force) {
      lastFetchedTop = -1
    }
    
    // If a fetch is already in flight, flag that we need another update after it finishes
    if (isFetching.value) {
      pendingUpdate = true
      return
    }

    if (!ticking) {
      ticking = true
      requestAnimationFrame(async () => {
        // Await the fetch so we don't start overlapping requests
        await updateVisible(false)
        ticking = false
        
        // If the user kept scrolling while we were fetching, run it again to catch up
        if (pendingUpdate) {
          pendingUpdate = false
          scheduleUpdate()
        }
      })
    }
  }

  // ── Compute visible window ─────────────────────────────────────────────

  async function updateVisible(force: boolean = false) {
    if (force) {
      lastFetchedTop = -1
    }
    rafId = null

    const container = opts.containerRef()
    if (!container) {
      console.warn(LOG, 'updateVisible: containerRef is null, skipping')
      return
    }

    const totalH = opts.totalHeight()
    const totalR = opts.totalRows()



    if (totalH === 0 || totalR === 0) {
      visibleRows.value   = []
      paddingTop.value    = 0
      paddingBottom.value = 0
      return
    }

    const scrollY = container.scrollTop
    const viewH   = containerHeight.value > 0 ? containerHeight.value : container.clientHeight

    if (viewH === 0) {
      console.warn(LOG, 'updateVisible: containerHeight is 0, skipping')
      return
    }

    const bufferH = 3000 // 3000px buffer for smooth scrolling
    const topY    = Math.max(0, scrollY - bufferH)
    const bottomY = scrollY + viewH + bufferH

    // Skip if the visible range hasn't actually shifted outside our last fetched bounding box
    if (
      lastFetchedTop !== -1 &&
      topY >= lastFetchedTop && 
      bottomY <= lastFetchedBottom &&
      totalR > 0 // if totalR is known and we didn't just reset
    ) {
      return
    }

    // We need a new superset. Let's fetch a slightly larger box.
    const requestTop    = Math.max(0, scrollY - bufferH * 1.5)
    const requestBottom = scrollY + viewH + bufferH * 1.5

    lastFetchedTop    = requestTop
    lastFetchedBottom = requestBottom

    const myFetchId = ++currentFetchId
    isFetching.value = true

    try {
      const rows = await opts.fetchRowsByY(requestTop, requestBottom)
      
      // If a newer fetch was started while we were waiting, discard this one
      if (myFetchId !== currentFetchId) return

      visibleRows.value = rows

      if (rows.length > 0) {
        const firstRow = rows[0] as any
        const lastRow  = rows[rows.length - 1] as any

        const firstY = typeof firstRow.y === 'number' ? firstRow.y : 0
        const lastY  = typeof lastRow.y  === 'number' ? lastRow.y  : 0
        const lastH  = typeof lastRow.height === 'number' ? lastRow.height : 0

        paddingTop.value    = Math.max(0, firstY)
        paddingBottom.value = Math.max(0, totalH - (lastY + lastH))
      } else {
        paddingTop.value    = requestTop
        paddingBottom.value = Math.max(0, totalH - paddingTop.value)
        console.warn(LOG, `  0 rows returned`)
      }
    } catch (err) {
      console.error(LOG, 'fetchRows FAILED:', err)
    } finally {
      if (myFetchId === currentFetchId) {
        isFetching.value = false
      }
    }
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────

  onMounted(() => {
    const el = opts.containerRef()
    if (!el) {
      console.warn(LOG, 'onMounted: containerRef is null')
      return
    }

    containerHeight.value = el.clientHeight


    resizeObserver = new ResizeObserver(entries => {
      const h = entries[0].contentRect.height
      const w = entries[0].contentRect.width

      if (h > 0 && Math.abs(h - containerHeight.value) > 1) {
        containerHeight.value = h
        // Container was resized — re-fetch visible rows for new viewport
        scheduleUpdate(true)
      }
    })
    resizeObserver.observe(el)
  })

  onBeforeUnmount(() => {
    resizeObserver?.disconnect()
    if (rafId !== null) cancelAnimationFrame(rafId)
  })

  function scrollToTop() {
    opts.containerRef()?.scrollTo({ top: 0 })
  }

  function scrollToBottom() {
    const el = opts.containerRef()
    if (el) el.scrollTo({ top: el.scrollHeight })
  }

  return {
    visibleRows, paddingTop, paddingBottom,
    startIndex, isFetching, containerHeight,
    onScroll, updateVisible, scrollToTop, scrollToBottom
  }
}
