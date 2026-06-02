<template>
  <div class="settings-view">
    <header class="settings-header">
      <h1 class="settings-title">{{ $t('settings.title') }}</h1>
      <button class="btn-close" title="关闭设置" @click="closeSettings">✕</button>
    </header>

    <main class="settings-content">
      <section class="settings-section">
        <h2 class="section-title">{{ $t('settings.general') }}</h2>
        
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
            @change="saveConfig('thumb_skip_max_kb', thumbSkipMaxKb.toString())"
          />
        </div>

        <div class="setting-item">
          <div class="setting-info">
            <div class="setting-label">{{ $t('settings.timelineScrollWidth') }}</div>
            <div class="setting-desc">{{ $t('settings.timelineScrollDesc') }}</div>
          </div>
          <input 
            type="number" 
            v-model.number="timelineScrollWidth" 
            min="2" 
            max="40"
            class="setting-input"
            @change="saveScrollbarWidth"
          />
        </div>
        <div class="setting-item">
          <div class="setting-info">
            <div class="setting-label">{{ $t('settings.uiFontSize') }}</div>
            <div class="setting-desc">{{ $t('settings.uiFontSizeDesc') }}</div>
          </div>
          <input 
            type="number" 
            v-model.number="uiFontSize" 
            min="12" 
            max="24"
            class="setting-input"
            @change="saveFontSize"
          />
        </div>

        <div class="setting-item">
          <div class="setting-info">
            <div class="setting-label">{{ $t('settings.hoverScale') }}</div>
            <div class="setting-desc">{{ $t('settings.hoverScaleDesc') }}</div>
          </div>
          <input 
            type="checkbox" 
            v-model="enableHoverScale" 
            @change="saveHoverScale"
            class="setting-checkbox"
          />
        </div>
      </section>
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { useUiStore } from '../stores/uiStore'
import { useI18n } from 'vue-i18n'

const ui = useUiStore()
const router = useRouter()
const { t } = useI18n()

const thumbSkipMaxKb = ref(200)
const timelineScrollWidth = ref(6)
const uiFontSize = ref(15)
const enableHoverScale = ref(true)

onMounted(async () => {
  try {
    const val1 = await invoke<string | null>('get_app_config', { key: 'thumb_skip_max_kb' })
    if (val1) thumbSkipMaxKb.value = parseInt(val1, 10)

    const val2 = await invoke<string | null>('get_app_config', { key: 'timeline_scroll_width' })
    if (val2) timelineScrollWidth.value = parseInt(val2, 10)

    const val3 = await invoke<string | null>('get_app_config', { key: 'ui_font_size' })
    if (val3) uiFontSize.value = parseInt(val3, 10)

    const val4 = await invoke<string | null>('get_app_config', { key: 'enable_thumb_hover_scale' })
    if (val4) enableHoverScale.value = val4 === 'true'
  } catch (e) {
    console.error('Failed to get config:', e)
  }
})

async function saveConfig(key: string, value: string) {
  try {
    await invoke('set_app_config', { key, value })
    ui.addToast('success', t('settings.saveSuccess'))
  } catch (e) {
    ui.addToast('error', t('settings.saveFailed', { error: String(e) }))
  }
}

async function saveScrollbarWidth() {
  await saveConfig('timeline_scroll_width', timelineScrollWidth.value.toString())
  // Apply globally immediately
  // 立即全局应用
  document.documentElement.style.setProperty('--scrollbar-width', `${timelineScrollWidth.value}px`)
}

async function saveFontSize() {
  await saveConfig('ui_font_size', uiFontSize.value.toString())
  const diff = uiFontSize.value - 15;
  document.documentElement.style.setProperty('--font-size-xs', `${12 + diff}px`);
  document.documentElement.style.setProperty('--font-size-sm', `${13 + diff}px`);
  document.documentElement.style.setProperty('--font-size-base', `${15 + diff}px`);
  document.documentElement.style.setProperty('--font-size-md', `${16 + diff}px`);
  document.documentElement.style.setProperty('--font-size-lg', `${19 + diff}px`);
  document.documentElement.style.setProperty('--font-size-xl', `${23 + diff}px`);
  document.documentElement.style.setProperty('--font-size-2xl', `${28 + diff}px`);
}

async function saveHoverScale() {
  await saveConfig('enable_thumb_hover_scale', enableHoverScale.value.toString())
  if (enableHoverScale.value) {
    document.documentElement.classList.remove('disable-hover-scale')
  } else {
    document.documentElement.classList.add('disable-hover-scale')
  }
}

function closeSettings() {
  router.push('/')
}
</script>

<style scoped>
.settings-view {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--color-bg-primary);
  overflow-y: auto;
}

.settings-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--spacing-xl) var(--spacing-2xl);
  border-bottom: 1px solid var(--color-border);
}

.settings-title {
  font-size: var(--font-size-xl);
  font-weight: 600;
  color: var(--color-text-primary);
  margin: 0;
}

.btn-close {
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  color: var(--color-text-secondary);
  font-size: 16px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all var(--transition-fast);
}

.btn-close:hover {
  background: var(--color-error);
  color: white;
  border-color: var(--color-error);
}

.settings-content {
  padding: var(--spacing-xl) var(--spacing-2xl);
  max-width: 800px;
}

.settings-section {
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  padding: var(--spacing-xl);
  margin-bottom: var(--spacing-xl);
}

.section-title {
  font-size: var(--font-size-lg);
  font-weight: 500;
  color: var(--color-text-primary);
  margin: 0 0 var(--spacing-lg) 0;
}

.setting-item {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--spacing-md);
  padding: var(--spacing-md) 0;
  border-bottom: 1px solid var(--color-border-subtle);
}

.setting-item:last-child {
  border-bottom: none;
  padding-bottom: 0;
}

.setting-info {
  flex: 1;
}

.setting-label {
  font-size: var(--font-size-md);
  font-weight: 500;
  color: var(--color-text-primary);
  margin-bottom: 4px;
}

.setting-desc {
  font-size: var(--font-size-sm);
  color: var(--color-text-tertiary);
  line-height: 1.5;
}

.setting-input {
  width: 100px;
  padding: 8px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg-primary);
  color: var(--color-text-primary);
  text-align: right;
  font-family: var(--font-mono);
  font-size: var(--font-size-base);
}

.setting-input:focus {
  outline: none;
  border-color: var(--color-accent);
}

.setting-checkbox {
  width: 20px;
  height: 20px;
  cursor: pointer;
  accent-color: var(--color-accent);
}
</style>
