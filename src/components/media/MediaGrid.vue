<template>
  <div class="media-grid-layout">
    <div class="media-grid-wrapper">
      <!-- 人物视图返回栏（问题4）：仅当处于某人物的照片视图时出现，点击或 ESC 回人物墙。 -->
      <button v-if="ui.activePersonId != null" class="person-view-bar" @click="backToPersons">
        <ChevronLeft :size="18" />
        <span class="person-view-bar__text">{{
          t('persons.backTo', { name: activePersonLabel })
        }}</span>
      </button>
      <div
        ref="gridRef"
        class="media-grid"
        :class="{ 'is-scrolling': isScrolling, 'is-compact': compactCells }"
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
          :style="{ height: spacerHeight + 'px', position: 'relative' }"
        >
          <!-- Render layer: in translated mode (>SAFE_MAX) its transform pins the -->
          <!-- visible window to the viewport; in normal mode it is a static offset. -->
          <!-- 渲染层：平移模式（>SAFE_MAX）下其 transform 把可视窗口钉到视口；普通模式下为静态偏移。 -->
          <div
            ref="layerRef"
            class="media-grid__layer"
            :style="{
              position: 'absolute',
              top: 0,
              left: 0,
              right: 0,
              willChange: 'transform',
            }"
          >
            <div
              v-for="row in visibleRows"
              :key="row.rowType === 'separator' ? `sep-${row.y}` : `row-${row.y}`"
              :class="row.rowType === 'separator' ? 'date-separator' : 'media-grid__row'"
              :style="{
                position: 'absolute',
                top: 0,
                transform: `translate3d(0, ${row.y - renderAnchor}px, 0)`,
                willChange: 'transform',
                left: 0,
                right: 0,
                height: row.height + 'px',
                gap: row.rowType === 'separator' ? undefined : GAP + 'px',
              }"
            >
              <!-- Date/Folder separator -->
              <!-- 日期/文件夹分隔符 -->
              <template v-if="row.rowType === 'separator'">
                <div
                  class="separator-content"
                  :style="{
                    position: ui.groupBy === 'folder' ? 'sticky' : 'static',
                    top: 0,
                    zIndex: 5,
                  }"
                >
                  <component
                    :is="ui.groupBy === 'folder' ? Folder : Calendar"
                    :size="18"
                    class="separator-icon"
                  />
                  <span class="separator-text">{{ row.separatorLabel }}</span>
                </div>
              </template>

              <!-- Normal row -->
              <!-- 正常行 -->
              <template v-else>
                <!-- 有意不用 v-memo(R2-3 删除):嵌套 v-for 下 memo 缓存槽按模板位置分配、被外层各行共享,
                     Vue 官方明示其在 v-for 内不生效;且原 deps 不含 item.id,grid 模式同尺寸未出图卡片
                     deps 全等时会错误复用他项 vnode(串位隐患)。MediaThumb 子组件 props 浅比较已提供等效跳渲。 -->
                <div
                  v-for="item in row.items"
                  :key="item.id"
                  class="media-card"
                  :data-item-id="item.id"
                  :class="{
                    'media-card--selection-mode': selection.isSelectionMode.value,
                    'media-card--compact': compactCells,
                    'media-card--pending-delete': isPendingDelete(item.id),
                  }"
                  :style="{ width: item.w + 'px', height: item.h + 'px' }"
                  role="button"
                  tabindex="0"
                  :aria-label="cardAriaLabel(item)"
                  :aria-pressed="selection.isSelectionMode.value ? selection.isSelected(item.id) : undefined"
                  @click="handleCardClick(item, $event)"
                  @keydown.enter.self.prevent="handleCardClick(item, $event)"
                  @keydown.space.self.prevent="handleCardClick(item, $event)"
                  @pointerdown="onCardPointerDown(item.id, $event)"
                  @contextmenu.prevent="onContextMenu($event, item.id)"
                >
                  <!-- 暂存删除标记：置灰 + 「待删除」角标，退出选择模式时统一移除（撤销可恢复）。 -->
                  <div v-if="isPendingDelete(item.id)" class="media-card__pending-badge">
                    {{ t('selection.pendingDelete') }}
                  </div>
                  <MediaThumb
                    :id="item.id"
                    :item="item"
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
                    :rating="item.rating"
                    :color-label="item.colorLabel"
                    :is-selected="selection.isSelected(item.id)"
                    :is-selection-mode="selection.isSelectionMode.value"
                    :cache-dir="cacheDir"
                    @request-thumb="onRequestThumb"
                    @cancel-thumb="onCancelThumb"
                    @favorite="handleFavorite"
                    @rate="handleRate"
                    @select="selection.toggleSelect(item.id)"
                  />
                </div>
              </template>
            </div>
            <!-- Close v-for row -->
          </div>
          <!-- Close render layer -->
        </div>
        <!-- Close media-grid-content -->
      </div>
      <!-- Close media-grid -->
    </div>
    <!-- Close media-grid-wrapper -->

    <div class="timeline-sidebar-wrapper" v-if="showTimeline">
      <div class="timeline-sidebar">
        <!-- 真·时间 scrubber（Part5 §3.3）：消费后端 monthBuckets（时间均布 + 密度热力），
             monthBuckets 空（folder/none 分组）时组件内回退到分隔符圆点。跳转经 @jump→scrollToY。 -->
        <TimelineScrubber
          v-if="
            media.totalRows > 0 &&
            ((media.layoutSummary?.monthBuckets || []).length > 0 ||
              (media.layoutSummary?.separators || []).length > 0)
          "
          :month-buckets="media.layoutSummary?.monthBuckets || []"
          :separators="media.layoutSummary?.separators || []"
          :total-height="media.totalHeight"
          :current-y="logicalScrollTop"
          @jump="scrollToY"
        />
      </div>
    </div>

    <button
      class="timeline-toggle-btn"
      :class="{ 'is-open': showTimeline }"
      @click="showTimeline = !showTimeline"
      :title="showTimeline ? t('toolbar.hideTimeline') : t('toolbar.showTimeline')"
      :aria-label="showTimeline ? t('toolbar.hideTimeline') : t('toolbar.showTimeline')"
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
      @add-to-collection="addSelectionToCollection"
      @batch-color="batchColor"
      @batch-delete="batchDelete"
      @batch-move="startBatchMove"
      @batch-copy="startBatchCopy"
      @select-all="selection.selectAll()"
      @invert-selection="selection.invertSelection()"
    />

    <FolderTreeSelectorDialog
      v-if="moveCopyDialog.isOpen"
      :title="moveCopyDialog.mode === 'move' ? t('common.moveToFolder') : t('common.copyToFolder')"
      @close="moveCopyDialog.isOpen = false"
      @confirm="onMoveCopyConfirm"
    />

    <!-- Floating Scroll Buttons -->
    <!-- 悬浮滚动按钮 -->
    <div v-if="media.totalRows > 0" class="scroll-fab">
      <button
        class="fab-btn"
        @click="scrollGridToTop"
        :title="$t('empty.scrollToTop')"
        :aria-label="$t('empty.scrollToTop')"
      >
        ↑
      </button>
      <button
        class="fab-btn"
        @click="scrollGridToBottom"
        :title="$t('empty.scrollToBottom')"
        :aria-label="$t('empty.scrollToBottom')"
      >
        ↓
      </button>
    </div>

    <!-- Floating ghost while dragging media → folder tree. Always mounted + positioned
         imperatively (transform) so per-frame moves don't re-render the grid (问题3).
         pointer-events:none so it never blocks elementFromPoint.
         拖动媒体到文件夹树时的浮动幽灵。常驻挂载、命令式定位（transform），逐帧移动不重渲染
         网格（问题3）。pointer-events:none，绝不挡住 elementFromPoint。 -->
    <Teleport to="body">
      <div ref="mediaGhostEl" class="media-drag-ghost">
        <span ref="mediaGhostBadgeEl" class="media-drag-ghost__badge">{{ t('common.move') }}</span>
        <ImageIcon :size="14" />
        <span ref="mediaGhostTextEl">{{ t('common.itemCount', { count: 0 }) }}</span>
      </div>
    </Teleport>
  </div>
  <!-- Close media-grid-layout -->
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, computed, markRaw } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { invokeIpc, ipcErrorMessage } from '../../utils/ipc'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { useMediaStore } from '../../stores/mediaStore'
import { useUiStore } from '../../stores/uiStore'
import { usePersonStore } from '../../stores/personStore'
import { useFilterStore } from '../../stores/filterStore'
import { useGalleryLayoutSource } from '../../composables/galleryLayoutSource'
import { useGridFlipReflow } from '../../composables/useGridFlipReflow'
import { useViewportDimPriority } from '../../composables/useViewportDimPriority'
import { useVirtualScroll } from '../../composables/useVirtualScroll'
import { useRequestQueue } from '../../composables/useRequestQueue'
import { useCollectionToast } from '../../composables/useCollectionToast'

