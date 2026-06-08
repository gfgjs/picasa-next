<template>
  <div class="media-grid-layout">
    <div class="media-grid-wrapper">
      <div
        ref="gridRef"
      class="media-grid"
      :class="{ 'is-scrolling': isScrolling }"
      @scroll.passive="onGridScroll"
    >
    <!-- Empty state -->
    <!-- 空状态 -->
    <div v-if="media.totalRows === 0 && !media.isComputingLayout" class="empty-state">
      <div class="empty-state__icon"><ImageIcon :size="48" /></div>
      <div class="empty-state__title">{{ emptyStateTitle }}</div>
      <div v-if="emptyStateDesc" class="empty-state__desc">{{ emptyStateDesc }}</div>
    </div>

    <!-- Loading -->
    <!-- 加载中 -->
    <div v-if="media.isComputingLayout" class="media-grid__loading">
      <div class="spinner" />
      <span>{{ $t('empty.computing') }}</span>
    </div>

    <!-- Virtual scroll wrapper (absolute positioning) -->
    <!-- 虚拟滚动包装器 (绝对定位) -->
    <div 
      v-if="media.totalRows > 0"
      class="media-grid__content"
      :style="{ height: media.totalHeight + 'px' }"
    >
      <div
        v-for="(row, ri) in visibleRows"
        :key="row.rowType === 'separator' ? `sep-${row.y}` : `row-${row.y}`"
        :class="row.rowType === 'separator' ? 'date-separator' : 'media-grid__row'"
        :style="{
          position: 'absolute',
          top: 0,
          transform: `translate3d(0, ${(row as any).y}px, 0)`,
          willChange: 'transform',
          left: 0,
          right: 0,
          height: (row as any).height + 'px',
          gap: row.rowType === 'separator' ? undefined : GAP + 'px'
        }"
      >
        <!-- Date/Folder separator -->
        <!-- 日期/文件夹分隔符 -->
        <template v-if="row.rowType === 'separator'">
          <div class="separator-content" :style="{ position: ui.groupBy === 'folder' ? 'sticky' : 'static', top: 0, zIndex: 5 }">
            <component :is="ui.groupBy === 'folder' ? Folder : Calendar" :size="18" class="separator-icon" />
            <span class="separator-text">{{ (row as any).separatorLabel }}</span>
          </div>
        </template>

        <!-- Normal row -->
        <!-- 正常行 -->
        <template v-else>
          <div
            v-for="item in (row as any).items"
            :key="item.id"
            class="media-card"
            :data-item-id="item.id"
            :class="{ 'media-card--selection-mode': selection.isSelectionMode.value }"
            :style="{ width: item.w + 'px', height: item.h + 'px' }"
            @click="handleCardClick(item.id, $event)"
            @pointerdown="selection.onPointerDown(item.id, $event)"
            @contextmenu.prevent="onContextMenu($event, item.id)"
          >
            <MediaThumb
              :id="item.id"
              :w="item.w"
              :h="item.h"
              :media-type="item.mediaType"
              :is-live-photo="item.isLivePhoto"
              :duration-ms="item.durationMs"
              :thumb-status="item.thumbStatus"
              :thumb-path="item.thumbPath"
              :thumbhash="item.thumbhash"
              :file-format="item.fileFormat"
              :file-size="item.fileSize"
              :similarity="item.similarity"
              :is-favorited="item.isFavorited"
              :is-selected="selection.isSelected(item.id)"
              :is-selection-mode="selection.isSelectionMode.value"
              :cache-dir="cacheDir"
              @request-thumb="onRequestThumb"
              @cancel-thumb="onCancelThumb"
              @favorite="handleFavorite"
              @select="selection.toggleSelect(item.id)"
            />
          </div>
        </template>
      </div> <!-- Close v-for row -->
    </div> <!-- Close media-grid-content -->
  </div> <!-- Close media-grid -->
