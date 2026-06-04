// src/composables/useSelection.ts
// Flagship-quality drag selection composable
// 旗舰级拖拽选择 composable

import { ref, computed, readonly, watch } from 'vue'

type DragMode = 'select' | 'deselect' | null

// Singleton pattern — all consumers share the same state
// 单例模式 — 所有消费者共享同一状态
const selectedIds = ref(new Set<number>())
const isSelectionMode = ref(false)
const isDragging = ref(false)
const dragMode = ref<DragMode>(null)
const dragStartPos = ref<{ x: number; y: number } | null>(null)
const dragStartId = ref<number | null>(null)
const lastHoveredId = ref<number | null>(null)
const hasDragMoved = ref(false)
const lastClickedId = ref<number | null>(null)
const dragStartContainer = ref<HTMLElement | null>(null)

// Tracking for drag sequence to allow reverse-selection and cross-row ranges
const initialSelection = ref(new Set<number>())
const lastValidRangeIds = ref<number[]>([])

// Move threshold: must move > 5px to count as drag (not click)
// 移动阈值：必须移动 > 5px 才算拖拽（不是单击）
const DRAG_THRESHOLD = 5

const selectedCount = computed(() => selectedIds.value.size)

// Auto-exit selection mode when nothing selected
// 无选中项时自动退出选择模式
watch(selectedCount, (count) => {
  if (count === 0 && isSelectionMode.value) {
    isSelectionMode.value = false
  }
})

