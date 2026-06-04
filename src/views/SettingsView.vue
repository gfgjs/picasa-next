<template>
  <div class="settings-view">
    <header class="settings-header">
      <h1 class="settings-title">{{ $t('settings.title') }}</h1>
      <button class="btn-close" title="关闭设置" @click="closeSettings"><X :size="18" /></button>
    </header>

    <main class="settings-content">
      <!-- ── 外观 ─────────────────────────────────────────── -->
      <div class="settings-card">
        <div class="settings-card__header">{{ $t('settings.general') }}</div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.language') }}</div>
            <div class="settings-card__desc">{{ $t('settings.languageDesc') }}</div>
          </div>
          <div class="select-wrap">
            <select
              v-model="ui.language"
              @change="ui.setLanguage(ui.language)"
              class="select"
            >
              <option value="zh-CN">简体中文</option>
              <option value="en-US">English</option>
            </select>
          </div>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.uiFontSize') }}</div>
            <div class="settings-card__desc">{{ $t('settings.uiFontSizeDesc') }}</div>
          </div>
          <input
            type="number"
            v-model.number="uiFontSize"
            min="12"
            max="24"
            class="input-number"
            @change="saveFontSize"
          />
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.hoverScale') }}</div>
            <div class="settings-card__desc">{{ $t('settings.hoverScaleDesc') }}</div>
          </div>
          <label class="toggle">
            <input
              type="checkbox"
              v-model="enableHoverScale"
              @change="saveHoverScale"
            />
            <span class="toggle__thumb" />
          </label>
        </div>
      </div>

      <!-- ── 缩略图 ───────────────────────────────────────── -->
      <div class="settings-card">
        <div class="settings-card__header">{{ $t('settings.thumbnails') || '缩略图' }}</div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.thumbDecodeStrategy') }}</div>
            <div class="settings-card__desc">{{ $t('settings.thumbDecodeDesc') }}</div>
          </div>
          <div class="select-wrap">
            <select v-model="thumbStrategy" @change="saveThumbStrategy" class="select">
              <option value="cpu">{{ $t('settings.thumbStrategyCpu') }}</option>
              <option value="gpu">{{ $t('settings.thumbStrategyGpu') }}</option>
              <option value="direct">{{ $t('settings.thumbStrategyDirect') }}</option>
            </select>
          </div>
        </div>

        <div class="settings-card__item" v-if="thumbStrategy === 'gpu'">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.gpuEngine') }}</div>
            <div class="settings-card__desc">{{ $t('settings.gpuEngineDesc') }}</div>
          </div>
          <div class="select-wrap">
            <select v-model="gpuEngine" @change="saveGpuEngine" class="select">
              <option value="wic">{{ $t('settings.gpuEngineWic') }}</option>
            </select>
          </div>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.thumbCacheDir') || '缓存目录' }}</div>
            <div 
              class="settings-card__desc clickable-path" 
              @click="openDirectory(thumbCacheDir)"
              title="点击在资源管理器中打开"
            >
              {{ thumbCacheDir || '正在获取路径...' }}
            </div>
          </div>
          <button class="btn btn-secondary" @click="changeCacheDir">
            {{ $t('settings.changeDir') || '更改目录' }}
          </button>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.thumbSize') || '缩略图大小' }}</div>
            <div class="settings-card__desc">{{ $t('settings.thumbSizeDesc') || '生成的缩略图的最大边长 (像素)' }}</div>
          </div>
          <div class="setting-slider-group">
            <input
              type="range"
              v-model.number="thumbSize"
              min="4"
              max="1024"
              step="1"
              class="input-range"
              @change="saveThumbSize"
            />
            <input
              type="number"
              v-model.number="thumbSize"
              min="4"
              max="1024"
              class="input-number"
              @change="saveThumbSize"
            />
          </div>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.thumbSkipMaxKb') }}</div>
            <div class="settings-card__desc">{{ $t('settings.thumbSkipDesc') }}</div>
          </div>
          <input
            type="number"
            v-model.number="thumbSkipMaxKb"
            min="0"
            max="1000000"
            class="input-number"
            @change="saveConfig('thumb_skip_max_kb', thumbSkipMaxKb.toString()); media.invalidateLayout()"
          />
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.timelineScrollWidth') }}</div>
            <div class="settings-card__desc">{{ $t('settings.timelineScrollDesc') }}</div>
          </div>
          <input
            type="number"
            v-model.number="timelineScrollWidth"
            min="2"
            max="40"
            class="input-number"
            @change="saveScrollbarWidth"
          />
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.fullThumbGen') }}</div>
            <div class="settings-card__desc">{{ $t('settings.fullThumbGenDesc') }}</div>
            <div v-if="scan.thumbGenProgress.status !== 'idle'" class="thumb-gen-status">
              <div class="progress-bar">
                <div
                  class="progress-bar__fill"
                  :class="{ 'progress-shimmer': scan.thumbGenProgress.isRunning }"
                  :style="{ width: thumbGenPercent + '%' }"
                />
              </div>
              <div class="thumb-gen-text">
                <span v-if="scan.thumbGenProgress.isRunning">{{ $t('settings.genStatusRunning', { generated: scan.thumbGenProgress.generated, total: scan.thumbGenProgress.total }) }}</span>
                <span v-else-if="scan.thumbGenProgress.status === 'completed'">{{ $t('settings.genStatusCompleted') }}</span>
                <span v-else-if="scan.thumbGenProgress.status === 'cancelled'">{{ $t('settings.genStatusCancelled') }}</span>
                <span v-else-if="scan.thumbGenProgress.status === 'error'">{{ $t('settings.genStatusError') }}</span>
              </div>
            </div>
          </div>
          <div class="setting-actions">
            <button
              v-if="scan.thumbGenProgress.isRunning"
              class="btn btn-secondary"
              @click="scan.stopFullThumbnailGeneration()"
            >
              {{ $t('settings.stopGen') }}
            </button>
            <button
              v-else
              class="btn btn-primary"
              @click="scan.startFullThumbnailGeneration()"
            >
              {{ $t('settings.startGen') }}
            </button>
          </div>
        </div>
      </div>

      <!-- ── AI 模型配置 ──────────────────────────────────── -->
      <div class="settings-card">
        <div class="settings-card__header">{{ $t('settings.aiModels') }}</div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.aiEngineStatus') }}</div>
            <div class="settings-card__desc">
              {{ ai.providerLabel }} {{ ai.status.gpuName ? `(${ai.status.gpuName})` : '' }}
              <span v-if="!ai.status.clipLoaded" style="color: var(--color-warning);"> (未加载)</span>
              <span v-else style="color: var(--color-success);"> (已加载)</span>
            </div>
          </div>
          <button class="btn btn-secondary" @click="ai.initEngine" :disabled="ai.status.clipLoaded">
            {{ $t('settings.aiTestLoad') }}
          </button>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.aiHardwareStrategy') }}</div>
            <div class="settings-card__desc">
              {{ $t('settings.aiHardwareDesc') }}
            </div>
          </div>
          <div class="select-wrap">
            <select v-model="aiProviderOverride" @change="saveAiProvider" class="select">
              <option value="auto">{{ $t('settings.aiAutoHardware') }}</option>
              <option value="cpu">{{ $t('settings.aiForceCpu') }}</option>
            </select>
          </div>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.aiImportModel') }}</div>
            <div class="settings-card__desc">{{ $t('settings.aiImportDesc') }}</div>
          </div>
          <div style="display: flex; align-items: center; gap: var(--spacing-sm);">
            <span style="font-size: var(--font-size-xs); color: var(--color-text-secondary);">{{ $t('settings.aiImportTip') }}</span>
            <button class="btn btn-secondary" @click="importModel">
              {{ $t('settings.aiImportBtn') }}
            </button>
          </div>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.aiVisionModel') }}</div>
            <div class="settings-card__desc">{{ $t('settings.aiVisionDesc') }}</div>
          </div>
          <div class="select-wrap">
            <select v-model="aiImageModel" @change="saveAiModels" class="select">
              <option v-for="m in availableAiModels" :key="m" :value="m">{{ m }}</option>
            </select>
          </div>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.aiTextModel') }}</div>
            <div class="settings-card__desc">{{ $t('settings.aiTextDesc') }}</div>
          </div>
          <div class="select-wrap">
            <select v-model="aiTextModel" @change="saveAiModels" class="select">
              <option v-for="m in availableAiModels" :key="m" :value="m">{{ m }}</option>
            </select>
          </div>
        </div>
      </div>

      <!-- ── 开发者工具 ─────────────────────────────────── -->
      <div class="settings-card">
        <div class="settings-card__header">{{ $t('sidebar.debugSettings') || '开发者工具' }}</div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.clearDb') }}</div>
            <div class="settings-card__desc">{{ $t('settings.clearDbDesc') }}</div>
          </div>
          <button class="btn btn-danger" @click="clearDb">
            <Database :size="14" /> {{ $t('settings.clearDbBtn') }}
          </button>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.clearSettings') }}</div>
            <div class="settings-card__desc">{{ $t('settings.clearSettingsDesc') }}</div>
          </div>
          <button class="btn btn-danger" @click="clearSettings">
            <Paintbrush :size="14" /> {{ $t('settings.clearSettingsBtn') }}
          </button>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.logLevel') }}</div>
            <div class="settings-card__desc">{{ $t('settings.logLevelDesc') }}</div>
          </div>
          <div class="select-wrap">
            <select
              v-model="logLevel"
              @change="saveLogLevel"
              class="select"
            >
              <option value="trace">{{ $t('settings.logLevelTrace') }}</option>
              <option value="debug">{{ $t('settings.logLevelDebug') }}</option>
              <option value="info">{{ $t('settings.logLevelInfo') }}</option>
              <option value="warn">{{ $t('settings.logLevelWarn') }}</option>
              <option value="error">{{ $t('settings.logLevelError') }}</option>
            </select>
          </div>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.logDir') }}</div>
            <div 
              class="settings-card__desc clickable-path" 
              @click="openDirectory(logDir)"
              title="点击在资源管理器中打开"
            >
              {{ logDir || '正在获取路径...' }}
            </div>
          </div>
          <button class="btn btn-secondary" @click="changeLogDir">
            {{ $t('settings.changeDir') }}
          </button>
        </div>

        <div class="settings-card__item">
          <div class="settings-card__info">
            <div class="settings-card__label">{{ $t('settings.clearBrowserCache') }}</div>
            <div class="settings-card__desc">{{ $t('settings.clearBrowserCacheDesc') }}</div>
          </div>
          <button class="btn btn-danger" @click="clearBrowserCache">
            <RotateCcw :size="14" /> {{ $t('settings.clearBrowserCacheBtn') }}
          </button>
        </div>
      </div>
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { useUiStore } from '../stores/uiStore'
import { useScanStore } from '../stores/scanStore'
import { useMediaStore } from '../stores/mediaStore'
import { useAiStore } from '../stores/aiStore'
import { useI18n } from 'vue-i18n'
import { X, Database, Paintbrush, RotateCcw } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { IPC } from '../constants/ipc'

