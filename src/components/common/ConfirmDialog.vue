<template>
  <!-- Shared promise-based confirm dialog. Mounted once; driven by useConfirm(). -->
  <!-- 共享的、基于 Promise 的确认对话框。仅挂载一次；由 useConfirm() 驱动。 -->
  <Teleport to="body">
    <div v-if="state.isOpen" class="dialog-overlay" @click.self="close(false)" @keydown.esc.stop="close(false)" tabindex="-1">
      <div class="dialog-content">
        <header class="dialog-header">
          <h2 class="dialog-title">{{ state.title }}</h2>
          <button class="btn-close" :title="state.cancelText" @click="close(false)">
            <X :size="18" />
          </button>
        </header>

        <main class="dialog-body">
          <p class="dialog-message">{{ state.message }}</p>

          <label v-if="state.showCheckbox" class="remember-checkbox">
            <input type="checkbox" v-model="state.checkboxValue" />
            <span>{{ state.checkboxLabel }}</span>
          </label>
        </main>

        <footer class="dialog-footer">
          <button class="btn btn-secondary" @click="close(false)">{{ state.cancelText }}</button>
          <button class="btn btn-primary" @click="close(true)">{{ state.confirmText }}</button>
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { X } from '@lucide/vue'
import { useConfirmDialogState } from '../../composables/useConfirm'

const { state, close } = useConfirmDialogState()
</script>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 10000;
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
  white-space: pre-line; /* honour \n in confirm messages | 保留确认信息中的换行 */
}

.remember-checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  user-select: none;
}
.remember-checkbox input[type="checkbox"] {
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
  from { opacity: 0; }
  to { opacity: 1; }
}
@keyframes slideUp {
  from { opacity: 0; transform: translateY(10px) scale(0.98); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}
</style>
