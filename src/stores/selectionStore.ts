// src/stores/selectionStore.ts
// 媒体项选择状态管理
// Media item selection state management

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { useMediaStore } from './mediaStore'
import { useUiStore } from './uiStore'
import { invoke } from '@tauri-apps/api/core'
import { IPC, EVENTS } from '../constants/ipc'
import { emit } from '@tauri-apps/api/event'
import i18n from '../i18n'

export const useSelectionStore = defineStore('selection', () => {
  const media = useMediaStore()
  const ui = useUiStore()

  // Set of selected item IDs
  // 选中的媒体项 ID 集合
  const selectedIds = ref<Set<number>>(new Set())

  // Selection mode is active if there is at least one item selected
  // 如果至少有一个项目被选中，则选择模式处于活动状态
  const isSelectionMode = computed(() => selectedIds.value.size > 0)
  const selectionCount = computed(() => selectedIds.value.size)

  function toggleSelection(id: number) {
    if (selectedIds.value.has(id)) {
      selectedIds.value.delete(id)
    } else {
      selectedIds.value.add(id)
    }
  }

  function selectItem(id: number) {
    selectedIds.value.add(id)
  }

  function deselectItem(id: number) {
    selectedIds.value.delete(id)
  }

  function clearSelection() {
    selectedIds.value.clear()
  }

  async function selectAll() {
    if (!media.layoutSummary) return
    try {
      const ids: number[] = await invoke(IPC.GET_LAYOUT_ITEM_IDS, {
        layoutVersion: media.layoutSummary.layoutVersion
      })
      selectedIds.value = new Set(ids)
    } catch (e) {
      console.error('Failed to get layout item IDs:', e)
    }
  }

  // Batch operations
  // 批量操作
  async function deleteSelected() {
    if (selectedIds.value.size === 0) return
    const ids = Array.from(selectedIds.value)
    try {
      await invoke(IPC.SOFT_DELETE_ITEMS, { itemIds: ids })
      // Trigger media layout recompute via rust event or local logic
      await emit(EVENTS.MEDIA_UPDATED)
      ui.addToast('success', i18n.global.t('empty.deletedN', { n: ids.length }))
      clearSelection()
    } catch (e) {
      console.error('Failed to delete items:', e)
      ui.addToast('error', '删除失败 / Delete failed')
    }
  }

  async function favoriteSelected(state: boolean) {
    if (selectedIds.value.size === 0) return
    const ids = Array.from(selectedIds.value)
    try {
      // Need a batch toggle favorite backend function, or loop. Loop is fine for sqlite but batch is better.
      // We will loop for now, if backend doesn't support batch favorite.
      // 循环调用 toggle_favorite
      for (const id of ids) {
        // Here we don't have a specific `set_favorite` API, only `toggle`.
        // To accurately set, we might need a `set_favorite_items(ids, state)` IPC.
        await invoke(IPC.TOGGLE_FAVORITE, { itemId: id })
      }
      await emit(EVENTS.MEDIA_UPDATED)
      ui.addToast('success', i18n.global.t('empty.updatedN', { n: ids.length }))
      clearSelection()
    } catch (e) {
      console.error('Failed to favorite items:', e)
      ui.addToast('error', '操作失败 / Action failed')
    }
  }

  return {
    selectedIds,
    isSelectionMode,
    selectionCount,
    toggleSelection,
    selectItem,
    deselectItem,
    clearSelection,
    selectAll,
    deleteSelected,
    favoriteSelected
  }
})
