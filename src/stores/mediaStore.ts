// src/stores/mediaStore.ts
// Layout and media state store
// 布局和媒体状态存储

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { LayoutRow, LayoutSummary } from '../types/layout'
import type { MediaDetail, AppStats, ThumbResult } from '../types/media'
import { IPC } from '../constants/ipc'
import { DEFAULTS } from '../constants/defaults'

export const useMediaStore = defineStore('media', () => {
  // ── Layout state ────────────────────────────────────────────────────────
  // ── 布局状态 ────────────────────────────────────────────────────────
  const layoutSummary   = ref<LayoutSummary | null>(null)
  const rowCache        = ref<Map<number, LayoutRow>>(new Map())
  const isComputingLayout = ref(false)
  const layoutDirty       = ref(false)

  // ── Detail view ─────────────────────────────────────────────────────────
  // ── 详情视图 ─────────────────────────────────────────────────────────
  const detailItem      = ref<MediaDetail | null>(null)
  const isDetailOpen    = ref(false)

  // ── Stats ────────────────────────────────────────────────────────────────
  // ── 统计 ────────────────────────────────────────────────────────────────
  const stats           = ref<AppStats | null>(null)

  // ── Computed ─────────────────────────────────────────────────────────────
  // ── 计算属性 ─────────────────────────────────────────────────────────────
  const totalItems   = computed(() => stats.value?.totalItems ?? 0)
  const totalHeight  = computed(() => layoutSummary.value?.totalHeight ?? 0)
  const totalRows    = computed(() => layoutSummary.value?.totalRows ?? 0)
  const layoutVersion = computed(() => layoutSummary.value?.layoutVersion ?? 0)

  // ── Actions ───────────────────────────────────────────────────────────────
  // ── 动作 ───────────────────────────────────────────────────────────────

  async function computeLayout(params: {
    directoryId?:    number | null
    filters?:        Record<string, unknown>
    containerWidth:  number
    rowHeight?:      number
    gap?:            number
    groupBy?:        string
    sortWithinGroup?: string
  }) {

    if (params.containerWidth < 100) {
      console.warn('[MediaStore] computeLayout: containerWidth too small, skipping')
      return
    }
    isComputingLayout.value = true
    rowCache.value.clear()
    try {
      layoutSummary.value = await invoke<LayoutSummary>(IPC.COMPUTE_LAYOUT, {
        params: {
          directoryId:   params.directoryId ?? null,
          filters:       params.filters ?? null,
          containerWidth: params.containerWidth,
          rowHeight:     params.rowHeight ?? DEFAULTS.GRID_ROW_HEIGHT,
          gap:           params.gap ?? DEFAULTS.GRID_GAP,
          groupBy:       params.groupBy ?? 'date',
          sortWithinGroup: params.sortWithinGroup ?? 'datetime',
        }
      })

    } catch (e) {
      console.error('[MediaStore] computeLayout FAILED:', e)
    } finally {
      isComputingLayout.value = false
    }
  }

  async function fetchRows(startRow: number, endRow: number): Promise<LayoutRow[]> {
    const version = layoutSummary.value?.layoutVersion

    try {
      const rows = await invoke<LayoutRow[]>(IPC.GET_LAYOUT_ROWS, {
        startRow,
        endRow,
        layoutVersion: version,
      })

      rows.forEach((row, i) => rowCache.value.set(startRow + i, row))
      return rows
    } catch (e) {
      console.error(`[MediaStore] fetchRows(${startRow}, ${endRow}) FAILED:`, e)
      throw e
    }
  }

  async function fetchRowsByY(topY: number, bottomY: number): Promise<LayoutRow[]> {
    const version = layoutSummary.value?.layoutVersion

    try {
      const rows = await invoke<LayoutRow[]>(IPC.GET_LAYOUT_ROWS_BY_Y, {
        topY,
        bottomY,
        layoutVersion: version,
      })
      return rows
    } catch (e) {
      console.error(`[MediaStore] fetchRowsByY(${topY}, ${bottomY}) FAILED:`, e)
      throw e
    }
  }

  async function openDetail(id: number) {
    detailItem.value = await invoke<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id })
    isDetailOpen.value = true
  }

  async function navigateDetail(offset: number) {
    if (!detailItem.value) return
    const adj = await invoke<MediaDetail | null>('get_adjacent_media', { 
      currentId: detailItem.value.id, 
      offset 
    })
    if (adj) {
      detailItem.value = adj
    }
  }

  function closeDetail() {
    isDetailOpen.value = false
    detailItem.value   = null
  }

  async function loadStats() {
    stats.value = await invoke<AppStats>(IPC.GET_STATS)
  }

  /** Mark the layout as stale — the next time the grid becomes visible it should recompute.
   *  将布局标记为过时 — 下次网格可见时应重新计算。 */
  function invalidateLayout() {
    layoutDirty.value = true
  }

  /** Consume the dirty flag (returns true if it was dirty, then resets).
   *  消费脏标志（如果为脏则返回 true，然后重置）。 */
  function consumeLayoutDirty(): boolean {
    if (layoutDirty.value) {
      layoutDirty.value = false
      return true
    }
    return false
  }

  async function toggleFavorite(id: number): Promise<boolean> {
    const newVal = await invoke<boolean>(IPC.TOGGLE_FAVORITE, { itemId: id })
    if (stats.value) {
      stats.value.totalFavorited += newVal ? 1 : -1
    }
    return newVal
  }

  async function setRating(id: number, rating: number) {
    await invoke(IPC.SET_RATING, { itemId: id, rating })
  }

  return {
    layoutSummary, rowCache, isComputingLayout, layoutDirty,
    detailItem, isDetailOpen,
    stats,
    totalItems, totalHeight, totalRows, layoutVersion,
    computeLayout, fetchRows, fetchRowsByY, openDetail, closeDetail, navigateDetail,
    loadStats, toggleFavorite, setRating, invalidateLayout, consumeLayoutDirty,
  }
})
