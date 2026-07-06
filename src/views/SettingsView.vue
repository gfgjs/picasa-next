<template>
  <div class="settings-view">
    <header class="settings-header">
      <h1 class="settings-title">{{ $t('settings.title') }}</h1>
      <div class="settings-header__actions">
        <!-- 一键全部折叠/展开：全部展开时收起全部，否则展开全部。 -->
        <button
          class="btn-toggle-all"
          :title="cards.allOpen.value ? $t('settings.collapseAll') : $t('settings.expandAll')"
          @click="toggleAllCards"
        >
          <component :is="cards.allOpen.value ? ChevronsDownUp : ChevronsUpDown" :size="15" />
          <span>{{
            cards.allOpen.value ? $t('settings.collapseAll') : $t('settings.expandAll')
          }}</span>
        </button>
        <button
          class="btn-close"
          :title="$t('settings.closeSettings')"
          :aria-label="$t('settings.closeSettings')"
          @click="closeSettings"
        >
          <X :size="18" />
        </button>
      </div>
    </header>

    <main class="settings-content">
      <!-- ── 外观 ─────────────────────────────────────────── -->
      <!-- 各卡行序与行体均由注册表驱动(设计 §8):行=SettingRow 外壳,特例行就地内嵌。 -->
      <CollapsibleCard id="general" :title="$t('settings.general')">
        <template v-for="key in sectionSettingKeys('general')" :key="key">
          <!-- 特例:主题行无右侧控件,其控件为下方 ThemePicker;钉住区仍用 compact select -->
          <template v-if="key === 'theme'">
            <SettingRow setting-key="theme" no-control />
            <ThemePicker />
          </template>
          <SettingRow v-else :setting-key="key" />
        </template>
      </CollapsibleCard>

      <!-- ── 缩略图 ───────────────────────────────────────── -->
      <CollapsibleCard id="thumbnails" :title="$t('settings.thumbnails')">
        <template v-for="key in sectionSettingKeys('thumbnails')" :key="key">
          <!-- 特例:悬停信息开关下挂信息元素多选面板 -->
          <SettingRow v-if="key === 'showThumbInfo'" setting-key="showThumbInfo">
            <div class="thumb-info-stack">
              <DynamicSettingControl setting-key="showThumbInfo" />
              <div v-if="ui.showThumbInfo" class="thumb-info-options">
                <label v-for="el in THUMB_INFO_ELEMENTS" :key="el.value" class="thumb-info-option">
                  <input
                    type="checkbox"
                    :checked="ui.thumbInfoElements.includes(el.value)"
                    @click="handleThumbInfoToggle($event, el.value)"
                  />{{ $t(el.labelKey) }}
                </label>
              </div>
            </div>
          </SettingRow>
          <!-- 特例:缓存目录(可点路径描述 + 换目录按钮) -->
          <SettingRow v-else-if="key === 'thumbCacheDir'" setting-key="thumbCacheDir">
            <template #desc>
              <div
                class="settings-card__desc clickable-path"
                @click="openDirectory(thumbCacheDir)"
                :title="$t('settings.openInExplorer')"
              >
                {{ thumbCacheDir || $t('settings.fetchingPath') }}
              </div>
            </template>
            <button class="btn btn-secondary" @click="changeCacheDir">
              {{ $t('settings.changeDir') }}
            </button>
          </SettingRow>
          <!-- 特例:全量缩略图生成(进度条 + 启停按钮) -->
          <SettingRow v-else-if="key === 'fullThumbGen'" setting-key="fullThumbGen">
            <template #extra>
              <div v-if="scan.thumbGenProgress.status !== 'idle'" class="thumb-gen-status">
                <div class="progress-bar">
                  <div
                    class="progress-bar__fill"
                    :class="{ 'progress-shimmer': scan.thumbGenProgress.isRunning }"
                    :style="{ width: thumbGenPercent + '%' }"
                  />
                </div>
                <div class="thumb-gen-text">
                  <span v-if="scan.thumbGenProgress.isRunning">{{
                    $t('settings.genStatusRunning', {
                      generated: scan.thumbGenProgress.generated,
                      total: scan.thumbGenProgress.total,
                    })
                  }}</span>
                  <span
                    v-if="scan.thumbGenProgress.isRunning && scan.thumbGenProgress.phase"
                    class="thumb-gen-phase"
                    >[{{ scan.thumbGenProgress.phase }}]</span
                  >
                  <span v-else-if="scan.thumbGenProgress.status === 'completed'">{{
                    $t('settings.genStatusCompleted')
                  }}</span>
                  <span v-else-if="scan.thumbGenProgress.status === 'cancelled'">{{
                    $t('settings.genStatusCancelled')
                  }}</span>
                  <span v-else-if="scan.thumbGenProgress.status === 'error'">{{
                    $t('settings.genStatusError')
                  }}</span>
                </div>
              </div>
            </template>
            <div class="setting-actions">
              <button
                v-if="scan.thumbGenProgress.isRunning"
                class="btn btn-secondary"
                @click="scan.stopFullThumbnailGeneration()"
              >
                {{ $t('settings.stopGen') }}
              </button>
              <button v-else class="btn btn-primary" @click="scan.startFullThumbnailGeneration()">
                {{ $t('settings.startGen') }}
              </button>
            </div>
          </SettingRow>
          <!-- gpuEngine 行仅在解码策略=GPU 时可见(rowVisible,与原 v-if 一致) -->
          <SettingRow v-else-if="rowVisible(key)" :setting-key="key" />
        </template>
      </CollapsibleCard>

      <!-- ── 视频 ─────────────────────────────────────────── -->
      <CollapsibleCard id="video" :title="$t('settings.video')">
        <SettingRow v-for="key in sectionSettingKeys('video')" :key="key" :setting-key="key" />
      </CollapsibleCard>

      <!-- ── AI 模型配置 ──────────────────────────────────── -->
      <CollapsibleCard id="aiModels" :title="$t('settings.aiModels')">
        <template v-for="key in sectionSettingKeys('aiModels')" :key="key">
          <!-- 特例:引擎状态(设备/显存/模型加载状态描述 + 测试加载按钮) -->
          <SettingRow v-if="key === 'aiEngineStatus'" setting-key="aiEngineStatus">
            <template #desc>
              <div class="settings-card__desc">
                {{ ai.providerLabel }} {{ ai.status.gpuName ? `(${ai.status.gpuName})` : '' }}
                <span v-if="ai.status.vramGb !== null">
                  [{{ $t('settings.aiVram') }}: {{ ai.status.vramGb }}GB]</span
                >
                <span v-if="!ai.status.clipLoaded" class="ai-status-warn">
                  {{ $t('settings.aiModelNotLoaded') }}</span
                >
                <span v-else class="ai-status-ok"> {{ $t('settings.aiModelLoaded') }}</span>
              </div>
            </template>
            <button
              class="btn btn-secondary"
              @click="ai.initEngine"
              :disabled="ai.status.clipLoaded"
            >
              {{ $t('settings.aiTestLoad') }}
            </button>
          </SettingRow>
          <SettingRow v-else :setting-key="key" />
        </template>
        <!-- 手动导入 / 图像·文本模型选择已移除：模型的下载与切换统一由下方「模型库」管理。 -->
      </CollapsibleCard>

      <!-- ── AI 模型库（下载与切换，Layer B）──────────────────── -->
      <ModelLibrary />

      <!-- ── 人脸模型库（F7，只读：双轨 + 安装状态）──────────────── -->
      <FaceModelLibrary />

      <!-- ── 网络存储（需求8 8B, §3.8）─────────────────────── -->
      <NetworkStorageSection />

      <!-- ── 已知卷（T13 §3.7 离线 UX）──────────────────────── -->
      <KnownVolumesSection />

      <!-- ── 开发者工具 ─────────────────────────────────── -->
      <CollapsibleCard id="debug" :title="$t('sidebar.debugSettings')">
        <template v-for="key in sectionSettingKeys('debug')" :key="key">
          <!-- 特例:日志目录(可点路径描述 + 换目录按钮) -->
          <SettingRow v-if="key === 'logDir'" setting-key="logDir">
            <template #desc>
              <div
                class="settings-card__desc clickable-path"
                @click="openDirectory(logDir)"
                :title="$t('settings.openInExplorer')"
              >
                {{ logDir || $t('settings.fetchingPath') }}
              </div>
            </template>
            <button class="btn btn-secondary" @click="changeLogDir">
              {{ $t('settings.changeDir') }}
            </button>
          </SettingRow>
          <SettingRow v-else :setting-key="key" />
        </template>
      </CollapsibleCard>
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed, onUnmounted } from 'vue'
import { invokeIpc } from '../utils/ipc'
import { useUiStore } from '../stores/uiStore'
import { useScanStore } from '../stores/scanStore'
import { useMediaStore } from '../stores/mediaStore'
import { useAiStore } from '../stores/aiStore'
import { useConfigStore } from '../stores/configStore'
import { useI18n } from 'vue-i18n'
import { X, ChevronsDownUp, ChevronsUpDown } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { IPC } from '../constants/ipc'
import { sectionSettingKeys } from '../constants/settingsMap'
import SettingRow from '../components/settings/SettingRow.vue'
import DynamicSettingControl from '../components/settings/DynamicSettingControl.vue'
import ThemePicker from '../components/settings/ThemePicker.vue'
import NetworkStorageSection from '../components/settings/NetworkStorageSection.vue'
import KnownVolumesSection from '../components/settings/KnownVolumesSection.vue'
import ModelLibrary from '../components/settings/ModelLibrary.vue'
import FaceModelLibrary from '../components/settings/FaceModelLibrary.vue'
import CollapsibleCard from '../components/settings/CollapsibleCard.vue'
import { useSettingsCards } from '../composables/useSettingsCards'