const ui = useUiStore()
const scan = useScanStore()
const media = useMediaStore()
const ai = useAiStore()
const router = useRouter()
const { t } = useI18n()

const thumbSkipMaxKb = ref(200)
const thumbSize = ref(300)
const thumbCacheDir = ref('')
const logDir = ref('')
const timelineScrollWidth = ref(6)
const uiFontSize = ref(16)
const enableHoverScale = ref(true)
const logLevel = ref('info')

const thumbStrategy = ref('cpu')
const gpuEngine = ref('wic')

const availableAiModels = ref<string[]>([])
const aiImageModel = ref('cn-clip-vit-b16-image.onnx')
const aiTextModel = ref('cn-clip-vit-b16-text.onnx')
const aiProviderOverride = ref('auto')

const thumbGenPercent = computed(() => {
  const { generated, total } = scan.thumbGenProgress
  if (!total) return 0
  return Math.min(100, Math.round((generated / total) * 100))
})

onMounted(async () => {
  try {
    const val1 = await invoke<string | null>('get_app_config', { key: 'thumb_skip_max_kb' })
    if (val1) thumbSkipMaxKb.value = parseInt(val1, 10)

    const strat = await invoke<string | null>('get_app_config', { key: 'thumb_strategy' })
    if (strat) {
      thumbStrategy.value = strat
      ui.thumbStrategy = strat
    }

    const gpu = await invoke<string | null>('get_app_config', { key: 'gpu_engine' })
    if (gpu) {
      gpuEngine.value = gpu
      ui.gpuEngine = gpu
    }

    const val2 = await invoke<string | null>('get_app_config', { key: 'timeline_scroll_width' })
    if (val2) timelineScrollWidth.value = parseInt(val2, 10)

    const val3 = await invoke<string | null>('get_app_config', { key: 'ui_font_size' })
    if (val3) uiFontSize.value = parseInt(val3, 10)

    const val4 = await invoke<string | null>('get_app_config', { key: 'enable_thumb_hover_scale' })
    if (val4) enableHoverScale.value = val4 === 'true'

    const val5 = await invoke<string | null>('get_app_config', { key: 'thumb_size' })
    if (val5) thumbSize.value = parseInt(val5, 10)

    try {
      thumbCacheDir.value = await invoke<string>('get_thumb_cache_dir')
    } catch (e) {
      console.warn('Failed to fetch resolved cache dir', e)
    }

    try {
      logDir.value = await invoke<string>('get_log_dir')
    } catch (e) {
      console.warn('Failed to fetch resolved log dir', e)
    }

    const val6 = await invoke<string | null>('get_app_config', { key: 'log_level' })
    if (val6) logLevel.value = val6

    const valImage = await invoke<string | null>('get_app_config', { key: 'ai_image_model' })
    if (valImage) aiImageModel.value = valImage

    const valText = await invoke<string | null>('get_app_config', { key: 'ai_text_model' })
    if (valText) aiTextModel.value = valText

    const valProvider = await invoke<string | null>('get_app_config', { key: 'ai_provider_override' })
    if (valProvider) aiProviderOverride.value = valProvider

    availableAiModels.value = await ai.listAiModels()
    await ai.fetchStatus()
  } catch (e) {
    console.error('Failed to get config:', e)
  }

  document.addEventListener('keydown', onKeyDown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', onKeyDown)
})

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') router.push('/')
}