</div> <!-- Close media-grid-wrapper -->

  <div class="timeline-sidebar-wrapper" v-if="showTimeline">
    <div class="timeline-sidebar">
      <div v-if="media.totalRows > 0 && (media.layoutSummary?.separators || []).length > 0" class="mini-timeline">
        <div 
          v-for="sep in (media.layoutSummary?.separators || [])" 
          :key="sep.y"
          class="mini-timeline__node"
          :style="{ top: `${(sep.y / Math.max(1, media.totalHeight)) * 100}%` }"
          @click.stop="scrollToY(sep.y)"
          :title="sep.label"
        ></div>
      </div>
    </div>
  </div>

  <button 
    class="timeline-toggle-btn" 
    :class="{ 'is-open': showTimeline }"
    @click="showTimeline = !showTimeline"
    :title="showTimeline ? '隐藏时间轴' : '显示时间轴'"
  >
    <ChevronRight v-if="showTimeline" :size="16" />
    <ChevronLeft v-else :size="16" />
  </button>
  
  <ContextMenu 
    :visible="ctxMenu.visible"
    :x="ctxMenu.x"
    :y="ctxMenu.y"
    :items="ctxMenu.items"
    @update:visible="ctxMenu.visible = $event"
  />

  <!-- Selection toolbar | 选择工具栏 -->
  <SelectionToolbar 
    @batch-favorite="batchFavorite" 
    @batch-unfavorite="batchUnfavorite"
    @batch-delete="batchDelete" 
    @batch-move="startBatchMove"
    @batch-copy="startBatchCopy"
    @select-all="selection.selectAll(getAllVisibleItemIds())"
    @invert-selection="selection.invertSelection(getAllVisibleItemIds())"
  />

  <FolderTreeSelectorDialog
    v-if="moveCopyDialog.isOpen"
    :title="moveCopyDialog.mode === 'move' ? '移动到文件夹' : '复制到文件夹'"
    @close="moveCopyDialog.isOpen = false"
    @confirm="onMoveCopyConfirm"
  />

  <!-- Floating Scroll Buttons -->
  <!-- 悬浮滚动按钮 -->
    <div v-if="media.totalRows > 0" class="scroll-fab">
      <button class="fab-btn" @click="scrollGridToTop" :title="$t('empty.scrollToTop')">
        ↑
      </button>
      <button class="fab-btn" @click="scrollGridToBottom" :title="$t('empty.scrollToBottom')">
        ↓
      </button>
    </div>
  </div> <!-- Close media-grid-layout -->
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, computed, markRaw } from 'vue'
import { useScanStore } from '../../stores/scanStore'
import { useFolderTree } from '../../composables/useFolderTree'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { appDataDir, join } from '@tauri-apps/api/path'
import { useI18n } from 'vue-i18n'

import { useMediaStore } from '../../stores/mediaStore'
import { useUiStore }    from '../../stores/uiStore'
import { useFilterStore } from '../../stores/filterStore'
import { useJustifiedLayout }  from '../../composables/useJustifiedLayout'
import { useVirtualScroll }    from '../../composables/useVirtualScroll'
import { useRequestQueue }     from '../../composables/useRequestQueue'

import MediaThumb from './MediaThumb.vue'
import SelectionToolbar from './SelectionToolbar.vue'
import ContextMenu, { type ContextMenuItem } from '../common/ContextMenu.vue'
import FolderTreeSelectorDialog from '../common/FolderTreeSelectorDialog.vue'
import { ImageIcon, Heart, Trash2, X, Folder, Calendar, Copy, FolderOpen, Image as ImageIconLucide, ChevronLeft, ChevronRight, Monitor, FolderInput } from '@lucide/vue'
import { useSelection } from '../../composables/useSelection'
import type { LayoutRow } from '../../types/layout'
import { DEFAULTS, SEPARATOR_HEIGHT } from '../../constants/defaults'
import { IPC, EVENTS } from '../../constants/ipc'

import { scrollCache } from '../../utils/scrollCache'

const GAP = DEFAULTS.GRID_GAP

const ui     = useUiStore()
const media  = useMediaStore()
const scan = useScanStore()
const folderTree = useFolderTree()
const filter = useFilterStore()
const queue  = useRequestQueue()
const { t }  = useI18n()

const selection = useSelection()
const dragHoverId = ref<number | null>(null)