import MediaThumb from './MediaThumb.vue'
import TimelineScrubber from './TimelineScrubber.vue'
import SelectionToolbar from './SelectionToolbar.vue'
import ContextMenu, { type ContextMenuItem } from '../common/ContextMenu.vue'
import FolderTreeSelectorDialog from '../common/FolderTreeSelectorDialog.vue'
import {
  ImageIcon,
  Folder,
  Calendar,
  Copy,
  FolderOpen,
  ChevronLeft,
  ChevronRight,
  Monitor,
  FolderInput,
} from '@lucide/vue'
import { useSelection, type BackendSelectionDescriptor } from '../../composables/useSelection'
import { useViewIds } from '../../composables/useViewIds'
import { useHistoryStore } from '../../stores/historyStore'
import { useMediaDragToFolder } from '../../composables/useMediaDragToFolder'
import type { LayoutRowItem } from '../../types/layout'
import type { DirNode } from '../../types/media'
import { DEFAULTS } from '../../constants/defaults'
import { IPC, EVENTS } from '../../constants/ipc'

import { scrollCache } from '../../utils/scrollCache'

const GAP = DEFAULTS.GRID_GAP

const ui = useUiStore()
const media = useMediaStore()
const person = usePersonStore()
const filter = useFilterStore()
const queue = useRequestQueue()
const { t } = useI18n()
const router = useRouter()

const selection = useSelection()
// 布局序全集 id:选区 range/全选/反选/物化 的顺序与全集来源（脱离可视 DOM）。
// 随 layoutVersion 失效重取（见下方 watcher + onMounted）。
const viewIds = useViewIds()
const history = useHistoryStore()
const collectionToast = useCollectionToast()

// ── Drag gallery media → folder tree (问题5/问题2/问题3) ──────────────────────
// The floating ghost is positioned IMPERATIVELY (transform on a template ref) so a 120fps
// drag never triggers a Vue re-render of the huge virtual grid (问题3 — the drag used to
// drop to ~30fps because each pointermove mutated a reactive ref read by this template).
// Only the hover-target dir id is reactive, and only written when it actually changes.
// 拖图的「尾随 click 抑制」统一走 selection 的拖拽标志（T5 消除 mediaWasDrag 双轨）:
// 拖图越阈 → selection.markDragMoved();尾随 click 读 selection.wasDrag() 抑制。
// ── 拖动画廊媒体 → 文件夹树（问题5/问题2/问题3） ──────────────────────────────
// 浮动幽灵以命令式定位（对模板 ref 写 transform），使 120fps 拖拽不会触发巨大虚拟网格的
// Vue 重渲染（问题3 —— 此前每次 pointermove 改动本模板读取的响应式 ref，拖拽掉到约 30fps）。
// 只有悬停目标 dir id 是响应式，且仅在真正变化时写。
const mediaGhostEl = ref<HTMLElement | null>(null)
const mediaGhostBadgeEl = ref<HTMLElement | null>(null)
const mediaGhostTextEl = ref<HTMLElement | null>(null)

// At tiny cell sizes the viewport holds ~1000 cells; enable `content-visibility`
// on each card so the browser skips rendering off-screen ones (paint/layout) and
// scrolling stays smooth. Gated to small sizes because content-visibility forces
// paint containment, which would clip the larger sizes' hover-pop (scale+shadow).
// 极小单元尺寸下视口可容纳约 1000 个单元；对每个卡片启用 `content-visibility`，
// 让浏览器跳过离屏单元的渲染（绘制/布局），保持滚动顺滑。仅在小尺寸启用：
// content-visibility 会强制 paint 包含，裁掉大尺寸下的 hover 放大+阴影外溢。
const compactCells = computed(() => ui.gridRowHeight < 100)

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

const gridRef = ref<HTMLElement | null>(null)
const cacheDir = ref('')
const isScrolling = ref(false)
const showTimeline = ref(true) // Toggle for timeline
let scrollTimeout: ReturnType<typeof setTimeout> | null = null

async function refreshCacheDir() {
  try {
    cacheDir.value = (await invokeIpc<string>(IPC.GET_THUMB_CACHE_DIR)).replace(/\\/g, '/')
  } catch (e) {
    console.error('[MediaGrid] get_thumb_cache_dir failed:', e)
  }
}

// ── Programmatic scroll guard (sidebar-click fly-over) ───────────────────────
// While true, onGridScroll suppresses the gallery→sidebar folder sync so the tree
// doesn't chase every folder flown over during a click-initiated smooth scroll.
// Cleared when the scroll settles; a safety timer also clears it in case no scroll
// event ever fires (e.g. the target equals the current position).
// ── 程序化滚动守卫（侧栏点击飞滚） ───────────────────────────────────────────
// 为 true 时 onGridScroll 抑制 画廊→侧栏 的文件夹联动，使树不追逐点击触发的平滑滚动
// 飞过的每个文件夹。滚动停稳后清除；并设安全计时器，以防完全没有滚动事件触发
// （例如目标位置等于当前位置）。
let programmaticScroll = false
let programmaticScrollSafety: ReturnType<typeof setTimeout> | null = null

