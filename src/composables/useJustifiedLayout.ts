// src/composables/useJustifiedLayout.ts
// Consumes backend row data and drives compute_layout re-runs.
// 消费后端行数据并驱动 compute_layout 重新运行。

import { ref, watch, onBeforeUnmount } from 'vue'
import { useMediaStore } from '../stores/mediaStore'
import { useFilterStore } from '../stores/filterStore'
import { useUiStore } from '../stores/uiStore'
import { DEFAULTS } from '../constants/defaults'
import type { LayoutRow } from '../types/layout'

export function useJustifiedLayout(containerWidthRef: () => number) {
  const media  = useMediaStore()
  const filter = useFilterStore()
  const ui     = useUiStore()

  let resizeTimer: ReturnType<typeof setTimeout> | null = null

  async function compute(width?: number) {
    const cw = width ?? containerWidthRef()



    // Container not ready yet — defer to next tick and retry once.
    // 容器尚未准备好 — 延迟到下一个 tick 并重试一次。
    if (cw < 100) {
      await new Promise(r => setTimeout(r, 50))
      const retryW = containerWidthRef()

      if (retryW < 100) {
        console.warn('[JustifiedLayout] compute() retry failed: width still <100, giving up')
        return
      }
      return compute(retryW)
    }

    const directoryId = ui.activeDirectoryId
    const filters     = filter.toApiFilter()

    if (ui.activeSmartAlbum === 'favorites') {
      filters.favoritedOnly = true
    }

    if (ui.searchQuery && ui.searchQuery.trim() !== '') {
      // Add searchQuery to the filters sent to Rust
      ;(filters as any).searchQuery = ui.searchQuery.trim()
      ;(filters as any).searchScope = ui.searchScope
    }
    await media.computeLayout({
      directoryId,
      filters,
      containerWidth: cw,
      rowHeight:      ui.gridRowHeight,
      gap:            DEFAULTS.GRID_GAP,
      groupBy:        ui.groupBy,
      sortWithinGroup: ui.sortWithinGroup,
      sortOrder:      ui.sortOrder,
    })
  }

  // Debounced resize handler
  // 防抖调整大小处理程序
  function onResize(newWidth: number) {
    if (resizeTimer) clearTimeout(resizeTimer)
    resizeTimer = setTimeout(() => compute(newWidth), DEFAULTS.RESIZE_DEBOUNCE_MS)
  }

  // Re-compute when filters or active view changes.
  // 当过滤器或活动视图改变时重新计算。
  // NOTE: totalItems changes (scan complete) are handled in MediaGrid.vue directly
  // 注意：totalItems 更改（扫描完成）直接在 MediaGrid.vue 中处理
  // to also call updateVisible() after compute.
  // 以在计算后也调用 updateVisible()。
  watch(
    [
      () => filter.mediaTypes,
      () => filter.favoritedOnly,
      () => filter.livePhotoOnly,
      () => filter.minRating,
      () => ui.activeSmartAlbum,
      () => ui.activeDirectoryId,
      () => ui.searchQuery,
      () => ui.searchScope,
      () => ui.gridRowHeight,
      () => ui.groupBy,
      () => ui.sortWithinGroup,
      () => ui.sortOrder,
    ],
    () => compute(),
    { deep: true }
  )

  onBeforeUnmount(() => {
    if (resizeTimer) clearTimeout(resizeTimer)
  })

  return {
    compute,
    onResize,
    layoutVersion: () => media.layoutSummary?.layoutVersion,
  }
}