const emptyStateText = computed(() => {
  if (ui.isSearching) {
    return t('empty.search', { query: ui.searchQuery })
  }
  if (ui.activeDirectoryId != null) {
    return t('empty.folder')
  }
  const album = ui.activeSmartAlbum
  if (album === 'all') return t('empty.allPhotos')
  if (album === 'recent') return t('empty.recentlyAdded')
  if (album === 'favorites') return t('empty.favorites')
  if (album === 'live-photos') return t('empty.livePhotos')
  
  return t('empty.allPhotos')
})

const emptyStateTitle = computed(() => emptyStateText.value.split('\n')[0])
const emptyStateDesc = computed(() => {
  const parts = emptyStateText.value.split('\n')
  return parts.length > 1 ? parts[1] : ''
})

const gridRef  = ref<HTMLElement | null>(null)
const cacheDir = ref('')
const isScrolling = ref(false)
const showTimeline = ref(true) // Toggle for timeline
let scrollTimeout: ReturnType<typeof setTimeout> | null = null

const moveCopyDialog = ref({
  isOpen: false,
  mode: 'move' as 'move' | 'copy'
})

// ── Context Menu ───────────────────────────────────────────────────────────
const ctxMenu = ref({
  visible: false,
  x: 0,
  y: 0,
  items: [] as ContextMenuItem[],
  targetId: null as number | null
})

async function onContextMenu(e: MouseEvent, id: number) {
  e.preventDefault()
  ctxMenu.value.targetId = id
  ctxMenu.value.x = e.clientX
  ctxMenu.value.y = e.clientY

  let item = null
  for (const row of visibleRows.value) {
    if ((row as any).rowType === 'normal') {
      item = (row as any).items.find((i: any) => i.id === id)
      if (item) break
    }
  }

  const items: ContextMenuItem[] = [
    {
      id: 'copy',
      label: t('contextMenu.copyImage') || '复制图片',
      icon: markRaw(Copy),
      action: () => invoke('copy_image_to_clipboard', { itemId: id })
    },
    {
      id: 'open_explorer',
      label: t('contextMenu.showInExplorer') || '在文件夹中显示',
      icon: markRaw(FolderOpen),
      action: () => invoke(IPC.SHOW_IN_EXPLORER, { itemId: id })
    },
    {
      id: 'move_to',
      label: '移动到...',
      icon: markRaw(FolderInput),
      action: () => {
        selection.clearSelection()
        selection.toggleSelect(id)
        startBatchMove()
      }
    },
    {
      id: 'copy_to',
      label: '复制到...',
      icon: markRaw(Copy),
      action: () => {
        selection.clearSelection()
        selection.toggleSelect(id)
        startBatchCopy()
      }
    }
  ]

  // --- 桌面壁纸 (Desktop Wallpaper) ---
  if (item && item.mediaType === 'image') {
    items.push({
      id: 'set_wallpaper',
      label: t('contextMenu.setWallpaper') || '设为壁纸',
      icon: markRaw(Monitor),
      action: async () => {
        try {
          await invoke('set_as_wallpaper', { itemId: id })
          if (typeof (ui as any).showToast === 'function') {
            ;(ui as any).showToast('已设为壁纸', 'success')
          }
        } catch (err) {
          console.error(err)
        }
      }
    })
  }

  ctxMenu.value.items = items
  ctxMenu.value.visible = true
}

function scrollToY(y: number) {
  if (gridRef.value) {
    gridRef.value.scrollTo({ top: y, behavior: 'smooth' })
  }
}

// ── Virtual scroll ─────────────────────────────────────────────────────────
// ── 虚拟滚动 ─────────────────────────────────────────────────────────

function getViewKey() {
  return ui.activeDirectoryId ? `dir-${ui.activeDirectoryId}` : `album-${ui.activeSmartAlbum}`
}

const {
  visibleRows, paddingTop, paddingBottom, updateVisible, onScroll,
} = useVirtualScroll({
  totalHeight:   () => media.totalHeight,
  totalRows:     () => media.totalRows,
  fetchRowsByY:  (topY, bottomY) => media.fetchRowsByY(topY, bottomY),
  containerRef:  () => gridRef.value,
})

