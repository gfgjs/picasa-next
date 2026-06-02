<template>
  <div class="settings-view">
    <header class="settings-header">
      <h1 class="settings-title">设置</h1>
      <button class="btn-close" title="关闭设置" @click="closeSettings">✕</button>
    </header>

    <main class="settings-content">
      <section class="settings-section">
        <h2 class="section-title">通用</h2>
        
        <div class="setting-item">
          <div class="setting-info">
            <div class="setting-label">小文件直显阈值 (KB)</div>
            <div class="setting-desc">小于此大小的图片将不生成缩略图，直接加载原图以节省磁盘空间并提高加载速度。设为 0 则对所有图片生成缩略图。</div>
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
            <div class="setting-label">右侧滚动块宽度 (px)</div>
            <div class="setting-desc">调整右侧时间轴滚动条的宽度大小，方便拖拽。</div>
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
            <div class="setting-label">界面文字大小 (px)</div>
            <div class="setting-desc">调整整个应用的基础字体大小（建议 12 - 20），默认 15px。</div>
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
            <div class="setting-label">鼠标划过缩略图时放大</div>
            <div class="setting-desc">开启后，鼠标悬停在瀑布流缩略图上时会略微放大。如果觉得卡顿可以关闭。</div>
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

const ui = useUiStore()
const router = useRouter()

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
    ui.addToast('success', '设置已保存')
  } catch (e) {
    ui.addToast('error', '保存失败: ' + e)
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
