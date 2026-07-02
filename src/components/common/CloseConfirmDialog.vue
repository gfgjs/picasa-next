<template>
  <div
    v-if="ui.showCloseConfirmDialog"
    ref="overlayEl"
    class="dialog-overlay"
    tabindex="-1"
    @click.self="cancel"
    @keydown.esc.stop="cancel"
  >
    <!-- R1-8 可访问性底线:dialog 语义 + aria-modal + 焦点陷阱(与 ConfirmDialog 同款)。 -->
    <div
      class="dialog-content"
      role="dialog"
      aria-modal="true"
      aria-labelledby="close-confirm-title"
      aria-describedby="close-confirm-message"
    >
      <header class="dialog-header">
        <h2 id="close-confirm-title" class="dialog-title">{{ t('closeConfirm.title') }}</h2>
        <button
          class="btn-close"
          :title="t('common.cancel')"
          :aria-label="t('common.cancel')"
          @click="cancel"
        >
          <X :size="18" />
        </button>
      </header>

      <main class="dialog-body">
        <p id="close-confirm-message" class="dialog-message">{{ t('closeConfirm.message') }}</p>

        <label class="remember-checkbox">
          <input type="checkbox" v-model="rememberChoice" />
          <span>{{ t('closeConfirm.remember') }}</span>
        </label>
      </main>

      <footer class="dialog-footer">
        <!-- 初始焦点落在「最小化到托盘」(最不具破坏性的选项)。 -->
        <button class="btn btn-secondary" data-autofocus @click="minimizeToTray">
          {{ t('closeConfirm.minimize') }}
        </button>
        <button class="btn btn-danger" @click="exitApp">{{ t('closeConfirm.exit') }}</button>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { X } from '@lucide/vue'
import { invokeIpc } from '../../utils/ipc'
import { IPC } from '../../constants/ipc'
import { useUiStore } from '../../stores/uiStore'
import { useFocusTrap } from '../../composables/useFocusTrap'

const { t } = useI18n()
const ui = useUiStore()
const rememberChoice = ref(false)

// 焦点陷阱:打开入框、Tab 循环、关闭归还(R1-8);Esc 取消为本次补齐(此前仅点击遮罩可关)。
const overlayEl = ref<HTMLElement | null>(null)
useFocusTrap(overlayEl, () => ui.showCloseConfirmDialog)

function cancel() {
  ui.showCloseConfirmDialog = false
}

async function minimizeToTray() {
  if (rememberChoice.value) {
    ui.setCloseBehavior('minimize_to_tray')
  }
  ui.showCloseConfirmDialog = false
  await invokeIpc(IPC.HIDE_WINDOW)
}

async function exitApp() {
  if (rememberChoice.value) {
    ui.setCloseBehavior('exit')
  }
  ui.showCloseConfirmDialog = false
  await invokeIpc(IPC.EXIT_APP)
}
</script>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  background: color-mix(in srgb, var(--color-bg-primary) 60%, transparent);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
  animation: fadeIn 0.2s ease-out;
}

.dialog-content {
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: 0 10px 40px rgba(0, 0, 0, 0.2);
  width: 100%;
  max-width: 420px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  animation: slideUp 0.2s ease-out;
}

.dialog-header {
  padding: var(--spacing-md) var(--spacing-lg);
  border-bottom: 1px solid var(--color-border);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.dialog-title {
  margin: 0;
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--color-text-primary);
}

.btn-close {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.btn-close:hover {
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
}

.dialog-body {
  padding: var(--spacing-lg);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}

.dialog-message {
  margin: 0;
  font-size: var(--font-size-base);
  color: var(--color-text-secondary);
  line-height: 1.5;
}

.remember-checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  user-select: none;
  margin-top: var(--spacing-sm);
}

.remember-checkbox input[type='checkbox'] {
  width: 16px;
  height: 16px;
  cursor: pointer;
  accent-color: var(--color-accent);
}

.dialog-footer {
  padding: var(--spacing-md) var(--spacing-lg);
  border-top: 1px solid var(--color-border);
  background: var(--color-bg-primary);
  display: flex;
  justify-content: flex-end;
  gap: var(--spacing-sm);
}

@keyframes fadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes slideUp {
  from {
    opacity: 0;
    transform: translateY(10px) scale(0.98);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}
</style>