function beginProgrammaticScroll() {
  programmaticScroll = true
  if (programmaticScrollSafety !== null) clearTimeout(programmaticScrollSafety)
  programmaticScrollSafety = setTimeout(() => {
    programmaticScroll = false
  }, 1500)
}

function endProgrammaticScroll() {
  programmaticScroll = false
  if (programmaticScrollSafety !== null) {
    clearTimeout(programmaticScrollSafety)
    programmaticScrollSafety = null
  }
}

const moveCopyDialog = ref({
  isOpen: false,
  mode: 'move' as 'move' | 'copy',
})

// ── Context Menu ───────────────────────────────────────────────────────────
const ctxMenu = ref({
  visible: false,
  x: 0,
  y: 0,
  items: [] as ContextMenuItem[],
  targetId: null as number | null,
})

async function onContextMenu(e: MouseEvent, id: number) {
  e.preventDefault()
  ctxMenu.value.targetId = id
  ctxMenu.value.x = e.clientX
  ctxMenu.value.y = e.clientY

  // 判别式收窄(R2-3,镜像 patchVisibleRating 示范):rowType 守卫后 items 已正确类型化。
  let item: LayoutRowItem | null = null
  for (const row of visibleRows.value) {
    if (row.rowType !== 'normal') continue
    item = row.items.find((i) => i.id === id) ?? null
    if (item) break
  }

  const items: ContextMenuItem[] = [
    {
      id: 'copy',
      label: t('contextMenu.copyImage'),
      icon: markRaw(Copy),
      action: () => invokeIpc(IPC.COPY_IMAGE_TO_CLIPBOARD, { itemId: id }),
    },
    {
      id: 'open_explorer',
      label: t('contextMenu.showInExplorer'),
      icon: markRaw(FolderOpen),
      action: () => invoke(IPC.SHOW_IN_EXPLORER, { itemId: id }),
    },
    {
      id: 'move_to',
      label: t('common.moveTo'),
      icon: markRaw(FolderInput),
      action: () => {
        selection.clearSelection()
        selection.toggleSelect(id)
        startBatchMove()
      },
    },
    {
      id: 'copy_to',
      label: t('common.copyTo'),
      icon: markRaw(Copy),
      action: () => {
        selection.clearSelection()
        selection.toggleSelect(id)
        startBatchCopy()
      },
    },
  ]

  // --- 桌面壁纸 (Desktop Wallpaper) ---
  if (item && item.mediaType === 'image') {
    items.push({
      id: 'set_wallpaper',
      label: t('contextMenu.setWallpaper'),
      icon: markRaw(Monitor),
      action: async () => {
        try {
          await invokeIpc(IPC.SET_AS_WALLPAPER, { itemId: id })
          ui.addToast('success', t('contextMenu.wallpaperSet'))
        } catch (err) {
          console.error(err)
        }
      },
    })
  }

  ctxMenu.value.items = items
  ctxMenu.value.visible = true
}

function scrollToY(y: number) {
  if (gridRef.value) {
    // `y` is a LOGICAL coordinate — map to physical for the scroll container.
    // `y` 是逻辑坐标 — 映射到物理坐标供滚动容器使用。
    gridRef.value.scrollTo({ top: logicalToPhysical(y), behavior: 'smooth' })
  }
}

// ── Virtual scroll ─────────────────────────────────────────────────────────
// ── 虚拟滚动 ─────────────────────────────────────────────────────────

function getViewKey() {
  return ui.activeDirectoryId ? `dir-${ui.activeDirectoryId}` : `album-${ui.activeSmartAlbum}`
}

const layerRef = ref<HTMLElement | null>(null)

// 容器内容区宽度（去 scrollbar）——布局重算的输入；在策略源之前声明，惰性 getter 透传。
const containerWidth = ref(0)

// 布局策略源（A1，见 plan-docs/refactor_2026/T20_T18-layout_布局策略接缝_合并设计.md）：
// 当前仅 justified 一种策略。useVirtualScroll 只依赖 source 的 totalHeight/totalRows/
// fetchRowsByY 抽象契约，与具体布局算法解耦——A2 将新增 grid 策略实现同一接口后由此切换。
const layoutSource = useGalleryLayoutSource(() => containerWidth.value)

const {
  visibleRows,
  updateVisible,
  onScroll,
  spacerHeight,
  renderAnchor,
  logicalScrollTop,
  logicalToPhysical,
} = useVirtualScroll({
  totalHeight: layoutSource.totalHeight,
  totalRows: layoutSource.totalRows,
  fetchRowsByY: layoutSource.fetchRowsByY,
  containerRef: () => gridRef.value,
  layerRef: () => layerRef.value,
  rowHeight: () => ui.gridRowHeight,
})

// ── Row-height anchor (问题1) ────────────────────────────────────────────────
// Dragging the toolbar thumbnail-size slider reflows the whole layout, so the old
// physical scrollTop (restored by the layoutVersion watcher) maps to a different
// logical position and the item the user was looking at jumps away. We capture the
// top-most visible item once per drag burst and re-scroll to it after each recompute,
// keeping it pinned on screen. Held until the burst settles to avoid drift.
// ── 行高锚点（问题1） ────────────────────────────────────────────────────────
// 拖动工具栏缩略图尺寸滑块会重排整个布局，旧的物理 scrollTop（由 layoutVersion watcher
// 恢复）映射到不同的逻辑位置，用户正在看的项会跳走。我们在每段拖动里捕获一次视口顶部的
// 项，并在每次重算后滚回它，使其钉在屏内。持有到拖动停稳，避免漂移。
let pendingAnchor: { id: number; screenOffset: number } | null = null
let anchorClearTimer: ReturnType<typeof setTimeout> | null = null

function scheduleAnchorClear() {
  if (anchorClearTimer !== null) clearTimeout(anchorClearTimer)
  anchorClearTimer = setTimeout(() => {
    pendingAnchor = null
    anchorClearTimer = null
  }, 400)
}

function captureRowHeightAnchor() {
  // Capture once per drag burst; keep the same anchor across the many recomputes a
  // single slider drag fires so the pinned item can't drift.
  // 每段拖动只捕获一次；在单次滑动触发的多次重算间保持同一锚点，避免钉住的项漂移。
  scheduleAnchorClear()
  if (pendingAnchor !== null || !gridRef.value) return
  const vTop = logicalScrollTop.value
  for (const row of visibleRows.value) {
    if (row.rowType !== 'normal') continue
    if (row.items.length && row.y + row.height > vTop) {
      pendingAnchor = { id: row.items[0].id, screenOffset: row.y - vTop }
      break
    }
  }
}

