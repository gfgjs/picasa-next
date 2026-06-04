// src/composables/useSelection.ts
// 媒体项多选状态管理 | Multi-select state management for media items
//
// 提供：选中集合、拖框多选、批量收藏/软删除
// Provides: selection set, rubber-band drag selection, batch favorite/soft-delete

import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { IPC } from '../constants/ipc'

// ── Types ─────────────────────────────────────────────────────────────────────

export interface SelectionItem {
  id:    number
  index: number  // 在扁平化列表中的位置，用于 range 选择 | flat-list index for range selection
}

// ── Composable ────────────────────────────────────────────────────────────────

export function useSelection() {
  // 已选 id 集合 | Set of selected item IDs
  const selectedIds = ref<Set<number>>(new Set())

  // 进入选择模式 | Selection mode active flag
  const isSelectionMode = ref(false)

  // 拖拽框选进行中 | Rubber-band drag in progress
  const isDragging = ref(false)
  const hasDragged = ref(false)

  // 上一次 anchor（Shift 起点）id | Last anchor ID for Shift+click range
  const anchorId = ref<number | null>(null)

  // ── 计算属性 ─────────────────────────────────────────────────────────────
  const selectedCount = computed(() => selectedIds.value.size)
  const hasSelection  = computed(() => selectedIds.value.size > 0)

  function isSelected(id: number): boolean {
    return selectedIds.value.has(id)
  }

  // ── 基础操作 ─────────────────────────────────────────────────────────────

  /** 切换单项选中状态，进入选择模式 | Toggle one item, enter selection mode */
  function toggleSelect(id: number) {
    const next = new Set(selectedIds.value)
    if (next.has(id)) {
      next.delete(id)
    } else {
      next.add(id)
      anchorId.value = id
    }
    selectedIds.value  = next
    isSelectionMode.value = next.size > 0
  }

  /**
   * 范围选择（Shift+点击）：从 anchorId 到 targetId 之间的所有项都选中。
   * Range select (Shift+click): select everything between anchorId and targetId.
   * @param allIds  当前视图中按顺序排列的所有 id | all IDs in display order
   */
  function selectRange(allIds: number[], targetId: number) {
    const anchor = anchorId.value
    if (anchor === null || anchor === targetId) {
      toggleSelect(targetId)
      return
    }
    const ai = allIds.indexOf(anchor)
    const ti = allIds.indexOf(targetId)
    if (ai === -1 || ti === -1) { toggleSelect(targetId); return }
    const [lo, hi] = ai < ti ? [ai, ti] : [ti, ai]
    const next = new Set(selectedIds.value)
    for (let i = lo; i <= hi; i++) next.add(allIds[i])
    selectedIds.value     = next
    isSelectionMode.value = true
  }

  /** 全选当前视图所有 id | Select all IDs in current view */
  function selectAll(allIds: number[]) {
    selectedIds.value     = new Set(allIds)
    isSelectionMode.value = true
    if (allIds.length > 0) anchorId.value = allIds[0]
  }

  /** 清空选择并退出选择模式 | Clear selection and exit selection mode */
  function clearSelection() {
    selectedIds.value     = new Set()
    isSelectionMode.value = false
    anchorId.value        = null
    isDragging.value      = false
  }

  // ── 拖框多选 ─────────────────────────────────────────────────────────────
  // 使用 dragAnchor 记录鼠标按下位置，dragOverIds 是拖拽过程中扫过的 ids
  // dragAnchor records pointer-down position; dragOverIds are IDs swept during drag

  const _dragAnchorId = ref<number | null>(null)

  /** 开始拖拽选择 | Begin rubber-band drag selection */
  function onDragStart(startId: number) {
    isDragging.value  = true
    hasDragged.value  = false
    _dragAnchorId.value = startId
    // 不立刻 toggleSelect，等待 onDragOver 确定方向
    // Don't toggleSelect yet; wait for onDragOver to determine direction
  }

  /**
   * 拖拽经过某个 item | Pointer enters an item during drag
   * @param currentId  当前鼠标经过的 item id
   * @param allIds     视图中所有 id（有序）
   */
  function onDragOver(currentId: number, allIds: number[]) {
    if (!isDragging.value || _dragAnchorId.value === null) return
    if (_dragAnchorId.value !== currentId) {
      hasDragged.value = true
    }
    const anchor = _dragAnchorId.value
    const ai = allIds.indexOf(anchor)
    const ci = allIds.indexOf(currentId)
    if (ai === -1 || ci === -1) return
    const [lo, hi] = ai < ci ? [ai, ci] : [ci, ai]
    const next = new Set<number>()
    for (let i = lo; i <= hi; i++) next.add(allIds[i])
    selectedIds.value     = next
    isSelectionMode.value = true
  }

  /** 结束拖拽 | End rubber-band drag */
  function onDragEnd() {
    isDragging.value     = false
    _dragAnchorId.value  = null
    if (selectedIds.value.size > 0) {
      anchorId.value = [...selectedIds.value][selectedIds.value.size - 1]
    }
  }

  // ── 批量操作 ─────────────────────────────────────────────────────────────

  /** 批量收藏 | Batch favorite all selected items */
  async function batchFavorite(): Promise<void> {
    const ids = [...selectedIds.value]
    if (ids.length === 0) return
    await invoke(IPC.BATCH_TOGGLE_FAVORITE, { itemIds: ids, value: true })
  }

  /** 批量取消收藏 | Batch unfavorite all selected items */
  async function batchUnfavorite(): Promise<void> {
    const ids = [...selectedIds.value]
    if (ids.length === 0) return
    await invoke(IPC.BATCH_TOGGLE_FAVORITE, { itemIds: ids, value: false })
  }

  /** 批量软删除 | Batch soft-delete all selected items */
  async function batchSoftDelete(): Promise<void> {
    const ids = [...selectedIds.value]
    if (ids.length === 0) return
    await invoke(IPC.SOFT_DELETE_ITEMS, { itemIds: ids })
  }

  return {
    // state
    selectedIds,
    isSelectionMode,
    isDragging,
    hasDragged,
    selectedCount,
    hasSelection,
    // helpers
    isSelected,
    // selection
    toggleSelect,
    selectRange,
    selectAll,
    clearSelection,
    // drag
    onDragStart,
    onDragOver,
    onDragEnd,
    // batch ops
    batchFavorite,
    batchUnfavorite,
    batchSoftDelete,
  }
}
