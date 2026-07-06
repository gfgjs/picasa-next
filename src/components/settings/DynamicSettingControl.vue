<template>
  <div :class="['dynamic-control', { compact }]">
    <!-- ── 特例:AI 批大小(固定 batch 钳制 + 分级风险提示,注册表 control='custom')── -->
    <template v-if="settingKey === 'aiBatchSize'">
      <div class="batch-size-stack">
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
        <span v-if="aiBatchSizeLocal === 0" class="batch-hint batch-hint--ok"
          >{{ $t('settings.aiBatchAutoAllocated')
          }}<template v-if="ai.status.activeFixedBatch">{{
            $t('settings.aiBatchNotLowerThan', { k: ai.status.activeFixedBatch })
          }}</template></span
        >
        <span
          v-else-if="ai.status.activeFixedBatch && aiBatchSizeLocal < ai.status.activeFixedBatch"
          class="batch-hint batch-hint--error"
          >{{ $t('settings.aiBatchFixedCannotBeLower', { k: ai.status.activeFixedBatch }) }}</span
        >
        <span v-else-if="aiBatchSizeLocal > 200" class="batch-hint batch-hint--error">{{
          $t('settings.aiBatchHighRisk')
        }}</span>
        <span v-else-if="aiBatchSizeLocal > 128" class="batch-hint batch-hint--warn">{{
          $t('settings.aiBatchWarning')
        }}</span>
        <span v-else-if="ai.status.activeFixedBatch" class="batch-hint batch-hint--muted">{{
          $t('settings.aiBatchFixedMin', { k: ai.status.activeFixedBatch })
        }}</span>
      </div>
    </template>

    <!-- ── 特例:缩略图尺寸档 segmented ─────────────────────────── -->
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

    <!-- ── 开关类(注册表 control='toggle',绑定见 toggleBindings)──── -->
    <template v-else-if="spec?.control === 'toggle' && hasToggleBinding">
      <label class="toggle" :class="{ 'compact-toggle': compact }">
        <input type="checkbox" v-model="toggleModel" />
        <span class="toggle__thumb" />
      </label>
    </template>

    <!-- ── 下拉类(control='select',选项表来自注册表)────────────── -->
    <template v-else-if="spec?.control === 'select' && hasSelectBinding">
      <div class="select-wrap" :class="{ 'compact-select-wrap': compact }">
        <select v-model="selectModel" class="select">
          <option v-for="opt in spec.options ?? []" :key="opt.value" :value="opt.value">
            {{ opt.labelKey ? $t(opt.labelKey) : opt.label }}
          </option>
        </select>
      </div>
    </template>

    <!-- ── 数字类(control='number',边界来自注册表;本地缓冲,change 时提交)── -->
    <template v-else-if="spec?.control === 'number' && hasNumberBinding">
      <input
        type="number"
        v-model.number="numberLocal"
        @change="commitNumber"
        :min="spec.min"
        :max="spec.max"
        class="input-number"
        :class="{ 'compact-input': compact }"
      />
    </template>

    <!-- ── 危险清理按钮类(compact 统一 RotateCcw 图标,全尺寸按键取各自图标)── -->
    <template v-else-if="dangerSpec && spec">
      <button
        v-if="compact"
        class="btn-icon danger-icon"
        @click="dangerSpec.onClick"
        :title="$t(spec.label)"
        :aria-label="$t(spec.label)"
      >
        <RotateCcw :size="14" />
      </button>
      <button v-else class="btn btn-danger" @click="dangerSpec.onClick">
        <component :is="dangerSpec.icon" :size="14" /> {{ $t(dangerSpec.btnLabelKey) }}
      </button>
    </template>

    <!-- ── 兜底:钉住区不支持的复杂项(目录/引擎状态/全量生成等特例行)── -->
    <template v-else>
      <span class="unsupported-hint">{{ $t('settings.unsupportedQuickAction') }}</span>
    </template>
  </div>
</template>

