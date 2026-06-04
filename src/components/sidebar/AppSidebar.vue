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

    <!-- Smart albums -->
    <!-- 智能相册 -->
    <section class="sidebar__section">
      <div class="sidebar__section-label">{{ $t('sidebar.library') }}</div>
      <ul class="sidebar__nav">
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
    </section>

    <!-- Divider -->
    <!-- 分隔线 -->
    <div class="sidebar__divider" />

    <!-- Tools -->
    <section class="sidebar__section">
      <div class="sidebar__section-label">工具 / TOOLS</div>
      <ul class="sidebar__nav">
        <li>
          <div class="sidebar__nav-item" style="flex-direction: column; align-items: flex-start; gap: 8px; cursor: default;">
            <div style="display: flex; align-items: center; justify-content: space-between; width: 100%;">
              <div style="display: flex; align-items: center; gap: 8px;">
                <span class="sidebar__nav-icon"><Zap :size="18" /></span>
                <span class="sidebar__nav-label">全量生成缩略图</span>
              </div>
              <button class="btn-icon" @click="toggleThumbGen" :title="scan.thumbGenProgress.isRunning ? '停止生成' : '开始生成'">
                <Square v-if="scan.thumbGenProgress.isRunning" :size="14" />
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
        <li>
          <div class="sidebar__nav-item" style="flex-direction: column; align-items: flex-start; gap: 8px; cursor: default;">
            <div style="display: flex; align-items: center; justify-content: space-between; width: 100%;">
              <div style="display: flex; align-items: center; gap: 8px;">
                <span class="sidebar__nav-icon"><Sparkles :size="18" /></span>
                <span class="sidebar__nav-label">全量 AI 分析</span>
              </div>
              <button class="btn-icon" @click="toggleAiAnalysis" :disabled="isAiInitialising" :title="ai.status.isAnalyzing ? '停止分析' : '开始分析'">
                <Square v-if="ai.status.isAnalyzing" :size="14" />
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
                <span style="font-family: monospace;">{{ ai.analyzeProgress }}%</span>
              </div>
            </div>
          </div>
        </li>
      </ul>
    </section>

    <!-- Divider -->
    <div class="sidebar__divider" />

    <!-- Scan roots / folder tree -->
    <!-- 扫描根目录 / 文件夹树 -->
    <section class="sidebar__section sidebar__section--tree">
      <div class="sidebar__section-label">
        <span>{{ $t('sidebar.folders') }}</span>
        <div style="display: flex; align-items: center; gap: 4px;">
          <!-- 显示全部按钮 | Show-all button to clear folder selection -->
          <button
            class="btn-icon sidebar__show-all-btn"
            :class="{ active: !ui.activeDirectoryId && ui.activeSmartAlbum === 'all' }"
            :title="$t('sidebar.allPhotos') + ' (显示全部)'" 
            @click="showAll"
          >{{ $t('sidebar.allPhotos').slice(0, 2) || '全部' }}</button>
          <button class="btn-icon" :title="$t('sidebar.addFolder')" @click="addRoot"><FolderPlus :size="16" /></button>
        </div>
      </div>

      <div v-if="folderTree.nodes.value.length === 0 && !scan.hasScanRoots" class="sidebar__empty">
        <span>{{ $t('sidebar.noFolders') }}</span>
      </div>

      <div class="sidebar__tree" v-if="folderTree.nodes.value.length > 0">
        <button
          v-for="node in folderTree.nodes.value"
          :key="node.id"
          class="sidebar__tree-item"
          :class="{
            active:    ui.activeDirectoryId === node.id,
            expanded:  node.expanded,
          }"
          :style="{ paddingLeft: (node.depth * 16 + 8) + 'px' }"
          @click="onNodeClick(node)"
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
    </section>

    <!-- Scan roots status -->
    <!-- 扫描根目录状态 -->
    <div v-if="scan.hasScanRoots" class="sidebar__scan-status">
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
              <Square v-if="scan.getProgress(root.id)?.isRunning" :size="14" />
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

    <!-- Settings / footer -->
    <!-- 设置 / 页脚 -->
    <div class="sidebar__footer">
      <router-link to="/settings" class="btn-icon" :title="$t('sidebar.settings')" style="text-decoration: none;"><Settings :size="18" /></router-link>
      <button class="btn-icon" :title="$t('sidebar.toggleTheme')" @click="ui.cycleTheme()">
        <Sun v-if="ui.theme === 'dark'" :size="18" />
        <Moon v-else-if="ui.theme === 'light'" :size="18" />
        <Monitor v-else :size="18" />
      </button>
    </div>

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
import {
  Aperture, FolderPlus, ChevronRight, Folder,
  Square, RefreshCw, Trash2, Settings,
  Sun, Moon, Monitor, ImageIcon, Heart, Sparkles, Clock, Play, Zap
} from '@lucide/vue'