function onGridScroll(e: Event) {
  onScroll()
  if (!isScrolling.value) {
    isScrolling.value = true
  }
  
  if (gridRef.value) {
    const scrollTop = gridRef.value.scrollTop
    if (media.layoutSummary?.separators) {
      let activeSep = null
      for (const sep of media.layoutSummary.separators) {
        if (sep.y <= scrollTop + 100) {
          activeSep = sep
        } else {
          break
        }
      }
      if (ui.groupBy === 'folder' && activeSep && activeSep.groupId) {
        ui.scrolledDirectoryId = parseInt(activeSep.groupId, 10)
      } else {
        ui.scrolledDirectoryId = null
      }
    }
  }

  if (scrollTimeout !== null) clearTimeout(scrollTimeout)
  scrollTimeout = setTimeout(() => {
    isScrolling.value = false
    if (gridRef.value) {
      scrollCache.set(getViewKey(), gridRef.value.scrollTop)
    }
  }, 150)
}

function scrollGridToTop() {
  if (!gridRef.value) return
  gridRef.value.scrollTo({ top: 0, behavior: 'smooth' })
}

function scrollGridToBottom() {
  if (!gridRef.value) return
  gridRef.value.scrollTo({ top: gridRef.value.scrollHeight, behavior: 'smooth' })
}

// ── Layout ─────────────────────────────────────────────────────────────────
// ── 布局 ─────────────────────────────────────────────────────────────────

const containerWidth = ref(0)
let resizeObserver: ResizeObserver | null = null

const { compute, onResize } = useJustifiedLayout(() => containerWidth.value)

onMounted(async () => {
  // Get cache dir from Tauri
  // 从 Tauri 获取缓存目录
  const dir = await appDataDir()
  cacheDir.value = (await join(dir, 'cache')).replace(/\\/g, '/')

  // Read container width immediately
  // 立即读取容器宽度
  if (gridRef.value) {
    // clientWidth 包含 padding，但布局计算需要的是内部内容区域宽度。
    // 左侧 padding = var(--scrollbar-width)
    // 右侧 padding = 0
    // 所以总 padding = var(--scrollbar-width)
    const style = getComputedStyle(document.documentElement)
    const swStr = style.getPropertyValue('--scrollbar-width').trim().replace('px', '')
    const sw = parseInt(swStr) || 6
    containerWidth.value = gridRef.value.clientWidth - sw
  } else {
    console.warn('[MediaGrid] onMounted: gridRef is null!')
  }

  resizeObserver = new ResizeObserver(entries => {
    const w = entries[0].contentRect.width
    // Ignore sub-pixel changes (often caused by scrollbar rendering glitches)
    // 忽略亚像素更改（通常由滚动条渲染故障引起）
    if (w > 0 && Math.abs(w - containerWidth.value) > 1) {
      containerWidth.value = w
      onResize(w)
    }
  })
  if (gridRef.value) resizeObserver.observe(gridRef.value)

  // Initial layout compute — after width is known
  // 初始布局计算 — 在知道宽度之后

  // Consume the dirty flag BEFORE compute so we don't trigger a redundant
  // second recompute via the layoutDirty watcher (which doesn't fire on
  // mount since Vue watch is not immediate by default).
  // 在 compute 前消费 dirty 标志，避免通过 layoutDirty watcher
  // 触发多余的二次重算（Vue watch 默认非 immediate，不会在挂载时触发）。
  media.consumeLayoutDirty()

  await compute()

  updateVisible()
})

// ── Thumbnail request handling ──────────────────────────────────────────────
// ── 缩略图请求处理 ──────────────────────────────────────────────

function onCancelThumb(id: number) {
  queue.cancel(id)
}

async function onRequestThumb(id: number) {
  try {
    const result = await queue.request(id)
    // Find and patch the item in visibleRows
    // 查找并修补 visibleRows 中的项目
    for (const row of visibleRows.value) {
      if ((row as any).items) {
        const item = (row as any).items.find((it: any) => it.id === id)
        if (item) {
          item.thumbStatus = result.thumbStatus
          item.thumbPath   = result.thumbPath
          item.thumbhash   = result.thumbhash
          break
        }
      }
    }
  } catch {
    // request cancelled or failed — leave placeholder
    // 请求取消或失败 — 保留占位符
  }
}