<script setup lang="ts">
// 注册式控件分派(设计 §8):按 SETTINGS_MAP.control 类型分派模板,替代原按
// settingKey 的 625 行 v-else-if 长链。「控件长什么样」在注册表声明,「读写哪个
// store」在下方三张同构绑定表声明——新增常规设置项零模板改动。
// 行为等价性依据:ui/config 两 store 的 setter 均自带状态赋值,writable computed
// 直调 setter 与原「v-model 先赋值 + @change 再调 setter」观察行为一致(checkbox/
// select 的 v-model 本就只在 change 时刻写入)。number 类保留「输入进本地缓冲、
// change 才提交」的原语义,避免逐键击发 IPC。
import { ref, computed, watch } from 'vue'
import type { Component } from 'vue'
import { invokeIpc } from '../../utils/ipc'
import { IPC } from '../../constants/ipc'
import { useUiStore } from '../../stores/uiStore'
import { useConfigStore } from '../../stores/configStore'
import { useMediaStore } from '../../stores/mediaStore'
import { useScanStore } from '../../stores/scanStore'
import { useAiStore } from '../../stores/aiStore'
import { useI18n } from 'vue-i18n'
import { THUMB_SIZE_TIERS } from '../../constants/defaults'
import { SETTINGS_MAP } from '../../constants/settingsMap'
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

const spec = computed(() => SETTINGS_MAP[props.settingKey])

/* ── 绑定表:声明各键「读哪、写哪」;控件形态由注册表声明 ─────────────── */

const toggleBindings: Record<string, { get: () => boolean; set: (v: boolean) => void }> = {
  hoverScale: {
    get: () => config.enableHoverScale,
    set: (v) => void config.setEnableHoverScale(v),
  },
  hoverAutoplay: { get: () => ui.hoverAutoplay, set: (v) => ui.setHoverAutoplay(v) },
  bucketScroll: {
    get: () => ui.bucketSegmentedScroll,
    set: (v) => ui.setBucketSegmentedScroll(v),
  },
  showThumbInfo: { get: () => ui.showThumbInfo, set: (v) => ui.setShowThumbInfo(v) },
  enableVideoCover: {
    get: () => config.enableVideoCover,
    set: (v) => void config.setEnableVideoCover(v),
  },
  enableVideoKeyframes: {
    get: () => config.enableVideoKeyframes,
    set: (v) => void config.setEnableVideoKeyframes(v),
  },
  aiHqCache: { get: () => config.aiHqCache, set: (v) => void config.setAiHqCache(v) },
}

const selectBindings: Record<string, { get: () => string; set: (v: string) => void }> = {
  theme: {
    get: () => ui.appearance,
    set: (v) => ui.setAppearance(v as typeof ui.appearance),
  },
  language: { get: () => ui.language, set: (v) => ui.setLanguage(v) },
  closeBehavior: {
    get: () => ui.closeBehavior,
    set: (v) => ui.setCloseBehavior(v as typeof ui.closeBehavior),
  },
  thumbDecodeStrategy: {
    get: () => config.thumbStrategy,
    set: (v) => void config.setThumbStrategy(v as typeof config.thumbStrategy),
  },
  gpuEngine: {
    get: () => config.gpuEngine,
    set: (v) => void config.setGpuEngine(v as typeof config.gpuEngine),
  },
  aiHardwareStrategy: {
    get: () => config.aiProviderOverride,
    set: (v) => void config.setAiProviderOverride(v as typeof config.aiProviderOverride),
  },
  logLevel: {
    get: () => config.logLevel,
    set: (v) => void config.setLogLevel(v as typeof config.logLevel),
  },
}

const numberBindings: Record<string, { get: () => number; set: (v: number) => void }> = {
  uiFontSize: { get: () => config.uiFontSize, set: (v) => void config.setUiFontSize(v) },
  // 跳过阈值变更须同步失效布局(缩略图形态随之改变)。
  thumbSkipMaxKb: {
    get: () => config.thumbSkipMaxKb,
    set: (v) => {
      void config.setThumbSkipMaxKb(v)
      media.invalidateLayout()
    },
  },
  thumbCacheMaxMb: {
    get: () => config.thumbCacheMaxMb,
    set: (v) => void config.setThumbCacheMaxMb(v),
  },
  timelineScrollWidth: {
    get: () => config.timelineScrollWidth,
    set: (v) => void config.setTimelineScrollWidth(v),
  },
}