const ui = useUiStore()
const scan = useScanStore()
const media = useMediaStore()
const ai = useAiStore()
const config = useConfigStore()
const { t } = useI18n()

// 设置卡片折叠协调器：驱动头部「一键全部折叠/展开」。
const cards = useSettingsCards()
function toggleAllCards() {
  cards.setAll(!cards.allOpen.value)
}

// gpuEngine 行仅在解码策略=GPU 时显示(注册式重构前的行级 v-if 原样保留,§8)。
function rowVisible(key: string): boolean {
  return key !== 'gpuEngine' || config.thumbStrategy === 'gpu'
}

// 悬停信息元素多选面板(顺序即渲染顺序;geo 的 i18n key 为历史命名 thumbInfoLocation)。
const THUMB_INFO_ELEMENTS = [
  { value: 'status', labelKey: 'settings.thumbInfoStatus' },
  { value: 'favorite', labelKey: 'settings.thumbInfoFavorite' },
  { value: 'size', labelKey: 'settings.thumbInfoSize' },
  { value: 'resolution', labelKey: 'settings.thumbInfoResolution' },
  { value: 'date', labelKey: 'settings.thumbInfoDate' },
  { value: 'filename', labelKey: 'settings.thumbInfoFilename' },
  { value: 'path', labelKey: 'settings.thumbInfoPath' },
  { value: 'geo', labelKey: 'settings.thumbInfoLocation' },
  { value: 'camera', labelKey: 'settings.thumbInfoCamera' },
  { value: 'params', labelKey: 'settings.thumbInfoParams' },
]

