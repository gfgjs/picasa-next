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
      <input
        type="number"
        v-model.number="uiFontSizeLocal"
        @change="config.setUiFontSize(uiFontSizeLocal)"
        min="12"
        max="24"
        class="input-number"
        :class="{ 'compact-input': compact }"
      />
    </template>

    <!-- Hover Scale -->
    <template v-else-if="settingKey === 'hoverScale'">
      <label class="toggle" :class="{ 'compact-toggle': compact }">
        <input
          type="checkbox"
          v-model="enableHoverScaleLocal"
          @change="config.setEnableHoverScale(enableHoverScaleLocal)"
        />
        <span class="toggle__thumb" />
      </label>
    </template>

    <!-- Close Behavior -->
    <template v-else-if="settingKey === 'closeBehavior'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select
          v-model="ui.closeBehavior"
          @change="ui.setCloseBehavior(ui.closeBehavior)"
          class="select"
        >
          <option value="ask">{{ $t('settings.closeBehaviorAsk') }}</option>
          <option value="minimize_to_tray">{{ $t('settings.closeBehaviorMinimize') }}</option>
          <option value="exit">{{ $t('settings.closeBehaviorExit') }}</option>
        </select>
      </div>
    </template>

    <!-- Hover Auto-play -->
    <template v-else-if="settingKey === 'hoverAutoplay'">
      <label class="toggle" :class="{ 'compact-toggle': compact }">
        <input
          type="checkbox"
          v-model="ui.hoverAutoplay"
          @change="ui.setHoverAutoplay(ui.hoverAutoplay)"
        />
        <span class="toggle__thumb" />
      </label>
    </template>

    <!-- Video Cover Extraction -->
    <template v-else-if="settingKey === 'enableVideoCover'">
      <label class="toggle" :class="{ 'compact-toggle': compact }">
        <input
          type="checkbox"
          v-model="enableVideoCoverLocal"
          @change="config.setEnableVideoCover(enableVideoCoverLocal)"
        />
        <span class="toggle__thumb" />
      </label>
    </template>

    <!-- Video Keyframe Extraction -->
    <template v-else-if="settingKey === 'enableVideoKeyframes'">
      <label class="toggle" :class="{ 'compact-toggle': compact }">
        <input
          type="checkbox"
          v-model="enableVideoKeyframesLocal"
          @change="config.setEnableVideoKeyframes(enableVideoKeyframesLocal)"
        />
        <span class="toggle__thumb" />
      </label>
    </template>

    <!-- AI High-quality Analysis Cache -->
    <template v-else-if="settingKey === 'aiHqCache'">
      <label class="toggle" :class="{ 'compact-toggle': compact }">
        <input
          type="checkbox"
          v-model="aiHqCacheLocal"
          @change="config.setAiHqCache(aiHqCacheLocal)"
        />
        <span class="toggle__thumb" />
      </label>
    </template>

    <!-- Show Thumb Info -->
    <template v-else-if="settingKey === 'showThumbInfo'">
      <label class="toggle" :class="{ 'compact-toggle': compact }">
        <input
          type="checkbox"
          v-model="ui.showThumbInfo"
          @change="ui.setShowThumbInfo(ui.showThumbInfo)"
        />
        <span class="toggle__thumb" />
      </label>
    </template>

    <!-- Thumb Decode Strategy -->
    <template v-else-if="settingKey === 'thumbDecodeStrategy'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select
          v-model="thumbStrategyLocal"
          @change="config.setThumbStrategy(thumbStrategyLocal)"
          class="select"
        >
          <option value="cpu">{{ $t('settings.thumbStrategyCpu') }}</option>
          <option value="gpu">{{ $t('settings.thumbStrategyGpu') }}</option>
          <option value="direct">{{ $t('settings.thumbStrategyDirect') }}</option>
        </select>
      </div>
    </template>

    <!-- GPU Engine -->
    <template v-else-if="settingKey === 'gpuEngine'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select
          v-model="gpuEngineLocal"
          @change="config.setGpuEngine(gpuEngineLocal)"
          class="select"
        >
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
      <input
        type="number"
        v-model.number="thumbSkipMaxKbLocal"
        @change="onThumbSkipMaxKbChange"
        min="0"
        max="1000000"
        class="input-number"
        :class="{ 'compact-input': compact }"
      />
    </template>

    <!-- Thumb Cache Max MB -->
    <template v-else-if="settingKey === 'thumbCacheMaxMb'">
      <input
        type="number"
        v-model.number="thumbCacheMaxMbLocal"
        @change="config.setThumbCacheMaxMb(thumbCacheMaxMbLocal)"
        min="100"
        max="100000"
        class="input-number"
        :class="{ 'compact-input': compact }"
      />
    </template>

    <!-- Timeline Scroll Width -->
    <template v-else-if="settingKey === 'timelineScrollWidth'">
      <input
        type="number"
        v-model.number="timelineScrollWidthLocal"
        @change="config.setTimelineScrollWidth(timelineScrollWidthLocal)"
        min="2"
        max="40"
        class="input-number"
        :class="{ 'compact-input': compact }"
      />
    </template>

    <!-- AI Hardware Strategy -->
    <template v-else-if="settingKey === 'aiHardwareStrategy'">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select
          v-model="aiProviderOverrideLocal"
          @change="config.setAiProviderOverride(aiProviderOverrideLocal)"
          class="select"
        >
          <option value="auto">{{ $t('settings.aiAutoHardware') }}</option>
          <option value="cpu">{{ $t('settings.aiForceCpu') }}</option>
        </select>
      </div>
    </template>

    <!-- AI Batch Size -->
    <template v-else-if="settingKey === 'aiBatchSize'">
      <div style="display: flex; flex-direction: column; align-items: flex-end; gap: 4px">
        <input
          type="number"
          v-model.number="aiBatchSizeLocal"
          @change="onBatchChange"
          min="0"
          max="512"
          class="input-number"
          :class="{ 'compact-input': compact }"
          :placeholder="$t('settings.aiBatchAutoPlaceholder')"
        />
        <span
          v-if="aiBatchSizeLocal === 0"
          style="font-size: 11px; color: var(--color-success); white-space: nowrap"
          >{{ $t('settings.aiBatchAutoAllocated')
          }}<template v-if="ai.status.activeFixedBatch">{{
            $t('settings.aiBatchNotLowerThan', { k: ai.status.activeFixedBatch })
          }}</template></span
        >
        <span
          v-else-if="ai.status.activeFixedBatch && aiBatchSizeLocal < ai.status.activeFixedBatch"
          style="font-size: 11px; color: var(--color-error); white-space: nowrap"
          >{{ $t('settings.aiBatchFixedCannotBeLower', { k: ai.status.activeFixedBatch }) }}</span
        >
        <span
          v-else-if="aiBatchSizeLocal > 200"
          style="font-size: 11px; color: var(--color-error); white-space: nowrap"
          >{{ $t('settings.aiBatchHighRisk') }}</span
        >
        <span
          v-else-if="aiBatchSizeLocal > 128"
          style="font-size: 11px; color: var(--color-warning); white-space: nowrap"
          >{{ $t('settings.aiBatchWarning') }}</span
        >
        <span
          v-else-if="ai.status.activeFixedBatch"
          style="font-size: 11px; color: var(--color-text-tertiary); white-space: nowrap"
          >{{ $t('settings.aiBatchFixedMin', { k: ai.status.activeFixedBatch }) }}</span
        >
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
      <button
        v-if="compact"
        class="btn-icon danger-icon"
        @click="handleClearDb"
        :title="$t('settings.clearDb')"
        :aria-label="$t('settings.clearDb')"
      >
        <RotateCcw :size="14" />
      </button>
      <button v-else class="btn btn-danger" @click="handleClearDb">
        <Database :size="14" /> {{ $t('settings.clearDbBtn') }}
      </button>
    </template>

    <!-- Clear Settings Button -->
    <template v-else-if="settingKey === 'clearSettings'">
      <button
        v-if="compact"
        class="btn-icon danger-icon"
        @click="handleClearSettings"
        :title="$t('settings.clearSettings')"
        :aria-label="$t('settings.clearSettings')"
      >
        <RotateCcw :size="14" />
      </button>
      <button v-else class="btn btn-danger" @click="handleClearSettings">
        <Paintbrush :size="14" /> {{ $t('settings.clearSettingsBtn') }}
      </button>
    </template>

    <!-- Clear All Thumbnails Button -->
    <template v-else-if="settingKey === 'clearAllThumbnails'">
      <button
        v-if="compact"
        class="btn-icon danger-icon"
        @click="handleClearAllThumbnails"
        :title="$t('settings.clearAllThumbnails')"
        :aria-label="$t('settings.clearAllThumbnails')"
      >
        <RotateCcw :size="14" />
      </button>
      <button v-else class="btn btn-danger" @click="handleClearAllThumbnails">
        <RotateCcw :size="14" /> {{ $t('settings.clearAllThumbnailsBtn') }}
      </button>
    </template>

    <!-- Clear Logs Button -->
    <template v-else-if="settingKey === 'clearLogs'">
      <button
        v-if="compact"
        class="btn-icon danger-icon"
        @click="handleClearLogs"
        :title="$t('settings.clearLogs')"
        :aria-label="$t('settings.clearLogs')"
      >
        <RotateCcw :size="14" />
      </button>
      <button v-else class="btn btn-danger" @click="handleClearLogs">
        <RotateCcw :size="14" /> {{ $t('settings.clearLogsBtn') }}
      </button>
    </template>

    <!-- Clear Browser Cache Button -->
    <template v-else-if="settingKey === 'clearBrowserCache'">
      <button
        v-if="compact"
        class="btn-icon danger-icon"
        @click="handleClearBrowserCache"
        :title="$t('settings.clearBrowserCache')"
        :aria-label="$t('settings.clearBrowserCache')"
      >
        <RotateCcw :size="14" />
      </button>
      <button v-else class="btn btn-danger" @click="handleClearBrowserCache">
        <RotateCcw :size="14" /> {{ $t('settings.clearBrowserCacheBtn') }}
      </button>
    </template>

    <!-- Custom fallback for unknown or very complex types -->
    <template v-else>
      <span style="font-size: 12px; color: var(--color-text-tertiary)">{{
        $t('settings.unsupportedQuickAction')
      }}</span>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { invokeIpc } from '../../utils/ipc'