const hasToggleBinding = computed(() => !!toggleBindings[props.settingKey])
const hasSelectBinding = computed(() => !!selectBindings[props.settingKey])
const hasNumberBinding = computed(() => !!numberBindings[props.settingKey])

const toggleModel = computed<boolean>({
  get: () => toggleBindings[props.settingKey]?.get() ?? false,
  set: (v) => toggleBindings[props.settingKey]?.set(v),
})

const selectModel = computed<string>({
  get: () => selectBindings[props.settingKey]?.get() ?? '',
  set: (v) => selectBindings[props.settingKey]?.set(v),
})

// number 类本地缓冲:输入不触发提交,change 才写 store(并跟随 store 外部变更回同步)。
const numberLocal = ref(0)
watch(
  () => numberBindings[props.settingKey]?.get(),
  (v) => {
    if (typeof v === 'number') numberLocal.value = v
  },
  { immediate: true },
)
function commitNumber() {
  numberBindings[props.settingKey]?.set(numberLocal.value)
}

/* ── 危险清理按钮表(icon=全尺寸按钮图标;compact 统一 RotateCcw)────── */

const dangerButtons: Record<
  string,
  { icon: Component; btnLabelKey: string; onClick: () => void }
> = {
  clearDb: { icon: Database, btnLabelKey: 'settings.clearDbBtn', onClick: () => void handleClearDb() },
  clearSettings: {
    icon: Paintbrush,
    btnLabelKey: 'settings.clearSettingsBtn',
    onClick: () => void handleClearSettings(),
  },
  clearAllThumbnails: {
    icon: RotateCcw,
    btnLabelKey: 'settings.clearAllThumbnailsBtn',
    onClick: () => void handleClearAllThumbnails(),
  },
  clearBrowserCache: {
    icon: RotateCcw,
    btnLabelKey: 'settings.clearBrowserCacheBtn',
    onClick: handleClearBrowserCache,
  },
  clearLogs: {
    icon: RotateCcw,
    btnLabelKey: 'settings.clearLogsBtn',
    onClick: () => void handleClearLogs(),
  },
}
const dangerSpec = computed(() => dangerButtons[props.settingKey])

/* ── AI 批大小特例(钳制 + 提示)──────────────────────────────────── */

const aiBatchSizeLocal = ref(config.aiBatchSize)
watch(
  () => config.aiBatchSize,
  (v) => (aiBatchSizeLocal.value = v),
)

// AI 批处理大小变更:固定 batch 模型下,非自动(0)的值不得小于其固定 k —— 自动钳制并提示。
// 0=自动 仍允许(后端会把有效 batch 抬到 ≥k)。
function onBatchChange() {
  const k = ai.status.activeFixedBatch
  if (k && aiBatchSizeLocal.value > 0 && aiBatchSizeLocal.value < k) {
    aiBatchSizeLocal.value = k
    ui.addToast('warning', t('settings.aiBatchAdjustedToFixed', { k }))
  }
  void config.setAiBatchSize(aiBatchSizeLocal.value)
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

/* ── 危险清理按钮 handlers(与重构前逐行一致)────────────────────── */

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
  // 「清浏览器缓存」语义是纯前端:带 cache-busting 查询串重载,绕过 webview 已缓存的图片。
  // 不存在 `clear_browser_cache` 后端命令——此前调它必失败弹错误 toast(与 SettingsView 同名实现对齐后修复)。
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
  /* 彩底文字用反色 token:暗色主题 accent 偏亮,白字不可读(同 S5 批2 纪律) */
  color: var(--color-text-inverse);
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

/* ── AI 批大小提示(原内联 style 收进 class,S6)─────────────────────── */
.batch-size-stack {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 4px;
}
.batch-hint {
  font-size: 11px;
  white-space: nowrap;
}
.batch-hint--ok {
  color: var(--color-success);
}
.batch-hint--error {
  color: var(--color-error);
}
.batch-hint--warn {
  color: var(--color-warning);
}
.batch-hint--muted {
  color: var(--color-text-tertiary);
}

.unsupported-hint {
  font-size: 12px;
  color: var(--color-text-tertiary);
}
</style>