const thumbCacheDir = ref('')
const logDir = ref('')

const thumbGenPercent = computed(() => {
  const { generated, total } = scan.thumbGenProgress
  if (!total) return 0
  return Math.min(100, Math.round((generated / total) * 100))
})

onMounted(async () => {
  await config.loadConfig()

  try {
    thumbCacheDir.value = await invokeIpc<string>(IPC.GET_THUMB_CACHE_DIR)
  } catch (e) {
    console.warn('Failed to fetch resolved cache dir', e)
  }

  try {
    logDir.value = await invokeIpc<string>(IPC.GET_LOG_DIR)
  } catch (e) {
    console.warn('Failed to fetch resolved log dir', e)
  }

  try {
    await ai.fetchStatus()
  } catch (e) {
    console.error('Failed to get ai status:', e)
  }

  document.addEventListener('keydown', onKeyDown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', onKeyDown)
})

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') ui.isSettingsOpen = false
}

async function changeLogDir() {
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: t('settings.chooseLogDir'),
    })
    if (selected && typeof selected === 'string') {
      await config.saveConfig('log_dir', selected)
      logDir.value = selected
      ui.addToast('success', t('settings.logDirChanged'))
    }
  } catch (e) {
    console.error('Failed to select log directory:', e)
  }
}

async function changeCacheDir() {
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: t('settings.chooseCacheDir'),
    })
    if (selected && typeof selected === 'string') {
      await config.saveConfig('thumb_cache_dir', selected)
      thumbCacheDir.value = selected
      ui.addToast('success', t('settings.cacheDirChanged'))
    }
  } catch (e) {
    console.error('Failed to select directory:', e)
  }
}

