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
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('theme') }" @click="ui.togglePinnedSetting('theme')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.theme') }}</div>
            <div class="settings-card__desc">{{ $t('settings.themeDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="theme" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('language') }" @click="ui.togglePinnedSetting('language')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.language') }}</div>
            <div class="settings-card__desc">{{ $t('settings.languageDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="language" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('uiFontSize') }" @click="ui.togglePinnedSetting('uiFontSize')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.uiFontSize') }}</div>
            <div class="settings-card__desc">{{ $t('settings.uiFontSizeDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="uiFontSize" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('hoverScale') }" @click="ui.togglePinnedSetting('hoverScale')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.hoverScale') }}</div>
            <div class="settings-card__desc">{{ $t('settings.hoverScaleDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="hoverScale" />
        </div>
        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('closeBehavior') }" @click="ui.togglePinnedSetting('closeBehavior')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.closeBehavior') || '关闭主窗口时' }}</div>
            <div class="settings-card__desc">{{ $t('settings.closeBehaviorDesc') || '选择点击主窗口关闭按钮时的行为' }}</div>
          </div>
          <DynamicSettingControl setting-key="closeBehavior" />
        </div>
      </div>

      <!-- ── 缩略图 ───────────────────────────────────────── -->
      <div class="settings-card">
        <div class="settings-card__header">{{ $t('settings.thumbnails') || '缩略图' }}</div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('showThumbInfo') }" @click="ui.togglePinnedSetting('showThumbInfo')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> 缩略图信息悬浮窗</div>
            <div class="settings-card__desc">在画廊缩略图上显示额外信息</div>
          </div>
          <div style="display: flex; flex-direction: column; gap: 8px; align-items: flex-end; max-width: 65%;">
            <DynamicSettingControl setting-key="showThumbInfo" />
            <div v-if="ui.showThumbInfo" style="display: flex; gap: 12px; font-size: 13px; align-items: center; flex-wrap: wrap; justify-content: flex-end; margin-top: 8px; color: var(--color-text-primary);">
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('status')" @click="handleThumbInfoToggle($event, 'status')">状态图标</label>
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('favorite')" @click="handleThumbInfoToggle($event, 'favorite')">收藏状态</label>
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('size')" @click="handleThumbInfoToggle($event, 'size')">文件大小</label>
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('resolution')" @click="handleThumbInfoToggle($event, 'resolution')">分辨率</label>
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('date')" @click="handleThumbInfoToggle($event, 'date')">日期</label>
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('filename')" @click="handleThumbInfoToggle($event, 'filename')">文件名</label>
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('path')" @click="handleThumbInfoToggle($event, 'path')">路径</label>
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('geo')" @click="handleThumbInfoToggle($event, 'geo')">地理位置</label>
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('camera')" @click="handleThumbInfoToggle($event, 'camera')">相机</label>
              <label style="display: flex; align-items: center; gap: 4px;"><input type="checkbox" :checked="ui.thumbInfoElements.includes('params')" @click="handleThumbInfoToggle($event, 'params')">拍摄参数</label>
            </div>
          </div>
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('thumbDecodeStrategy') }" @click="ui.togglePinnedSetting('thumbDecodeStrategy')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.thumbDecodeStrategy') }}</div>
            <div class="settings-card__desc">{{ $t('settings.thumbDecodeDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="thumbDecodeStrategy" />
        </div>

        <div class="settings-card__item" v-if="config.thumbStrategy === 'gpu'">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('gpuEngine') }" @click="ui.togglePinnedSetting('gpuEngine')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.gpuEngine') }}</div>
            <div class="settings-card__desc">{{ $t('settings.gpuEngineDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="gpuEngine" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('thumbCacheDir') }" @click="ui.togglePinnedSetting('thumbCacheDir')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.thumbCacheDir') || '缓存目录' }}</div>
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
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('thumbSize') }" @click="ui.togglePinnedSetting('thumbSize')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.thumbSize') || '缩略图大小' }}</div>
            <div class="settings-card__desc">{{ $t('settings.thumbSizeHint') || '更高档位缩略图占用更多磁盘空间' }}</div>
          </div>
          <DynamicSettingControl setting-key="thumbSize" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('thumbSkipMaxKb') }" @click="ui.togglePinnedSetting('thumbSkipMaxKb')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.thumbSkipMaxKb') }}</div>
            <div class="settings-card__desc">{{ $t('settings.thumbSkipDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="thumbSkipMaxKb" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('thumbCacheMaxMb') }" @click="ui.togglePinnedSetting('thumbCacheMaxMb')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.thumbCacheMaxMb') || '缩略图缓存上限 (MB)' }}</div>
            <div class="settings-card__desc">{{ $t('settings.thumbCacheDesc') || '超出此限制时，将自动清理最旧的缓存文件。' }}</div>
          </div>
          <DynamicSettingControl setting-key="thumbCacheMaxMb" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('timelineScrollWidth') }" @click="ui.togglePinnedSetting('timelineScrollWidth')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.timelineScrollWidth') }}</div>
            <div class="settings-card__desc">{{ $t('settings.timelineScrollDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="timelineScrollWidth" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('fullThumbGen') }" @click="ui.togglePinnedSetting('fullThumbGen')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.fullThumbGen') }}</div>
            <div class="settings-card__desc">{{ $t('settings.fullThumbGenDesc') }}</div>
            <div v-if="scan.thumbGenProgress.status !== 'idle'" class="thumb-gen-status">
              <div class="progress-bar">
                <div
                  class="progress-bar__fill"
                  :class="{ 'progress-shimmer': scan.thumbGenProgress.isRunning }"
                  :style="{ width: thumbGenPercent + '%' }"
                />
              </div>
              <div class="thumb-gen-text" style="display: flex; align-items: center; gap: 6px;">
                <span v-if="scan.thumbGenProgress.isRunning">{{ $t('settings.genStatusRunning', { generated: scan.thumbGenProgress.generated, total: scan.thumbGenProgress.total }) }}</span>
                <span v-if="scan.thumbGenProgress.isRunning && scan.thumbGenProgress.phase" style="font-weight: 500; color: var(--color-warning); border: 1px solid currentColor; padding: 0 4px; border-radius: 4px; font-size: 10px; line-height: 1.2;">[{{ scan.thumbGenProgress.phase }}]</span>
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
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('aiEngineStatus') }" @click="ui.togglePinnedSetting('aiEngineStatus')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.aiEngineStatus') }}</div>
            <div class="settings-card__desc">
              {{ ai.providerLabel }} {{ ai.status.gpuName ? `(${ai.status.gpuName})` : '' }}
              <span v-if="ai.status.vramGb !== null"> [显存: {{ ai.status.vramGb }}GB]</span>
              <span v-if="!ai.status.clipLoaded" style="color: var(--color-warning);"> (未加载)</span>
              <span v-else style="color: var(--color-success);"> (已加载)</span>
            </div>
          </div>
          <button class="btn btn-secondary" @click="ai.initEngine" :disabled="ai.status.clipLoaded">
            {{ $t('settings.aiTestLoad') }}
          </button>
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('aiBatchSize') }" @click="ui.togglePinnedSetting('aiBatchSize')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> AI 批处理大小 (Batch Size)</div>
            <div class="settings-card__desc">推入 GPU 并行计算的图片数量。设为 0 表示自动侦测显存并应用安全限制。警告：设置过大会导致显存交换，极大降低性能。</div>
          </div>
          <DynamicSettingControl setting-key="aiBatchSize" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('aiHardwareStrategy') }" @click="ui.togglePinnedSetting('aiHardwareStrategy')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.aiHardwareStrategy') }}</div>
            <div class="settings-card__desc">
              {{ $t('settings.aiHardwareDesc') }}
            </div>
          </div>
          <DynamicSettingControl setting-key="aiHardwareStrategy" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('aiImportModel') }" @click="ui.togglePinnedSetting('aiImportModel')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.aiImportModel') }}</div>
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
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('aiVisionModel') }" @click="ui.togglePinnedSetting('aiVisionModel')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.aiVisionModel') }}</div>
            <div class="settings-card__desc">{{ $t('settings.aiVisionDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="aiVisionModel" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('aiTextModel') }" @click="ui.togglePinnedSetting('aiTextModel')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.aiTextModel') }}</div>
            <div class="settings-card__desc">{{ $t('settings.aiTextDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="aiTextModel" />
        </div>
      </div>

      <!-- ── 开发者工具 ─────────────────────────────────── -->
      <div class="settings-card">
        <div class="settings-card__header">{{ $t('sidebar.debugSettings') || '开发者工具' }}</div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('clearDb') }" @click="ui.togglePinnedSetting('clearDb')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.clearDb') }}</div>
            <div class="settings-card__desc">{{ $t('settings.clearDbDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="clearDb" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('clearSettings') }" @click="ui.togglePinnedSetting('clearSettings')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.clearSettings') }}</div>
            <div class="settings-card__desc">{{ $t('settings.clearSettingsDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="clearSettings" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('logLevel') }" @click="ui.togglePinnedSetting('logLevel')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.logLevel') }}</div>
            <div class="settings-card__desc">{{ $t('settings.logLevelDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="logLevel" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('logDir') }" @click="ui.togglePinnedSetting('logDir')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.logDir') }}</div>
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
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('clearAllThumbnails') }" @click="ui.togglePinnedSetting('clearAllThumbnails')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.clearAllThumbnails') || '清除所有缩略图' }}</div>
            <div class="settings-card__desc">{{ $t('settings.clearAllThumbnailsDesc') || '删除所有已生成的缩略图文件，并重置数据库状态。' }}</div>
          </div>
          <DynamicSettingControl setting-key="clearAllThumbnails" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('clearBrowserCache') }" @click="ui.togglePinnedSetting('clearBrowserCache')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.clearBrowserCache') }}</div>
            <div class="settings-card__desc">{{ $t('settings.clearBrowserCacheDesc') }}</div>
          </div>
          <DynamicSettingControl setting-key="clearBrowserCache" />
        </div>

        <div class="settings-card__item">
                    <button class="pin-btn" :class="{ active: ui.pinnedSettings.includes('clearLogs') }" @click="ui.togglePinnedSetting('clearLogs')" title="固定到侧边栏">
            <Pin :size="14" />
            </button>
          <div class="settings-card__info">
            <div class="settings-card__label" style="display: flex; align-items: center; gap: 8px;"> {{ $t('settings.clearLogs') || '清除日志' }}</div>
            <div class="settings-card__desc">{{ $t('settings.clearLogsDesc') || '删除所有存储的日志文件' }}</div>
          </div>
          <DynamicSettingControl setting-key="clearLogs" />
        </div>
      </div>
    <!-- </div> -->
    </main>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useUiStore } from '../stores/uiStore'
