<template>
  <Transition name="slide-up">
    <div v-if="selection.isSelectionMode.value" class="selection-toolbar-wrapper">
      <div 
        class="selection-toolbar"
        :class="{ 'is-dragging': isDragging }"
        :style="{ transform: `translate(${offsetX}px, ${offsetY}px)` }"
      >
        <div class="drag-handle" @pointerdown="onDragStart" title="拖动">
          <GripVertical :size="16" />
        </div>
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
          <button class="selection-action" @click="$emit('batch-unfavorite')" title="取消收藏">
            <HeartOff :size="16" />
            <span>取消收藏</span>
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
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Heart, HeartOff, Trash2, X, CheckSquare, CopyMinus, GripVertical } from '@lucide/vue'
import { useSelection } from '../../composables/useSelection'

defineEmits<{
  (e: 'batch-favorite'): void
  (e: 'batch-unfavorite'): void
  (e: 'batch-delete'): void
  (e: 'select-all'): void
  (e: 'invert-selection'): void
}>()

const selection = useSelection()

const offsetX = ref(0)
const offsetY = ref(0)
const isDragging = ref(false)
let startX = 0
let startY = 0
let initOffsetX = 0
let initOffsetY = 0

function onDragStart(e: PointerEvent) {
  // Ignore right clicks
  if (e.button !== 0) return
  isDragging.value = true
  startX = e.clientX
  startY = e.clientY
  initOffsetX = offsetX.value
  initOffsetY = offsetY.value
  
  const target = e.currentTarget as HTMLElement
  target.setPointerCapture(e.pointerId)
  
  target.addEventListener('pointermove', onDragMove)
  target.addEventListener('pointerup', onDragEnd)
  target.addEventListener('pointercancel', onDragEnd)
}

function onDragMove(e: PointerEvent) {
  if (!isDragging.value) return
  offsetX.value = initOffsetX + (e.clientX - startX)
  offsetY.value = initOffsetY + (e.clientY - startY)
}

function onDragEnd(e: PointerEvent) {
  isDragging.value = false
  const target = e.currentTarget as HTMLElement
  target.removeEventListener('pointermove', onDragMove)
  target.removeEventListener('pointerup', onDragEnd)
  target.removeEventListener('pointercancel', onDragEnd)
  target.releasePointerCapture(e.pointerId)
}

// Reset position when selection mode is exited
import { watch } from 'vue'
watch(() => selection.isSelectionMode.value, (newVal) => {
  if (!newVal) {
    offsetX.value = 0
    offsetY.value = 0
  }
})
</script>

<style scoped>
.selection-toolbar-wrapper {
  position: absolute;
  bottom: 32px;
  left: 0;
  right: 0;
  display: flex;
  justify-content: center;
  z-index: 50;
  pointer-events: none; /* Let clicks pass through outside toolbar */
}

.selection-toolbar {
  pointer-events: auto; /* Enable clicks on the toolbar itself */
  width: max-content;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 8px 16px 8px 8px; /* Less padding on left because of drag handle */
  background: color-mix(in srgb, var(--color-bg-surface) 90%, transparent);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  color: var(--color-text-primary);
  border: 1px solid var(--color-border);
  border-radius: 99px;
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.2);
}

.selection-count {
  font-size: 14px;
  font-weight: 600;
  white-space: nowrap;
}

.drag-handle {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-secondary);
  cursor: grab;
  padding: 4px;
  border-radius: var(--radius-sm);
  transition: all var(--transition-fast);
}

.drag-handle:hover {
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
}

.drag-handle:active {
  cursor: grabbing;
}

.selection-toolbar.is-dragging {
  transition: none; /* disable transition while dragging */
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.3);
}

.selection-toolbar__actions {
  display: flex;
  gap: 8px;
}

.selection-action {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border-radius: 99px;
  background: transparent;
  color: var(--color-text-primary);
  border: 1px solid transparent;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all var(--transition-fast);
}

.selection-action:hover {
  background: var(--color-bg-hover);
}

.selection-action--danger {
  color: var(--color-error);
}

.selection-action--danger:hover {
  background: var(--color-error);
  color: #fff;
}

.slide-up-enter-active,
.slide-up-leave-active {
  transition: all 0.3s cubic-bezier(0.16, 1, 0.3, 1);
}
.slide-up-enter-from,
.slide-up-leave-to {
  transform: translateY(30px);
  opacity: 0;
}
</style>
