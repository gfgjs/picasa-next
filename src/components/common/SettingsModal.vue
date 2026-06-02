<template>
  <div v-if="isOpen" class="modal-overlay" @click.self="close">
    <div class="modal-content">
      <h2 class="modal-title">{{ $t('settings.title') }}</h2>
      
      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-label">{{ $t('settings.language') }}</div>
          <div class="setting-desc">{{ $t('settings.languageDesc') }}</div>
        </div>
        <select v-model="ui.language" @change="ui.setLanguage(ui.language)" class="setting-input">
          <option value="zh-CN">简体中文</option>
          <option value="en-US">English</option>
        </select>
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-label">{{ $t('settings.thumbSkipMaxKb') }}</div>
          <div class="setting-desc">{{ $t('settings.thumbSkipDesc') }}</div>
        </div>
        <input 
          type="number" 
          v-model.number="thumbSkipMaxKb" 
          min="0" 
          max="1000000"
          class="setting-input"
        />
      </div>

      <div class="setting-item">
        <div class="setting-info">
          <div class="setting-label">{{ $t('settings.clearCache') || '清除缓存' }}</div>
          <div class="setting-desc">{{ $t('settings.clearCacheDesc') || '强制清理浏览器缓存的图片并重载应用' }}</div>
        </div>
        <button class="btn btn-secondary" @click="clearBrowserCache">清除</button>
      </div>

      <div class="modal-actions">
        <button class="btn btn-secondary" @click="close">取消</button>
        <button class="btn btn-primary" @click="save">保存</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useUiStore } from '../../stores/uiStore'
import { useI18n } from 'vue-i18n'

const ui = useUiStore()
const { t } = useI18n()

const isOpen = ref(false)
const thumbSkipMaxKb = ref(200)

async function openModal() {
  isOpen.value = true
  try {
    const val = await invoke<string | null>('get_app_config', { key: 'thumb_skip_max_kb' })
    if (val) {
      thumbSkipMaxKb.value = parseInt(val, 10)
    }
  } catch (e) {
    console.error('Failed to get config:', e)
  }
}

async function save() {
  try {
    await invoke('set_app_config', { key: 'thumb_skip_max_kb', value: thumbSkipMaxKb.value.toString() })
    ui.addToast('success', t('settings.saveSuccess'))
    isOpen.value = false
  } catch (e) {
    ui.addToast('error', t('settings.saveFailed', { error: String(e) }))
  }
}

function close() {
  isOpen.value = false
}

function clearBrowserCache() {
  window.location.href = window.location.pathname + '?clear=' + Date.now()
}

defineExpose({ openModal })
</script>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  backdrop-filter: blur(4px);
}
.modal-content {
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  padding: var(--spacing-xl);
  width: 400px;
  max-width: 90vw;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.3);
}
.modal-title {
  margin: 0 0 var(--spacing-lg);
  font-size: var(--font-size-lg);
  font-weight: 600;
  color: var(--color-text-primary);
}
.setting-item {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--spacing-md);
  margin-bottom: var(--spacing-lg);
}
.setting-info {
  flex: 1;
}
.setting-label {
  font-size: var(--font-size-sm);
  font-weight: 500;
  color: var(--color-text-primary);
  margin-bottom: 4px;
}
.setting-desc {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  line-height: 1.4;
}
.setting-input {
  width: 80px;
  padding: 6px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg-primary);
  color: var(--color-text-primary);
  text-align: right;
  font-family: var(--font-mono);
}
.setting-input:focus {
  outline: none;
  border-color: var(--color-accent);
}
.modal-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--spacing-md);
}
.btn {
  padding: 6px 16px;
  border-radius: var(--radius-sm);
  font-size: var(--font-size-sm);
  cursor: pointer;
  border: none;
  font-weight: 500;
}
.btn-secondary {
  background: transparent;
  color: var(--color-text-secondary);
}
.btn-secondary:hover {
  background: var(--color-sidebar-hover-bg);
}
.btn-primary {
  background: var(--color-accent);
  color: #fff;
}
.btn-primary:hover {
  filter: brightness(1.1);
}

.thumb-gen-status {
  margin-top: var(--spacing-sm);
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.progress-bar {
  width: 100%;
  height: 4px;
  border-radius: 2px;
  background: var(--color-border);
  overflow: hidden;
}
.progress-bar__fill {
  height: 100%;
  background: var(--color-accent);
  transition: width 100ms linear;
}
.progress-shimmer {
  background: linear-gradient(
    90deg,
    var(--color-accent) 0%,
    var(--color-accent-hover) 50%,
    var(--color-accent) 100%
  );
  background-size: 200% 100%;
  animation: shimmer 1.5s ease-in-out infinite;
}
.thumb-gen-text {
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
@keyframes shimmer {
  0% { background-position: -200% 0; }
  100% { background-position: 200% 0; }
}
.setting-actions {
  display: flex;
  align-items: center;
}
</style>