import { useScanStore } from '../stores/scanStore'
import { useMediaStore } from '../stores/mediaStore'
import { useAiStore } from '../stores/aiStore'
import { useConfigStore } from '../stores/configStore'
import { useI18n } from 'vue-i18n'
import { X, Database, Paintbrush, RotateCcw, Pin } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { IPC } from '../constants/ipc'
import DynamicSettingControl from '../components/settings/DynamicSettingControl.vue'

const ui = useUiStore()
const scan = useScanStore()
const media = useMediaStore()
const ai = useAiStore()
const config = useConfigStore()
const { t } = useI18n()

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
    thumbCacheDir.value = await invoke<string>('get_thumb_cache_dir')
  } catch (e) {
    console.warn('Failed to fetch resolved cache dir', e)
  }

  try {
    logDir.value = await invoke<string>('get_log_dir')
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
      title: '选择日志存储目录',
    })
    if (selected && typeof selected === 'string') {
      await config.saveConfig('log_dir', selected)
      logDir.value = selected
      ui.addToast('success', '日志存储目录已更改，重启应用后生效，旧日志需手动清理。')
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
      title: '选择缩略图缓存目录',
    })
    if (selected && typeof selected === 'string') {
      await config.saveConfig('thumb_cache_dir', selected)
      thumbCacheDir.value = selected
      ui.addToast('success', '缓存目录已更改，旧缓存不会自动移动，请根据需要手动清理。')
    }
  } catch (e) {
    console.error('Failed to select directory:', e)
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

async function importModel() {
  try {
    const selected = await openDialog({
      multiple: false,
      title: '导入 AI 模型 (.onnx)',
      filters: [{ name: 'ONNX Model', extensions: ['onnx'] }]
    })
    if (selected && typeof selected === 'string') {
      await ai.importAiModel(selected)
      ui.addToast('success', '模型导入成功，请在下拉列表中选择。')
    }
  } catch (e) {
    ui.addToast('error', `导入失败: ${e}`)
  }
}

async function saveAiModels() {
  await config.saveConfig('ai_image_model', config.aiImageModel)
  await config.saveConfig('ai_text_model', config.aiTextModel)
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

async function clearLogs() {
  try {
    await invoke('clear_logs')
    ui.addToast('success', t('settings.clearLogsSuccess') || '日志文件已清除')
  } catch (e) {
    ui.addToast('error', `清除日志失败: ${e}`)
  }
}

async function clearAllThumbnails() {
  if (!confirm(t('sidebar.clearThumbnailsConfirm') || '确定要清除所有缩略图？此操作不可撤销，且后续浏览时会重新生成。')) return
  try {
    await invoke('clear_all_thumbnails')
    media.invalidateLayout()
    ui.addToast('success', t('sidebar.clearThumbnailsSuccess') || '所有缩略图已清除')
  } catch (e) {
    ui.addToast('error', `清除缩略图失败: ${e}`)
  }
}

function clearBrowserCache() {
  window.location.href = window.location.pathname + '?clear=' + Date.now()
}

function handleThumbInfoToggle(e: MouseEvent, val: string) {
  const el = e.target as HTMLInputElement
  const isChecked = el.checked
  const isAdvanced = ['geo', 'camera', 'params'].includes(val)
  
  if (isChecked && isAdvanced) {
    if (!window.confirm('开启高级元数据（如地理信息、相机参数等）将增加布局计算时的性能开销，导致相册加载变慢。确定要开启吗？')) {
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
  max-width: 860px;
  margin: 0 auto;
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 48px;
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
  gap: var(--spacing-sm);
}

.pin-btn {
  background: transparent;
  border: none;
  color: var(--color-text-tertiary);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all var(--transition-fast);
}
.pin-btn:hover {
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
}
.pin-btn.active {
  color: var(--color-primary);
  background: color-mix(in srgb, var(--color-primary) 10%, transparent);
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