async function handleFavorite(itemId: number) {
  // Toggle favorite and get new state | 切换收藏并获取新状态
  const newValue = await media.toggleFavorite(itemId)
  // Patch item in visibleRows for instant feedback | 修补可见行中的项以即时反馈
  for (const row of visibleRows.value) {
    if ((row as any).items) {
      const item = (row as any).items.find((it: any) => it.id === itemId)
      if (item) {
        item.isFavorited = newValue
        break
      }
    }
  }
  // In favorites view, recompute layout to remove unfavorited item
  // 在收藏视图中，重新计算布局以移除取消收藏的项
  if (ui.activeSmartAlbum === 'favorites') {
    await compute()
    updateVisible()
  }
}

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
})

// ── Detail ─────────────────────────────────────────────────────────────────
// ── 详情 ─────────────────────────────────────────────────────────────────

function handleCardClick(id: number, event: MouseEvent) {
  // If drag just ended, don't treat as click
  // 如果拖拽刚结束，不视为单击
  if (selection.wasDrag()) return

  if (event.ctrlKey || event.metaKey) {
    // Ctrl/Cmd+Click: toggle selection
    // Ctrl/Cmd+单击：切换选中
    selection.toggleSelect(id)
    return
  }

  if (selection.isSelectionMode.value && event.shiftKey) {
    // Shift+Click: range select
    // Shift+单击：范围选中
    selection.selectRange(selection.lastClickedId.value, id, getAllVisibleItemIds())
    return
  }

  // Normal mode OR normal click in selection mode: open detail
  // 普通模式 或 选择模式下的普通单击：打开详情
  media.openDetailFromLayout(id)
}

/**
 * Get all item IDs from currently visible rows, in display order.
 * 获取当前可见行中所有项目 ID，按显示顺序排列。
 */
function getAllVisibleItemIds(): number[] {
  const ids: number[] = []
  for (const row of visibleRows.value) {
    if ((row as any).items) {
      for (const item of (row as any).items) {
        ids.push(item.id)
      }
    }
  }
  return ids
}

// ── Batch operations | 批量操作 ──

async function batchFavorite() {
  const ids = Array.from(selection.selectedIds.value)
  if (ids.length === 0) return
  await invoke('batch_toggle_favorite', { itemIds: ids, value: true })
  // Update visible items | 更新可见项
  for (const row of visibleRows.value) {
    if ((row as any).items) {
      for (const item of (row as any).items) {
        if (selection.isSelected(item.id)) {
          item.isFavorited = true
        }
      }
    }
  }
  await media.loadStats()
  // Optional: check if showToast exists before calling
  if (typeof (ui as any).showToast === 'function') {
    ;(ui as any).showToast(t('selection.favorited', { count: ids.length }))
  }
  selection.clearSelection()
}

async function batchUnfavorite() {
  const ids = Array.from(selection.selectedIds.value)
  if (ids.length === 0) return
  await invoke('batch_toggle_favorite', { itemIds: ids, value: false })
  // Update visible items | 更新可见项
  for (const row of visibleRows.value) {
    if ((row as any).items) {
      for (const item of (row as any).items) {
        if (selection.isSelected(item.id)) {
          item.isFavorited = false
        }
      }
    }
  }
  await media.loadStats()
  if (typeof (ui as any).showToast === 'function') {
    ;(ui as any).showToast(`已取消收藏 ${ids.length} 项`)
  }
  selection.clearSelection()
}

async function batchDelete() {
  const ids = Array.from(selection.selectedIds.value)
  if (ids.length === 0) return
  await invoke('soft_delete_items', { itemIds: ids })
  selection.clearSelection()
  // Recompute layout to remove deleted items | 重新计算布局以移除已删除项
  await compute()
  updateVisible()
  if (typeof (ui as any).showToast === 'function') {
    ;(ui as any).showToast(t('selection.deleted', { count: ids.length }))
  }
}

