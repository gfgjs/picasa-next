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
  fetchRows:    (start: number, end: number) => Promise<LayoutRow[]>
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

  // True while a fetchRows IPC call is in flight.
  // New scroll events while busy just re-schedule for the next frame after
  // the current fetch finishes — they never pile up as concurrent requests.
  let busy = false
  let pendingUpdate = false   // there is a scroll event we haven't acted on yet

  // ── Scroll handler (called by host @scroll) ────────────────────────────

  function onScroll() {
    if (busy) {
      // Mark that another update is needed once the current fetch finishes
      pendingUpdate = true
      return
    }
    scheduleUpdate()
  }

  function scheduleUpdate() {
    if (rafId !== null) cancelAnimationFrame(rafId)
    rafId = requestAnimationFrame(updateVisible)
  }

  // ── Compute visible window ─────────────────────────────────────────────

  async function updateVisible() {
    rafId = null

    if (busy) {
      pendingUpdate = true
      return
    }

    const container = opts.containerRef()
    if (!container) {
      console.warn(LOG, 'updateVisible: containerRef is null, skipping')
      return
    }

    const totalH = opts.totalHeight()
    const totalR = opts.totalRows()

    console.log(LOG, `updateVisible — totalH=${totalH} totalR=${totalR} containerH=${containerHeight.value} scrollTop=${container.scrollTop}`)

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

    const avgRowH = totalH / totalR
    const bufferH = DEFAULTS.SCROLL_BUFFER_ROWS * avgRowH
    const topY    = Math.max(0, scrollY - bufferH)
    const bottomY = scrollY + viewH + bufferH

    const estStart = Math.max(0, Math.floor(topY / avgRowH))
    const estEnd   = Math.min(totalR - 1, Math.ceil(bottomY / avgRowH))

    console.log(LOG, `  scrollY=${scrollY.toFixed(0)} viewH=${viewH} avgRowH=${avgRowH.toFixed(1)} → rows [${estStart}, ${estEnd}]`)

    busy = true
    isFetching.value = true
    pendingUpdate = false

    try {
      const rows = await opts.fetchRows(estStart, estEnd + 1)

      console.log(LOG, `  fetchRows(${estStart}, ${estEnd + 1}) → ${rows.length} rows`)

      visibleRows.value = rows
      startIndex.value  = estStart

      if (rows.length > 0) {
        const firstRow = rows[0] as any
        const lastRow  = rows[rows.length - 1] as any

        const firstY = typeof firstRow.y === 'number' ? firstRow.y : estStart * avgRowH
        const lastY  = typeof lastRow.y  === 'number' ? lastRow.y  : 0
        const lastH  = typeof lastRow.height === 'number' ? lastRow.height : avgRowH

        paddingTop.value    = Math.max(0, firstY)
        // Ensure paddingTop + visibleRowsHeight + paddingBottom == totalH.
        // The visible rows height is (lastY + lastH - firstY). So:
        //   paddingBottom = totalH - paddingTop - (lastY + lastH - firstY)
        //                 = totalH - firstY - lastY - lastH + firstY
        //                 = totalH - lastY - lastH
        paddingBottom.value = Math.max(0, totalH - (lastY + lastH))

        console.log(LOG, `  padding top=${paddingTop.value.toFixed(0)} bottom=${paddingBottom.value.toFixed(0)}`)
      } else {
        paddingTop.value    = estStart * avgRowH
        paddingBottom.value = Math.max(0, totalH - paddingTop.value)
        console.warn(LOG, `  0 rows returned`)
      }
    } catch (err) {
      console.error(LOG, 'fetchRows FAILED:', err)
    } finally {
      busy = false
      isFetching.value = false

      // If a scroll event arrived while we were busy, handle it now
      if (pendingUpdate) {
        pendingUpdate = false
        scheduleUpdate()
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
    console.log(LOG, `onMounted: clientHeight=${el.clientHeight} clientWidth=${el.clientWidth}`)

    resizeObserver = new ResizeObserver(entries => {
      const h = entries[0].contentRect.height
      const w = entries[0].contentRect.width
      console.log(LOG, `ResizeObserver fired: h=${h.toFixed(0)} w=${w.toFixed(0)}`)
      if (h > 0 && h !== containerHeight.value) {
        containerHeight.value = h
        // Container was resized — re-fetch visible rows for new viewport
        scheduleUpdate()
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

  return {
    visibleRows, paddingTop, paddingBottom,
    startIndex, isFetching, containerHeight,
    onScroll, updateVisible, scrollToTop,
  }
}
