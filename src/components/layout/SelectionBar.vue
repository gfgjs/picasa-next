<template>
  <Transition name="slide-up">
    <div v-if="selection.isSelectionMode" class="selection-bar">
      <div class="selection-bar__content">
        <div class="selection-bar__info">
          <X class="selection-bar__close" :size="20" @click="selection.clearSelection()" />
          <span class="selection-bar__count">已选择 {{ selection.selectionCount }} 项</span>
        </div>
        
        <div class="selection-bar__actions">
          <button class="selection-bar__btn" @click="selection.selectAll()">
            <CheckSquare :size="16" />
            <span>全选</span>
          </button>
          
          <button class="selection-bar__btn" @click="selection.favoriteSelected(true)">
            <Heart :size="16" />
            <span>收藏</span>
          </button>
          
          <button class="selection-bar__btn selection-bar__btn--danger" @click="confirmDelete">
            <Trash2 :size="16" />
            <span>删除</span>
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { X, CheckSquare, Heart, Trash2 } from '@lucide/vue'
import { useSelectionStore } from '../../stores/selectionStore'
import { useUiStore } from '../../stores/uiStore'

const selection = useSelectionStore()
const ui = useUiStore()

function confirmDelete() {
  if (confirm(`确定要删除选中的 ${selection.selectionCount} 项吗？`)) {
    selection.deleteSelected()
  }
}
</script>

<style scoped>
.selection-bar {
  position: fixed;
  bottom: 24px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 100;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
  padding: 8px 16px;
  color: var(--color-text-primary);
  min-width: 320px;
}

.slide-up-enter-active,
.slide-up-leave-active {
  transition: all 0.3s cubic-bezier(0.175, 0.885, 0.32, 1.275);
}

.slide-up-enter-from,
.slide-up-leave-to {
  opacity: 0;
  transform: translate(-50%, 20px) scale(0.95);
}

.selection-bar__content {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
}

.selection-bar__info {
  display: flex;
  align-items: center;
  gap: 12px;
}

.selection-bar__close {
  cursor: pointer;
  color: var(--color-text-secondary);
  transition: color var(--transition-fast);
}

.selection-bar__close:hover {
  color: var(--color-text-primary);
}

.selection-bar__count {
  font-weight: 600;
  font-size: var(--font-size-md);
}

.selection-bar__actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.selection-bar__btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: var(--radius-md);
  border: none;
  background: var(--color-bg-surface);
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
  transition: all var(--transition-fast);
}

.selection-bar__btn:hover {
  background: var(--color-bg-overlay);
  color: var(--color-text-primary);
}

.selection-bar__btn--danger {
  color: hsl(0, 70%, 60%);
}

.selection-bar__btn--danger:hover {
  background: hsl(0, 70%, 60%);
  color: white;
}
</style>
