// src/composables/useJustifiedLayout.ts
// Consumes backend row data and drives compute_layout re-runs.

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
    if (cw < 100) return

    const directoryId = ui.activeDirectoryId
    const filters     = filter.toApiFilter()

    // Set favorited filter from smart album
    if (ui.activeSmartAlbum === 'favorites') {
      filters.favoritedOnly = true
    }

    await media.computeLayout({
      directoryId,
      filters,
      containerWidth: cw,
      rowHeight:      DEFAULTS.GRID_ROW_HEIGHT,
      gap:            DEFAULTS.GRID_GAP,
    })
  }

  // Debounced resize handler
  function onResize(newWidth: number) {
    if (resizeTimer) clearTimeout(resizeTimer)
    resizeTimer = setTimeout(() => compute(newWidth), DEFAULTS.RESIZE_DEBOUNCE_MS)
  }

  // Re-compute when filters or active view changes
  watch(
    [
      () => filter.mediaTypes,
      () => filter.favoritedOnly,
      () => filter.livePhotoOnly,
      () => filter.minRating,
      () => ui.activeSmartAlbum,
      () => ui.activeDirectoryId,
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
