<template>
  <div class="media-grid-wrapper">
    <div
      ref="gridRef"
      class="media-grid"
      :class="{ 'is-scrolling': isScrolling, 'selection-mode': selection.isSelectionMode }"
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
        <!-- Date separator -->
        <!-- 日期分隔符 -->
        <template v-if="row.rowType === 'separator'">
          {{ (row as any).separatorLabel }}
        </template>

        <!-- Normal row -->
        <!-- 正常行 -->
        <template v-else>
          <div
            v-for="item in (row as any).items"
            :key="item.id"
            class="media-card"
            :style="{ width: item.w + 'px', height: item.h + 'px' }"
            @click="openDetail(item.id)"
            @mousedown="onCardMouseDown($event, item.id)"
            @mouseenter="onCardMouseEnter($event, item.id)"
            @mouseup="onCardMouseUp"
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
              :cache-dir="cacheDir"
              :is-favorited="item.isFavorited"
              :is-selected="selection.selectedIds.has(item.id)"
              :is-selection-mode="selection.isSelectionMode"
              @select="onCheckboxClick(item.id)"
              @request-thumb="onRequestThumb"
              @cancel-thumb="onCancelThumb"
              @favorite="handleFavorite"
            />
          </div>
        </template>
      </div>
    </div>
  </div>

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
    <!-- Batch selection bar: shows when items are selected -->
    <!-- 批量选择栏：有项目被选中时显示 -->
    <Transition name="batch-bar">
      <div v-if="selection.isSelectionMode" class="batch-bar">
        <span class="batch-bar__count">已选 {{ selection.selectionCount }} 张</span>
        <div class="batch-bar__actions">
          <button class="batch-bar__btn" @click="selection.favoriteSelected(true)" title="收藏选中项目">
            ♥ 收藏
          </button>
          <button class="batch-bar__btn" @click="selection.favoriteSelected(false)" title="取消收藏">
            ♡ 取消收藏
          </button>
          <button class="batch-bar__btn batch-bar__btn--danger" @click="selection.deleteSelected()" title="删除选中项目">
            🗑 删除
          </button>
          <button class="batch-bar__btn batch-bar__btn--ghost" @click="selection.clearSelection()" title="取消选择">
            ✕ 取消
          </button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, computed } from 'vue'
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
import { useSelectionStore }   from '../../stores/selectionStore'

import MediaThumb from './MediaThumb.vue'
import { ImageIcon } from '@lucide/vue'
import type { LayoutRow } from '../../types/layout'
import { DEFAULTS, SEPARATOR_HEIGHT } from '../../constants/defaults'
import { IPC, EVENTS } from '../../constants/ipc'

import { scrollCache } from '../../utils/scrollCache'

const GAP = DEFAULTS.GRID_GAP

const media  = useMediaStore()
const ui     = useUiStore()
const filter = useFilterStore()
const queue  = useRequestQueue()
const selection = useSelectionStore()
const { t }  = useI18n()

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
let scrollTimeout: ReturnType<typeof setTimeout> | null = null

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
  if (scrollTimeout !== null) clearTimeout(scrollTimeout)
  scrollTimeout = setTimeout(() => {
    isScrolling.value = false
    if (gridRef.value) {
      scrollCache.set(getViewKey(), gridRef.value.scrollTop)
    }
  }, 150)
}

function openDetail(id: number) {
  if (selection.isSelectionMode) {
    selection.toggleSelection(id)
    return
  }
  // Store scroll position before opening viewer
  if (gridRef.value) {
    scrollCache.set(String(ui.activeDirectoryId || ui.activeSmartAlbum), gridRef.value.scrollTop)
  }
  media.openDetail(id)
}

// ── Slide-to-select logic ────────────────────────────────────────────────
let isDraggingSelection = false
let dragStartId: number | null = null
let dragSelectState = true // true = selecting, false = deselecting