async function openDirectory(path: string) {
  if (!path) return
  try {
    await invokeIpc(IPC.OPEN_DIRECTORY, { path })
  } catch (e) {
    ui.addToast('error', t('settings.openDirFailed', { error: e }))
  }
}

function handleThumbInfoToggle(e: MouseEvent, val: string) {
  const el = e.target as HTMLInputElement
  const isChecked = el.checked
  const isAdvanced = ['geo', 'camera', 'params'].includes(val)

  if (isChecked && isAdvanced) {
    if (!window.confirm(t('settings.advancedMetadataWarning'))) {
      e.preventDefault()
      return
    }
  }

  const current = new Set(ui.thumbInfoElements)
  if (isChecked) {
    current.add(val)
  } else {
    current.delete(val)
  }
  ui.setThumbInfoElements(Array.from(current))
  media.invalidateLayout()
}

function closeSettings() {
  ui.isSettingsOpen = false
}
</script>

<style scoped>
.settings-view {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  flex-direction: column;
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

.settings-header__actions {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
}

/* 一键全部折叠/展开 */
.btn-toggle-all {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border-radius: var(--radius-md);
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition:
    background var(--transition-fast),
    color var(--transition-fast),
    border-color var(--transition-fast);
}
.btn-toggle-all:hover {
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
  border-color: var(--color-text-tertiary);
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
  color: var(--color-text-inverse);
  border-color: var(--color-error);
}

.settings-content {
  padding: var(--spacing-lg) var(--spacing-xl);
  max-width: 860px;
  margin: 0 auto;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

/* ── Card overrides (extend global .settings-card) ─────────────────────── */
/* The base .settings-card styles are in index.css. */
/* 行外壳(__item 内边距/pin 钮)样式已随 §8 注册式重构移入 SettingRow.vue。 */

.clickable-path {
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 2px;
  text-decoration-color: transparent;
  transition:
    text-decoration-color var(--transition-fast),
    color var(--transition-fast);
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
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
/* 生成阶段小徽章(原内联 style 收进 class,S6) */
.thumb-gen-phase {
  font-weight: 500;
  color: var(--color-warning);
  border: 1px solid currentColor;
  padding: 0 4px;
  border-radius: 4px;
  font-size: 10px;
  line-height: 1.2;
}

/* ── 悬停信息元素多选面板(原内联 style 收进 class,S6)─────────────────── */
.thumb-info-stack {
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-end;
  max-width: 65%;
}
.thumb-info-options {
  display: flex;
  gap: 12px;
  font-size: 13px;
  align-items: center;
  flex-wrap: wrap;
  justify-content: flex-end;
  margin-top: 8px;
  color: var(--color-text-primary);
}
.thumb-info-option {
  display: flex;
  align-items: center;
  gap: 4px;
}

/* ── AI 引擎状态描述(原内联 style 收进 class,S6)──────────────────────── */
.ai-status-warn {
  color: var(--color-warning);
}
.ai-status-ok {
  color: var(--color-success);
}
@keyframes shimmer {
  0% {
    background-position: -200% 0;
  }
  100% {
    background-position: 200% 0;
  }
}

/* ── Buttons ───────────────────────────────────────────────────────────── */
.setting-actions {
  display: flex;
  gap: var(--spacing-sm);
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
  /* 彩底文字用反色 token:暗色主题 accent 偏亮,白字不可读(同 S5 批2 纪律) */
  color: var(--color-text-inverse);
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
  color: var(--color-text-inverse);
  border-color: var(--color-error);
}
</style>