import { IPC } from '../../constants/ipc'
import { useUiStore } from '../../stores/uiStore'
import { useConfigStore } from '../../stores/configStore'
import { useMediaStore } from '../../stores/mediaStore'
import { useScanStore } from '../../stores/scanStore'
import { useAiStore } from '../../stores/aiStore'
import { useI18n } from 'vue-i18n'
import { THUMB_SIZE_TIERS } from '../../constants/defaults'
import { RotateCcw, Database, Paintbrush } from '@lucide/vue'

defineProps<{
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

// 缩略图跳过阈值变更：写配置 + 失效布局（抽成方法——Vue 内联多语句处理器会被 Prettier
// semi:false 拆行破坏，故必须单方法调用）。
function onThumbSkipMaxKbChange() {
  config.setThumbSkipMaxKb(thumbSkipMaxKbLocal.value)
  media.invalidateLayout()
}
const thumbCacheMaxMbLocal = ref(config.thumbCacheMaxMb)
const timelineScrollWidthLocal = ref(config.timelineScrollWidth)
const aiProviderOverrideLocal = ref(config.aiProviderOverride)
const aiBatchSizeLocal = ref(config.aiBatchSize)
const logLevelLocal = ref(config.logLevel)
const enableVideoCoverLocal = ref(config.enableVideoCover)
const enableVideoKeyframesLocal = ref(config.enableVideoKeyframes)
const aiHqCacheLocal = ref(config.aiHqCache)

// Sync from store
watch(
  () => config.uiFontSize,
  (v) => (uiFontSizeLocal.value = v),
)
watch(
  () => config.enableHoverScale,
  (v) => (enableHoverScaleLocal.value = v),
)
watch(
  () => config.thumbStrategy,
  (v) => (thumbStrategyLocal.value = v),
)
watch(
  () => config.gpuEngine,
  (v) => (gpuEngineLocal.value = v),
)
watch(
  () => config.thumbSkipMaxKb,
  (v) => (thumbSkipMaxKbLocal.value = v),
)
watch(
  () => config.thumbCacheMaxMb,
  (v) => (thumbCacheMaxMbLocal.value = v),
)
watch(
  () => config.timelineScrollWidth,
  (v) => (timelineScrollWidthLocal.value = v),
)
watch(
  () => config.aiProviderOverride,
  (v) => (aiProviderOverrideLocal.value = v),
)
watch(
  () => config.aiBatchSize,
  (v) => (aiBatchSizeLocal.value = v),
)
watch(
  () => config.logLevel,
  (v) => (logLevelLocal.value = v),
)
watch(
  () => config.enableVideoCover,
  (v) => (enableVideoCoverLocal.value = v),
)
watch(
  () => config.enableVideoKeyframes,
  (v) => (enableVideoKeyframesLocal.value = v),
)
watch(
  () => config.aiHqCache,
  (v) => (aiHqCacheLocal.value = v),
)

// AI 批处理大小变更：固定 batch 模型下，非自动(0)的值不得小于其固定 k —— 自动钳制并提示。
// 0=自动 仍允许（后端会把有效 batch 抬到 ≥k）。
function onBatchChange() {
  const k = ai.status.activeFixedBatch
  if (k && aiBatchSizeLocal.value > 0 && aiBatchSizeLocal.value < k) {
    aiBatchSizeLocal.value = k
    ui.addToast('warning', t('settings.aiBatchAdjustedToFixed', { k }))
  }
  config.setAiBatchSize(aiBatchSizeLocal.value)
}

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
  if (!confirm(t('sidebar.clearDbConfirm'))) return
  try {
    await scan.clearDatabase()
    media.loadStats()
    ui.addToast('success', t('sidebar.clearDbSuccess'))
  } catch (e) {
    ui.addToast('error', t('sidebar.clearDbFailed', { error: e }))
  }
}

