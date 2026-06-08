<template>
  <nav class="sidebar">
    <!-- App title + logo -->
    <!-- 应用标题 + Logo -->
    <div class="sidebar__header">
      <div class="sidebar__logo">
        <span class="sidebar__logo-icon"><Aperture :size="20" /></span>
        <span class="sidebar__logo-text">Picasa Next</span>
      </div>
    </div>

    <!-- Scrollable content area -->
    <div class="sidebar__scroll-area">
      <!-- Smart albums -->
      <!-- 智能相册 -->
      <section class="sidebar__section">
      <div class="sidebar__section-label" @click="isLibraryExpanded = !isLibraryExpanded" style="cursor: pointer; display: flex; align-items: center; gap: 4px; justify-content: flex-start; user-select: none;">
        <ChevronRight :size="14" style="transition: transform 0.2s" :style="{ transform: isLibraryExpanded ? 'rotate(90deg)' : 'rotate(0deg)' }" />
        {{ $t('sidebar.library') }}
      </div>
      <transition name="collapse" @enter="onEnter" @after-enter="onAfterEnter" @leave="onLeave">
        <ul class="sidebar__nav" v-show="isLibraryExpanded">
          <li v-for="album in smartAlbums" :key="album.id">
            <button
              class="sidebar__nav-item"
              :class="{ active: ui.activeSmartAlbum === album.id && !ui.activeDirectoryId }"
              @click="handleSmartAlbumClick(album.id)"
            >
              <span class="sidebar__nav-icon"><component :is="album.icon" :size="18" /></span>
              <span class="sidebar__nav-label">{{ album.label }}</span>
              <span v-if="album.count != null" class="sidebar__nav-count">{{ formatCount(album.count) }}</span>
            </button>
          </li>
        </ul>
      </transition>
    </section>

    <!-- Divider -->
    <!-- 分隔线 -->
    <div class="sidebar__divider" />

    <!-- Tools -->
    <section class="sidebar__section">
      <div class="sidebar__section-label" @click="isToolsExpanded = !isToolsExpanded" style="cursor: pointer; display: flex; align-items: center; gap: 4px; justify-content: flex-start; user-select: none;">
        <ChevronRight :size="14" style="transition: transform 0.2s" :style="{ transform: isToolsExpanded ? 'rotate(90deg)' : 'rotate(0deg)' }" />
        工具 / TOOLS
      </div>
      <transition name="collapse" @enter="onEnter" @after-enter="onAfterEnter" @leave="onLeave">
        <div v-show="isToolsExpanded">
          <ul class="sidebar__nav sidebar__nav--tools" style="padding-bottom: 4px;">
            <template v-for="key in ui.pinnedSettings" :key="key">
            <!-- 特殊处理：全量生成缩略图 -->
            <li v-if="key === 'fullThumbGen'">
              <div class="sidebar__nav-item" style="flex-direction: column; align-items: flex-start; gap: 8px; cursor: default;">
                <div style="display: flex; align-items: center; justify-content: space-between; width: 100%;">
                  <div style="display: flex; align-items: center; gap: 8px;">
                    <span class="sidebar__nav-icon"><Zap :size="18" /></span>
                    <span class="sidebar__nav-label">全量生成缩略图</span>
                  </div>
                  <button class="btn-icon" @click="toggleThumbGen" :title="scan.thumbGenProgress.isRunning ? '停止生成' : '开始生成'">
                    <Square v-if="scan.thumbGenProgress.isRunning" :size="14" color="var(--color-error)" fill="var(--color-error)" />
                    <Play v-else :size="14" />
                  </button>
                </div>
                
                <div v-if="scan.thumbGenProgress.isRunning || scan.thumbGenProgress.status === 'completed'" style="width: 100%; font-size: 12px; color: var(--color-text-tertiary);">
                  <div v-if="scan.thumbGenProgress.isRunning" class="progress-bar" style="margin-bottom: 4px;">
                    <div class="progress-bar__fill" style="background: var(--color-accent);" :style="{ width: ((scan.thumbGenProgress.generated / Math.max(scan.thumbGenProgress.total, 1)) * 100) + '%' }" />
                  </div>
                  <div style="display: flex; justify-content: space-between;">
                    <span>{{ scan.thumbGenProgress.generated }} / {{ scan.thumbGenProgress.total }}</span>
                    <span v-if="elapsedTimeStr" style="font-family: monospace;">{{ elapsedTimeStr }}</span>
                  </div>
                </div>
              </div>
            </li>
            
            <!-- 动态渲染其他设置项 -->
            <li v-else-if="SETTINGS_MAP[key]">
              <div class="sidebar__nav-item" style="flex-direction: column; align-items: flex-start; gap: 4px; cursor: default;">
                <div style="display: flex; align-items: center; justify-content: space-between; width: 100%;">
                  <div style="display: flex; align-items: center; gap: 8px; flex: 1; margin-bottom: 4px;">
                    <span class="sidebar__nav-icon"><component :is="SETTINGS_MAP[key].icon" :size="18" /></span>
                    <span class="sidebar__nav-label" style="flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                      {{ $t(SETTINGS_MAP[key].label) }}
                    </span>
                  </div>
                </div>
                <DynamicSettingControl :setting-key="key" compact />
              </div>
            </li>
          </template>

          <!-- 全量 AI 分析 (未在设置页提供图钉，这里作为常驻核心项) -->
          <li>
            <div class="sidebar__nav-item" style="flex-direction: column; align-items: flex-start; gap: 8px; cursor: default;">
              <div style="display: flex; align-items: center; justify-content: space-between; width: 100%;">
                <div style="display: flex; align-items: center; gap: 8px;">
                  <span class="sidebar__nav-icon"><Sparkles :size="18" /></span>
                  <span class="sidebar__nav-label">全量 AI 分析</span>
                </div>
                <button class="btn-icon" @click="toggleAiAnalysis" :disabled="isAiInitialising" :title="ai.status.isAnalyzing ? '停止分析' : '开始分析'">
                  <Square v-if="ai.status.isAnalyzing" :size="14" color="var(--color-error)" fill="var(--color-error)" />
                  <RefreshCw v-else-if="isAiInitialising" :size="14" class="spinning" style="animation: spin 1s linear infinite;" />
                  <Play v-else :size="14" />
                </button>
              </div>
              
              <div v-if="ai.status.isAnalyzing || ai.status.totalItems > 0" style="width: 100%; font-size: 12px; color: var(--color-text-tertiary);">
                <div v-if="ai.status.isAnalyzing" class="progress-bar" style="margin-bottom: 4px;">
                  <div class="progress-bar__fill" style="background: var(--color-accent);" :style="{ width: ai.analyzeProgress + '%' }" />
                </div>
                <div style="display: flex; justify-content: space-between;">
                  <span>{{ ai.status.analyzedItems }} / {{ ai.status.totalItems }}</span>
                  <span v-if="aiElapsedTimeStr" style="font-family: monospace; margin-right: auto; margin-left: 8px;">{{ aiElapsedTimeStr }}</span>
                  <span style="font-family: monospace;">{{ ai.analyzeProgress }}%</span>
                </div>
              </div>
            </div>
          </li>
          </ul>
        </div>
      </transition>
    </section>

    <!-- Divider -->
    <div class="sidebar__divider" />

    <!-- Scan roots / folder tree -->
    <!-- 扫描根目录 / 文件夹树 -->
    <section class="sidebar__section sidebar__section--tree">
      <div class="sidebar__section-label" @click="isFoldersExpanded = !isFoldersExpanded" style="cursor: pointer; justify-content: space-between; user-select: none;">
        <div style="display: flex; align-items: center; gap: 4px;">
          <ChevronRight :size="14" style="transition: transform 0.2s" :style="{ transform: isFoldersExpanded ? 'rotate(90deg)' : 'rotate(0deg)' }" />
          <span>{{ $t('sidebar.folders') }}</span>
        </div>
        <div style="display: flex; align-items: center; gap: 4px;" @click.stop>
          <button
            class="sidebar-show-all-btn"
            :class="{ active: ui.activeSmartAlbum === 'all' && !ui.activeDirectoryId }"
            @click="showAll"
            :title="$t('sidebar.showAllTitle')"
          >
            {{ $t('sidebar.showAll') }}
          </button>
          <button class="btn-icon" :title="$t('sidebar.addFolder') || '导入已有文件夹'" @click="addRoot"><FolderSearch :size="16" /></button>
          <button class="btn-icon" title="新建空白文件夹" @click="createNewGlobalFolder"><FolderPlus :size="16" /></button>
        </div>
      </div>

      <transition name="collapse" @enter="onEnter" @after-enter="onAfterEnter" @leave="onLeave">
        <div v-show="isFoldersExpanded">
          <div v-if="folderTree.nodes.value.length === 0 && !scan.hasScanRoots" class="sidebar__empty">
            <span>{{ $t('sidebar.noFolders') }}</span>
          </div>

          <div class="sidebar__tree" v-if="folderTree.nodes.value.length > 0">
            <button
              v-for="node in folderTree.nodes.value"
              :key="node.id"
              class="sidebar__tree-item"
              :data-dir-id="node.id"
              :class="{
                active:    (ui.groupBy === 'folder' ? ui.scrolledDirectoryId === node.id : ui.activeDirectoryId === node.id),
                expanded:  node.expanded,
              }"
              :style="{ paddingLeft: (node.depth * 16 + 8) + 'px' }"
              @click="onNodeClick(node)"
              @contextmenu.prevent="onNodeContextMenu($event, node)"
            >
              <span class="sidebar__tree-arrow" @click.stop="folderTree.toggleNode(node)">
                <ChevronRight v-if="node.hasChildren" :size="14" class="sidebar__tree-chevron" :class="{ expanded: node.expanded }" />
                <span v-else class="sidebar__tree-chevron-spacer" />
              </span>
              <span class="sidebar__tree-icon"><Folder :size="15" /></span>
              <span class="sidebar__tree-label" :title="node.relPath">{{ node.name }}</span>
              <span class="sidebar__tree-count">{{ node.mediaCount }}</span>
            </button>
          </div>
        </div>
      </transition>
    </section>

    <!-- Divider -->
    <div v-if="scan.hasScanRoots" class="sidebar__divider" />

    <!-- Management -->
    <!-- 管理 -->
    <section v-if="scan.hasScanRoots" class="sidebar__section">
      <div class="sidebar__section-label" @click="isManagementExpanded = !isManagementExpanded" style="cursor: pointer; display: flex; align-items: center; gap: 4px; justify-content: flex-start; user-select: none;">
        <ChevronRight :size="14" style="transition: transform 0.2s" :style="{ transform: isManagementExpanded ? 'rotate(90deg)' : 'rotate(0deg)' }" />
        管理 / MANAGEMENT
      </div>
      <transition name="collapse" @enter="onEnter" @after-enter="onAfterEnter" @leave="onLeave">
        <div v-show="isManagementExpanded" style="overflow: hidden;">
          <div class="sidebar__scan-status" style="border-top: none; padding-top: 4px;">
            <div
              v-for="root in scan.scanRoots"
              :key="root.id"
              class="scan-root-item"
            >
              <div class="scan-root-item__info">
                <span class="scan-root-item__alias">{{ root.alias ?? root.path.split('/').pop() }}</span>
                <div style="display: flex; gap: 4px;">
                  <button
                    class="btn-icon scan-root-item__scan-btn"
                    :class="{ active: scan.getProgress(root.id)?.isRunning }"
                    @click="toggleScan(root.id)"
                    :title="scan.getProgress(root.id)?.isRunning ? $t('sidebar.stopScan') : $t('sidebar.rescan')"
                  >
                    <Square v-if="scan.getProgress(root.id)?.isRunning" :size="14" color="var(--color-error)" fill="var(--color-error)" />
                    <RefreshCw v-else :size="14" />
                  </button>
                  <button
                    class="btn-icon scan-root-item__scan-btn"
                    style="color: var(--color-error); opacity: 0.7;"
                    :title="$t('sidebar.removeFolder')"
                    @click="removeRoot(root.id)"
                  >
                    <Trash2 :size="14" />
                  </button>
                </div>
              </div>
              <div v-if="scan.getProgress(root.id)?.isRunning" class="scan-root-item__progress">
                <div class="progress-bar">
                  <div
                    class="progress-bar__fill progress-shimmer"
                    :style="{ width: (scan.getProgress(root.id)?.status === 'discovering' ? 100 : progressPercent(root.id)) + '%' }"
                    :class="{ 'progress-bar__fill--discovering': scan.getProgress(root.id)?.status === 'discovering' }"
                  />
                </div>
                <span class="scan-root-item__count">
                  <template v-if="scan.getProgress(root.id)?.status === 'discovering'">
                    {{ $t('sidebar.discoveringFiles', { count: scan.getProgress(root.id)?.scanned ?? 0 }) }}
                  </template>
                  <template v-else>
                    {{ scan.getProgress(root.id)?.scanned ?? 0 }} / {{ scan.getProgress(root.id)?.total ?? 0 }}
                  </template>
                </span>
              </div>
            </div>
          </div>
        </div>
      </transition>
    </section>
    </div>

    <!-- Settings / footer -->
    <!-- 设置 / 页脚 -->
    <div class="sidebar__footer">
      <button class="btn-icon" :title="$t('sidebar.settings')" @click="ui.isSettingsOpen = true"><Settings :size="18" /></button>
      <button class="btn-icon" :title="$t('sidebar.toggleTheme')" @click="ui.cycleTheme()">
        <Sun v-if="ui.isDark" :size="18" />
        <Moon v-else :size="18" />
      </button>
    </div>

    <!-- Custom Confirm Dialog -->
    <div v-if="confirmDialog.isOpen" class="custom-modal-overlay">
      <div class="custom-modal">
        <h3 class="custom-modal__title">{{ confirmDialog.title }}</h3>
        <p class="custom-modal__message">{{ confirmDialog.message }}</p>
        
        <div v-if="confirmDialog.showCheckbox" class="custom-modal__checkbox">
          <label class="checkbox-label" style="display: flex; align-items: center; gap: 8px; font-size: 13px; cursor: pointer;">
            <input type="checkbox" v-model="confirmDialog.checkboxValue" />
            {{ confirmDialog.checkboxLabel }}
          </label>
        </div>

        <div class="custom-modal__actions">
          <button class="btn btn-secondary" @click="closeConfirmDialog(false)">{{ confirmDialog.cancelText }}</button>
          <button class="btn btn-primary" @click="closeConfirmDialog(true)">{{ confirmDialog.confirmText }}</button>
        </div>
      </div>
    </div>

    <!-- Context Menu for Tree Node -->
    <ContextMenu
      :items="treeContextMenu.items"
      :visible="treeContextMenu.visible"
      :x="treeContextMenu.x"
      :y="treeContextMenu.y"
      @update:visible="treeContextMenu.visible = $event"
    />

    <!-- Folder Create Dialog -->
    <FolderCreateDialog
      v-if="folderCreateDialog.isOpen"
      :base-path="folderCreateDialog.basePath"
      @close="folderCreateDialog.isOpen = false"
      @created="onFolderCreated"
    />
  </nav>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, markRaw } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { open } from '@tauri-apps/plugin-dialog'
