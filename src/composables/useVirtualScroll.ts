// src/composables/useVirtualScroll.ts
// Row-level virtual scrolling with coordinate translation (§10.3 + B1)
// 行级虚拟滚动 + 坐标平移 (§10.3 + B1)
//
// Why coordinate translation:
// 为什么需要坐标平移：
//   Chromium/WebView2 clamp a single element's height at ~16.7M px. A million-photo
//   gallery is ~40M px tall, which would break the native scrollbar (can't reach the
//   bottom, fires endless scroll events). So when the LOGICAL layout height exceeds a
//   safe ceiling we cap the PHYSICAL scroll spacer at SAFE_MAX and map between the two
//   coordinate systems linearly.
//
//   Chromium/WebView2 会把单个元素高度钳制在约 1677 万 px。百万张图库约 4000 万 px 高，
//   会击穿原生滚动条（到不了底、滚动事件无限触发）。因此当逻辑布局高度超过安全上限时，
//   我们把物理滚动占位高度封顶在 SAFE_MAX，并在两套坐标系间做线性映射。
//
// Model:
// 模型：
//   physicalScrollTop ∈ [0, physMax]      ← native container.scrollTop
//   logicalScrollTop  = physicalScrollTop / physMax * logMax
//   Rows render inside a "render layer". A row at logical y is placed at
//   (row.y - renderAnchor) inside the layer (small, precision-safe), and the layer is
//   translated by  contentOffset = renderAnchor + (physicalScrollTop - logicalScrollTop)
//   to pin the visible window to the viewport. In normal mode (height ≤ SAFE_MAX) δ = 0
//   and the layer offset is constant between fetches, so native scrolling behaves exactly
//   as before.
//
//   行渲染在一个"渲染层"内。逻辑 y 的行被放在层内 (row.y - renderAnchor) 处（数值小、精度安全），
//   层整体平移 contentOffset = renderAnchor + (physicalScrollTop - logicalScrollTop) 以把可视窗口
//   钉到视口。普通模式（高度 ≤ SAFE_MAX）下 δ = 0，层偏移在两次取数之间恒定，原生滚动与此前一致。
//
// The layer transform is applied IMPERATIVELY (direct style write) so that fast scrolling
// in translated mode does not trigger a Vue re-render of the row list every frame.
// 层 transform 以命令式（直接写 style）应用，避免平移模式下快速滚动每帧都触发行列表的 Vue 重渲染。

import { ref, onMounted, onBeforeUnmount } from 'vue'
import type { LayoutRow } from '../types/layout'

const LOG = '[VirtualScroll]'

/// Physical scroll-spacer ceiling. Comfortably under the ~16.7M px element-height clamp.
/// 物理滚动占位上限，安全地低于约 1677 万 px 的元素高度钳制阈值。
/// NOTE: translated mode (logical height > this) has a known scroll-misalignment bug
/// (shelved). At 10_000_000 it stays dormant for libraries under ~250k items.
/// 注意：平移模式（逻辑高度 > 此值）存在已知滚动错位 bug（已搁置）。
/// 设为 10_000_000 时，约 25 万项以下的库不会进入平移模式。
const SAFE_MAX = 10_000_000

interface UseVirtualScrollOptions {
  totalHeight:  () => number
  totalRows:    () => number
  fetchRowsByY: (topY: number, bottomY: number) => Promise<LayoutRow[]>
  containerRef: () => HTMLElement | null
  /// The render-layer element whose transform pins the visible window.
  /// 渲染层元素，其 transform 把可视窗口钉到视口。
  layerRef:     () => HTMLElement | null
}

