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
          <div class="toast__row">
            <component :is="iconMap[toast.type]" :size="16" class="toast__icon" />
            <span class="toast__msg">{{ toast.message }}</span>
            <X :size="14" class="toast__close" @click="ui.removeToast(toast.id)" />
          </div>
          <!-- 交互式快捷 chips（如「加入收藏夹」），点击后执行并关闭该 toast -->
          <div v-if="toast.actions?.length" class="toast__actions">
            <button
              v-for="(action, i) in toast.actions"
              :key="i"
              class="toast__chip"
              @click="onAction(toast.id, action)"
            >
              {{ action.label }}
            </button>
          </div>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { useUiStore } from '../../stores/uiStore'
import { Check, X, AlertTriangle, Info } from '@lucide/vue'
import type { Component } from 'vue'
import type { ToastAction } from '../../types/ui'

const ui = useUiStore()
const iconMap: Record<string, Component> = {
  success: Check,
  error: X,
  warning: AlertTriangle,
  info: Info,
}

// Run a chip action, then dismiss its toast. | 执行 chip 动作后关闭该 toast。
async function onAction(toastId: string, action: ToastAction) {
  try {
    await action.onClick()
  } finally {
    ui.removeToast(toastId)
  }
}
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
  z-index: 99999;
  pointer-events: none;
}
.toast {
  display: flex;
  flex-direction: column;
  gap: 8px;
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
.toast__row {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
}
.toast__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding-left: 24px; /* align under message, past the icon | 与消息对齐，让过图标 */
}
.toast__chip {
  padding: 3px 10px;
  border-radius: 999px;
  font-size: var(--font-size-xs);
  font-weight: 600;
  background: rgba(255, 255, 255, 0.25);
  color: inherit;
  border: 1px solid rgba(255, 255, 255, 0.35);
  cursor: pointer;
  transition: background var(--transition-fast);
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.toast__chip:hover {
  background: rgba(255, 255, 255, 0.4);
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
/* 状态色走主题 token(S5:原 rgba 字面量重复且绕开了各主题的明暗适配);
   文字用 text-inverse——亮主题白字压深色 token,暗主题深字压亮色 token。 */
.toast--success {
  background: var(--color-success);
  color: var(--color-text-inverse);
}
.toast--error {
  background: var(--color-error);
  color: var(--color-text-inverse);
}
.toast--warning {
  background: var(--color-warning);
  color: var(--color-text-inverse);
}
.toast--info {
  background: var(--color-info);
  color: var(--color-text-inverse);
}

.toast-enter-from {
  opacity: 0;
  transform: translateY(12px);
}
.toast-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
.toast-enter-active,
.toast-leave-active {
  transition: all 200ms ease;
}
</style>
