<template>
  <Transition name="slide-down">
    <div v-if="selection.isSelectionMode.value" class="selection-toolbar">
      <div class="selection-toolbar__left">
        <span class="selection-count">
          {{ $t('selection.selected', { count: selection.selectedCount.value }) }}
        </span>
      </div>
      <div class="selection-toolbar__actions">
        <button class="selection-action" @click="$emit('select-all')" title="全选">
          <CheckSquare :size="16" />
          <span>全选</span>
        </button>
        <button class="selection-action" @click="$emit('invert-selection')" title="反选">
          <CopyMinus :size="16" />
          <span>反选</span>
        </button>
        <button class="selection-action" @click="$emit('batch-favorite')" :title="$t('selection.favorite')">
          <Heart :size="16" />
          <span>{{ $t('selection.favorite') }}</span>
        </button>
        <button class="selection-action selection-action--danger" @click="$emit('batch-delete')" :title="$t('selection.delete')">
          <Trash2 :size="16" />
          <span>{{ $t('selection.delete') }}</span>
        </button>
        <button class="selection-action" @click="selection.clearSelection()" :title="$t('selection.cancel')">
          <X :size="16" />
          <span>{{ $t('selection.cancel') }}</span>
        </button>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { Heart, Trash2, X, CheckSquare, CopyMinus } from '@lucide/vue'
import { useSelection } from '../../composables/useSelection'

defineEmits<{
  (e: 'batch-favorite'): void
  (e: 'batch-delete'): void
  (e: 'select-all'): void
  (e: 'invert-selection'): void
}>()

const selection = useSelection()
</script>

<style scoped>
.selection-toolbar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 16px;
  background: color-mix(in srgb, var(--color-bg-surface) 80%, transparent);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  color: var(--color-text-primary);
  border-bottom: 1px solid var(--color-border);
}

.selection-count {
  font-size: 14px;
  font-weight: 600;
}

.selection-toolbar__actions {
  display: flex;
  gap: 8px;
}

.selection-action {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 12px;
  border-radius: var(--radius-md);
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
  border: 1px solid transparent;
  font-size: 13px;
  cursor: pointer;
  transition: all var(--transition-fast);
}

.selection-action:hover {
  background: var(--color-bg-active);
}

.selection-action--danger {
  color: var(--color-error);
}

.selection-action--danger:hover {
  background: var(--color-error);
  color: #fff;
}

.slide-down-enter-active,
.slide-down-leave-active {
  transition: all 0.2s ease;
}
.slide-down-enter-from,
.slide-down-leave-to {
  transform: translateY(-100%);
  opacity: 0;
}
</style>