function onCheckboxClick(id: number) {
  selection.toggleSelection(id)
}

function onCardMouseDown(e: MouseEvent, id: number) {
  if (e.button !== 0) return // Only left click

  if (e.ctrlKey || e.metaKey || e.shiftKey) {
    selection.toggleSelection(id)
    return
  }

  // 立即决定拖拽方向（选中 or 取消）并选中起始卡片 /
  // Immediately determine drag direction and toggle the starting card.
  dragSelectState = !selection.selectedIds.has(id)
  if (dragSelectState) selection.selectItem(id)
  else selection.deselectItem(id)

  isDraggingSelection = true
  dragStartId = null  // 起始卡片已处理，无需在 mouseenter 重复处理 / starting card done, no re-process needed
}

function onCardMouseEnter(e: MouseEvent, id: number) {
  // e.buttons bitmask: 1 = Left button held. Stop dragging if released.
  // e.buttons 位模板：1 = 左键按下。如果已松开则停止拖拽。
  if ((e.buttons & 1) === 0) {
    isDraggingSelection = false
    return
  }

  if (!isDraggingSelection) return

  // 划过任意一张卡片即可触发 / Any card entered while dragging gets toggled.
  if (dragSelectState) selection.selectItem(id)
  else selection.deselectItem(id)
}

function onCardMouseUp() {
  isDraggingSelection = false
  dragStartId = null
}

// Global mouse up to catch drags outside
onMounted(() => {
  window.addEventListener('mouseup', onCardMouseUp)
})
onBeforeUnmount(() => {
  window.removeEventListener('mouseup', onCardMouseUp)
})
// ─────────────────────────────────────────────────────────────────────────

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

// 网格内容的左右内边距 (px)，用于覆盖 scale(1.06) 约 6px 的溢出量
// Horizontal padding for the grid content to absorb scale(1.06) bleed
const GRID_PADDING = 12

const { compute, onResize } = useJustifiedLayout(() => containerWidth.value - GRID_PADDING * 2)