async function restoreRowHeightAnchor(): Promise<boolean> {
  if (!pendingAnchor || !gridRef.value) return false
  const anchor = pendingAnchor
  try {
    const y = await invoke<number | null>(IPC.GET_ITEM_Y_BY_ID, { itemId: anchor.id })
    if (y !== null && gridRef.value) {
      const physY = logicalToPhysical(Math.max(0, y - anchor.screenOffset))
      gridRef.value.scrollTop = physY
      scrollCache.set(getViewKey(), physY)
      return true
    }
  } catch (e) {
    console.error('[MediaGrid] restoreRowHeightAnchor failed:', e)
  }
  return false
}

// Capture before the layout recomputes (this watcher is `pre`-flush; the recompute
// in useJustifiedLayout is `post`-flush, so it runs after we've grabbed the anchor).
// 在布局重算前捕获（本 watcher 是 pre-flush；useJustifiedLayout 的重算是 post-flush，
// 因此在我们抓到锚点之后才运行）。
watch(
  () => ui.gridRowHeight,
  () => captureRowHeightAnchor(),
)

function onGridScroll() {
  onScroll()
  if (!isScrolling.value) {
    isScrolling.value = true
  }

  // Skip the gallery→sidebar folder sync while a click-initiated smooth scroll is
  // flying over folders — otherwise the tree bounces (问题3) and the clicked target
  // gets scrolled off-screen (问题2). The target is pinned in scrollToDir(); genuine
  // user scrolling resumes the sync once the scroll settles.
  // 飞滚期间跳过 画廊→侧栏 的文件夹联动，否则树会来回跳（问题3）、被点击的目标会被挤出
  // 可视区（问题2）。目标已在 scrollToDir() 中钉好；滚动停稳后恢复真实滚动的联动。
  if (!programmaticScroll && gridRef.value) {
    // separator y values are LOGICAL — compare against logicalScrollTop, not the
    // physical scrollTop (they diverge once coordinate translation is active).
    // 分隔符 y 是逻辑坐标 — 与 logicalScrollTop 比较，而非物理 scrollTop（平移激活后二者不同）。
    const scrollTop = logicalScrollTop.value
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
    // Smooth scroll has settled — re-enable the gallery→sidebar sync.
    // 平滑滚动已停稳 — 恢复 画廊→侧栏 联动。
    endProgrammaticScroll()
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

let resizeObserver: ResizeObserver | null = null

// compute / onResize 由布局策略源提供（A1 接缝）；当前为 justified 策略。
// containerWidth 已上移至 useVirtualScroll 之前声明（策略源构造需要）。
const { recompute: compute, onResize } = layoutSource

onMounted(async () => {
  // 缩略图缓存目录允许在设置中自定义；必须以后端运行时配置为准。
  await refreshCacheDir()

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

  resizeObserver = new ResizeObserver((entries) => {
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

  // 初始/重挂载时主动拉一次布局序全集（layoutVersion watcher 仅在变化时触发,挂载不触发）。
  void viewIds.ensureFresh(media.layoutVersion)

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
      if (row.rowType !== 'normal') continue
      const item = row.items.find((it) => it.id === id)
      if (item) {
        item.thumbStatus = result.thumbStatus
        item.thumbPath = result.thumbPath
        item.thumbhash = result.thumbhash
        break
      }
    }
  } catch {
    // request cancelled or failed — leave placeholder
    // 请求取消或失败 — 保留占位符
  }
}

// 可视窗口优先取尺寸已抽到 useViewportDimPriority（自包含 feature：注入 visibleRows/isScrolling
// 与 recompute/refresh，内部自持去重集 + 防抖调度，resetKey 变化即清去重集）。
useViewportDimPriority({
  visibleRows,
  isScrolling,
  recompute: compute,
  refresh: updateVisible,
  resetKey: () => [ui.activeDirectoryId, ui.activeSmartAlbum],
})

// 切视图时重置行高锚点（原与 dim-priority 去重集重置共用一个 watcher；去重集那半已随 feature 迁出）。
watch(
  () => [ui.activeDirectoryId, ui.activeSmartAlbum],
  () => {
    pendingAnchor = null
  },
)

async function handleFavorite(itemId: number) {
  // Toggle favorite and get new state | 切换收藏并获取新状态
  const newValue = await media.toggleFavorite(itemId)
  // On favoriting (not unfavoriting), prompt to add into a collection (需求7 §3.7).
  // 收藏（而非取消）时，提示加入收藏夹（需求7 §3.7）。
  if (newValue) collectionToast.showAddToCollection([itemId])
  // Patch item in visibleRows for instant feedback | 修补可见行中的项以即时反馈
  patchVisibleFavorite(new Set([itemId]), newValue)
  // In favorites view (or a system collection, which is type + is_favorited), recompute
  // layout to remove the just-unfavorited item.
  // 在收藏视图（或系统收藏夹，本质是 类型 + is_favorited）中，重算布局以移除刚取消收藏的项。
  if (ui.activeSmartAlbum === 'favorites' || ui.activeCollection?.kind === 'system') {
    await compute()
    updateVisible()
  }
}

// 乐观更新：把若干项的 rating 就地写回可见布局行，使星级即时显形。
// visibleRows 是深响应式 ref（仅可视窗口几十项，非百万级），改 item.rating 直接触发那一格
// 重渲染，无需 triggerRef —— 与收藏/缩略图回写同一机制。被 hover 单项评分与键盘批量评分共用。
function patchVisibleRating(ids: Set<number>, rating: number) {
  // 用判别字段 rowType 收窄到 LayoutRowNormal（其 items 已正确类型化），避免周边的 (row as any) 写法。
  for (const row of visibleRows.value) {
    if (row.rowType !== 'normal') continue
    for (const item of row.items) {
      if (ids.has(item.id)) item.rating = rating
    }
  }
}

// 乐观更新收藏态（镜像 patchVisibleRating）。供 in-grid 收藏 + 详情页回灌信号共用。
function patchVisibleFavorite(ids: Set<number>, isFavorited: boolean) {
  for (const row of visibleRows.value) {
    if (row.rowType !== 'normal') continue
    for (const item of row.items) {
      if (ids.has(item.id)) item.isFavorited = isFavorited
    }
  }
}

// 详情页 / 外部单项改 favorite·rating·colorLabel → 回灌画廊 visibleRows（修复:详情设色/评分/收藏
// 后画廊缩略图不刷新、需手动刷新）。根因：visibleRows 由本组件持有（经 fetchRowsByY 拉取），store 侧
// 无行缓存可改（R2-2 已删 rowCache/patchRowItem 旁路），故由 itemPatchSignal 通知本组件就地回灌 visibleRows。
// 仅做视觉回灌（幂等，与 in-grid 同步内联 patch 无冲突）；筛选剔除重算仍由各 in-grid handler 负责。
watch(
  () => media.itemPatchSignal,
  (p) => {
    if (!p) return
    const ids = new Set([p.id])
    if (p.field === 'rating') patchVisibleRating(ids, p.value as number)
    else if (p.field === 'colorLabel') patchVisibleColorLabel(ids, p.value as number)
    else if (p.field === 'isFavorited') patchVisibleFavorite(ids, p.value as boolean)
  },
)

// 缩略图 hover 快捷评分（单项）：落库 + 乐观刷新星级。
// 若「≥N 星」筛选激活且新评分跌破阈值，该项应离开视图 → 重算（镜像收藏视图取消收藏时的重算）。
async function handleRate(itemId: number, value: number) {
  await media.setRating(itemId, value)
  patchVisibleRating(new Set([itemId]), value)
  if (filter.minRating > 0 && value < filter.minRating) {
    await compute()
    updateVisible()
  }
}

// 乐观更新颜色标签（镜像 patchVisibleRating）。
function patchVisibleColorLabel(ids: Set<number>, colorLabel: number) {
  for (const row of visibleRows.value) {
    if (row.rowType !== 'normal') continue
    for (const item of row.items) {
      if (ids.has(item.id)) item.colorLabel = colorLabel
    }
  }
}

// ── 选区批量操作的公共出入口（R1-2/S4）────────────────────────────────────────

// 选区 → 后端描述符：全选走 selectAll（payload 恒定，id 物化收敛后端）;
// 语义搜索视图不可 SQL 描述（toBackendDescriptor 返 null）时回退 Explicit 物化。
function selectionDescriptor(): BackendSelectionDescriptor {
  return selection.toBackendDescriptor() ?? { kind: 'explicit', ids: selection.materializeIds() }
}

// 批量路径的乐观刷新：对选区内**可见**项打补丁。SelectAll 态不物化 id，以 isSelected
// 谓词判定（可见窗口仅几十项，逐项判定廉价）;单项路径仍用上面的 Set 版补丁函数。
function patchVisibleSelected(apply: (item: LayoutRowItem) => void) {
  for (const row of visibleRows.value) {
    if (row.rowType !== 'normal') continue
    for (const item of row.items) {
      if (selection.isSelected(item.id)) apply(item)
    }
  }
}

// 选区批量设色（SelectionToolbar 色块 / Ban 清除触发）：落库 + 乐观刷新 + 必要时重算。
// value=0 清除。镜像 batchFavorite + handleRate 的「按色筛选下改后不匹配则重算移除」。
async function batchColor(value: number) {
  if (selection.selectedCount.value === 0) return
  const n = await media.batchSetColorLabel(selectionDescriptor(), value)
  patchVisibleSelected((it) => {
    it.colorLabel = value
  })
  if (filter.colorLabel > 0 && value !== filter.colorLabel) {
    await compute()
    updateVisible()
  }
  ui.addToast(
    'success',
    value === 0 ? t('selection.colorCleared', { count: n }) : t('selection.colorSet', { count: n }),
  )
}

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
})

// ── Detail ─────────────────────────────────────────────────────────────────
// ── 详情 ─────────────────────────────────────────────────────────────────

// 画廊媒体拖拽到文件夹树（T18：整簇抽到 useMediaDragToFolder）。幽灵 DOM + ctxMenu 仍在本模板，
// 经 deps 注入其 ref；composable 持拖拽状态机与命中/落点逻辑，只回吐模板绑定的 onCardPointerDown。
const { onCardPointerDown } = useMediaDragToFolder({
  selection,
  ui,
  history,
  ctxMenu,
  ghostEl: mediaGhostEl,
  ghostBadgeEl: mediaGhostBadgeEl,
  ghostTextEl: mediaGhostTextEl,
})

// 事件类型收 MouseEvent | KeyboardEvent(R1-8):键盘激活(Enter/Space)完整复用点击语义——
// 两类事件都携带 ctrlKey/metaKey/shiftKey,Ctrl+Enter 切选中、Shift+Enter 范围选中随之免费成立。
function handleCardClick(item: LayoutRowItem, event: MouseEvent | KeyboardEvent) {
  const id = item.id
  // 拖拽（框选或拖图）刚结束 → 吞掉尾随的单击,避免「拖完又触发单击」(开详情/翻转选中)。
  // 单一标志统一判定,下次交互起手 onCardPointerDown→beginInteraction() 复位（T5）。
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
    selection.selectRange(selection.lastClickedId.value, id)
    return
  }

  // Documents open in the dedicated reader route (§5.1); audio opens the player route (§3.6);
  // other types use the detail overlay.
  // 文档进入专用阅读器路由（§5.1）；音频进入播放器路由（§3.6）；其它类型仍走详情覆盖层。
  if (item.mediaType === 'document') {
    router.push(`/doc/${id}`)
    return
  }
  if (item.mediaType === 'audio') {
    router.push(`/audio/${id}`)
    return
  }

  // Normal mode OR normal click in selection mode: open detail
  // 普通模式 或 选择模式下的普通单击：打开详情
  media.openDetail(id, true)
}

