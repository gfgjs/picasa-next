<template>
  <div v-if="ui.showCloseConfirmDialog" class="dialog-overlay" @click.self="cancel">
    <div class="dialog-content">
      <header class="dialog-header">
        <h2 class="dialog-title">确认关闭</h2>
        <button class="btn-close" title="取消" @click="cancel">
          <X :size="18" />
        </button>
      </header>
      
      <main class="dialog-body">
        <p class="dialog-message">您想退出 Picasa Next，还是最小化到系统托盘？</p>
        
        <label class="remember-checkbox">
          <input type="checkbox" v-model="rememberChoice" />
          <span>记住我的选择，不再提示</span>
        </label>
      </main>

      <footer class="dialog-footer">
        <button class="btn btn-secondary" @click="minimizeToTray">最小化到托盘</button>
        <button class="btn btn-danger" @click="exitApp">退出应用</button>
      </footer>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { X } from '@lucide/vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useUiStore } from '../../stores/uiStore'

const ui = useUiStore()
const rememberChoice = ref(false)

function cancel() {
  ui.showCloseConfirmDialog = false
}

async function minimizeToTray() {
  if (rememberChoice.value) {
    ui.setCloseBehavior('minimize_to_tray')
  }
  ui.showCloseConfirmDialog = false
  await invoke('hide_window')
}

async function exitApp() {
  if (rememberChoice.value) {
    ui.setCloseBehavior('exit')
  }
  ui.showCloseConfirmDialog = false
  await invoke('exit_app')
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
