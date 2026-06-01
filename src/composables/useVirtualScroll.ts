// src/composables/useVirtualScroll.ts
// Row-level virtual scrolling composable (§10.3)

import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { DEFAULTS, SEPARATOR_HEIGHT } from '../constants/defaults'
import type { LayoutRow } from '../types/layout'

interface UseVirtualScrollOptions {
  totalHeight:   () => number
  totalRows:     () => number
  fetchRows:     (start: number, end: number) => Promise<LayoutRow[]>
  containerRef:  () => HTMLElement | null
}

export function useVirtualScroll(opts: UseVirtualScrollOptions) {
  const scrollTop      = ref(0)
  const containerHeight = ref(0)
  const visibleRows    = ref<LayoutRow[]>([])
  const startIndex     = ref(0)
  const paddingTop     = ref(0)
  const paddingBottom  = ref(0)
  const isFetching     = ref(false)

  let rafId: number | null = null
  let resizeObserver: ResizeObserver | null = null

  // ── Scroll handler ─────────────────────────────────────────────────────

  function onScroll(e: Event) {
    scrollTop.value = (e.target as HTMLElement).scrollTop
    if (rafId !== null) cancelAnimationFrame(rafId)
    rafId = requestAnimationFrame(updateVisible)
  }

  // ── Compute visible window ─────────────────────────────────────────────

  async function updateVisible() {
    rafId = null
    const container = opts.containerRef()
    if (!container) return

    const totalH  = opts.totalHeight()
    const totalR  = opts.totalRows()
    if (totalR === 0) {
      visibleRows.value   = []
      paddingTop.value    = 0
      paddingBottom.value = 0
      return
    }

    const scrollY    = scrollTop.value
    const viewH      = containerHeight.value
    const avgRowH    = totalH / totalR

    // Estimate visible row range
    const bufferH    = DEFAULTS.SCROLL_BUFFER_ROWS * avgRowH
    const topY       = Math.max(0, scrollY - bufferH)
    const bottomY    = scrollY + viewH + bufferH

    // Binary estimate start row (approximate — layout rows have variable height)
    const estStart = Math.max(0, Math.floor(topY / avgRowH))
    const estEnd   = Math.min(totalR - 1, Math.ceil(bottomY / avgRowH))

    if (isFetching.value) return
    isFetching.value = true
    try {
      const rows = await opts.fetchRows(estStart, estEnd + 1)
      visibleRows.value = rows

      // Update padding
      startIndex.value    = estStart
      paddingTop.value    = estStart * avgRowH
      paddingBottom.value = Math.max(0, totalH - (estStart + rows.length) * avgRowH)
    } finally {
      isFetching.value = false
    }
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────

  onMounted(() => {
    const el = opts.containerRef()
    if (!el) return

    containerHeight.value = el.clientHeight
    el.addEventListener('scroll', onScroll, { passive: true })

    resizeObserver = new ResizeObserver(entries => {
      containerHeight.value = entries[0].contentRect.height
      updateVisible()
    })
    resizeObserver.observe(el)
  })

  onBeforeUnmount(() => {
    const el = opts.containerRef()
    el?.removeEventListener('scroll', onScroll)
    resizeObserver?.disconnect()
    if (rafId !== null) cancelAnimationFrame(rafId)
  })

  function scrollToTop() {
    opts.containerRef()?.scrollTo({ top: 0 })
    scrollTop.value = 0
  }

  return {
    scrollTop, visibleRows, paddingTop, paddingBottom,
    startIndex, isFetching, containerHeight,
    updateVisible, scrollToTop,
  }
}