// 媒体卡的可访问名（R1-8）：行数据为内存精简刻意不含 fileName（见 types/layout.ts），
// 读屏退而报媒体类型；选中态经 aria-pressed 单独暴露。
function cardAriaLabel(item: { isLivePhoto?: boolean; mediaType?: string }): string {
  if (item.isLivePhoto) return t('media.cardLivePhoto')
  switch (item.mediaType) {
    case 'video':
      return t('media.cardVideo')
    case 'audio':
      return t('media.cardAudio')
    case 'document':
      return t('media.cardDocument')
    default:
      return t('media.cardPhoto')
  }
}

// ── Batch operations | 批量操作 ──

// 加入收藏夹（T21）：把选区交给「加入收藏夹」chips 提示（挑已有夹 / 新建）。复用收藏后同款 UX,
// prefix 传「选中 N 项」（非收藏动作，不显「已收藏」）。
async function addSelectionToCollection() {
  const ids = selection.materializeIds()
  if (ids.length === 0) return
  await collectionToast.showAddToCollection(ids, t('selection.selected', { count: ids.length }))
}

async function batchFavorite() {
  if (selection.selectedCount.value === 0) return
  // R1-2/S4：描述符直传（全选百万项 payload 恒定）;受影响计数以后端返回为准。
  const n = await invokeIpc<number>(IPC.BATCH_TOGGLE_FAVORITE, {
    selection: selectionDescriptor(),
    value: true,
  })
  patchVisibleSelected((it) => {
    it.isFavorited = true
  })
  await media.loadStats()
  ui.addToast('success', t('selection.favorited', { count: n }))
}