onMounted(async () => {
  // Get cache dir from Tauri
  // 从 Tauri 获取缓存目录
  const dir = await appDataDir()
  cacheDir.value = (await join(dir, 'cache')).replace(/\\/g, '/')

  // Read container width immediately
  // 立即读取容器宽度
  if (gridRef.value) {
    containerWidth.value = gridRef.value.clientWidth
  } else {
    console.warn('[MediaGrid] onMounted: gridRef is null!')
  }

  resizeObserver = new ResizeObserver(entries => {
    const w = entries[0].contentRect.width
    // Ignore sub-pixel changes (often caused by scrollbar rendering glitches)
    // 忽略亚像素更改（通常由滚动条渲染故障引起）
    if (w > 0 && Math.abs(w - containerWidth.value) > 1) {
      containerWidth.value = w
      onResize(w - GRID_PADDING * 2)
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

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
})

// ── Detail ─────────────────────────────────────────────────────────────────
// ── 详情 ─────────────────────────────────────────────────────────────────

async function handleFavorite(id: number) {
  const newVal = await media.toggleFavorite(id)
  // 同步更新可见行中的收藏状态 / sync isFavorited in visible rows
  for (const row of visibleRows.value) {
    if ((row as any).items) {
      const item = (row as any).items.find((it: any) => it.id === id)
      if (item) {
        item.isFavorited = newVal
        break
      }
    }
  }
  // 收藏视图中切换后需移除该项，触发重新布局 / remove item from favorites view
  if (ui.activeSmartAlbum === 'favorites') {
    await compute()
  }
}

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

// 5.1: When detail overlay closes, restore grid scroll position
// 5.1: 详情覆盖层关闭时，恢复网格滚动位置
watch(
  () => media.isDetailOpen,
  (open) => {
    if (!open && gridRef.value) {
      // Defer by one frame so the overlay animation doesn't steal focus
      // 延迟一帧，避免覆盖层动画干扰焦点
      requestAnimationFrame(() => {
        if (gridRef.value) {
          const saved = scrollCache.get(getViewKey()) || 0
          gridRef.value.scrollTop = saved
        }
      })
    }
  }
)
</script>

<style scoped>
.media-grid-wrapper {
  position: relative;
  height: 100%;
  width: 100%;
}

.media-grid {
  height: 100%;
  overflow-y: scroll;
  overflow-x: hidden;
  padding: 0;
  position: relative;
  overflow-anchor: none;
}

.media-grid.is-scrolling .media-card {
  pointer-events: none !important;
}

.media-grid__content {
  position: relative;
  width: 100%;
  /* 左右内边距：覆盖 scale(1.06) 约 6px 的溢出量，避免悬停时卡片被裁剪 */
  /* Horizontal padding: absorbs ~6px bleed from scale(1.06) hover effect */
  padding: 0 12px;
  box-sizing: border-box;
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
}

.date-separator {
  display: flex;
  font-size: 16px;
  font-weight: 600;
  color: var(--color-text-primary);
  align-items: center;
  /* 与图片左边缘对齐（考虑网格内边距） / align with photo left edge */
  padding-left: calc(var(--spacing-sm) + 12px);
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
  z-index: 0;

  /* On hover-out, delay z-index reset until the scale-down finishes (220ms) */
  /* 鼠标移出时，延迟 z-index 重置直到缩放完成（220毫秒） */
  transition:
    transform 220ms cubic-bezier(0.34, 1.18, 0.64, 1),
    box-shadow 220ms ease,
    z-index 0ms 220ms;
}

.media-grid:not(.selection-mode) .media-card:hover {
  z-index: 10;
  transform: scale(1.02);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  /* On hover-in, apply z-index immediately (no delay) */
  /* 鼠标悬停时，立即应用 z-index（无延迟） */
  transition:
    transform 220ms cubic-bezier(0.34, 1.18, 0.64, 1),
    box-shadow 220ms ease,
    z-index 0ms 0ms;
}
/* 选择模式：禁用放大效果、重置已放大的卡片、光标改为默认 */
/* Selection mode: disable scale, reset any elevated card, use default cursor */
.media-grid.selection-mode .media-card {
  transform: none !important;
  box-shadow: none !important;
  cursor: default;
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

/* ── 批量操作栏 / Batch operations bar ──────────────────────────────────── */
.batch-bar {
  position: absolute;
  bottom: 60px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 20;
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  padding: 10px 20px;
  background: color-mix(in srgb, var(--color-bg-surface) 92%, transparent);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: 1px solid var(--color-border);
  border-radius: 99px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.2);
  white-space: nowrap;
}
.batch-bar-enter-active, .batch-bar-leave-active { transition: opacity 0.2s ease, transform 0.2s ease; }
.batch-bar-enter-from, .batch-bar-leave-to { opacity: 0; transform: translateX(-50%) translateY(12px); }
.batch-bar__count { font-size: var(--font-size-sm); font-weight: 600; color: var(--color-text-primary); }
.batch-bar__actions { display: flex; align-items: center; gap: 4px; }
.batch-bar__btn {
  font-size: var(--font-size-xs);
  font-weight: 500;
  padding: 5px 12px;
  border-radius: 99px;
  border: 1px solid var(--color-border);
  background: var(--color-bg-elevated);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-fast);
  white-space: nowrap;
}
.batch-bar__btn:hover { background: var(--color-accent); border-color: var(--color-accent); color: #fff; }
.batch-bar__btn--danger:hover { background: hsl(0 70% 50%); border-color: hsl(0 70% 50%); }
.batch-bar__btn--ghost { background: transparent; border-color: transparent; }
.batch-bar__btn--ghost:hover { background: var(--color-bg-hover); border-color: var(--color-border); color: var(--color-text-primary); }

</style>