const ui       = useUiStore()
const scan     = useScanStore()
const media    = useMediaStore()
const ai       = useAiStore()
const folderTree = useFolderTree()
const router   = useRouter()
const route    = useRoute()
const { t }    = useI18n()



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
  ui.setActiveDirectory(node.id)
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

// 显示全部：清空文件夹选择，切换到全量照片视图 | Show all: clear folder selection, switch to all photos
function showAll() {
  ui.setSmartAlbum('all')
  ui.setActiveDirectory(null)
  if (route.path !== '/') {
    router.push('/')
  }
}

// ── Thumbnail Gen Controls ──────────────────────────────────────────────────
const thumbGenStartTime = ref<number | null>(null)
const thumbGenElapsedTime = ref<number>(0)

const elapsedTimeStr = computed(() => {
  if (thumbGenElapsedTime.value === 0 && !scan.thumbGenProgress.isRunning && scan.thumbGenProgress.status !== 'completed') return ''
  const secs = Math.floor(thumbGenElapsedTime.value / 1000)
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${m}m ${s}s`
})

let timerInterval: number | null = null

watch(() => scan.thumbGenProgress.isRunning, (isRunning) => {
  if (isRunning) {
    thumbGenStartTime.value = Date.now()
    thumbGenElapsedTime.value = 0
    if (timerInterval) clearInterval(timerInterval)
    timerInterval = window.setInterval(() => {
      if (thumbGenStartTime.value) {
        thumbGenElapsedTime.value = Date.now() - thumbGenStartTime.value
      }
    }, 1000)
  } else {
    if (timerInterval) {
      clearInterval(timerInterval)
      timerInterval = null
      if (thumbGenStartTime.value) {
        // Final exact time calculation
        thumbGenElapsedTime.value = Date.now() - thumbGenStartTime.value
      }
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

// ── Scan controls ──────────────────────────────────────────────────────────
// ── 扫描控制 ──────────────────────────────────────────────────────────

function progressPercent(rootId: number): number {
  const p = scan.getProgress(rootId)
  if (!p || !p.total || p.status === 'discovering') return 0
  return Math.round((p.scanned / p.total) * 100)
}

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
  if (!confirm(t('sidebar.confirmRemove'))) return
  try {
    await scan.removeScanRoot(id)
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
.sidebar {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
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
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.sidebar__section-label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px var(--spacing-md);
  font-size: var(--font-size-xs);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--color-text-tertiary);
}
.sidebar__divider {
  height: 1px;
  background: var(--color-border);
  margin: var(--spacing-xs) var(--spacing-md);
  flex-shrink: 0;
}

/* ── Nav items ────────────────────────────────────────────────────────── */
/* ── 导航项 ────────────────────────────────────────────────────────── */
.sidebar__nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 var(--spacing-xs);
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
  flex: 1;
  overflow-y: auto;
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

/* 显示全部按钮 | Show-all button */
.sidebar__show-all-btn {
  font-size: var(--font-size-xs);
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  color: var(--color-text-tertiary);
  transition: color var(--transition-fast), background var(--transition-fast);
}
.sidebar__show-all-btn:hover {
  color: var(--color-text-primary);
  background: var(--color-sidebar-hover-bg);
}
.sidebar__show-all-btn.active {
  color: var(--color-accent);
  font-weight: 600;
}
</style>