import { invoke } from '@tauri-apps/api/core'
import { IPC } from '../../constants/ipc'
import { useUiStore } from '../../stores/uiStore'
import { useScanStore } from '../../stores/scanStore'
import { useMediaStore } from '../../stores/mediaStore'
import { useAiStore } from '../../stores/aiStore'
import { useFolderTree } from '../../composables/useFolderTree'
import { SETTINGS_MAP } from '../../constants/settingsMap'
import DynamicSettingControl from '../settings/DynamicSettingControl.vue'
import ContextMenu, { ContextMenuItem } from '../common/ContextMenu.vue'
import FolderCreateDialog from '../common/FolderCreateDialog.vue'
import {
  Aperture, FolderPlus, FolderSearch, ChevronRight, Folder,
  Square, RefreshCw, Trash2, Settings,
  Sun, Moon, Monitor, ImageIcon, Heart, Sparkles, Clock, Play, Zap, RotateCcw, Database
} from '@lucide/vue'

const ui       = useUiStore()
const scan     = useScanStore()
const media    = useMediaStore()
const ai       = useAiStore()
const folderTree = useFolderTree()
const router   = useRouter()
const route    = useRoute()
const { t }    = useI18n()

// ── Collapsible Sections ───────────────────────────────────────────────────
const isLibraryExpanded = ref(true)
const isToolsExpanded = ref(true)
const isFoldersExpanded = ref(true)
const isManagementExpanded = ref(true)