async function saveConfig(key: string, value: string) {
  try {
    await invoke('set_app_config', { key, value })
    ui.addToast('success', t('settings.saveSuccess') || '保存成功')
  } catch (e) {
    ui.addToast('error', t('settings.saveFailed', { error: String(e) }) || `保存失败: ${e}`)
  }
}

async function saveThumbStrategy() {
  await saveConfig('thumb_strategy', thumbStrategy.value)
  ui.setThumbStrategy(thumbStrategy.value)
  media.invalidateLayout()
  ui.addToast('success', '解码策略已修改，直接显示的项将在回到画廊时重新生成缩略图')
}

async function saveGpuEngine() {
  await saveConfig('gpu_engine', gpuEngine.value)
  ui.setGpuEngine(gpuEngine.value)
}

async function saveThumbSize() {
  await saveConfig('thumb_size', thumbSize.value.toString())
  ui.addToast('success', '已修改生成尺寸，新尺寸将在生成新缩略图时生效')
}

async function changeCacheDir() {
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: '选择缩略图缓存目录',
    })
    if (selected && typeof selected === 'string') {
      await saveConfig('thumb_cache_dir', selected)
      thumbCacheDir.value = selected
      ui.addToast('success', '缓存目录已更改，旧缓存不会自动移动，请根据需要手动清理。')
    }
  } catch (e) {
    console.error('Failed to select directory:', e)
  }
}