export function useSelection() {
  // ── Single item operations | 单项操作 ──

  function toggleSelect(id: number) {
    const newSet = new Set(selectedIds.value)
    if (newSet.has(id)) {
      newSet.delete(id)
    } else {
      newSet.add(id)
    }
    selectedIds.value = newSet
    lastClickedId.value = id
    if (newSet.size > 0) isSelectionMode.value = true
  }

  function selectRange(fromId: number | null, toId: number, orderedIds: number[]) {
    if (fromId === null) {
      toggleSelect(toId)
      return
    }
    const fromIndex = orderedIds.indexOf(fromId)
    const toIndex = orderedIds.indexOf(toId)
    if (fromIndex === -1 || toIndex === -1) return

    const start = Math.min(fromIndex, toIndex)
    const end = Math.max(fromIndex, toIndex)
    const newSet = new Set(selectedIds.value)
    for (let i = start; i <= end; i++) {
      newSet.add(orderedIds[i])
    }
    selectedIds.value = newSet
    lastClickedId.value = toId
  }

  function selectAll(allIds: number[]) {
    selectedIds.value = new Set(allIds)
    isSelectionMode.value = true
  }

  function clearSelection() {
    selectedIds.value = new Set()
    isSelectionMode.value = false
    lastClickedId.value = null
  }

  function invertSelection(allIds: number[]) {
    const newSet = new Set<number>()
    for (const id of allIds) {
      if (!selectedIds.value.has(id)) {
        newSet.add(id)
      }
    }
    selectedIds.value = newSet
    if (newSet.size === 0) {
      isSelectionMode.value = false
    } else {
      isSelectionMode.value = true
    }
  }

  function isSelected(id: number): boolean {
    return selectedIds.value.has(id)
  }

  // ── Drag selection | 拖拽选择 ──

  function onPointerDown(id: number, event: PointerEvent) {
    // Only handle primary button (left click)
    // 仅处理主按钮（左键）
    if (event.button !== 0) return

    dragStartPos.value = { x: event.clientX, y: event.clientY }
    dragStartId.value = id
    hasDragMoved.value = false
    lastHoveredId.value = id

    const target = event.target as HTMLElement | null
    dragStartContainer.value = target?.closest('.media-grid, .semantic-panel__grid') as HTMLElement | null

    // Determine drag mode: if starting on a selected item, deselect; otherwise select
    // 确定拖拽模式：如果从已选中项开始，则反选；否则选中
    if (isSelectionMode.value && selectedIds.value.has(id)) {
      dragMode.value = 'deselect'
    } else {
      dragMode.value = 'select'
    }

    initialSelection.value = new Set(selectedIds.value)
    lastValidRangeIds.value = []

    // Register document-level listeners (cleaned up in onPointerUp)
    // 注册文档级监听器（在 onPointerUp 中清理）
    document.addEventListener('pointermove', onPointerMoveGlobal)
    document.addEventListener('pointerup', onPointerUpGlobal)

    // Prevent text selection during drag
    // 防止拖拽期间的文本选择
    event.preventDefault()
  }

  function onPointerMoveGlobal(event: PointerEvent) {
    if (!dragStartPos.value) return

    // Check if we've exceeded the move threshold
    // 检查是否超过移动阈值
    if (!hasDragMoved.value) {
      const dx = event.clientX - dragStartPos.value.x
      const dy = event.clientY - dragStartPos.value.y
      if (Math.sqrt(dx * dx + dy * dy) < DRAG_THRESHOLD) return

      // Threshold exceeded — officially start drag
      // 超过阈值 — 正式开始拖拽
      hasDragMoved.value = true
      isDragging.value = true

      // Apply initial item
      if (dragStartId.value !== null) {
        applyDragRange(dragStartId.value, dragStartId.value)
      }
    }

    // Find the item under the pointer via elementFromPoint
    // 通过 elementFromPoint 找到指针下方的项
    const el = document.elementFromPoint(event.clientX, event.clientY)
    if (!el) return

    const card = (el as HTMLElement).closest('[data-item-id]') as HTMLElement | null
    if (!card) return

    const itemId = parseInt(card.dataset.itemId!, 10)
    if (isNaN(itemId)) return

    if (itemId !== lastHoveredId.value) {
      lastHoveredId.value = itemId
      
      applyDragRange(dragStartId.value!, itemId)
    }
  }

  function applyDragRange(startId: number, endId: number) {
    const container = dragStartContainer.value || document
    const cards = container.querySelectorAll('[data-item-id]')
    const visibleIds: number[] = []
    for (let i = 0; i < cards.length; i++) {
      const id = parseInt((cards[i] as HTMLElement).dataset.itemId || '', 10)
      if (!isNaN(id)) visibleIds.push(id)
    }

    const startIndex = visibleIds.indexOf(startId)
    const endIndex = visibleIds.indexOf(endId)

    let rangeIds: number[] = []
    if (startIndex !== -1 && endIndex !== -1) {
      const minIdx = Math.min(startIndex, endIndex)
      const maxIdx = Math.max(startIndex, endIndex)
      for (let i = minIdx; i <= maxIdx; i++) {
        rangeIds.push(visibleIds[i])
      }
      lastValidRangeIds.value = rangeIds
    } else {
      // Fallback if scrolled out
      rangeIds = [...lastValidRangeIds.value]
      if (!rangeIds.includes(endId)) {
        rangeIds.push(endId)
      }
    }

    const newSet = new Set(initialSelection.value)
    for (const id of rangeIds) {
      if (dragMode.value === 'select') {
        newSet.add(id)
      } else if (dragMode.value === 'deselect') {
        newSet.delete(id)
      }
    }
    
    selectedIds.value = newSet
    if (newSet.size > 0 && !isSelectionMode.value) {
      isSelectionMode.value = true
    } else if (newSet.size === 0 && isSelectionMode.value) {
      isSelectionMode.value = false
    }
  }

  function onPointerUpGlobal(_event: PointerEvent) {
    // Clean up document listeners
    // 清理文档监听器
    document.removeEventListener('pointermove', onPointerMoveGlobal)
    document.removeEventListener('pointerup', onPointerUpGlobal)

    isDragging.value = false
    dragStartPos.value = null
    dragStartId.value = null
    lastHoveredId.value = null
    dragMode.value = null
    dragStartContainer.value = null
  }

  // ── Keyboard handling | 键盘处理 ──

  function onKeyDown(event: KeyboardEvent, getAllVisibleIds: () => number[]) {
    if (event.key === 'Escape' && isSelectionMode.value) {
      clearSelection()
      event.preventDefault()
    }
    if ((event.ctrlKey || event.metaKey) && event.key === 'a' && isSelectionMode.value) {
      selectAll(getAllVisibleIds())
      event.preventDefault()
    }
  }

  // Return value — was the pointerdown a drag or a click?
  // 返回值 — pointerdown 是拖拽还是单击？
  function wasDrag(): boolean {
    return hasDragMoved.value
  }

  return {
    selectedIds: readonly(selectedIds),
    isSelectionMode: readonly(isSelectionMode),
    isDragging: readonly(isDragging),
    selectedCount,
    lastClickedId: readonly(lastClickedId),

    toggleSelect,
    selectRange,
    selectAll,
    clearSelection,
    invertSelection,
    isSelected,

    onPointerDown,
    wasDrag,

    onKeyDown,
  }
}
