<template>
  <teleport to="body">
    <div v-if="visible" class="context-menu-overlay" @click.stop="hide" @contextmenu.prevent="hide">
      <div class="context-menu" :style="{ left: x + 'px', top: y + 'px' }" @click.stop>
        <button
          v-for="item in items"
          :key="item.id"
          class="context-menu__item"
          @click="handleItemClick(item)"
        >
          <span v-if="item.icon" class="context-menu__icon">
            <component :is="item.icon" :size="16" />
          </span>
          <span class="context-menu__label">{{ item.label }}</span>
        </button>
      </div>
    </div>
  </teleport>
</template>

<script setup lang="ts">
import type { Component } from 'vue'

export interface ContextMenuItem {
  id: string
  label: string
  icon?: Component
  action: () => void
}

defineProps<{
  items: ContextMenuItem[]
  visible: boolean
  x: number
  y: number
}>()

const emit = defineEmits<{
  (e: 'update:visible', value: boolean): void
}>()

function hide() {
  emit('update:visible', false)
}

function handleItemClick(item: ContextMenuItem) {
  item.action()
  hide()
}
</script>

<style scoped>
.context-menu-overlay {
  position: fixed;
  inset: 0;
  z-index: 100000;
}

.context-menu {
  position: absolute;
  min-width: 160px;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.context-menu__item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-primary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  text-align: left;
  border: none;
  transition: background-color var(--transition-fast);
}

.context-menu__item:hover {
  background: var(--color-bg-hover);
}

.context-menu__icon {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-secondary);
}
</style>