function onEnter(el: Element) {
  const htmlEl = el as HTMLElement
  htmlEl.style.height = '0'
  htmlEl.style.opacity = '0'
  void htmlEl.offsetHeight // force reflow
  htmlEl.style.height = htmlEl.scrollHeight + 'px'
  htmlEl.style.opacity = '1'
}

function onAfterEnter(el: Element) {
  const htmlEl = el as HTMLElement
  htmlEl.style.height = ''
  htmlEl.style.opacity = ''
}

function onLeave(el: Element) {
  const htmlEl = el as HTMLElement
  htmlEl.style.height = htmlEl.offsetHeight + 'px'
  htmlEl.style.opacity = '1'
  void htmlEl.offsetHeight // force reflow
  htmlEl.style.height = '0'
  htmlEl.style.opacity = '0'
}

// ── Custom Confirm Dialog ──────────────────────────────────────────────────
interface ConfirmDialogOptions {
  title: string
  message: string
  confirmText?: string
  cancelText?: string
  showCheckbox?: boolean
  checkboxLabel?: string
  checkboxValue?: boolean
}

const confirmDialog = ref({
  isOpen: false,
  title: '',
  message: '',
  confirmText: '确认',
  cancelText: '取消',
  showCheckbox: false,
  checkboxLabel: '',
  checkboxValue: true,
  resolve: null as ((val: boolean) => void) | null
})