async function changeLogDir() {
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: '选择日志存储目录',
    })
    if (selected && typeof selected === 'string') {
      await saveConfig('log_dir', selected)
      logDir.value = selected
      ui.addToast('success', '日志存储目录已更改，重启应用后生效，旧日志需手动清理。')
    }
  } catch (e) {
    console.error('Failed to select log directory:', e)
  }
}

async function openDirectory(path: string) {
  if (!path) return
  try {
    await invoke('open_directory', { path })
  } catch (e) {
    ui.addToast('error', `无法打开目录: ${e}`)
  }
}

async function saveLogLevel() {
  await saveConfig('log_level', logLevel.value)
  ui.addToast('success', '日志级别已修改，重启应用后生效')
}

async function saveScrollbarWidth() {
  await saveConfig('timeline_scroll_width', timelineScrollWidth.value.toString())
  document.documentElement.style.setProperty('--scrollbar-width', `${timelineScrollWidth.value}px`)
}

async function saveFontSize() {
  await saveConfig('ui_font_size', uiFontSize.value.toString())
  const diff = uiFontSize.value - 16;
  document.documentElement.style.setProperty('--font-size-xs', `${12 + diff}px`);
  document.documentElement.style.setProperty('--font-size-sm', `${13 + diff}px`);
  document.documentElement.style.setProperty('--font-size-base', `${16 + diff}px`);
  document.documentElement.style.setProperty('--font-size-md', `${17 + diff}px`);
  document.documentElement.style.setProperty('--font-size-lg', `${20 + diff}px`);
  document.documentElement.style.setProperty('--font-size-xl', `${24 + diff}px`);
  document.documentElement.style.setProperty('--font-size-2xl', `${30 + diff}px`);
}

