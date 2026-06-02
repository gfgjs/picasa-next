<template>
  <div class="app-shell" :data-theme="ui.theme">
    <!-- Sidebar -->
    <!-- 侧边栏 -->
    <aside
      class="app-sidebar"
      :style="{ width: ui.sidebarWidth + 'px' }"
    >
      <slot name="sidebar" />
      <!-- Drag handle -->
      <!-- 拖拽手柄 -->
      <div
        class="sidebar-resize-handle"
        @mousedown="resizer.onMouseDown"
        :class="{ resizing: resizer.isResizing.value }"
      />
    </aside>

    <!-- Main area -->
    <!-- 主区域 -->
    <div class="app-main">
      <header class="app-toolbar">
        <slot name="toolbar" />
      </header>

      <main class="app-content">
        <slot />
      </main>

      <footer class="app-statusbar">
        <slot name="statusbar" />
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount } from 'vue'
import { useUiStore } from '../../stores/uiStore'
import { useSidebarResize } from '../../composables/useSidebarResize'

const ui      = useUiStore()
const resizer = useSidebarResize()

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'F11') {
    e.preventDefault()
    ui.toggleFullscreen()
  }
}

onMounted(() => {
  ui.initFullscreen()
  window.addEventListener('keydown', onKeyDown)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown)
})
</script>

<style scoped>
.app-shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
  background-color: var(--color-bg-primary);
  color: var(--color-text-primary);
}

.app-sidebar {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 180px;
  max-width: 400px;
  background-color: var(--color-bg-secondary);
  border-right: 1px solid var(--color-border);
  overflow: hidden;
  flex-shrink: 0;
}

.sidebar-resize-handle {
  position: absolute;
  top: 0;
  right: 0;
  width: 4px;
  height: 100%;
  cursor: ew-resize;
  z-index: 10;
  background: transparent;
  transition: background var(--transition-fast);
}
.sidebar-resize-handle:hover,
.sidebar-resize-handle.resizing {
  background: var(--color-accent);
}

.app-main {
  display: flex;
  flex-direction: column;
  flex: 1;
  overflow: hidden;
  min-width: 0;
}

.app-toolbar {
  height: var(--toolbar-height);
  min-height: var(--toolbar-height);
  display: flex;
  align-items: center;
  padding: 0 var(--spacing-md);
  background-color: var(--color-bg-secondary);
  border-bottom: 1px solid var(--color-border);
  gap: var(--spacing-sm);
  flex-shrink: 0;
}

.app-content {
  flex: 1;
  overflow: hidden;
  position: relative;
}

.app-statusbar {
  height: var(--statusbar-height);
  min-height: var(--statusbar-height);
  display: flex;
  align-items: center;
  padding: 0 var(--spacing-md);
  background-color: var(--color-bg-secondary);
  border-top: 1px solid var(--color-border);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
  flex-shrink: 0;
}
</style>