function startBatchMove() {
  const ids = Array.from(selection.selectedIds.value)
  if (ids.length === 0) return
  moveCopyDialog.value.mode = 'move'
  moveCopyDialog.value.isOpen = true
}

function startBatchCopy() {
  const ids = Array.from(selection.selectedIds.value)
  if (ids.length === 0) return
  moveCopyDialog.value.mode = 'copy'
  moveCopyDialog.value.isOpen = true
}

async function onMoveCopyConfirm(targetNode: any) {
  const ids = Array.from(selection.selectedIds.value)
  if (ids.length === 0 || (!targetNode.absPath && !targetNode.relPath)) return
  
  const targetDir = targetNode.absPath || targetNode.relPath
  moveCopyDialog.value.isOpen = false
  const cmd = moveCopyDialog.value.mode === 'move' ? 'move_media_items' : 'copy_media_items'
  
  try {
    await invoke(cmd, { mediaIds: ids, targetDir })
    if (typeof (ui as any).showToast === 'function') {
      ;(ui as any).showToast(moveCopyDialog.value.mode === 'move' ? `已移动 ${ids.length} 项` : `已复制 ${ids.length} 项`, 'success')
    }
    selection.clearSelection()
    
    // For move, remove items from view immediately
    if (moveCopyDialog.value.mode === 'move') {
      await compute()
      updateVisible()
      
      // Manually decrement source node count if possible
      if (ui.activeDirectoryId) {
        const srcNode = folderTree.nodes.value.find(n => n.id === ui.activeDirectoryId)
        if (srcNode) {
          srcNode.mediaCount = Math.max(0, srcNode.mediaCount - ids.length)
        }
      }
    }
    
    // Manually increment target node count
    if (targetNode) {
      targetNode.mediaCount += ids.length
    }
    
    // Let backend trigger a scan update, but we can also manually tell store to refresh
    await media.loadStats()

    // Important: trigger background scan on target root to ingest new files into DB
    if (targetNode.rootId) {
      scan.startScan(targetNode.rootId, async () => {
        window.dispatchEvent(new CustomEvent('folder-stats-changed'))
      })
    }
  } catch (e) {
    if (typeof (ui as any).showToast === 'function') {
      ;(ui as any).showToast(`操作失败: ${e}`, 'error')
    }
  }
}

function onKeyDown(e: KeyboardEvent) {
  selection.onKeyDown(e, getAllVisibleItemIds)
}

function onFolderStatsChanged() {
  // Re-fetch gallery to show newly ingested items (if any are applicable to current view)
  compute()
  media.loadStats()
}

onMounted(() => {
  document.addEventListener('keydown', onKeyDown)
  window.addEventListener('folder-stats-changed', onFolderStatsChanged)
})
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeyDown)
  window.removeEventListener('folder-stats-changed', onFolderStatsChanged)
})

// ── Listen to enrichment events ────────────────────────────────────────────
// ── 监听增强事件 ────────────────────────────────────────────

let unlistenEnriched: UnlistenFn | null = null
let enrichedDebounceTimer: ReturnType<typeof setTimeout> | null = null

onMounted(async () => {
  unlistenEnriched = await listen(EVENTS.MEDIA_ENRICHED, () => {
    // Enrichment fires once per 500-item batch — debounce so we recompute
    // at most once every 2s during active enrichment instead of per-batch.
    // 数据增强每处理 500 项触发一次批处理 — 进行防抖处理，以便在活跃的增强期间
    // 最多每 2 秒重新计算一次，而不是在每次批处理之后都计算。
    if (enrichedDebounceTimer !== null) clearTimeout(enrichedDebounceTimer)
    enrichedDebounceTimer = setTimeout(async () => {
      enrichedDebounceTimer = null

      await compute()
      updateVisible()
    }, 2000)
  })
})

onBeforeUnmount(() => {
  unlistenEnriched?.()
  if (enrichedDebounceTimer !== null) clearTimeout(enrichedDebounceTimer)
})