export function useVirtualScroll(opts: UseVirtualScrollOptions) {
  const containerHeight = ref(0)
  const visibleRows     = ref<LayoutRow[]>([])
  const startIndex      = ref(0)
  const paddingTop      = ref(0)   // retained for API compatibility (rows are absolutely positioned)
  const paddingBottom   = ref(0)
  const isFetching      = ref(false)

  // ── Coordinate-translation state ───────────────────────────────────────
  // ── 坐标平移状态 ───────────────────────────────────────
  /// Physical height of the scroll spacer (capped at SAFE_MAX). Reactive — bound to the
  /// spacer div height; changes only when the layout changes.
  const spacerHeight    = ref(0)
  /// Logical y that row offsets are measured from for the current window. Reactive —
  /// bound to each row's transform; changes only on fetch.
  const renderAnchor    = ref(0)
  /// Current logical scroll position (consumers use this instead of scrollTop). Read in
  /// JS only (not a hot render binding), so per-frame updates don't re-render.
  const logicalScrollTop = ref(0)
  /// True when the layout is taller than SAFE_MAX and translation is active.
  const isTranslated    = ref(false)

  /// Last transform applied to the layer — change-guard to skip redundant style writes.
  /// 上次施加到层的 transform —— 变更守卫，跳过冗余的 style 写入。
  let lastAppliedOffset = Number.NaN

  let rafId:          number | null = null
  let resizeObserver: ResizeObserver | null = null

  let currentFetchId = 0
  let lastFetchedTop    = -1
  let lastFetchedBottom = -1
  let ticking = false
  let pendingUpdate = false

  // ── Geometry helpers ───────────────────────────────────────────────────
  // ── 几何辅助 ───────────────────────────────────────────────────

  /// Compute the physical→logical mapping for the current container + layout.
  /// 计算当前容器 + 布局的物理→逻辑映射。
  function geometry() {
    const container = opts.containerRef()
    const viewH = containerHeight.value > 0
      ? containerHeight.value
      : (container?.clientHeight ?? 0)
    const logicalTotal  = opts.totalHeight()
    const physicalTotal = Math.min(logicalTotal, SAFE_MAX)
    const physMax = Math.max(0, physicalTotal - viewH)
    const logMax  = Math.max(0, logicalTotal - viewH)
    return { viewH, logicalTotal, physicalTotal, physMax, logMax }
  }

  function physicalToLogical(physicalTop: number): number {
    const { physMax, logMax } = geometry()
    if (physMax <= 0) return 0
    return (physicalTop / physMax) * logMax
  }

  /// Convert a logical y to the physical scrollTop that brings it to the viewport top.
  /// 将逻辑 y 转换为能把它带到视口顶部的物理 scrollTop。
  function logicalToPhysical(logicalY: number): number {
    const { physMax, logMax } = geometry()
    if (logMax <= 0) return 0
    return (Math.max(0, logicalY) / logMax) * physMax
  }

  /// Imperatively pin the render layer to the viewport for the current scroll position.
  /// Runs on every scroll frame; cheap and skips redundant writes.
  /// 命令式地把渲染层钉到当前滚动位置对应的视口；每帧运行，开销小且跳过冗余写入。
  function syncTransform() {
    const container = opts.containerRef()
    if (!container) return
    const physicalTop = container.scrollTop
    const logical = physicalToLogical(physicalTop)
    logicalScrollTop.value = logical
    const offset = renderAnchor.value + (physicalTop - logical)
    if (offset !== lastAppliedOffset) {
      const layer = opts.layerRef()
      if (layer) layer.style.transform = `translate3d(0, ${offset}px, 0)`
      lastAppliedOffset = offset
    }
  }

  // ── Scroll handler (called by host @scroll) ────────────────────────────
  // ── 滚动处理程序（由宿主 @scroll 调用） ────────────────────────────

  function onScroll() {
    // Keep the render layer glued to the viewport on every frame (matters in
    // translated mode where native scroll moves at the wrong logical rate).
    // 每帧把渲染层钉在视口上（平移模式下原生滚动的逻辑速率不对，必须每帧修正）。
    syncTransform()
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

    const { viewH, logicalTotal, physicalTotal, logMax } = geometry()
    const totalR = opts.totalRows()

    // Keep the physical spacer height + translated flag in sync with the layout.
    // 让物理占位高度 + 平移标志与布局保持同步。
    spacerHeight.value = physicalTotal
    isTranslated.value = logicalTotal > SAFE_MAX

    if (logicalTotal === 0 || totalR === 0) {
      visibleRows.value   = []
      paddingTop.value    = 0
      paddingBottom.value = 0
      renderAnchor.value  = 0
      logicalScrollTop.value = 0
      lastAppliedOffset = Number.NaN
      syncTransform()
      return
    }

    if (viewH === 0) {
      console.warn(LOG, 'updateVisible: containerHeight is 0, skipping')
      return
    }

    // Work entirely in LOGICAL coordinates for fetching.
    // 取数完全在逻辑坐标系中进行。
    const physicalTop = container.scrollTop
    const scrollY = physicalToLogical(physicalTop)
    logicalScrollTop.value = scrollY

    const bufferH = 1000 // 1000px logical buffer for smooth scrolling
                         // 1000px 逻辑缓冲区以实现平滑滚动
    const topY    = Math.max(0, scrollY - bufferH)
    const bottomY = Math.min(logMax + viewH, scrollY + viewH + bufferH)

    // Skip if the visible range hasn't actually shifted outside our last fetched bounding box
    // 如果可见范围实际上没有移出我们上次获取的边界框，则跳过
    if (
      lastFetchedTop !== -1 &&
      topY >= lastFetchedTop &&
      bottomY <= lastFetchedBottom &&
      totalR > 0
    ) {
      return
    }

    // We need a new superset. Fetch a slightly larger logical box.
    // 我们需要一个新的超集。获取一个稍大的逻辑框。
    const requestTop    = Math.max(0, scrollY - bufferH * 1.2)
    const requestBottom = scrollY + viewH + bufferH * 1.2

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

      // Anchor row offsets to the window top so per-row transforms stay small
      // (precision-safe at the 40M px logical scale), then re-pin the layer.
      // 把行偏移锚定到窗口顶部，使逐行 transform 保持很小（在 4000 万 px 逻辑尺度下
      // 仍精度安全），随后重新钉住渲染层。
      renderAnchor.value = Math.floor(requestTop)
      syncTransform()

      if (rows.length > 0) {
        const firstRow = rows[0] as any
        const lastRow  = rows[rows.length - 1] as any
        const firstY = typeof firstRow.y === 'number' ? firstRow.y : 0
        const lastY  = typeof lastRow.y  === 'number' ? lastRow.y  : 0
        const lastH  = typeof lastRow.height === 'number' ? lastRow.height : 0
        paddingTop.value    = Math.max(0, firstY)
        paddingBottom.value = Math.max(0, logicalTotal - (lastY + lastH))
      } else {
        paddingTop.value    = requestTop
        paddingBottom.value = Math.max(0, logicalTotal - paddingTop.value)
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
    // Coordinate-translation surface (bind these in the host template):
    // 坐标平移接口（在宿主模板中绑定）：
    spacerHeight, renderAnchor, logicalScrollTop, isTranslated,
    logicalToPhysical,
    onScroll, updateVisible, scrollToTop, scrollToBottom,
  }
}
