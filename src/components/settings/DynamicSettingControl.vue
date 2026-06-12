<template>
  <div :class="['dynamic-control', { compact }]">
    <!-- Theme -->
    <template v-if="settingKey === 'theme'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="ui.theme" @change="ui.setTheme(ui.theme)" class="select">
          <option value="system">{{ $t('settings.themeSystem') }}</option>
          <option value="light">{{ $t('settings.themeLight') }}</option>
          <option value="dark">{{ $t('settings.themeDark') }}</option>
        </select>
      </div>
    </template>

    <!-- Language -->
    <template v-else-if="settingKey === 'language'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="ui.language" @change="ui.setLanguage(ui.language)" class="select">
          <option value="zh-CN">简体中文</option>
          <option value="en-US">English</option>
        </select>
      </div>
    </template>

    <!-- Font Size -->
    <template v-else-if="settingKey === 'uiFontSize'">
      <input type="number" v-model.number="uiFontSizeLocal" @change="config.setUiFontSize(uiFontSizeLocal)" min="12" max="24" class="input-number" :class="{ 'compact-input': compact }" />
    </template>

    <!-- Hover Scale -->
    <template v-else-if="settingKey === 'hoverScale'">
      <label class="toggle" :class="{ 'compact-toggle': compact }">
        <input type="checkbox" v-model="enableHoverScaleLocal" @change="config.setEnableHoverScale(enableHoverScaleLocal)" />
        <span class="toggle__thumb" />
      </label>
    </template>

    <!-- Close Behavior -->
    <template v-else-if="settingKey === 'closeBehavior'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="ui.closeBehavior" @change="ui.setCloseBehavior(ui.closeBehavior)" class="select">
          <option value="ask">{{ $t('settings.closeBehaviorAsk') || '每次询问' }}</option>
          <option value="minimize_to_tray">{{ $t('settings.closeBehaviorMinimize') || '最小化' }}</option>
          <option value="exit">{{ $t('settings.closeBehaviorExit') || '退出' }}</option>
        </select>
      </div>
    </template>

    <!-- Show Thumb Info -->
    <template v-else-if="settingKey === 'showThumbInfo'">
      <label class="toggle" :class="{ 'compact-toggle': compact }">
        <input type="checkbox" v-model="ui.showThumbInfo" @change="ui.setShowThumbInfo(ui.showThumbInfo)" />
        <span class="toggle__thumb" />
      </label>
    </template>

    <!-- Thumb Decode Strategy -->
    <template v-else-if="settingKey === 'thumbDecodeStrategy'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="thumbStrategyLocal" @change="config.setThumbStrategy(thumbStrategyLocal)" class="select">
          <option value="cpu">{{ $t('settings.thumbStrategyCpu') }}</option>
          <option value="gpu">{{ $t('settings.thumbStrategyGpu') }}</option>
          <option value="direct">{{ $t('settings.thumbStrategyDirect') }}</option>
        </select>
      </div>
    </template>

    <!-- GPU Engine -->
    <template v-else-if="settingKey === 'gpuEngine'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="gpuEngineLocal" @change="config.setGpuEngine(gpuEngineLocal)" class="select">
          <option value="wic">{{ $t('settings.gpuEngineWic') }}</option>
        </select>
      </div>
    </template>

    <!-- Thumb Size Segmented Control -->
    <template v-else-if="settingKey === 'thumbSize'">
      <div class="segmented-control" :class="{ 'compact-segmented': compact }">
        <button
          v-for="tier in THUMB_SIZE_TIERS"
          :key="tier"
          class="segmented-btn"
          :class="{ active: config.thumbSize === tier }"
          @click="config.setThumbSize(tier)"
        >
          {{ getTierLabel(tier) }}
        </button>
      </div>
    </template>

    <!-- Thumb Skip Max KB -->
    <template v-else-if="settingKey === 'thumbSkipMaxKb'">
      <input type="number" v-model.number="thumbSkipMaxKbLocal" @change="config.setThumbSkipMaxKb(thumbSkipMaxKbLocal); media.invalidateLayout()" min="0" max="1000000" class="input-number" :class="{ 'compact-input': compact }" />
    </template>

    <!-- Thumb Cache Max MB -->
    <template v-else-if="settingKey === 'thumbCacheMaxMb'">
      <input type="number" v-model.number="thumbCacheMaxMbLocal" @change="config.setThumbCacheMaxMb(thumbCacheMaxMbLocal)" min="100" max="100000" class="input-number" :class="{ 'compact-input': compact }" />
    </template>

    <!-- Timeline Scroll Width -->
    <template v-else-if="settingKey === 'timelineScrollWidth'">
      <input type="number" v-model.number="timelineScrollWidthLocal" @change="config.setTimelineScrollWidth(timelineScrollWidthLocal)" min="2" max="40" class="input-number" :class="{ 'compact-input': compact }" />
    </template>

    <!-- AI Hardware Strategy -->
    <template v-else-if="settingKey === 'aiHardwareStrategy'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="aiProviderOverrideLocal" @change="config.setAiProviderOverride(aiProviderOverrideLocal)" class="select">
          <option value="auto">{{ $t('settings.aiAutoHardware') }}</option>
          <option value="cpu">{{ $t('settings.aiForceCpu') }}</option>
        </select>
      </div>
    </template>

    <!-- AI Batch Size -->
    <template v-else-if="settingKey === 'aiBatchSize'">
      <div style="display: flex; flex-direction: column; align-items: flex-end; gap: 4px;">
        <input type="number" v-model.number="aiBatchSizeLocal" @change="config.setAiBatchSize(aiBatchSizeLocal)" min="0" max="512" class="input-number" :class="{ 'compact-input': compact }" placeholder="0 = 自动" />
        <span v-if="aiBatchSizeLocal === 0" style="font-size: 11px; color: var(--color-success); white-space: nowrap;">自动分配</span>
        <span v-else-if="aiBatchSizeLocal > 200" style="font-size: 11px; color: var(--color-error); white-space: nowrap;">高危: 可能引发性能断崖或崩溃</span>
        <span v-else-if="aiBatchSizeLocal > 128" style="font-size: 11px; color: var(--color-warning); white-space: nowrap;">⚠️ 较高: 需确保显存充裕</span>
      </div>
    </template>

    <!-- AI Image Model -->
    <template v-else-if="settingKey === 'aiVisionModel'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="aiImageModelLocal" @change="config.setAiImageModel(aiImageModelLocal)" class="select">
          <option v-for="m in availableAiModels" :key="m" :value="m">{{ m }}</option>
        </select>
      </div>
    </template>

    <!-- AI Text Model -->
    <template v-else-if="settingKey === 'aiTextModel'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="aiTextModelLocal" @change="config.setAiTextModel(aiTextModelLocal)" class="select">
          <option v-for="m in availableAiModels" :key="m" :value="m">{{ m }}</option>
        </select>
      </div>
    </template>

    <!-- Log Level -->
    <template v-else-if="settingKey === 'logLevel'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="logLevelLocal" @change="config.setLogLevel(logLevelLocal)" class="select">
          <option value="trace">{{ $t('settings.logLevelTrace') }}</option>
          <option value="debug">{{ $t('settings.logLevelDebug') }}</option>
          <option value="info">{{ $t('settings.logLevelInfo') }}</option>
          <option value="warn">{{ $t('settings.logLevelWarn') }}</option>
          <option value="error">{{ $t('settings.logLevelError') }}</option>
        </select>
      </div>
    </template>

    <!-- Clear DB Button -->
    <template v-else-if="settingKey === 'clearDb'">
      <button v-if="compact" class="btn-icon danger-icon" @click="handleClearDb" :title="$t('settings.clearDb')"><RotateCcw :size="14" /></button>
      <button v-else class="btn btn-danger" @click="handleClearDb"><Database :size="14" /> {{ $t('settings.clearDbBtn') }}</button>
    </template>

    <!-- Clear Settings Button -->
    <template v-else-if="settingKey === 'clearSettings'">
      <button v-if="compact" class="btn-icon danger-icon" @click="handleClearSettings" :title="$t('settings.clearSettings')"><RotateCcw :size="14" /></button>
      <button v-else class="btn btn-danger" @click="handleClearSettings"><Paintbrush :size="14" /> {{ $t('settings.clearSettingsBtn') }}</button>
    </template>

    <!-- Clear All Thumbnails Button -->
    <template v-else-if="settingKey === 'clearAllThumbnails'">
      <button v-if="compact" class="btn-icon danger-icon" @click="handleClearAllThumbnails" :title="$t('settings.clearAllThumbnails') || '清除所有缩略图'"><RotateCcw :size="14" /></button>
      <button v-else class="btn btn-danger" @click="handleClearAllThumbnails"><RotateCcw :size="14" /> {{ $t('settings.clearAllThumbnailsBtn') || '清除缩略图' }}</button>
    </template>

    <!-- Clear Logs Button -->
    <template v-else-if="settingKey === 'clearLogs'">
      <button v-if="compact" class="btn-icon danger-icon" @click="handleClearLogs" :title="$t('settings.clearLogs') || '清除日志'"><RotateCcw :size="14" /></button>
      <button v-else class="btn btn-danger" @click="handleClearLogs"><RotateCcw :size="14" /> {{ $t('settings.clearLogsBtn') || '清除' }}</button>
    </template>

    <!-- Clear Browser Cache Button -->
    <template v-else-if="settingKey === 'clearBrowserCache'">
      <button v-if="compact" class="btn-icon danger-icon" @click="handleClearBrowserCache" :title="$t('settings.clearBrowserCache')"><RotateCcw :size="14" /></button>
      <button v-else class="btn btn-danger" @click="handleClearBrowserCache"><RotateCcw :size="14" /> {{ $t('settings.clearBrowserCacheBtn') }}</button>
    </template>

    <!-- Custom fallback for unknown or very complex types -->
    <template v-else>
      <span style="font-size: 12px; color: var(--color-text-tertiary);">（该项在此暂不支持快速操作）</span>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { IPC } from '../../constants/ipc'
