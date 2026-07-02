// src/composables/useJustifiedLayout.ts
// Consumes backend row data and drives compute_layout re-runs.
// 消费后端行数据并驱动 compute_layout 重新运行。

import { watch, onBeforeUnmount } from 'vue'
import { useMediaStore } from '../stores/mediaStore'
import { useFilterStore } from '../stores/filterStore'
import { useUiStore } from '../stores/uiStore'
import { useAiStore } from '../stores/aiStore'
import { DEFAULTS } from '../constants/defaults'

export function useJustifiedLayout(containerWidthRef: () => number) {
  const media = useMediaStore()
  const filter = useFilterStore()
  const ui = useUiStore()
  const ai = useAiStore()

  let resizeTimer: ReturnType<typeof setTimeout> | null = null

  async function compute(width?: number) {
    const cw = width ?? containerWidthRef()

    // Container not ready yet — defer to next tick and retry once.
    // 容器尚未准备好 — 延迟到下一个 tick 并重试一次。
    if (cw < 100) {
      await new Promise((r) => setTimeout(r, 50))
      const retryW = containerWidthRef()

      if (retryW < 100) {
        console.warn('[JustifiedLayout] compute() retry failed: width still <100, giving up')
        return
      }
      return compute(retryW)
    }

    const directoryId = ui.activeDirectoryId
    // Record<string, unknown> 与 mediaStore.computeLayout 的 filters 入参同型，
    // 可直接增补 scope 维度字段（personId/albumId/...）而无需 any。
    // 🔴 R1-2：本段视图装配与 useViewDescriptor.buildCurrentViewDescriptor() 一一对应
    // （后者供 SelectAll 批量操作描述同一视图）——新增视图维度必须两处同步，否则全选目标集漂移。
    const filters: Record<string, unknown> = filter.toApiFilter()

    // An open collection takes precedence: system folders ≈ type + is_favorited;
    // user folders restrict to album_items membership (albumId).
    // 打开的收藏夹优先：系统夹 ≈ 类型 + is_favorited；用户夹按 album_items 成员（albumId）限定。
    if (ui.activePersonId) {
      // 人物视图（F6）：限定为包含该人物簇人脸的图像。与其它视图互斥（setActivePerson 已清兄弟）。
      filters.personId = ui.activePersonId
    } else if (ui.activeCollection) {
      const c = ui.activeCollection
      if (c.kind === 'system' && c.mediaTypeFilter) {
        filters.mediaTypes = [c.mediaTypeFilter]
        filters.favoritedOnly = true
      } else {
        filters.albumId = c.id
      }
    } else if (ui.activeSmartAlbum === 'favorites') {
      filters.favoritedOnly = true
    } else if (ui.activeSmartAlbum === 'live-photos') {
      filters.livePhotoOnly = true
    } else if (ui.activeSmartAlbum === 'recent') {
      filters.recentOnly = true
    } else if (ui.activeSmartAlbum === 'trash') {
      filters.trashedOnly = true
    }

    if (ui.searchQuery && ui.searchQuery.trim() !== '') {
      // Add searchQuery to the filters sent to Rust
      filters.searchQuery = ui.searchQuery.trim()
      filters.searchScope = ui.searchScope
    }

    if (ai.isSemanticMode) {
      filters.aiSearch = true
      filters.aiThreshold = ai.similarityThreshold
    }

    await media.computeLayout({
      directoryId,
      filters,
      containerWidth: cw,
      rowHeight: ui.gridRowHeight,
      gap: DEFAULTS.GRID_GAP,
      groupBy: ui.groupBy,
      sortWithinGroup: ui.sortWithinGroup,
      sortOrder: ui.sortOrder,
      // 布局模式（T20）：'grid' 走后端均匀宫格排版，否则等高行。后端产出同一 LayoutRow 枚举。
      layoutMode: ui.layoutMode,
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
      // colorLabel 此前漏入监听源（T16 遗留）：切换颜色筛选 chip 不触发 relayout，画廊不更新。
      () => filter.colorLabel,
      // 日期范围（T15）：from/to 任一变更都需重算；toApiFilter 仅在两者皆备时下发谓词。
      () => filter.dateFrom,
      () => filter.dateTo,
      () => ui.activeSmartAlbum,
      () => ui.activeDirectoryId,
      () => ui.activeCollection,
      () => ui.activePersonId,
      () => ui.searchQuery,
      () => ui.searchScope,
      () => ui.gridRowHeight,
      () => ui.groupBy,
      () => ui.sortWithinGroup,
      () => ui.sortOrder,
      // 布局模式切换（T20）→ 重算（后端换排版算法，前端同一取行通路）。
      () => ui.layoutMode,
      () => ai.isSemanticMode,
      () => ai.similarityThreshold,
    ],
    () => compute(),
    { flush: 'post' },
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
