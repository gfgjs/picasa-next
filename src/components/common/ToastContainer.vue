<template>
  <Teleport to="body">
    <div class="toast-container">
      <TransitionGroup name="toast">
        <div
          v-for="toast in ui.toasts"
          :key="toast.id"
          class="toast"
          :class="`toast--${toast.type}`"
        >
          <component :is="iconMap[toast.type]" :size="16" class="toast__icon" />
          <span class="toast__msg">{{ toast.message }}</span>
          <X :size="14" class="toast__close" @click="ui.removeToast(toast.id)" />
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { useUiStore } from '../../stores/uiStore'
import { Check, X, AlertTriangle, Info } from '@lucide/vue'

const ui = useUiStore()
const iconMap: Record<string, any> = { success: Check, error: X, warning: AlertTriangle, info: Info }
</script>

<style scoped>
.toast-container {
  position: fixed;
  bottom: 48px;
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--spacing-sm);
  z-index: 999;
  pointer-events: none;
}
.toast {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: 10px 16px;
  border-radius: var(--radius-lg);
  font-size: var(--font-size-sm);
  font-weight: 500;
  pointer-events: auto;
  cursor: default;
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  box-shadow: var(--shadow-lg);
  max-width: 460px;
  user-select: text;
}
.toast__msg {
  flex-grow: 1;
  word-break: break-all;
}
.toast__close {
  cursor: pointer;
  opacity: 0.7;
  transition: opacity 0.2s;
  flex-shrink: 0;
  margin-left: 8px;
}
.toast__close:hover {
  opacity: 1;
}
.toast--success { background: rgba(52, 199, 89, 0.9);  color: #fff; }
.toast--error   { background: rgba(255, 59, 48, 0.9);  color: #fff; }
.toast--warning { background: rgba(255, 149, 0, 0.9);  color: #fff; }
.toast--info    { background: rgba(90, 200, 250, 0.9); color: #000; }

.toast-enter-from { opacity: 0; transform: translateY(12px); }
.toast-leave-to   { opacity: 0; transform: translateY(-8px); }
.toast-enter-active, .toast-leave-active { transition: all 200ms ease; }
</style>