function showConfirmDialog(options: ConfirmDialogOptions): Promise<boolean> {
  return new Promise(resolve => {
    confirmDialog.value = {
      isOpen: true,
      title: options.title,
      message: options.message,
      confirmText: options.confirmText || '确认',
      cancelText: options.cancelText || '取消',
      showCheckbox: options.showCheckbox || false,
      checkboxLabel: options.checkboxLabel || '',
      checkboxValue: options.checkboxValue ?? true,
      resolve
    }
  })
}

function closeConfirmDialog(result: boolean) {
  if (confirmDialog.value.resolve) {
    confirmDialog.value.resolve(result)
  }
  confirmDialog.value.isOpen = false
}

// ── Smart albums ───────────────────────────────────────────────────────────
// ── 智能相册 ───────────────────────────────────────────────────────────

const smartAlbums = computed(() => [
  { id: 'all'         as const, icon: markRaw(ImageIcon), label: t('sidebar.allPhotos'),      count: media.stats?.totalItems },
  { id: 'favorites'   as const, icon: markRaw(Heart),     label: t('sidebar.favorites'),      count: media.stats?.totalFavorited },
  { id: 'live-photos' as const, icon: markRaw(Sparkles),  label: t('sidebar.livePhotos'), count: media.stats?.totalLivePhotos },
  { id: 'recent'      as const, icon: markRaw(Clock),     label: t('sidebar.recentlyAdded'),      count: null },
  { id: 'trash'       as const, icon: markRaw(Trash2),    label: t('sidebar.trash'),    count: media.stats?.totalDeleted },
])