import { useUiStore } from '../../stores/uiStore'
import { useConfigStore } from '../../stores/configStore'
import { useMediaStore } from '../../stores/mediaStore'
import { useScanStore } from '../../stores/scanStore'
import { useAiStore } from '../../stores/aiStore'
import { useI18n } from 'vue-i18n'
import { THUMB_SIZE_TIERS } from '../../constants/defaults'
import { RotateCcw, Database, Paintbrush } from '@lucide/vue'

const props = defineProps<{
  settingKey: string
  compact?: boolean
}>()

const ui = useUiStore()
const config = useConfigStore()
const media = useMediaStore()
const scan = useScanStore()
const ai = useAiStore()
const { t } = useI18n()

// Local bindings for inputs
const uiFontSizeLocal = ref(config.uiFontSize)
const enableHoverScaleLocal = ref(config.enableHoverScale)
const thumbStrategyLocal = ref(config.thumbStrategy)
const gpuEngineLocal = ref(config.gpuEngine)
const thumbSkipMaxKbLocal = ref(config.thumbSkipMaxKb)
const thumbCacheMaxMbLocal = ref(config.thumbCacheMaxMb)
const timelineScrollWidthLocal = ref(config.timelineScrollWidth)
const aiProviderOverrideLocal = ref(config.aiProviderOverride)
const aiBatchSizeLocal = ref(config.aiBatchSize)
const aiImageModelLocal = ref(config.aiImageModel)
const aiTextModelLocal = ref(config.aiTextModel)
const logLevelLocal = ref(config.logLevel)
const availableAiModels = ref<string[]>([])