async function batchUnfavorite() {
  if (selection.selectedCount.value === 0) return
  const n = await invokeIpc<number>(IPC.BATCH_TOGGLE_FAVORITE, {
    selection: selectionDescriptor(),
    value: false,
  })
  patchVisibleSelected((it) => {
    it.isFavorited = false
  })
  await media.loadStats()
  ui.addToast('success', t('selection.unfavorited', { count: n }))
}

// 删除/移除后的平滑重排动画（FLIP + 淡出）已抽到 useGridFlipReflow（自包含 DOM 工具，仅依赖
// 渲染层 layerRef）。仅用于删除/移除路径，绝不挂到滚动驱动的 updateVisible（避免与虚拟滚动 +
// renderAnchor 打架）。
const { flipReflow, fadeOutCells } = useGridFlipReflow(() => layerRef.value)

// ── 暂存删除（置灰 + 退出选择时一次重排 + 撤销）─────────────────────────────────
// 用户反馈：每次删除立即重算仍闪一下。改为：删除**即落库**（进回收站）但**不立即重排**，
// 仅把项加入 pendingDeleteIds 置灰、保持选中；待**退出选择模式**（Esc/清空）时一次性
// fadeOut + FLIP 重排移除全部暂存项。撤销经 toast「撤销」chip → restore_items 恢复。
const pendingDeleteIds = ref<Set<number>>(new Set())
function isPendingDelete(id: number): boolean {
  return pendingDeleteIds.value.has(id)
}

async function batchDelete() {
  // 跳过已暂存的项（避免对同一选区重复 soft_delete / 重复 toast）。
  const ids = selection.materializeIds().filter(
    (id) => !pendingDeleteIds.value.has(id),
  )
  if (ids.length === 0) return
  // 即落库（进回收站），但不重排——仅置灰暂存，退出选择时统一重排。
  // 破坏性操作必须有失败反馈（审查 R0-5）：落库失败时绝不进暂存集（否则前端置灰、
  // 后端未删，退出选择时重排会「凭空消失」未删除的项），toast 告知后原样保留选区。
  // R1-2 注：删除仍物化 id 后以 Explicit 描述符传参——暂存置灰集/撤销闭包/FLIP 重排都
  // 需要具体 id，SelectAll 深迁移（对全选表达暂存态）属 T18 后续。
  try {
    await invokeIpc(IPC.SOFT_DELETE_ITEMS, { selection: { kind: 'explicit', ids } })
  } catch (e) {
    ui.addToast('error', t('selection.deleteFailed', { error: ipcErrorMessage(e) }))
    return
  }
  const next = new Set(pendingDeleteIds.value)
  ids.forEach((id) => next.add(id))
  pendingDeleteIds.value = next
  // 撤销 toast（6s 窗口）：点「撤销」→ restore_items 恢复。
  ui.addToast('info', t('selection.deleted', { count: ids.length }), 6000, [
    { label: t('common.undo'), onClick: () => undoDelete(ids) },
  ])
}

async function undoDelete(ids: number[]) {
  // 撤销失败同样要反馈(审查 R0-5):恢复未落库时保持暂存置灰状态不动(与后端一致),仅 toast。
  try {
    await invokeIpc(IPC.RESTORE_ITEMS, { selection: { kind: 'explicit', ids } })
  } catch (e) {
    ui.addToast('error', t('common.undoFailed', { error: ipcErrorMessage(e) }))
    return
  }
  const stillStaged = ids.some((id) => pendingDeleteIds.value.has(id))
  if (stillStaged) {
    // 仍在暂存（未退出选择）：项还在前端布局里（soft_delete 未触发重算），仅去掉置灰即可。
    // **不重算**——否则 compute() 会把其它仍 is_deleted=1 的暂存项一并移除（误伤）。
    const next = new Set(pendingDeleteIds.value)
    ids.forEach((id) => next.delete(id))
    pendingDeleteIds.value = next
  } else {
    // 已退出选择被重排移除：需重算把恢复的项带回。
    await compute()
    updateVisible()
  }
}

// 退出选择模式 → 一次性提交暂存项的重排（fadeOut 暂存项 + 幸存项 FLIP 滑入），随后清空暂存集。
async function commitPendingReflow() {
  if (pendingDeleteIds.value.size === 0) return
  const ids = Array.from(pendingDeleteIds.value)
  pendingDeleteIds.value = new Set() // 先清空，避免重入
  await fadeOutCells(ids)
  await flipReflow(async () => {
    await compute()
    updateVisible()
  })
}

watch(
  () => selection.isSelectionMode.value,
  (now, prev) => {
    // true → false 即「退出选择状态」：把暂存的删除一次性重排掉。
    if (prev && !now) commitPendingReflow()
  },
)

function startBatchMove() {
  const ids = selection.materializeIds()
  if (ids.length === 0) return
  moveCopyDialog.value.mode = 'move'
  moveCopyDialog.value.isOpen = true
}

function startBatchCopy() {
  const ids = selection.materializeIds()
  if (ids.length === 0) return
  moveCopyDialog.value.mode = 'copy'
  moveCopyDialog.value.isOpen = true
}

