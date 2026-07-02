<template>
  <Transition name="slide-up">
    <div v-if="selection.isSelectionMode.value" class="selection-toolbar-wrapper">
      <div
        class="selection-toolbar"
        :class="{ 'is-dragging': isDragging }"
        :style="{ transform: `translate(${offsetX}px, ${offsetY}px)` }"
      >
        <div class="drag-handle" @pointerdown="onDragStart" :title="$t('selection.drag')">
          <GripVertical :size="16" />
        </div>
        <div class="selection-toolbar__left">
          <span class="selection-count">
            {{ $t('selection.selected', { count: selection.selectedCount.value }) }}
          </span>
        </div>
        <div class="selection-toolbar__actions">
          <button
            class="selection-action"
            @click="$emit('select-all')"
            :data-tooltip="$t('common.selectAll')"
            :aria-label="$t('common.selectAll')"
          >
            <CheckSquare :size="18" />
          </button>
          <button
            class="selection-action"
            @click="$emit('invert-selection')"
            :data-tooltip="$t('selection.invert')"
            :aria-label="$t('selection.invert')"
          >
            <CopyMinus :size="18" />
          </button>
          <button
            class="selection-action"
            @click="$emit('batch-favorite')"
            :data-tooltip="$t('selection.favorite')"
            :aria-label="$t('selection.favorite')"
          >
            <Heart :size="18" />
          </button>
          <button
            class="selection-action"
            @click="$emit('batch-unfavorite')"
            :data-tooltip="$t('selection.unfavorite')"
            :aria-label="$t('selection.unfavorite')"
          >
            <HeartOff :size="18" />
          </button>
          <!-- 加入收藏夹（T21）：把选区加入用户收藏夹（复用收藏后的 chips 提示选夹/新建）。 -->
          <button
            class="selection-action"
            @click="$emit('add-to-collection')"
            :data-tooltip="$t('selection.addToCollection')"
            :aria-label="$t('selection.addToCollection')"
          >
            <FolderPlus :size="18" />
          </button>
          <!-- 批量颜色标签（T16）：点色块给选区设色，Ban 清除。:allow-clear=false 因每点即设
               （无单一"当前色"可 toggle-off）；清除走独立 Ban 按钮 emit batch-color(0)。 -->
          <ColorLabelPicker
            class="toolbar-colors"
            :model-value="0"
            :size="16"
            :allow-clear="false"
            @change="(v: number) => $emit('batch-color', v)"
          />
          <button
            class="selection-action"
            @click="$emit('batch-color', 0)"
            :data-tooltip="$t('selection.clearColor')"
            :aria-label="$t('selection.clearColor')"
          >
            <Ban :size="18" />
          </button>
          <button
            class="selection-action selection-action--danger"
            @click="$emit('batch-delete')"
            :data-tooltip="$t('selection.delete')"
            :aria-label="$t('selection.delete')"
          >
            <Trash2 :size="18" />
          </button>
          <div class="divider"></div>
          <button
            class="selection-action"
            @click="$emit('batch-move')"
            :data-tooltip="$t('common.moveTo')"
            :aria-label="$t('common.moveTo')"
          >
            <FolderInput :size="18" />
          </button>
          <button
            class="selection-action"
            @click="$emit('batch-copy')"
            :data-tooltip="$t('common.copyTo')"
            :aria-label="$t('common.copyTo')"
          >
            <Copy :size="18" />
          </button>
          <div class="divider"></div>
          <button
            class="selection-action"
            @click="selection.clearSelection()"
            :data-tooltip="$t('selection.cancel')"
            :aria-label="$t('selection.cancel')"
          >
            <X :size="18" />
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import {
  Heart,
  HeartOff,
  Trash2,
  X,
  CheckSquare,
  CopyMinus,
  GripVertical,
  FolderInput,
  FolderPlus,
  Copy,
  Ban,
} from '@lucide/vue'
import ColorLabelPicker from '../common/ColorLabelPicker.vue'
import { useSelection } from '../../composables/useSelection'

defineEmits<{
  (e: 'batch-favorite'): void
  (e: 'batch-unfavorite'): void
  (e: 'add-to-collection'): void
  (e: 'batch-delete'): void
  (e: 'batch-move'): void
  (e: 'batch-copy'): void
  (e: 'batch-color', value: number): void
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
watch(
  () => selection.isSelectionMode.value,
  (newVal) => {
    if (!newVal) {
      offsetX.value = 0
      offsetY.value = 0
    }
  },
)
</script>

<style scoped>
.selection-toolbar-wrapper {
  position: absolute;
  bottom: 32px;
  left: 0;
  right: 0;
  display: flex;
  justify-content: center;
  z-index: 200;
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
  align-items: center;
  gap: 8px;
}

/* 批量色块在动作条里垂直居中，与圆形动作按钮对齐。 */
.toolbar-colors {
  align-self: center;
}

.selection-action {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: transparent;
  color: var(--color-text-primary);
  border: none;
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

/* ── Custom CSS Tooltip ──────────────────────────────────────────────── */
[data-tooltip] {
  position: relative;
}

[data-tooltip]::after {
  content: attr(data-tooltip);
  position: absolute;
  bottom: calc(100% + 10px);
  left: 50%;
  transform: translateX(-50%) translateY(4px);
  padding: 6px 10px;
  background: var(--color-bg-elevated);
  color: var(--color-text-primary);
  font-size: 12px;
  font-weight: 500;
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  white-space: nowrap;
  pointer-events: none;
  opacity: 0;
  visibility: hidden;
  transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  z-index: 1000;
}

[data-tooltip]:hover::after {
  opacity: 1;
  visibility: visible;
  transform: translateX(-50%) translateY(0);
}

.divider {
  width: 1px;
  height: 20px;
  background: var(--color-border);
  margin: 0 4px;
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