function formatCount(n: number | undefined | null): string {
  if (n == null) return ''
  if (n >= 1000) return (n / 1000).toFixed(1) + 'k'
  return String(n)
}

// ── Folder tree ────────────────────────────────────────────────────────────
// ── 文件夹树 ────────────────────────────────────────────────────────────

function onNodeClick(node: any) {
  if (ui.groupBy === 'folder') {
    ui.pendingScrollLabel = node.name
    if (ui.activeSmartAlbum !== 'all' || ui.activeDirectoryId !== null) {
      ui.setSmartAlbum('all')
      ui.setActiveDirectory(null)
    }
  } else {
    ui.setActiveDirectory(node.id)
  }
  
  if (route.path !== '/') {
    router.push('/')
  }
}

function handleSmartAlbumClick(albumId: string) {
  ui.setSmartAlbum(albumId as any)
  if (route.path !== '/') {
    router.push('/')
  }
}

function showAll() {
  ui.setSmartAlbum('all')
  ui.setActiveDirectory(null)
}

// ── Context Menu and Folder Creation ───────────────────────────────────────
const treeContextMenu = ref({
  visible: false,
  x: 0,
  y: 0,
  activeNode: null as any,
  items: [] as ContextMenuItem[]
})

const folderCreateDialog = ref({
  isOpen: false,
  basePath: ''
})

function onNodeContextMenu(event: MouseEvent, node: any) {
  treeContextMenu.value.activeNode = node
  treeContextMenu.value.items = [
    {
      id: 'new_subfolder',
      label: '新建子文件夹',
      icon: markRaw(FolderPlus),
      action: () => {
        folderCreateDialog.value.basePath = node.absPath || ''
        folderCreateDialog.value.isOpen = true
      }
    }
  ]
  treeContextMenu.value.x = event.clientX
  treeContextMenu.value.y = event.clientY
  treeContextMenu.value.visible = true
}

function createNewGlobalFolder() {
  folderCreateDialog.value.basePath = ''
  folderCreateDialog.value.isOpen = true
}

async function onFolderCreated() {
  // 刷新左侧树
  await scan.loadScanRoots()
  folderTree.loadRoots(scan.scanRoots)
}

// ── Thumbnail Gen Controls ──────────────────────────────────────────────────
const thumbGenStartTime = ref<number | null>(null)
const thumbGenElapsedTime = ref<number>(0)

const elapsedTimeStr = computed(() => {
  if (thumbGenElapsedTime.value === 0 && !scan.thumbGenProgress.isRunning && scan.thumbGenProgress.status !== 'completed') return ''
  const ms = thumbGenElapsedTime.value
  const secs = Math.floor(ms / 1000)
  const m = Math.floor(secs / 60)
  const s = secs % 60
  const msPart = String(ms % 1000).padStart(3, '0')
  return `${m}m ${s}.${msPart}s`
})

let thumbTimerFrame: number | null = null

function updateThumbTimer() {
  if (thumbGenStartTime.value && scan.thumbGenProgress.isRunning) {
    thumbGenElapsedTime.value = Date.now() - thumbGenStartTime.value
    thumbTimerFrame = requestAnimationFrame(updateThumbTimer)
  }
}

watch(() => scan.thumbGenProgress.isRunning, (isRunning) => {
  if (isRunning) {
    thumbGenStartTime.value = Date.now()
    thumbGenElapsedTime.value = 0
    if (thumbTimerFrame) cancelAnimationFrame(thumbTimerFrame)
    thumbTimerFrame = requestAnimationFrame(updateThumbTimer)
  } else {
    if (thumbTimerFrame) {
      cancelAnimationFrame(thumbTimerFrame)
      thumbTimerFrame = null
    }
    if (thumbGenStartTime.value) {
      thumbGenElapsedTime.value = Date.now() - thumbGenStartTime.value
    }
  }
})

function toggleThumbGen() {
  if (scan.thumbGenProgress.isRunning) {
    scan.stopFullThumbnailGeneration()
  } else {
    scan.startFullThumbnailGeneration()
  }
}

const isAiInitialising = ref(false)

const aiStartTime = ref<number | null>(null)
const aiElapsedTime = ref<number>(0)

const aiElapsedTimeStr = computed(() => {
  if (aiElapsedTime.value === 0 && !ai.status.isAnalyzing) return ''
  const ms = aiElapsedTime.value
  const secs = Math.floor(ms / 1000)
  const m = Math.floor(secs / 60)
  const s = secs % 60
  const msPart = String(ms % 1000).padStart(3, '0')
  return `${m}m ${s}.${msPart}s`
})

let aiTimerFrame: number | null = null

function updateAiTimer() {
  if (aiStartTime.value && ai.status.isAnalyzing) {
    aiElapsedTime.value = Date.now() - aiStartTime.value
    aiTimerFrame = requestAnimationFrame(updateAiTimer)
  }
}