async function onMoveCopyConfirm(targetNode: DirNode | null) {
  const ids = selection.materializeIds()
  // targetNode 为 FolderTreeSelectorDialog 选中的 DirNode,以 id（目录 id）为落点。
  if (ids.length === 0 || targetNode?.id == null) return
  moveCopyDialog.value.isOpen = false

  // T6：经 historyStore 走 relocate_media_items / copy_media_items_db（DB 级、可撤销），
  // 与拖图落点 performMediaDrop 同一路径（DRY）。history 内部 refresh() 已重载文件夹树（实时计数）
  // + 重算网格,故不再手动 compute/loadStats/计数调整;copy 走 _db 直接建行,无需再 startScan 重扫。
  const mode = moveCopyDialog.value.mode
  try {
    const n =
      mode === 'copy'
        ? await history.copyMedia(ids, targetNode.id, `复制 ${ids.length} 项`)
        : await history.moveMedia(ids, targetNode.id, `移动 ${ids.length} 项`)
    if (n > 0) {
      // 移动后源项已离开当前视图 → 清选区;复制保留选区（项仍在原处）。
      if (mode === 'move') selection.clearSelection()
      ui.addToast(
        'success',
        mode === 'copy'
          ? t('common.copiedCount', { count: n })
          : t('common.movedCount', { count: n }),
      )
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    ui.addToast(
      'error',
      mode === 'copy'
        ? t('common.copyFailed', { error: msg })
        : t('common.moveFailed', { error: msg }),
    )
  }
}

// 人物视图（问题4）：当前人物名（人物墙已加载则取名，否则回退「人物」）。
const activePersonLabel = computed(() => {
  if (ui.activePersonId == null) return ''
  const p = person.persons.find((pp) => pp.id === ui.activePersonId)
  return p?.name || t('persons.unnamedPerson')
})

// 返回人物墙：跳回 /persons。activePersonId 保留即可（人物墙不依赖它，且使「人物」侧栏项保持高亮）。
function backToPersons() {
  if (router.currentRoute.value.path !== '/persons') router.push('/persons')
}

function onKeyDown(e: KeyboardEvent) {
  if (media.isDetailOpen) return // 大图打开时 ESC 归 MediaDetailOverlay 关预览，天然避冲突（问题4）
  // 人物视图下 ESC 返回人物墙；但若处于多选态，优先让 selection 清选区（标准相册行为）。
  if (e.key === 'Escape' && ui.activePersonId != null && !selection.isSelectionMode.value) {
    e.preventDefault()
    backToPersons()
    return
  }
  // 选择态下数字键 1-5 给选区批量评分、0 清空（标准相册快捷键）。守门:仅多选态、非输入
  // 元素、无修饰键,避免与浏览/搜索框输入/Ctrl 组合键冲突。
  if (
    selection.isSelectionMode.value &&
    !e.ctrlKey &&
    !e.metaKey &&
    !e.altKey &&
    /^[0-5]$/.test(e.key)
  ) {
    const el = e.target as HTMLElement | null
    const editing =
      el?.tagName === 'INPUT' || el?.tagName === 'TEXTAREA' || el?.isContentEditable === true
    if (!editing) {
      if (selection.selectedCount.value > 0) {
        e.preventDefault()
        const rating = Number(e.key)
        media
          .batchSetRating(selectionDescriptor(), rating)
          .then(async (n) => {
            // 乐观刷新选区星级（rating 已入布局行，可即时显形）。
            patchVisibleSelected((it) => {
              it.rating = rating
            })
            ui.addToast(
              'success',
              rating === 0
                ? t('selection.ratingCleared', { count: n })
                : t('selection.rated', { count: n, rating }),
            )
            // 「≥N 星」筛选激活且批量评分跌破阈值 → 这些项应离开视图，重算。
            if (filter.minRating > 0 && rating < filter.minRating) {
              await compute()
              updateVisible()
            }
          })
          .catch((err) => {
            ui.addToast(
              'error',
              t('selection.rateFailed', {
                error: err instanceof Error ? err.message : String(err),
              }),
            )
          })
        return
      }
    }
  }
  selection.onKeyDown(e)
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
let unlistenVolumes: UnlistenFn | null = null
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

  // 卷插拔监听（Part2 T2）：卷在线态变化 → 重算刷新 availability 徽标（离线灰显隐）。
  // 卷态变化频率极低（拔插），无需防抖，直接重算即可。
  unlistenVolumes = await listen(EVENTS.VOLUMES_CHANGED, async () => {
    if (containerWidth.value < 100) return
    await compute()
    updateVisible()
  })
})

onBeforeUnmount(() => {
  unlistenEnriched?.()
  unlistenVolumes?.()
  if (enrichedDebounceTimer !== null) clearTimeout(enrichedDebounceTimer)
})

// When totalItems changes (scan complete / clear data), recompute and refresh
// 当 totalItems 发生变化（扫描完成 / 清除数据）时，重新计算并刷新
watch(
  () => media.totalItems,
  async () => {
    if (containerWidth.value < 100) return
    await compute()
    // updateVisible will be called by the layoutVersion watch below
    // updateVisible 将被下方的 layoutVersion watch 调用
  },
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
    await refreshCacheDir()
    await compute()
    // updateVisible is handled by the layoutVersion watcher below
    // updateVisible 由下方的 layoutVersion watcher 处理
  },
)

// When layout changes (due to resize, folder switch, filters, etc.), refresh visible rows
// 当布局发生变化时（由于调整大小、文件夹切换、过滤器等原因），刷新可见的行
watch(
  () => media.layoutVersion,
  async (v) => {
    // 布局变了 → flat_ids 随之变,刷新选区用的布局序全集（与后端缓存版本对齐;version 相同则 no-op）。
    // fire-and-forget:不阻塞滚动恢复,失败自清空待下次。
    void viewIds.ensureFresh(v)
    // Wait for the DOM to allow setting scrollTop before layout renders
    // 等待 DOM，允许在布局渲染之前设置 scrollTop
    if (gridRef.value) {
      // Row-height reflow: re-anchor to the previously-viewed item (问题1). Falls back
      // to the saved physical scrollTop for ordinary layout changes (folder/filter/…).
      // 行高重排：重新锚定到之前浏览的项（问题1）。普通布局变化（文件夹/筛选/…）回退到
      // 保存的物理 scrollTop。
      const restored = await restoreRowHeightAnchor()
      if (!restored) {
        const saved = scrollCache.get(getViewKey()) || 0
        gridRef.value.scrollTop = saved
      }
    }
    updateVisible(true)
  },
)

// When the info overlay is on, lazily fetch heavy metadata (EXIF/GPS/name/path)
// for the visible items only — these fields were stripped from the resident
// layout cache (A1) and are served on demand via get_meta_for_viewport.
// 当信息浮层开启时，仅为可视项懒加载重型元数据（EXIF/GPS/名称/路径）——
// 这些字段已从常驻布局缓存剥离（A1），经 get_meta_for_viewport 按需提供。
watch(
  [visibleRows, () => ui.showThumbInfo],
  () => {
    if (!ui.showThumbInfo) return
    const ids: number[] = []
    for (const row of visibleRows.value) {
      if (row.rowType === 'normal') {
        for (const it of row.items) ids.push(it.id)
      }
    }
    if (ids.length > 0) media.ensureMeta(ids)
  },
  { immediate: true },
)

