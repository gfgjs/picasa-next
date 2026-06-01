<template>
  <nav class="sidebar">
    <!-- App title + logo -->
    <div class="sidebar__header">
      <div class="sidebar__logo">
        <span class="sidebar__logo-icon">✦</span>
        <span class="sidebar__logo-text">Picasa Next</span>
      </div>
    </div>

    <!-- Smart albums -->
    <section class="sidebar__section">
      <div class="sidebar__section-label">媒体库</div>
      <ul class="sidebar__nav">
        <li v-for="album in smartAlbums" :key="album.id">
          <button
            class="sidebar__nav-item"
            :class="{ active: ui.activeSmartAlbum === album.id && !ui.activeDirectoryId }"
            @click="ui.setSmartAlbum(album.id)"
          >
            <span class="sidebar__nav-icon">{{ album.icon }}</span>
            <span class="sidebar__nav-label">{{ album.label }}</span>
            <span v-if="album.count != null" class="sidebar__nav-count">{{ formatCount(album.count) }}</span>
          </button>
        </li>
      </ul>
    </section>

    <!-- Divider -->
    <div class="sidebar__divider" />

    <!-- Scan roots / folder tree -->
    <section class="sidebar__section sidebar__section--tree">
      <div class="sidebar__section-label">
        <span>文件夹</span>
        <button class="btn-icon" title="添加文件夹" @click="addRoot">＋</button>
      </div>

      <div v-if="folderTree.nodes.value.length === 0 && !scan.hasScanRoots" class="sidebar__empty">
        <span>暂无文件夹</span>
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
            {{ node.hasChildren ? (node.expanded ? '▼' : '▶') : '　' }}
          </span>
          <span class="sidebar__tree-icon">📁</span>
          <span class="sidebar__tree-label" :title="node.relPath">{{ node.name }}</span>
          <span class="sidebar__tree-count">{{ node.mediaCount }}</span>
        </button>
      </div>
    </section>

    <!-- Scan roots status -->
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
              :title="scan.getProgress(root.id)?.isRunning ? '停止扫描' : '重新扫描'"
            >
              {{ scan.getProgress(root.id)?.isRunning ? '⏹' : '⟳' }}
            </button>
            <button
              class="btn-icon scan-root-item__scan-btn"
              style="color: var(--color-error); opacity: 0.7;"
              title="移除该文件夹"
              @click="removeRoot(root.id)"
            >
              🗑️
            </button>
          </div>
        </div>
        <div v-if="scan.getProgress(root.id)?.isRunning" class="scan-root-item__progress">
          <div class="progress-bar">
            <div
              class="progress-bar__fill progress-shimmer"
              :style="{ width: progressPercent(root.id) + '%' }"
            />
          </div>
          <span class="scan-root-item__count">
            {{ scan.getProgress(root.id)?.scanned ?? 0 }} / {{ scan.getProgress(root.id)?.total ?? 0 }}
          </span>
        </div>
      </div>
    </div>

    <!-- Settings / footer -->
    <div class="sidebar__footer">
      <button class="btn-icon" title="切换主题" @click="ui.cycleTheme()">
        {{ ui.theme === 'dark' ? '☀️' : ui.theme === 'light' ? '🌙' : '🖥️' }}
      </button>
      <!-- Dev-only: clear all data -->
      <button
        class="btn-icon btn-danger-sm"
        title="[开发] 清除所有数据"
        @click="clearAll"
      >🗑️ 清空</button>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { computed, watch, onMounted } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { useUiStore } from '../../stores/uiStore'
import { useScanStore } from '../../stores/scanStore'
import { useMediaStore } from '../../stores/mediaStore'
import { useFolderTree } from '../../composables/useFolderTree'
import type { DirNode } from '../../types/media'

const ui       = useUiStore()
const scan     = useScanStore()
const media    = useMediaStore()
const folderTree = useFolderTree()

// ── Smart albums ───────────────────────────────────────────────────────────

const smartAlbums = computed(() => [
  { id: 'all'         as const, icon: '🖼️', label: '全部',      count: media.stats?.totalItems },
  { id: 'favorites'   as const, icon: '❤️', label: '收藏',      count: media.stats?.totalFavorited },
  { id: 'live-photos' as const, icon: '✨', label: 'Live 照片', count: media.stats?.totalLivePhotos },
  { id: 'recent'      as const, icon: '🕐', label: '最近',      count: null },
  { id: 'trash'       as const, icon: '🗑️', label: '回收站',    count: media.stats?.totalDeleted },
])

function formatCount(n: number | undefined | null): string {
  if (n == null) return ''
  if (n >= 1000) return (n / 1000).toFixed(1) + 'k'
  return String(n)
}

// ── Folder tree ────────────────────────────────────────────────────────────

function onNodeClick(node: DirNode) {
  ui.setActiveDirectory(node.id)
}

// ── Watch scan roots for live updates (NOT immediate — onMounted handles init) ─
// Using immediate:true here causes a double-load: the watch fires once before
// onMounted (with empty array) and again after loadScanRoots() resolves,
// making loadRoots() called twice and duplicating folder entries.
watch(() => scan.scanRoots, (roots) => {
  // Only react to changes that happen AFTER initial mount (scan add/remove)
  if (roots.length) folderTree.loadRoots(roots)
})

// ── Scan controls ──────────────────────────────────────────────────────────

function progressPercent(rootId: number): number {
  const p = scan.getProgress(rootId)
  if (!p || !p.total) return 0
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
  const selected = await open({
    directory: true,
    multiple:  false,
    title:     '选择媒体文件夹',
  })
  if (!selected) return
  const path = typeof selected === 'string' ? selected : selected[0]
  if (!path) return
  const root = await scan.addScanRoot(path)
  await scan.startScan(root.id, () => {
    media.loadStats()
    folderTree.loadRoots(scan.scanRoots)
  })
}

async function removeRoot(id: number) {
  if (!confirm('确定要移除此文件夹吗？它的所有媒体信息将从库中删除，但不会删除本地文件。')) return
  await scan.removeScanRoot(id)
  media.loadStats()
  folderTree.loadRoots(scan.scanRoots)
}

onMounted(async () => {
  // Sequential init: load roots first, THEN tree — no parallel races
  await scan.loadScanRoots()
  await media.loadStats()
  if (scan.scanRoots.length) {
    await folderTree.loadRoots(scan.scanRoots)
  }
})

async function clearAll() {
  if (!confirm('确定清除所有数据吗？\n\n这将删除所有扫描根目录、媒体库记录、缩略图索引。\n本地文件不受影响。')) return
  try {
    await scan.clearAllData()
    folderTree.loadRoots([])
    media.loadStats()
    ui.addToast('success', '数据已清除')
  } catch (e) {
    ui.addToast('error', '清除失败: ' + e)
  }
}
</script>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

/* ── Header ───────────────────────────────────────────────────────────── */
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
.scan-root-item__count { font-size: 10px; color: var(--color-text-tertiary); white-space: nowrap; }

/* ── Footer ───────────────────────────────────────────────────────────── */
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
</style>
