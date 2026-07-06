<template>
  <!-- data-theme 单源在 documentElement(uiStore.applyAppearance 唯一写点);此处
       不得再绑一份——双源曾导致 system 模式规则不匹配(Part5 F1)与主题切换脱同步。 -->
  <div class="app-shell">
    <!-- Sidebar -->
    <!-- 侧边栏 -->
    <aside class="app-sidebar" :style="{ width: ui.sidebarWidth + 'px' }">
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

    <!-- Hold-Esc-to-exit-fullscreen hint (browser-style) | 按住 Esc 退出全屏提示（浏览器风格） -->
    <Teleport to="body">
      <Transition name="fs-hint">
        <div v-if="fsHintVisible" class="fs-exit-hint" role="status">
          <span class="fs-exit-hint__text">{{ $t('toolbar.holdEscToExit') }}</span>
          <div class="fs-exit-hint__track">
            <div
              class="fs-exit-hint__bar"
              :style="{
                width: fsHolding ? '100%' : '0%',
                transitionDuration: fsHolding ? fsHoldMs + 'ms' : '120ms',
              }"
            />
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount } from 'vue'
import { useUiStore } from '../../stores/uiStore'
import { useSidebarResize } from '../../composables/useSidebarResize'
import { useFullscreenExitGuard } from '../../composables/useFullscreenExitGuard'

const ui = useUiStore()
const resizer = useSidebarResize()

// Browser-style "hold Esc to exit fullscreen" guard (问题4): a single Esc tap no
// longer exits; the hint below fills while Esc is held and exits only on completion.
// 浏览器风格的「按住 Esc 退出全屏」守卫（问题4）：单击 Esc 不再退出；按住时下方提示进度条
// 填充，填满才退出。
const {
  hintVisible: fsHintVisible,
  holding: fsHolding,
  holdMs: fsHoldMs,
} = useFullscreenExitGuard()

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
  /* chrome 材质叠层:仅「宣」等有纸纹的主题非 none;画布区永不消费此 token */
  background-image: var(--texture-chrome);
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
  background-image: var(--texture-chrome);
  border-bottom: 1px solid var(--color-border);
  gap: var(--spacing-sm);
  flex-shrink: 0;
}

.app-content {
  flex: 1;
  overflow: hidden;
  position: relative;
  display: flex;
  flex-direction: column;
}

.app-statusbar {
  height: var(--statusbar-height);
  min-height: var(--statusbar-height);
  display: flex;
  align-items: center;
  padding: 0 var(--spacing-md);
  background-color: var(--color-bg-secondary);
  background-image: var(--texture-chrome);
  border-top: 1px solid var(--color-border);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
  flex-shrink: 0;
}

/* ── Hold-Esc-to-exit-fullscreen hint ──────────────────────────────────────── */
/* ── 按住 Esc 退出全屏提示 ─────────────────────────────────────────────────── */
.fs-exit-hint {
  position: fixed;
  top: 24px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 99999;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 10px 18px;
  /* 硬编码豁免(S5,设计 §6.2):全屏 HUD 语义为「永远深色玻璃浮层」(视频播放器
     OSD 同款),不随主题——亮色主题下亮底 HUD 反而在全屏媒体上不可读。 */
  background: rgba(20, 20, 20, 0.82);
  color: #fff;
  border-radius: 12px;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.35);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  pointer-events: none;
  user-select: none;
}
.fs-exit-hint__text {
  font-size: 13px;
  font-weight: 500;
  letter-spacing: 0.2px;
  white-space: nowrap;
}
.fs-exit-hint__track {
  width: 160px;
  height: 4px;
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.22);
  overflow: hidden;
}
.fs-exit-hint__bar {
  height: 100%;
  width: 0;
  border-radius: 2px;
  background: #fff;
  transition-property: width;
  transition-timing-function: linear;
}

.fs-hint-enter-active,
.fs-hint-leave-active {
  transition:
    opacity 160ms ease,
    transform 160ms ease;
}
.fs-hint-enter-from,
.fs-hint-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-8px);
}
</style>