// ── Pending scroll (e.g. from sidebar folder click) ────────────────────────
// Scroll by the unique directory id (group id) — not the folder name — so that
// duplicate-named folders at different paths each land on their own separator.
// 按唯一目录 id（分组 id）滚动，而非文件夹名字：不同路径下的同名文件夹各自定位到自己的分隔符。
async function scrollToDir(dirId: number) {
  // Pin the sidebar to the clicked folder up-front and suppress fly-over sync, so the
  // tree locates/highlights the target once and keeps it visible (问题2/问题3). The
  // scrolledDirectoryId set here triggers FoldersSection's expandToNode → scrollIntoView.
  // 先把侧栏钉到被点击的文件夹并抑制飞滚联动，使树只定位/高亮目标一次并保持可见（问题2/3）。
  // 这里设置的 scrolledDirectoryId 会触发 FoldersSection 的 expandToNode → scrollIntoView。
  beginProgrammaticScroll()
  if (ui.groupBy === 'folder') ui.scrolledDirectoryId = dirId
  try {
    // Subtree-aware target: the folder's own separator if it has direct media, else its
    // first descendant subfolder that does — so clicking an "empty" parent jumps to its
    // first media-bearing child instead of doing nothing (问题1).
    // 子树感知目标：该文件夹有直接媒体则用其分隔符，否则用首个有媒体的后代子文件夹——
    // 这样点击「空」父文件夹会跳到首个含媒体的子项，而非毫无反应（问题1）。
    const target = await invoke<{ dirId: number; y: number } | null>(
      IPC.GET_SUBTREE_SCROLL_TARGET,
      { dirId },
    )
    if (target && gridRef.value) {
      // `y` is LOGICAL; scrollCache stores PHYSICAL scrollTop → map through.
      // `y` 是逻辑坐标；scrollCache 存物理 scrollTop → 需映射。
      // Landed on a descendant → highlight/expand that subfolder, not the empty parent.
      // 落到后代 → 高亮/展开该子文件夹，而非空父文件夹。
      if (ui.groupBy === 'folder' && target.dirId !== dirId) ui.scrolledDirectoryId = target.dirId
      const physY = logicalToPhysical(Math.max(0, target.y))
      gridRef.value.scrollTo({ top: physY, behavior: 'smooth' })
      scrollCache.set(getViewKey(), physY)
    } else {
      // Nothing to scroll — drop the guard immediately instead of waiting for settle.
      // 无需滚动 — 立即放下守卫，不必等待停稳。
      endProgrammaticScroll()
    }
  } catch (e) {
    console.error('Failed to get scroll target:', e)
    endProgrammaticScroll()
  } finally {
    ui.pendingScrollDirId = null
  }
}

watch(
  () => ui.pendingScrollDirId,
  async (dirId) => {
    if (dirId === null) return
    if (media.isComputingLayout) {
      const unwatch = watch(
        () => media.isComputingLayout,
        async (isComp) => {
          if (!isComp) {
            unwatch()
            // Wait briefly so layoutVersion watcher can apply its default scroll
            setTimeout(async () => {
              await scrollToDir(dirId)
            }, 50)
          }
        },
      )
    } else {
      await scrollToDir(dirId)
    }
  },
)
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

/* 人物视图返回栏（问题4）：置于网格上方、不参与虚拟滚动。 */
.person-view-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  padding: 8px var(--spacing-md);
  background: var(--color-bg-surface);
  border: none;
  border-bottom: 1px solid var(--color-border);
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  text-align: left;
  transition:
    background var(--transition-fast),
    color var(--transition-fast);
}
.person-view-bar:hover {
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
}
.person-view-bar__text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
  /* Clip the transformed render layer to this box (B1). In coordinate-translation
     mode the buffer rows are transform-positioned slightly outside [0, spacerHeight];
     without clipping they leak into the scroller's scrollable overflow, ballooning
     scrollHeight past spacerHeight — which breaks the physical↔logical scroll mapping
     (scrollbar jitter, jump-to-bottom, misalignment). Clipping pins scrollHeight to
     spacerHeight. Visible rows always fall within [0, spacerHeight], so nothing on
     screen is ever clipped; only off-viewport buffer rows are.
     裁剪被 transform 定位的渲染层到本盒子（B1）。坐标平移模式下缓冲行会被定位到
     [0, spacerHeight] 之外，不裁剪会泄漏进滚动容器的可滚动溢出区，使 scrollHeight
     超过 spacerHeight，从而破坏物理↔逻辑映射（滚动条抖动、跳到底部、错位）。
     裁剪后 scrollHeight 恒等于 spacerHeight；可视行始终落在 [0, spacerHeight] 内，
     屏幕上不会被裁剪，仅裁剪视口外的缓冲行。 */
  overflow: hidden;
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

/* compact 模式下禁用行级 z-index transition 以减少不必要的重绘 */
.is-compact .media-grid__row {
  transition: none;
}
.is-compact .media-grid__row:hover {
  z-index: 2;
  transition: none;
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
  box-shadow: -2px 0 4px rgba(0, 0, 0, 0.1);
  transition:
    right var(--transition-fast),
    color var(--transition-fast),
    background var(--transition-fast);
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

/* ── 暂存删除（待重排）：置灰 + 降透明 + 「待删除」角标 ─────────────────────── */
/* 退出选择模式时这些项会被 fadeOut + FLIP 一次性重排移除（撤销可恢复）。 */
.media-card--pending-delete {
  filter: grayscale(1) brightness(0.7);
  opacity: 0.45;
  transition:
    opacity 0.2s ease,
    filter 0.2s ease;
}
.media-card__pending-badge {
  position: absolute;
  top: 4px;
  left: 4px;
  z-index: 12; /* 盖过缩略图徽标 */
  font-size: 9px;
  font-weight: 700;
  line-height: 1;
  padding: 2px 5px;
  border-radius: var(--radius-sm);
  color: #fff;
  background: rgba(220, 53, 69, 0.92); /* 红：待删除 */
  letter-spacing: 0.04em;
  pointer-events: none;
}

/* Tiny-cell mode: let the browser skip rendering off-screen cells. Cells keep
   their explicit inline width/height, so size containment can't collapse them.
   极小单元模式：让浏览器跳过离屏单元的渲染。单元有显式行内宽高，
   故尺寸包含不会使其塌陷。
   同时禁用 will-change / transition / hover-scale 以释放 800+ GPU 合成层。 */
.media-card--compact {
  content-visibility: auto;
  will-change: auto;
  transition: none;
}

/* compact 模式下彻底禁用 hover 放大——60px 格子放大 6% 仅多 3.6px，
   视觉收益微乎其微但 800+ 合成层开销巨大 */
.media-card--compact:hover {
  transform: none;
  box-shadow: none;
  z-index: 2;
  transition: none;
}

.media-card:hover {
  transform: scale(1.06);
  z-index: 100;
  box-shadow:
    0 8px 28px color-mix(in srgb, var(--color-text-primary) 15%, transparent),
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
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  transition:
    transform 0.2s cubic-bezier(0.34, 1.18, 0.64, 1),
    background 0.2s,
    box-shadow 0.2s;
  opacity: 0.8;
}

.fab-btn:hover {
  transform: scale(1.1);
  background: var(--color-bg-hover);
  opacity: 1;
  box-shadow: 0 6px 16px rgba(0, 0, 0, 0.3);
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

/* ── Floating ghost for media drag-to-folder (问题5/问题2/问题3) ───────────────
   Positioned via transform (imperative); hidden by default, shown via .is-active. */
.media-drag-ghost {
  position: fixed;
  top: 0;
  left: 0;
  z-index: 10001;
  pointer-events: none; /* must not block elementFromPoint | 不能挡住 elementFromPoint */
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  font-size: 12px;
  color: #fff;
  background: var(--color-accent);
  border-radius: var(--radius-md);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  opacity: 0;
  transition: opacity 120ms ease;
  will-change: transform;
}
.media-drag-ghost.is-active {
  opacity: 1;
}
.media-drag-ghost__badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.25);
  font-weight: 600;
}
</style>