async function saveHoverScale() {
  await saveConfig('enable_thumb_hover_scale', enableHoverScale.value.toString())
  if (enableHoverScale.value) {
    document.documentElement.classList.remove('disable-hover-scale')
  } else {
    document.documentElement.classList.add('disable-hover-scale')
  }
}

async function importModel() {
  try {
    const selected = await openDialog({
      multiple: false,
      title: '导入 AI 模型 (.onnx)',
      filters: [{ name: 'ONNX Model', extensions: ['onnx'] }]
    })
    if (selected && typeof selected === 'string') {
      await ai.importAiModel(selected)
      availableAiModels.value = await ai.listAiModels()
      ui.addToast('success', '模型导入成功，请在下拉列表中选择。')
    }
  } catch (e) {
    ui.addToast('error', `导入失败: ${e}`)
  }
}

async function saveAiProvider() {
  await saveConfig('ai_provider_override', aiProviderOverride.value)
  ui.addToast('success', '硬件加速策略已保存！需点击“测试加载”或重载引擎后生效。')
}

async function saveAiModels() {
  await saveConfig('ai_image_model', aiImageModel.value)
  await saveConfig('ai_text_model', aiTextModel.value)
  try {
    await ai.reloadAiEngine()
    ui.addToast('success', 'AI 引擎重载成功，已应用新模型！')
  } catch (e) {
    ui.addToast('error', `AI 引擎重载失败: ${e}`)
  }
}

// ── Debug functions (moved from sidebar) ────────────────────────────────────
// ── 调试功能（从侧边栏迁移） ──────────────────────────────────────────────

async function clearDb() {
  if (!confirm(t('sidebar.clearDbConfirm') || '确定要清除所有数据？此操作不可撤销。')) return
  try {
    await scan.clearDatabase()
    media.loadStats()
    ui.addToast('success', t('sidebar.clearDbSuccess') || '数据已清除')
  } catch (e) {
    ui.addToast('error', `清除数据失败: ${e}`)
  }
}

async function clearSettings() {
  if (!confirm(t('sidebar.clearSettingsConfirm') || '确定要重置所有设置？')) return
  try {
    await invoke(IPC.CLEAR_SETTINGS)
    window.location.reload()
  } catch (e) {
    ui.addToast('error', `清除设置失败: ${e}`)
  }
}

function clearBrowserCache() {
  window.location.href = window.location.pathname + '?clear=' + Date.now()
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
  position: sticky;
  top: 0;
  z-index: 10;
  background: color-mix(in srgb, var(--color-bg-primary) 85%, transparent);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--spacing-lg) var(--spacing-xl);
  border-bottom: 1px solid var(--color-border);
  flex-shrink: 0;
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
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    background var(--transition-fast),
    color var(--transition-fast),
    border-color var(--transition-fast);
}
.btn-close:hover {
  background: var(--color-error);
  color: white;
  border-color: var(--color-error);
}

.settings-content {
  padding: var(--spacing-lg) var(--spacing-xl);
  max-width: 640px;
  margin: 0 auto;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: var(--spacing-lg);
}

/* ── Card overrides (extend global .settings-card) ─────────────────────── */
/* The base .settings-card styles are in index.css. */
/* Here we only add component-specific refinements. */

.clickable-path {
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 2px;
  text-decoration-color: transparent;
  transition: text-decoration-color var(--transition-fast), color var(--transition-fast);
}
.clickable-path:hover {
  color: var(--color-accent);
  text-decoration-color: var(--color-accent);
}

/* ── Thumbnail generation ──────────────────────────────────────────────── */
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

/* ── Buttons ───────────────────────────────────────────────────────────── */
.setting-actions {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}
.btn {
  padding: 7px 16px;
  border-radius: var(--radius-md);
  font-size: var(--font-size-sm);
  font-weight: 500;
  cursor: pointer;
  border: none;
  display: inline-flex;
  align-items: center;
  gap: var(--spacing-xs);
  transition:
    background var(--transition-fast),
    filter var(--transition-fast);
  white-space: nowrap;
}
.btn-secondary {
  background: transparent;
  color: var(--color-text-secondary);
  border: 1px solid var(--color-border);
}
.btn-secondary:hover {
  background: var(--color-bg-hover);
}
.btn-primary {
  background: var(--color-accent);
  color: #fff;
}
.btn-primary:hover {
  filter: brightness(1.1);
}
.btn-danger {
  background: transparent;
  color: var(--color-error);
  border: 1px solid var(--color-border);
}
.btn-danger:hover {
  background: var(--color-error);
  color: #fff;
  border-color: var(--color-error);
}
</style>