// When totalItems changes (scan complete / clear data), recompute and refresh
// 当 totalItems 发生变化（扫描完成 / 清除数据）时，重新计算并刷新
watch(
  () => media.totalItems,
  async (newVal, oldVal) => {

    if (containerWidth.value < 100) return
    await compute()
    // updateVisible will be called by the layoutVersion watch below
    // updateVisible 将被下方的 layoutVersion watch 调用
  }
)

// When layout is marked dirty (e.g. after full thumbnail regeneration completes
// while the grid is already mounted), recompute from DB.
// 当布局被标记为脏时（例如全量缩略图生成完成且网格已挂载），从数据库重新计算。
watch(
  () => media.layoutDirty,
  async (dirty) => {
    if (!dirty) return
    if (containerWidth.value < 100) return
    media.consumeLayoutDirty()
    await compute()
    // updateVisible is handled by the layoutVersion watcher below
    // updateVisible 由下方的 layoutVersion watcher 处理
  }
)

// When layout changes (due to resize, folder switch, filters, etc.), refresh visible rows
// 当布局发生变化时（由于调整大小、文件夹切换、过滤器等原因），刷新可见的行
watch(
  () => media.layoutVersion,
  async () => {

    // Wait for the DOM to allow setting scrollTop before layout renders
    // 等待 DOM，允许在布局渲染之前设置 scrollTop
    if (gridRef.value) {
      const saved = scrollCache.get(getViewKey()) || 0
      gridRef.value.scrollTop = saved
    }
    updateVisible(true)
  }
)

// ── Pending scroll (e.g. from sidebar folder click) ────────────────────────
async function scrollToLabel(label: string) {
  try {
    const y = await invoke<number | null>(IPC.GET_SEPARATOR_Y_BY_LABEL, { label })
    if (y !== null && gridRef.value) {
      const targetY = Math.max(0, y)
      gridRef.value.scrollTo({ top: targetY, behavior: 'smooth' })
      scrollCache.set(getViewKey(), targetY)
    }
  } catch (e) {
    console.error('Failed to get separator y:', e)
  } finally {
    ui.pendingScrollLabel = null
  }
}

watch(() => ui.pendingScrollLabel, async (label) => {
  if (!label) return
  if (media.isComputingLayout) {
    const unwatch = watch(() => media.isComputingLayout, async (isComp) => {
      if (!isComp) {
        unwatch()
        // Wait briefly so layoutVersion watcher can apply its default scroll
        setTimeout(async () => {
          await scrollToLabel(label)
        }, 50)
      }
    })
  } else {
    await scrollToLabel(label)
  }
})

</script>

<style scoped>
.media-grid-layout {
  display: flex;
  flex-direction: row;
  width: 100%;
  height: 100%;
  overflow: hidden;
  position: relative;
}

.media-grid-wrapper {
  position: relative;
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.media-grid {
  flex: 1;
  min-height: 0;
  overflow-y: scroll;
  overflow-x: hidden;
  padding-left: var(--scrollbar-width, 6px);
  padding-right: 0;
  position: relative;
  overflow-anchor: none;
}

.media-grid.is-scrolling .media-card {
  pointer-events: none !important;
}

.media-grid__content {
  position: relative;
  width: 100%;
}

.media-grid__loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-xl);
  color: var(--color-text-tertiary);
}

.media-grid__row {
  display: flex;
  flex-wrap: nowrap;
  /* overflow must be visible so hover-scaled cards can bleed outside the row */
  /* 溢出部分必须可见，这样悬停时缩放的卡片就可以超出该行的边界 */
  overflow: visible;
  z-index: 2;
  transition: z-index 220ms linear;
}

.media-grid__row:hover {
  z-index: 100;
  transition: z-index 0ms;
}

.date-separator {
  display: flex;
  font-size: 16px;
  font-weight: 600;
  color: var(--color-text-primary);
  align-items: center;
  padding-left: 0;
  background: var(--color-bg-primary);
}

.separator-content {
  display: flex;
  align-items: center;
  gap: 8px;
  background: rgba(var(--color-bg-primary-rgb, 255, 255, 255), 0.85);
  backdrop-filter: blur(8px);
  padding: 4px 12px;
  border-radius: var(--radius-md);
  margin-top: 4px;
}

.separator-icon {
  color: var(--color-text-secondary);
}

