// src/composables/useVirtualScroll.ts
// Row-level virtual scrolling composable (§10.3)
// 行级虚拟滚动组合式函数 (§10.3)
//
// Key design constraint:
// 关键设计约束：
//   scrollHeight must equal totalHeight at all times.
//   scrollHeight 必须始终等于 totalHeight。
//   If scrollHeight > totalHeight, the browser clamps scrollTop when
//   如果 scrollHeight > totalHeight，当用户到达底部时，浏览器会限制 scrollTop，
//   the user reaches the bottom, fires a new scroll event, which triggers
//   触发一个新的滚动事件，这将触发
//   another fetch → infinite request loop.
//   另一个获取操作 → 无限请求循环。
//
//   The caller must ensure the DOM representation of rows does NOT add extra
//   调用者必须确保行的 DOM 表示不添加额外的
//   height beyond what the layout reports (e.g. no marginBottom on row wrappers;
//   超出布局报告的高度（例如，行包装器上没有 marginBottom；
//   use CSS gap on the flex container instead, since flex gap does not add
//   使用 flex 容器上的 CSS gap 代替，因为 flex gap 不会在
//   trailing space after the last child).
//   最后一个子元素之后添加尾随空格）。
//
// Concurrency model:
// 并发模型：
//   A boolean `busy` flag ensures only one fetch is in-flight at a time.
//   一个布尔 `busy` 标志确保一次只有一个获取操作在进行中。
//   While busy, incoming updateVisible calls schedule themselves via rAF so
//   繁忙时，传入的 updateVisible 调用通过 rAF 调度自身，因此
//   the LAST position wins when the current fetch completes.
//   当当前获取操作完成时，最后一个位置获胜。

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
  // ── 滚动处理程序（由宿主 @scroll 调用） ────────────────────────────

  let pendingUpdate = false

  function onScroll() {
    scheduleUpdate()
  }

  function scheduleUpdate(force = false) {
    if (force) {
      lastFetchedTop = -1
    }
    
    // If a fetch is already in flight, flag that we need another update after it finishes
    // 如果获取操作已经在进行中，则标记我们需要在它完成后进行另一次更新
    if (isFetching.value) {
      pendingUpdate = true
      return
    }

    if (!ticking) {
      ticking = true
      requestAnimationFrame(async () => {
        // Await the fetch so we don't start overlapping requests
        // 等待获取，这样我们就不会开始重叠的请求
        await updateVisible(false)
        ticking = false
        
        // If the user kept scrolling while we were fetching, run it again to catch up
        // 如果用户在获取时保持滚动，请再次运行它以赶上
        if (pendingUpdate) {
          pendingUpdate = false
          scheduleUpdate()
        }
      })
    }
  }

  // ── Compute visible window ─────────────────────────────────────────────
  // ── 计算可见窗口 ─────────────────────────────────────────────

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
                         // 3000px 缓冲区以实现平滑滚动
    const topY    = Math.max(0, scrollY - bufferH)
    const bottomY = scrollY + viewH + bufferH

    // Skip if the visible range hasn't actually shifted outside our last fetched bounding box
    // 如果可见范围实际上没有移出我们上次获取的边界框，则跳过
    if (
      lastFetchedTop !== -1 &&
      topY >= lastFetchedTop && 
      bottomY <= lastFetchedBottom &&
      totalR > 0 // if totalR is known and we didn't just reset
                 // 如果 totalR 已知并且我们没有刚刚重置
    ) {
      return
    }

    // We need a new superset. Let's fetch a slightly larger box.
    // 我们需要一个新的超集。让我们获取一个稍大的框。
    const requestTop    = Math.max(0, scrollY - bufferH * 1.5)
    const requestBottom = scrollY + viewH + bufferH * 1.5

    lastFetchedTop    = requestTop
    lastFetchedBottom = requestBottom

    const myFetchId = ++currentFetchId
    isFetching.value = true

    try {
      const rows = await opts.fetchRowsByY(requestTop, requestBottom)
      
      // If a newer fetch was started while we were waiting, discard this one
      // 如果在我们等待时开始了更新的获取，则丢弃这个
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
  // ── 生命周期 ──────────────────────────────────────────────────────────

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
        // 容器已调整大小 — 重新获取新视口的可见行
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
