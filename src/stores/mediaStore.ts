// src/stores/mediaStore.ts
// Layout and media state store
// 布局和媒体状态存储

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { LayoutRow, LayoutSummary, MediaMeta } from '../types/layout'
import type { MediaDetail, AppStats, ThumbResult } from '../types/media'
import { IPC } from '../constants/ipc'
import { DEFAULTS } from '../constants/defaults'

import { useUiStore } from './uiStore'

export const useMediaStore = defineStore('media', () => {
  // ── Layout state ────────────────────────────────────────────────────────
  // ── 布局状态 ────────────────────────────────────────────────────────
  const layoutSummary   = ref<LayoutSummary | null>(null)
  const rowCache        = ref<Map<number, LayoutRow>>(new Map())
  const isComputingLayout = ref(false)
  const layoutDirty       = ref(false)

  // ── Lazy viewport metadata (EXIF / GPS / file name / dir path) ──────────────
  // Heavy fields stripped from the resident layout cache; fetched per-window
  // only when the card info overlay is enabled.
  // ── 可视区懒加载元数据（EXIF / GPS / 文件名 / 目录路径） ──────────────────
  // 重型字段已从常驻布局缓存剥离；仅在卡片信息浮层开启时按窗口拉取。
  const viewportMeta = ref<Map<number, MediaMeta>>(new Map())
  const pendingMetaIds = new Set<number>()
  let metaTimer: ReturnType<typeof setTimeout> | null = null

  async function flushMeta() {
    metaTimer = null
    if (pendingMetaIds.size === 0) return
    const ids = Array.from(pendingMetaIds)
    pendingMetaIds.clear()
    try {
      const metas = await invoke<MediaMeta[]>(IPC.GET_META_FOR_VIEWPORT, { ids })
      // Reassign the Map so the ref triggers reactivity in consuming components.
      // 重新赋值 Map，使 ref 在消费组件中触发响应式更新。
      const next = new Map(viewportMeta.value)
      for (const m of metas) next.set(m.id, m)
      viewportMeta.value = next
    } catch (e) {
      console.error('[MediaStore] get_meta_for_viewport FAILED:', e)
    }
  }

  /** Ensure metadata is loaded for the given ids (debounced, fetch-once). */
  /** 确保给定 id 的元数据已加载（防抖、只取一次）。 */
  function ensureMeta(ids: number[]) {
    let added = false
    for (const id of ids) {
      if (!viewportMeta.value.has(id) && !pendingMetaIds.has(id)) {
        pendingMetaIds.add(id)
        added = true
      }
    }
    if (!added) return
    if (metaTimer === null) metaTimer = setTimeout(flushMeta, 120)
  }

  // ── Detail view ─────────────────────────────────────────────────────────
  // ── 详情视图 ─────────────────────────────────────────────────────────
  interface NavigationContext {
    type: 'layout' | 'search'
    itemIds: number[]
    currentIndex: number
  }

  const navContext      = ref<NavigationContext | null>(null)
  const detailItem      = ref<MediaDetail | null>(null)
  const isDetailOpen    = ref(false)

  // ── Stats ────────────────────────────────────────────────────────────────
  // ── 统计 ────────────────────────────────────────────────────────────────
  const stats           = ref<AppStats | null>(null)

  // ── Computed ─────────────────────────────────────────────────────────────
  // ── 计算属性 ─────────────────────────────────────────────────────────────
  const totalItems   = computed(() => stats.value?.totalItems ?? 0)
  const viewTotalItems = computed(() => layoutSummary.value?.totalItems ?? 0)
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
    sortOrder?:       string
  }) {

    if (params.containerWidth < 100) {
      console.warn('[MediaStore] computeLayout: containerWidth too small, skipping')
      return
    }
    isComputingLayout.value = true
    rowCache.value.clear()
    // Drop stale viewport metadata — the visible window will re-fetch what it needs.
    // 丢弃过时的可视区元数据 —— 可视窗口会按需重新拉取。
    if (viewportMeta.value.size > 0) viewportMeta.value = new Map()
    if (metaTimer) {
      clearTimeout(metaTimer)
      metaTimer = null
    }
    pendingMetaIds.clear()
    const ui = useUiStore()
    const needsMeta = ui.thumbInfoElements.some(el => ['geo', 'camera', 'params'].includes(el))

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
          sortOrder:     params.sortOrder ?? 'desc',
          includeMeta:   needsMeta,
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

  async function openDetailFromSearch(id: number, resultIds: number[]) {
    navContext.value = {
      type: 'search',
      itemIds: resultIds,
      currentIndex: resultIds.indexOf(id),
    }
    detailItem.value = await invoke<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id })
    isDetailOpen.value = true
  }

  async function openDetail(id: number, fromLayout = false) {
    if (fromLayout) {
      navContext.value = null
    }
    detailItem.value = await invoke<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id })
    isDetailOpen.value = true
  }

  async function navigateDetail(offset: number) {
    if (!detailItem.value) return

    if (navContext.value) {
      const nextIndex = navContext.value.currentIndex + offset
      if (nextIndex >= 0 && nextIndex < navContext.value.itemIds.length) {
        navContext.value.currentIndex = nextIndex
        const nextId = navContext.value.itemIds[nextIndex]
        detailItem.value = await invoke<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id: nextId })
      }
      return
    }

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
    navContext.value   = null
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
    // Update in row cache to reflect in grid immediately if toggled from detail view
    // 如果在详情视图中切换，则在行缓存中更新以立即反映在网格中
    for (const row of rowCache.value.values()) {
      if (row.rowType === 'normal') {
        const item = row.items.find(i => i.id === id)
        if (item) {
          ;(item as any).isFavorited = newVal
          break // An item only appears once
        }
      }
    }
    return newVal
  }

  async function setRating(id: number, rating: number) {
    await invoke(IPC.SET_RATING, { itemId: id, rating })
  }

  return {
    layoutSummary, rowCache, isComputingLayout, layoutDirty,
    detailItem, isDetailOpen, navContext,
    stats, viewportMeta,
    totalItems, viewTotalItems, totalHeight, totalRows, layoutVersion,
    computeLayout, fetchRows, fetchRowsByY, ensureMeta, openDetail, openDetailFromSearch, closeDetail, navigateDetail,
    loadStats, toggleFavorite, setRating, invalidateLayout, consumeLayoutDirty,
  }
})