watch(() => ai.status.isAnalyzing, (isAnalyzing) => {
  if (isAnalyzing) {
    aiStartTime.value = Date.now()
    aiElapsedTime.value = 0
    if (aiTimerFrame) cancelAnimationFrame(aiTimerFrame)
    aiTimerFrame = requestAnimationFrame(updateAiTimer)
  } else {
    if (aiTimerFrame) {
      cancelAnimationFrame(aiTimerFrame)
      aiTimerFrame = null
    }
    if (aiStartTime.value) {
      aiElapsedTime.value = Date.now() - aiStartTime.value
    }
  }
})

async function toggleAiAnalysis() {
  if (ai.status.isAnalyzing) {
    await ai.stopAnalysis()
  } else {
    if (isAiInitialising.value) return
    isAiInitialising.value = true
    try {
      if (!ai.status.clipLoaded) {
        await ai.initEngine()
      }
      await ai.startAnalysis()
    } finally {
      isAiInitialising.value = false
    }
  }
}

// ── Watch scan roots for live updates (NOT immediate — onMounted handles init) ─
// ── 监听扫描根目录以进行实时更新（非 immediate — onMounted 处理初始化） ─
watch(() => scan.scanRoots, (roots) => {
  // Only react to changes that happen AFTER initial mount (scan add/remove)
  // 仅对初始挂载之后发生的变化（扫描添加/删除）做出反应
  if (roots.length) folderTree.loadRoots(roots)
})

watch(() => ui.activeDirectoryId, (newId) => {
  if (newId !== null) {
    folderTree.expandToNode(newId)
  }
})

watch(() => ui.scrolledDirectoryId, (newId) => {
  if (ui.groupBy === 'folder' && newId !== null) {
    folderTree.expandToNode(newId)
  }
})
// ── Scan controls ──────────────────────────────────────────────────────────
// ── 扫描控制 ──────────────────────────────────────────────────────────

function progressPercent(rootId: number): number {
  const p = scan.getProgress(rootId)
  if (!p || !p.total || p.status === 'discovering') return 0
  return Math.round((p.scanned / p.total) * 100)
}

// ── Folder tree refresh ────────────────────────────────────────────────────
// ── 文件夹树刷新 ────────────────────────────────────────────────────
onMounted(() => {
  window.addEventListener('folder-stats-changed', () => {
    // Refresh only the expanded nodes to preserve tree state
    const expandedIds = folderTree.nodes.value.filter(n => n.expanded).map(n => n.id)
    folderTree.loadRoots(scan.scanRoots).then(() => {
      // Re-expand previously expanded nodes
      expandedIds.forEach(id => {
        const node = folderTree.nodes.value.find(n => n.id === id)
        if (node) folderTree.toggleNode(node)
      })
    })
  })
})

async function toggleScan(rootId: number) {
  const p = scan.getProgress(rootId)
  if (p?.isRunning) {
    await scan.stopScan(rootId)
  } else {
    await scan.startScan(rootId, () => {
      media.loadStats()
      folderTree.loadRoots(scan.scanRoots)
    })
  }
}

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

export interface OverlapInfo {
  id: number
  path: string
  alias: string | null
}

export interface FolderOverlapResult {
  children: OverlapInfo[]
  parents: OverlapInfo[]
}

async function addRoot() {
  try {
    const selected = await open({
      directory: true,
      multiple:  false,
      title:     t('sidebar.chooseDir'),
    })
    if (!selected) return
    const path = typeof selected === 'string' ? selected : selected[0]
    if (!path) return

    // Step 1: Check for overlaps
    const overlap = await invoke<FolderOverlapResult>('check_folder_overlap', { newPath: path })
    
    if (overlap.children.length > 0) {
      const childNames = overlap.children.map(c => c.alias || c.path).join(', ')
      const merge = await showConfirmDialog({
        title: t('sidebar.overlapDetected'),
        message: t('sidebar.overlapParentMsg', { path, children: childNames }),
        confirmText: t('sidebar.mergeAndReplace'),
        cancelText: t('sidebar.addAnyway'),
      })
      if (merge) {
        for (const child of overlap.children) {
          await invoke('remove_scan_root_with_options', {
            id: child.id, clearThumbnails: false,
          })
        }
      }
    } else if (overlap.parents.length > 0) {
      const parentNames = overlap.parents.map(p => p.alias || p.path).join(', ')
      const proceed = await showConfirmDialog({
        title: t('sidebar.overlapDetected'),
        message: t('sidebar.overlapChildMsg', { path, parents: parentNames }),
        confirmText: t('sidebar.addAnyway'),
        cancelText: t('sidebar.cancel') || '取消',
      })
      if (!proceed) return
    }

    try {
      const root = await scan.addScanRoot(path)
      
      // 立即重新加载目录树，并在左侧选中该文件夹
      await scan.loadScanRoots()
      await folderTree.loadRoots(scan.scanRoots)
      
      const targetNode = folderTree.nodes.value.find(n => n.rootId === root.id && n.parentId === null)
      if (targetNode) {
        ui.setActiveDirectory(targetNode.id)
        if (route.path !== '/') router.push('/')
      }

      await scan.startScan(root.id, () => {
        media.loadStats()
        folderTree.loadRoots(scan.scanRoots)
      })
    } catch (e) {
      ui.addToast('error', t('sidebar.addFolderFailed') + ' ' + e)
    }
  } catch (e) {
    ui.addToast('error', t('sidebar.chooseDirFailed') + ' ' + e)
  }
}