.timeline-sidebar-wrapper {
  position: relative;
  display: flex;
  flex-direction: row;
  height: 100%;
}

.timeline-toggle-btn {
  position: absolute;
  right: 0px;
  top: 16px;
  width: 24px;
  height: 24px;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-right: none;
  border-radius: 12px 0 0 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  z-index: 60;
  color: var(--color-text-secondary);
  box-shadow: -2px 0 4px rgba(0,0,0,0.1);
  transition: right var(--transition-fast), color var(--transition-fast), background var(--transition-fast);
}
.timeline-toggle-btn.is-open {
  right: 24px;
}

.timeline-toggle-btn:hover {
  color: var(--color-accent);
  background: var(--color-bg-hover);
}

.timeline-sidebar {
  width: 24px;
  height: 100%;
  background: var(--color-bg-primary);
  border-left: 1px solid var(--color-border);
  position: relative;
  flex-shrink: 0;
  display: flex;
  justify-content: center;
}

.mini-timeline {
  position: absolute;
  left: 0;
  right: 0;
  top: 10px;
  bottom: 10px;
  background: transparent;
  pointer-events: none;
}

.mini-timeline__node {
  position: absolute;
  left: 50%;
  width: 6px;
  height: 6px;
  background: var(--color-text-tertiary);
  border-radius: 50%;
  transform: translate(-50%, -50%);
  pointer-events: auto;
  cursor: pointer;
  transition: transform var(--transition-fast), background var(--transition-fast);
  opacity: 0.8;
}

.mini-timeline__node:hover {
  transform: translate(-50%, -50%) scale(2);
  background: var(--color-accent);
  opacity: 1;
}

.media-card {
  position: relative;
  /* shape clipping is handled inside .media-thumb; keep card visible */
  /* 形状剪裁在 .media-thumb 内部处理；保持卡片可见 */
  overflow: visible;
  border-radius: 2px;
  cursor: pointer;
  flex: 0 0 auto;
  box-sizing: border-box;

  /* base: sits behind neighbours */
  /* 基础状态: 位于相邻元素之后 */
  z-index: 2;

  /* On hover-out, use linear z-index interpolation to smoothly drop it */
  /* 鼠标移出时，使用线性的 z-index 插值，使其平滑下降，避免被邻居瞬间遮挡，也能保证新 hovered 项在最前 */
  transition:
    transform 220ms cubic-bezier(0.34, 1.18, 0.64, 1),
    box-shadow 220ms ease,
    z-index 220ms linear;
}

.media-card:hover {
  transform: scale(1.06);
  z-index: 100;
  box-shadow: 0 8px 28px color-mix(in srgb, var(--color-text-primary) 15%, transparent),
              0 2px 8px color-mix(in srgb, var(--color-text-primary) 5%, transparent);

  /* On hover-in, apply z-index immediately (no delay) */
  /* 鼠标悬停时，立即应用 z-index（无延迟） */
  transition:
    transform 220ms cubic-bezier(0.34, 1.18, 0.64, 1),
    box-shadow 220ms ease,
    z-index 0ms;
}

.scroll-fab {
  position: absolute;
  bottom: 32px;
  right: 32px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  z-index: 100;
}

.fab-btn {
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  color: var(--color-text-primary);
  font-size: 20px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 4px 12px rgba(0,0,0,0.2);
  transition: transform 0.2s cubic-bezier(0.34, 1.18, 0.64, 1), background 0.2s, box-shadow 0.2s;
  opacity: 0.8;
}

.fab-btn:hover {
  transform: scale(1.1);
  background: var(--color-bg-hover);
  opacity: 1;
  box-shadow: 0 6px 16px rgba(0,0,0,0.3);
}

.fab-btn:active {
  transform: scale(0.95);
}

/* Selection mode: suppress hover scale to avoid visual conflict with selection overlay */
/* 选择模式：抑制悬停缩放以避免与选择遮罩的视觉冲突 */
.media-card--selection-mode:hover {
  transform: none;
  box-shadow: none;
}

/* Slide-down transition for toolbar is handled in SelectionToolbar.vue */

</style>
