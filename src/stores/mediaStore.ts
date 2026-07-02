// src/stores/mediaStore.ts
// Layout and media state store
// 布局和媒体状态存储

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { LayoutRow, LayoutSummary, MediaMeta } from '../types/layout'
import type { MediaDetail, AppStats } from '../types/media'
import { IPC } from '../constants/ipc'
import { invokeIpc } from '../utils/ipc'
// type-only：无运行时环（useSelection → useViewDescriptor → mediaStore 的反向链在编译期被擦除）。
import type { BackendSelectionDescriptor } from '../composables/useSelection'
import { DEFAULTS } from '../constants/defaults'

import { useUiStore } from './uiStore'

export const useMediaStore = defineStore('media', () => {
  // ── Layout state ────────────────────────────────────────────────────────
  // ── 布局状态 ────────────────────────────────────────────────────────
  const layoutSummary = ref<LayoutSummary | null>(null)
  const isComputingLayout = ref(false)
  const layoutDirty = ref(false)

  // 单项标量改动信号（详情页 / 外部就地改 favorite / rating / colorLabel 时发出）。
  // 画廊显示的 visibleRows 由 MediaGrid 持有（经 fetchRowsByY 拉取），store 侧无行缓存（R2-2 已删
  // rowCache/patchRowItem 旁路）。MediaGrid 监听本信号把改动回灌它持有的 visibleRows。seq 单调自增，保证即便
  // (id,field,value) 重复也触发 watch。in-grid 操作另有同步内联 patch（即时反馈），与本信号的
  // 回灌幂等无冲突。
  type ItemFieldPatch = {
    id: number
    field: 'isFavorited' | 'rating' | 'colorLabel'
    value: number | boolean
    seq: number
  }
  const itemPatchSignal = ref<ItemFieldPatch | null>(null)
  let patchSeq = 0
  function signalItemPatch(id: number, field: ItemFieldPatch['field'], value: number | boolean) {
    itemPatchSignal.value = { id, field, value, seq: ++patchSeq }
  }

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
      const metas = await invokeIpc<MediaMeta[]>(IPC.GET_META_FOR_VIEWPORT, { ids })
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

  const navContext = ref<NavigationContext | null>(null)
  const detailItem = ref<MediaDetail | null>(null)
  const isDetailOpen = ref(false)

  // ── Stats ────────────────────────────────────────────────────────────────
  // ── 统计 ────────────────────────────────────────────────────────────────
  const stats = ref<AppStats | null>(null)

  // ── Computed ─────────────────────────────────────────────────────────────
  // ── 计算属性 ─────────────────────────────────────────────────────────────
  const totalItems = computed(() => stats.value?.totalItems ?? 0)
  const viewTotalItems = computed(() => layoutSummary.value?.totalItems ?? 0)
  const totalHeight = computed(() => layoutSummary.value?.totalHeight ?? 0)
  const totalRows = computed(() => layoutSummary.value?.totalRows ?? 0)
  const layoutVersion = computed(() => layoutSummary.value?.layoutVersion ?? 0)

  // ── Actions ───────────────────────────────────────────────────────────────
  // ── 动作 ───────────────────────────────────────────────────────────────

  // computeLayout 入参形状（pending/current 复用，替代 any）。
  type ComputeLayoutArgs = {
    directoryId?: number | null
    filters?: Record<string, unknown>
    containerWidth: number
    rowHeight?: number
    gap?: number
    groupBy?: string
    sortWithinGroup?: string
    sortOrder?: string
    layoutMode?: string
  }
  let pendingComputeParams: ComputeLayoutArgs | null = null
  let isComputingInternal = false

  async function computeLayout(params: ComputeLayoutArgs) {
    if (params.containerWidth < 100) {
      console.warn('[MediaStore] computeLayout: containerWidth too small, skipping')
      return
    }

    if (isComputingInternal) {
      pendingComputeParams = params
      return
    }

    isComputingInternal = true
    isComputingLayout.value = true

    let currentParams: ComputeLayoutArgs | null = params

    while (currentParams) {
      // Watchdog (问题6): compute_layout has been observed to hang intermittently after
      // deleting/re-adding a folder (suspected read-pool exhaustion / lock contention with
      // an in-flight scan or thumb-gen). If the invoke never returns, the "正在计算布局"
      // spinner sticks forever. Clear the flag after a generous timeout so the UI recovers
      // and the user can retry (switching folders triggers a fresh compute).
      // 看门狗（问题6）：删除/重加文件夹后曾偶发 compute_layout 卡住（疑似读连接池耗尽，
      // 或与在途扫描/缩略图生成的锁竞争）。若 invoke 永不返回，「正在计算布局」会永久卡住。
      // 超时后复位标志，使 UI 恢复、用户可重试（切换文件夹会触发新的计算）。
      const computeWatchdog = setTimeout(() => {
        if (isComputingLayout.value) {
          console.warn(
            '[MediaStore] computeLayout watchdog fired (>30s) — clearing isComputingLayout',
          )
          isComputingLayout.value = false
          isComputingInternal = false
        }
      }, 30000)
      // Drop stale viewport metadata — the visible window will re-fetch what it needs.
      // 丢弃过时的可视区元数据 —— 可视窗口会按需重新拉取。
      if (viewportMeta.value.size > 0) viewportMeta.value = new Map()
      if (metaTimer) {
        clearTimeout(metaTimer)
        metaTimer = null
      }
      pendingMetaIds.clear()
      const ui = useUiStore()
      const needsMeta = ui.thumbInfoElements.some((el) => ['geo', 'camera', 'params'].includes(el))

      try {
        layoutSummary.value = await invokeIpc<LayoutSummary>(IPC.COMPUTE_LAYOUT, {
          params: {
            directoryId: currentParams.directoryId ?? null,
            filters: currentParams.filters ?? null,
            containerWidth: currentParams.containerWidth,
            rowHeight: currentParams.rowHeight ?? DEFAULTS.GRID_ROW_HEIGHT,
            gap: currentParams.gap ?? DEFAULTS.GRID_GAP,
            groupBy: currentParams.groupBy ?? 'date',
            sortWithinGroup: currentParams.sortWithinGroup ?? 'datetime',
            sortOrder: currentParams.sortOrder ?? 'desc',
            layoutMode: currentParams.layoutMode ?? 'justified',
            includeMeta: needsMeta,
          },
        })
      } catch (e) {
        console.error('[MediaStore] computeLayout FAILED:', e)
      } finally {
        clearTimeout(computeWatchdog)
      }

      if (pendingComputeParams) {
        currentParams = pendingComputeParams
        pendingComputeParams = null
      } else {
        break
      }
    }

    isComputingInternal = false
    isComputingLayout.value = false
  }

  async function fetchRowsByY(topY: number, bottomY: number): Promise<LayoutRow[]> {
    const version = layoutSummary.value?.layoutVersion

    try {
      const rows = await invokeIpc<LayoutRow[]>(IPC.GET_LAYOUT_ROWS_BY_Y, {
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
    detailItem.value = await invokeIpc<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id })
    isDetailOpen.value = true
  }

  async function openDetail(id: number, fromLayout = false) {
    if (fromLayout) {
      navContext.value = null
    }
    detailItem.value = await invokeIpc<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id })
    isDetailOpen.value = true
  }

  async function navigateDetail(offset: number) {
    if (!detailItem.value) return

    if (navContext.value) {
      const nextIndex = navContext.value.currentIndex + offset
      if (nextIndex >= 0 && nextIndex < navContext.value.itemIds.length) {
        navContext.value.currentIndex = nextIndex
        const nextId = navContext.value.itemIds[nextIndex]
        detailItem.value = await invokeIpc<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id: nextId })
      }
      return
    }

    const adj = await invokeIpc<MediaDetail | null>(IPC.GET_ADJACENT_MEDIA, {
      currentId: detailItem.value.id,
      offset,
    })
    if (adj) {
      detailItem.value = adj
    }
  }

  function closeDetail() {
    isDetailOpen.value = false
    detailItem.value = null
    navContext.value = null
  }

  /**
   * 重连自动恢复（T13 §3.7 离线 UX 验收点）：卷插拔时重取当前查看项的可用态。
   * **只**回写 availability 字段（不换 detailItem 引用）——避免触发覆盖层上重置 zoom / 人脸 /
   * exotic gate 的重 watch；卷态变化极罕见（拔插），单次 IPC 成本可忽略。
   */
  async function refreshDetailAvailability() {
    const cur = detailItem.value
    if (!cur) return
    const fresh = await invokeIpc<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id: cur.id })
    // 仅当仍是同一项时回写（防拔插期间用户已切换/关闭导致错写）。
    if (detailItem.value?.id === cur.id) detailItem.value.availability = fresh.availability
  }

  async function loadStats() {
    stats.value = await invokeIpc<AppStats>(IPC.GET_STATS)
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
    const newVal = await invokeIpc<boolean>(IPC.TOGGLE_FAVORITE, { itemId: id })
    if (stats.value) {
      stats.value.totalFavorited += newVal ? 1 : -1
    }
    // 详情页切收藏 → 通知画廊回灌 visibleRows（in-grid 收藏另走同步内联 patch，幂等无冲突）。
    signalItemPatch(id, 'isFavorited', newVal)
    return newVal
  }

  async function setRating(id: number, rating: number) {
    await invokeIpc<void>(IPC.SET_RATING, { itemId: id, rating })
    // 详情页评分 → 通知画廊回灌 visibleRows（修复:此前详情改评分画廊不刷新、需手动刷新页面）。
    signalItemPatch(id, 'rating', rating)
  }

  /** 批量评分:对选区一次性设为 rating（0-5,0=清空）。返回受影响行数。
   *  R1-2/S4:入参 SelectionDescriptor——全选不整包传 id,后端解析后分块 UPDATE。 */
  async function batchSetRating(
    selection: BackendSelectionDescriptor,
    rating: number,
  ): Promise<number> {
    return await invokeIpc<number>(IPC.BATCH_SET_RATING, { selection, rating })
  }

  /** 设置颜色标签（0=清除 / 1-7 色档，T16）。镜像 setRating。 */
  async function setColorLabel(id: number, colorLabel: number) {
    await invokeIpc<void>(IPC.SET_COLOR_LABEL, { itemId: id, colorLabel })
    // 详情页设色 → 通知画廊回灌 visibleRows（修复:此前详情设色画廊不刷新、需手动刷新页面）。
    signalItemPatch(id, 'colorLabel', colorLabel)
  }

  /** 批量设色:对选区一次性设为 colorLabel。返回受影响行数。镜像 batchSetRating（R1-2/S4）。 */
  async function batchSetColorLabel(
    selection: BackendSelectionDescriptor,
    colorLabel: number,
  ): Promise<number> {
    return await invokeIpc<number>(IPC.BATCH_SET_COLOR_LABEL, { selection, colorLabel })
  }

  return {
    layoutSummary,
    isComputingLayout,
    layoutDirty,
    itemPatchSignal,
    detailItem,
    isDetailOpen,
    navContext,
    stats,
    viewportMeta,
    totalItems,
    viewTotalItems,
    totalHeight,
    totalRows,
    layoutVersion,
    computeLayout,
    fetchRowsByY,
    ensureMeta,
    openDetail,
    openDetailFromSearch,
    refreshDetailAvailability,
    closeDetail,
    navigateDetail,
    loadStats,
    toggleFavorite,
    setRating,
    batchSetRating,
    setColorLabel,
    batchSetColorLabel,
    invalidateLayout,
    consumeLayoutDirty,
  }
})