async function removeRoot(id: number) {
  const proceed = await showConfirmDialog({
    title: t('sidebar.removeFolder'),
    message: t('sidebar.confirmRemove'),
    confirmText: t('sidebar.removeFolder') || '移除',
    cancelText: t('sidebar.cancel') || '取消',
    showCheckbox: true,
    checkboxLabel: t('sidebar.clearThumbnails'),
    checkboxValue: true
  })
  if (!proceed) return

  try {
    const result = await invoke<{ cleared_count: number }>(
      'remove_scan_root_with_options',
      { id, clearThumbnails: confirmDialog.value.checkboxValue }
    )
    if (result.cleared_count > 0) {
      ui.addToast('success', t('sidebar.thumbnailsCleared', { count: result.cleared_count }))
    }

    await scan.loadScanRoots()
    media.loadStats()
    folderTree.loadRoots(scan.scanRoots)
  } catch (e) {
    ui.addToast('error', t('sidebar.removeFolderFailed') + ' ' + e)
  }
}

onMounted(async () => {
  // Sequential init: load roots first, THEN tree — no parallel races
  // 顺序初始化：先加载根目录，然后加载树 — 没有并行竞争
  await scan.loadScanRoots()
  await media.loadStats()
  if (scan.scanRoots.length) {
    await folderTree.loadRoots(scan.scanRoots)
  }
})

</script>

<style scoped>
.collapse-enter-active,
.collapse-leave-active {
  transition: height 0.3s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  overflow: hidden;
}

.sidebar {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}
.sidebar__scroll-area {
  flex: 1;
  overflow-y: overlay; /* Use overlay to prevent layout shift */
  overflow-x: hidden;
  display: flex;
  flex-direction: column;
  scrollbar-gutter: stable; /* Fallback for modern Chrome to prevent shift */
}

/* VS Code style floating scrollbar for sidebar */
.sidebar__scroll-area::-webkit-scrollbar {
  width: 6px;
  background: transparent;
}
.sidebar__scroll-area::-webkit-scrollbar-track {
  background: transparent;
}
.sidebar__scroll-area::-webkit-scrollbar-thumb {
  background: transparent;
  border-radius: 3px;
}
.sidebar__scroll-area:hover::-webkit-scrollbar-thumb {
  background: var(--color-scrollbar-thumb);
}
.sidebar__scroll-area::-webkit-scrollbar-thumb:hover {
  background: var(--color-scrollbar-thumb-hover);
}

/* ── Header ───────────────────────────────────────────────────────────── */
/* ── 头部 ───────────────────────────────────────────────────────────── */
.sidebar__header {
  padding: var(--spacing-md);
  flex-shrink: 0;
}
.sidebar__logo {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
}
.sidebar__logo-icon {
  font-size: 20px;
  color: var(--color-accent);
}
.sidebar__logo-text {
  font-size: var(--font-size-md);
  font-weight: 700;
  color: var(--color-text-primary);
  letter-spacing: -0.3px;
}

/* ── Section ─────────────────────────────────────────────────────────── */
/* ── 区块 ─────────────────────────────────────────────────────────── */
.sidebar__section {
  padding: var(--spacing-sm) 0;
  flex-shrink: 0;
}
.sidebar__section--tree {
  /* Just a regular section now, no flex constraints needed */
}
.sidebar__section-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px var(--spacing-md);
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.02em;
  color: var(--color-text-secondary);
  transition: color 0.2s, background-color 0.2s;
  cursor: pointer;
}
.sidebar__section-label:hover {
  color: var(--color-text-primary);
  background-color: var(--color-bg-hover);
}
.sidebar__divider {
  height: 1px;
  background: var(--color-border);
  margin: var(--spacing-xs) var(--spacing-md);
  flex-shrink: 0;
}
.sidebar-show-all-btn {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-tertiary);
  border: 1px solid var(--color-border);
  cursor: pointer;
  transition: all var(--transition-fast);
}
.sidebar-show-all-btn:hover {
  background: var(--color-sidebar-hover-bg, var(--color-bg-hover));
  color: var(--color-text-primary);
}
.sidebar-show-all-btn.active {
  background: var(--color-accent);
  color: #fff;
  border-color: var(--color-accent);
}