onMounted(async () => {
  if (props.settingKey === 'aiVisionModel' || props.settingKey === 'aiTextModel') {
    availableAiModels.value = await ai.listAiModels()
  }
})

// Sync from store
watch(() => config.uiFontSize, (v) => uiFontSizeLocal.value = v)
watch(() => config.enableHoverScale, (v) => enableHoverScaleLocal.value = v)
watch(() => config.thumbStrategy, (v) => thumbStrategyLocal.value = v)
watch(() => config.gpuEngine, (v) => gpuEngineLocal.value = v)
watch(() => config.thumbSkipMaxKb, (v) => thumbSkipMaxKbLocal.value = v)
watch(() => config.thumbCacheMaxMb, (v) => thumbCacheMaxMbLocal.value = v)
watch(() => config.timelineScrollWidth, (v) => timelineScrollWidthLocal.value = v)
watch(() => config.aiProviderOverride, (v) => aiProviderOverrideLocal.value = v)
watch(() => config.aiBatchSize, (v) => aiBatchSizeLocal.value = v)
watch(() => config.aiImageModel, (v) => aiImageModelLocal.value = v)
watch(() => config.aiTextModel, (v) => aiTextModelLocal.value = v)
watch(() => config.logLevel, (v) => logLevelLocal.value = v)

