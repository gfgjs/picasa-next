// src/stores/mediaStore.ts
// Layout and media state store

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { LayoutRow, LayoutSummary } from '../types/layout'
import type { MediaDetail, AppStats, ThumbResult } from '../types/media'
import { IPC } from '../constants/ipc'
import { DEFAULTS } from '../constants/defaults'

export const useMediaStore = defineStore('media', () => {
  // ── Layout state ────────────────────────────────────────────────────────
  const layoutSummary   = ref<LayoutSummary | null>(null)
  const rowCache        = ref<Map<number, LayoutRow>>(new Map())
  const isComputingLayout = ref(false)

  // ── Detail view ─────────────────────────────────────────────────────────
  const detailItem      = ref<MediaDetail | null>(null)
  const isDetailOpen    = ref(false)

  // ── Stats ────────────────────────────────────────────────────────────────
  const stats           = ref<AppStats | null>(null)

  // ── Computed ─────────────────────────────────────────────────────────────
  const totalItems   = computed(() => stats.value?.totalItems ?? 0)
  const totalHeight  = computed(() => layoutSummary.value?.totalHeight ?? 0)
  const totalRows    = computed(() => layoutSummary.value?.totalRows ?? 0)
  const layoutVersion = computed(() => layoutSummary.value?.layoutVersion ?? 0)

  // ── Actions ───────────────────────────────────────────────────────────────

  async function computeLayout(params: {
    directoryId?:    number | null
    filters?:        Record<string, unknown>
    containerWidth:  number
    rowHeight?:      number
    gap?:            number
  }) {
    console.log('[MediaStore] computeLayout: containerWidth=', params.containerWidth, 'directoryId=', params.directoryId)
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
        }
      })
      console.log('[MediaStore] computeLayout result: totalRows=', layoutSummary.value?.totalRows,
        'totalHeight=', layoutSummary.value?.totalHeight,
        'version=', layoutSummary.value?.layoutVersion)
    } catch (e) {
      console.error('[MediaStore] computeLayout FAILED:', e)
    } finally {
      isComputingLayout.value = false
    }
  }

  async function fetchRows(startRow: number, endRow: number): Promise<LayoutRow[]> {
    const version = layoutSummary.value?.layoutVersion
    console.log(`[MediaStore] fetchRows(${startRow}, ${endRow}) layoutVersion=${version}`)
    try {
      const rows = await invoke<LayoutRow[]>(IPC.GET_LAYOUT_ROWS, {
        startRow,
        endRow,
        layoutVersion: version,
      })
      console.log(`[MediaStore] fetchRows(${startRow}, ${endRow}) → ${rows.length} rows`)
      rows.forEach((row, i) => rowCache.value.set(startRow + i, row))
      return rows
    } catch (e) {
      console.error(`[MediaStore] fetchRows(${startRow}, ${endRow}) FAILED:`, e)
      throw e
    }
  }

  async function openDetail(id: number) {
    detailItem.value = await invoke<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id })
    isDetailOpen.value = true
  }

  function closeDetail() {
    isDetailOpen.value = false
    detailItem.value   = null
  }

  async function loadStats() {
    stats.value = await invoke<AppStats>(IPC.GET_STATS)
  }

  async function toggleFavorite(id: number): Promise<boolean> {
    return invoke<boolean>(IPC.TOGGLE_FAVORITE, { itemId: id })
  }

  async function setRating(id: number, rating: number) {
    await invoke(IPC.SET_RATING, { itemId: id, rating })
  }

  return {
    layoutSummary, rowCache, isComputingLayout,
    detailItem, isDetailOpen,
    stats,
    totalItems, totalHeight, totalRows, layoutVersion,
    computeLayout, fetchRows, openDetail, closeDetail,
    loadStats, toggleFavorite, setRating,
  }
})