async function handleClearSettings() {
  if (!confirm(t('sidebar.clearSettingsConfirm'))) return
  try {
    await invokeIpc(IPC.CLEAR_SETTINGS)
    window.location.reload()
  } catch (e) {
    ui.addToast('error', t('sidebar.clearSettingsFailed', { error: e }))
  }
}

async function handleClearLogs() {
  try {
    await invokeIpc(IPC.CLEAR_LOGS)
    ui.addToast('success', t('settings.clearLogsSuccess'))
  } catch (e) {
    ui.addToast('error', t('settings.clearLogsFailed', { error: e }))
  }
}

async function handleClearAllThumbnails() {
  if (!confirm(t('sidebar.clearThumbnailsConfirm'))) return
  try {
    await invokeIpc(IPC.CLEAR_ALL_THUMBNAILS)
    media.invalidateLayout()
    ui.addToast('success', t('sidebar.clearThumbnailsSuccess'))
  } catch (e) {
    ui.addToast('error', t('sidebar.clearThumbnailsFailed', { error: e }))
  }
}

function handleClearBrowserCache() {
  // 「清浏览器缓存」语义是纯前端：带 cache-busting 查询串重载，绕过 webview 已缓存的图片。
  // 不存在 `clear_browser_cache` 后端命令——此前调它必失败弹错误 toast（与 SettingsView 同名实现对齐后修复）。
  window.location.href = window.location.pathname + '?clear=' + Date.now()
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
.segmented-btn:last-child {
  border-right: none;
}
.segmented-btn:hover {
  background: var(--color-bg-hover);
}
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