/* ── Nav items ────────────────────────────────────────────────────────── */
/* ── 导航项 ────────────────────────────────────────────────────────── */
.sidebar__nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 var(--spacing-xs);
}
.sidebar__nav--tools {
  gap: 8px;
  padding: 4px var(--spacing-xs) 8px;
}
.sidebar__nav--tools .sidebar__nav-item {
  background: var(--color-bg-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: 10px var(--spacing-sm);
  box-shadow: 0 1px 2px rgba(0,0,0,0.03);
}
.sidebar__nav--tools .sidebar__nav-item:hover {
  background: var(--color-bg-hover);
  border-color: var(--color-border-strong);
}
.sidebar__nav-item {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  width: 100%;
  padding: 6px var(--spacing-sm);
  border-radius: var(--radius-md);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  transition:
    background-color var(--transition-fast),
    color            var(--transition-fast);
  text-align: left;
}
.sidebar__nav-item:hover {
  background: var(--color-sidebar-hover-bg);
  color: var(--color-text-primary);
}
.sidebar__nav-item.active {
  background: var(--color-sidebar-active-bg);
  color: var(--color-sidebar-active-text);
  font-weight: 600;
}
.sidebar__nav-icon { font-size: 16px; width: 20px; text-align: center; }
.sidebar__nav-label { flex: 1; }
.sidebar__nav-count {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}

/* ── Folder tree ──────────────────────────────────────────────────────── */
/* ── 文件夹树 ──────────────────────────────────────────────────────── */
.sidebar__empty {
  padding: var(--spacing-md);
  font-size: var(--font-size-sm);
  color: var(--color-text-tertiary);
  text-align: center;
}
.sidebar__tree {
  padding: 0 var(--spacing-xs);
}
.sidebar__tree-item {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  height: 28px;
  border-radius: var(--radius-sm);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: background-color var(--transition-fast);
  overflow: hidden;
}
.sidebar__tree-item:hover {
  background: var(--color-sidebar-hover-bg);
  color: var(--color-text-primary);
}
.sidebar__tree-item.active {
  background: var(--color-sidebar-active-bg);
  color: var(--color-sidebar-active-text);
}
.sidebar__tree-arrow { width: 16px; font-size: 9px; color: var(--color-text-tertiary); }
.sidebar__tree-icon  { width: 18px; font-size: 14px; }
.sidebar__tree-label { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sidebar__tree-count { font-size: var(--font-size-xs); color: var(--color-text-tertiary); margin-right: 4px; }

/* ── Scan status ──────────────────────────────────────────────────────── */
/* ── 扫描状态 ──────────────────────────────────────────────────────── */
.sidebar__scan-status {
  border-top: 1px solid var(--color-border);
  padding: var(--spacing-sm) var(--spacing-md);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-xs);
  flex-shrink: 0;
}
.scan-root-item__info {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.scan-root-item__alias {
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.scan-root-item__scan-btn { font-size: 14px; }
.scan-root-item__progress {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
}
.progress-bar {
  flex: 1;
  height: 3px;
  border-radius: 2px;
  background: var(--color-border);
  overflow: hidden;
}
.progress-bar__fill {
  height: 100%;
  border-radius: 2px;
  transition: width 100ms linear;
}
.progress-bar__fill--discovering {
  width: 100%;
  background: var(--color-accent);
  animation: breathe 1.5s ease-in-out infinite;
}
@keyframes breathe {
  0%, 100% { opacity: 0.4; }
  50% { opacity: 1; }
}
.scan-root-item__count { font-size: 10px; color: var(--color-text-tertiary); white-space: nowrap; }

/* ── Footer ───────────────────────────────────────────────────────────── */
/* ── 页脚 ───────────────────────────────────────────────────────────── */
.sidebar__footer {
  border-top: 1px solid var(--color-border);
  padding: var(--spacing-sm) var(--spacing-md);
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
  gap: var(--spacing-sm);
}

.btn-danger-sm {
  font-size: var(--font-size-xs);
  color: var(--color-error, #f87171);
  opacity: 0.7;
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  transition: opacity var(--transition-fast), background var(--transition-fast);
}
.btn-danger-sm:hover {
  opacity: 1;
  background: rgba(248, 113, 113, 0.12);
}
.sidebar__tree-chevron {
  transition: transform var(--transition-fast);
  flex-shrink: 0;
  color: var(--color-text-tertiary);
}
.sidebar__tree-chevron.expanded {
  transform: rotate(90deg);
}
.sidebar__tree-chevron-spacer {
  width: 14px;
  flex-shrink: 0;
}

/* ── Custom Modal ──────────────────────────────────────────────────────── */
.custom-modal-overlay {
  position: fixed;
  inset: 0;
  z-index: 10000;
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
}
.custom-modal {
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  padding: var(--spacing-lg);
  width: 400px;
  max-width: 90vw;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.2);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}
.custom-modal__title {
  margin: 0;
  font-size: var(--font-size-md);
  font-weight: 600;
  color: var(--color-text-primary);
}
.custom-modal__message {
  margin: 0;
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  line-height: 1.5;
}
.custom-modal__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--spacing-sm);
  margin-top: var(--spacing-sm);
}
</style>