function getTierLabel(tier: number): string {
  const labels: Record<number, string> = {
    120: t('settings.thumbTierS'),
    240: t('settings.thumbTierM'),
    480: t('settings.thumbTierL'),
    960: t('settings.thumbTierXL'),
  }
  return labels[tier] ?? `${tier}px`
}

async function handleClearDb() {
  if (!confirm(t('sidebar.clearDbConfirm') || '确定要清除所有数据？此操作不可撤销。')) return
  try {
    await scan.clearDatabase()
    media.loadStats()
    ui.addToast('success', t('sidebar.clearDbSuccess') || '数据已清除')
  } catch (e) {
    ui.addToast('error', `清除数据失败: ${e}`)
  }
}

async function handleClearSettings() {
  if (!confirm(t('sidebar.clearSettingsConfirm') || '确定要重置所有设置？')) return
  try {
    await invoke(IPC.CLEAR_SETTINGS)
    window.location.reload()
  } catch (e) {
    ui.addToast('error', `清除设置失败: ${e}`)
  }
}

async function handleClearLogs() {
  try {
    await invoke('clear_logs')
    ui.addToast('success', t('settings.clearLogsSuccess') || '日志文件已清除')
  } catch (e) {
    ui.addToast('error', `清除日志失败: ${e}`)
  }
}

async function handleClearAllThumbnails() {
  if (!confirm(t('sidebar.clearThumbnailsConfirm') || '确定要清除所有缩略图？此操作不可撤销，且后续浏览时会重新生成。')) return
  try {
    await invoke('clear_all_thumbnails')
    media.invalidateLayout()
    ui.addToast('success', t('sidebar.clearThumbnailsSuccess') || '所有缩略图已清除')
  } catch (e) {
    ui.addToast('error', `清除缩略图失败: ${e}`)
  }
}

async function handleClearBrowserCache() {
  try {
    await invoke('clear_browser_cache')
    ui.addToast('success', t('settings.clearBrowserCacheSuccess') || '浏览器缓存已清除')
  } catch (e) {
    ui.addToast('error', `清除浏览器缓存失败: ${e}`)
  }
}
</script>

<style scoped>
/* ── Segmented Control ─────────────────────────────────────────────────── */
.segmented-control {
  display: inline-flex;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
  overflow: hidden;
}
.segmented-btn {
  padding: 8px 16px;
  font-size: 13px;
  background: transparent;
  color: var(--color-text-secondary);
  border: none;
  border-right: 1px solid var(--color-border);
  cursor: pointer;
  transition: all var(--transition-fast);
}
.segmented-btn:last-child { border-right: none; }
.segmented-btn:hover { background: var(--color-bg-hover); }
.segmented-btn.active {
  background: var(--color-accent);
  color: #fff;
}

.dynamic-control {
  display: flex;
  align-items: center;
  justify-content: flex-end;
}

.dynamic-control.compact {
  justify-content: flex-start;
  margin-top: 4px;
  width: 100%;
}

.compact-select-wrap {
  width: 100% !important;
  max-width: 100%;
}
.compact-select-wrap .select {
  padding: 4px 24px 4px 8px;
  font-size: 12px;
  height: 26px;
  min-height: 26px;
  width: 100%;
}

.compact-input {
  width: 60px;
  padding: 2px 6px;
  font-size: 12px;
  height: 26px;
}

.compact-toggle {
  transform: scale(0.8);
  transform-origin: left center;
}

.compact-segmented {
  flex-wrap: wrap;
  gap: 2px;
}
.compact-segmented .segmented-btn {
  padding: 2px 6px;
  font-size: 11px;
}

.danger-icon {
  color: var(--color-error);
}
</style>
