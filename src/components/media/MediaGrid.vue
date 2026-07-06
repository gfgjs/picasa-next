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
        :class="{
          'is-scrolling': isScrolling,
          'is-compact': compactCells,
          'media-grid--bucket': bucketActive,
        }"
        @scroll.passive="onGridScroll"
        @wheel.passive="onGridWheel"
        @keydown="onGridKeydown"
        @touchmove.passive="onGridTouchmove"
      >
        <!-- Empty state -->
        <!-- 空状态 -->
        <div v-if="media.totalRows === 0 && !media.isComputingLayout" class="empty-state">
          <div class="empty-state__icon"><ImageIcon :size="48" /></div>
          <div class="empty-state__title">{{ emptyStateTitle }}</div>
          <div v-if="emptyStateDesc" class="empty-state__desc">{{ emptyStateDesc }}</div>
          <!-- 空库「下一步动作」(§6.3):按钮触发 FoldersSection 的完整加目录流程 -->
          <button
            v-if="showEmptyAction"
            class="btn btn-primary empty-state__action"
            @click="requestAddFolder"
          >
            <FolderPlus :size="16" />
            {{ $t('sidebar.addFolder') }}
          </button>
        </div>

        <!-- Loading: compute_layout 首屏阶段用骨架屏占位(S5),视觉上预演网格落位;
             可访问性上仍以文案播报状态(骨架对读屏不可见)。 -->
        <div
          v-if="media.isComputingLayout"
          class="media-grid__skeleton"
          role="status"
          :aria-label="$t('empty.computing')"
        >
          <div v-for="i in 12" :key="i" class="skeleton-block media-grid__skeleton-cell" />
        </div>

        <!-- T16 方案B(B1.5):bucket 分段渲染。容器总高 = 真实逻辑高、零坐标平移;
             等高算术分段(useBucketVirtualScroll),仅渲染愿望窗口内的 1-3 个段——可见性
             是纯算术,无 IntersectionObserver、无全量占位 div、无内联函数 ref(B1 真机
             根因 C/D 由此结构性消除)。段行未到时以骨架条纹占位(--loading)。与下方
             方案 A 分支经 bucketActive 互斥,方案 A 零改动保留、开关即回退。卡片标记与
             方案 A 保持一致(data-item-id 与全部 handlers),使选区/拖拽/可视 patch
             消费面两边等价;B2 再抽公共行组件去重。 -->
        <div
          v-if="media.totalRows > 0 && bucketActive"
          ref="bucketContentRef"
          class="media-grid__content media-grid__content--bucket"
          :style="{ height: bucketSpacerHeight + 'px', position: 'relative' }"
        >
          <div
            v-for="seg in bucketSegments"
            :key="seg.index"
            class="media-grid__segment"
            :class="{ 'media-grid__segment--loading': seg.state !== 'ready' }"
            :style="{
              position: 'absolute',
              top: seg.start - bucketAnchorDelta + 'px',
              left: 0,
              right: 0,
              height: seg.end - seg.start + 'px',
            }"
          >
            <template v-if="seg.rows">
              <!-- 行体 = 双引擎公共组件 MediaGridRow(T16 收尾抽取,DOM 与原内联模板
                   逐字节等价);offset-y = 段起点。 -->
              <MediaGridRow
                v-for="row in seg.rows"
                :key="row.rowType === 'separator' ? `sep-${row.y}` : `row-${row.y}`"
                :row="row"
                :offset-y="seg.start"
                :gap="GAP"
                :group-by="ui.groupBy"
                :compact-cells="compactCells"
                :selection-mode="selection.isSelectionMode.value"
                :cache-dir="cacheDir"
                :pending-delete-label="t('selection.pendingDelete')"
                :is-selected="selection.isSelected"
                :is-pending-delete="isPendingDelete"
                :card-aria-label="cardAriaLabel"
                :on-card-click="handleCardClick"
                :on-card-pointer-down="onCardPointerDown"
                :on-card-context-menu="onContextMenu"
                :on-request-thumb="onRequestThumb"
                :on-cancel-thumb="onCancelThumb"
                :on-favorite="handleFavorite"
                :on-rate="handleRate"
                :on-select="selection.toggleSelect"
              />
            </template>
          </div>
        </div>

        <!-- Virtual scroll wrapper (absolute positioning) -->
        <!-- 虚拟滚动包装器 (绝对定位) -->
        <div
          v-else-if="media.totalRows > 0"
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
            <!-- 行体 = 双引擎公共组件 MediaGridRow(T16 收尾抽取,DOM 与原内联模板
                 逐字节等价);offset-y = renderAnchor,行加 will-change(平移模式
                 高频重钉合成层)。 -->
            <MediaGridRow
              v-for="row in visibleRows"
              :key="row.rowType === 'separator' ? `sep-${row.y}` : `row-${row.y}`"
              :row="row"
              :offset-y="renderAnchor"
              :row-will-change="true"
              :gap="GAP"
              :group-by="ui.groupBy"
              :compact-cells="compactCells"
              :selection-mode="selection.isSelectionMode.value"
              :cache-dir="cacheDir"
              :pending-delete-label="t('selection.pendingDelete')"
              :is-selected="selection.isSelected"
              :is-pending-delete="isPendingDelete"
              :card-aria-label="cardAriaLabel"
              :on-card-click="handleCardClick"
              :on-card-pointer-down="onCardPointerDown"
              :on-card-context-menu="onContextMenu"
              :on-request-thumb="onRequestThumb"
              :on-cancel-thumb="onCancelThumb"
              :on-favorite="handleFavorite"
              :on-rate="handleRate"
              :on-select="selection.toggleSelect"
            />
          </div>
          <!-- Close render layer -->
        </div>
        <!-- Close media-grid-content -->
      </div>
      <!-- Close media-grid -->

      <!-- T16 B3.2:bucket 引擎自研逻辑滚动条(原生条已隐藏,见 .media-grid--bucket)。
           拇指渲染纯逻辑百分比,与画廊逐帧同步;映射态停稳偿债只动物理 scrollTop,
           对本条零感知——原生拇指「急速滚动往回跳」由此根治。 -->
      <MediaScrollbar
        v-if="bucketActive && media.totalRows > 0"
        :total-height="media.totalHeight"
        :current-y="currentLogicalY"
        :active="isScrolling"
        @jump="onScrollbarJump"
      />
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
          :current-y="currentLogicalY"
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
import { ref, watch, onMounted, onBeforeUnmount, computed, markRaw, nextTick } from 'vue'
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
import { useBucketVirtualScroll } from '../../composables/useBucketVirtualScroll'
import { useRequestQueue } from '../../composables/useRequestQueue'
import { useCollectionToast } from '../../composables/useCollectionToast'

import MediaGridRow from './MediaGridRow.vue'
import TimelineScrubber from './TimelineScrubber.vue'
import MediaScrollbar from './MediaScrollbar.vue'
import SelectionToolbar from './SelectionToolbar.vue'
import ContextMenu, { type ContextMenuItem } from '../common/ContextMenu.vue'
import FolderTreeSelectorDialog from '../common/FolderTreeSelectorDialog.vue'
import {
  ImageIcon,
  Copy,
  FolderOpen,
  FolderPlus,
  ChevronLeft,
  ChevronRight,
  Monitor,
  FolderInput,
} from '@lucide/vue'
import { useSelection, type BackendSelectionDescriptor } from '../../composables/useSelection'
import { useViewIds } from '../../composables/useViewIds'
import { useHistoryStore } from '../../stores/historyStore'
import { useMediaDragToFolder } from '../../composables/useMediaDragToFolder'
import type { LayoutRow, LayoutRowItem } from '../../types/layout'
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

// 空状态「下一步动作」(§6.3):仅真正的空库场景(全部照片视图,无搜索/目录/人物过滤)
// 出「添加文件夹」按钮;动作经 request-add-folder 事件复用 FoldersSection 的完整
// addRoot 流程(重叠检测/自动扫描/树选中),不在此复制该逻辑(事件通道同 folder-stats-changed 惯例)。
const showEmptyAction = computed(
  () =>
    !ui.isSearching &&
    ui.activeDirectoryId == null &&
    ui.activePersonId == null &&
    ui.activeSmartAlbum === 'all',
)
function requestAddFolder() {
  window.dispatchEvent(new CustomEvent('request-add-folder'))
}

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
  for (const row of activeRows()) {
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
  // `y` 是逻辑坐标。bucket 走统一入口(B3 映射态:近距平滑、远跳重锚+立即落点);
  // 方案 A 经线性平移映射后平滑滚动。
  if (bucketActive.value) {
    void bucketScroll.scrollToLogicalY(y, { smooth: true })
    return
  }
  if (gridRef.value) {
    gridRef.value.scrollTo({ top: logicalToPhysical(y), behavior: 'smooth' })
  }
}

// 自研逻辑滚动条的拖拽/轨道点击跳转(B3.2,仅 bucket 引擎挂载):即时落点——拖拽中
// 平滑动画只会让拇指跟手性变差;远跳/近跳分流由 scrollToLogicalY 统一处理。
function onScrollbarJump(y: number) {
  void bucketScroll.scrollToLogicalY(y)
}

// ── Virtual scroll ─────────────────────────────────────────────────────────
// ── 虚拟滚动 ─────────────────────────────────────────────────────────

function getViewKey() {
  return ui.activeDirectoryId ? `dir-${ui.activeDirectoryId}` : `album-${ui.activeSmartAlbum}`
}

const layerRef = ref<HTMLElement | null>(null)
// bucket 分支的内容容器(B2):FLIP/fadeOut 在 bucket 模式以它为根查询 [data-item-id]。
const bucketContentRef = ref<HTMLElement | null>(null)

// 容器内容区宽度（去 scrollbar）——布局重算的输入；在策略源之前声明，惰性 getter 透传。
const containerWidth = ref(0)

// 布局策略源（A1，见 plan-docs/refactor_2026/T20_T18-layout_布局策略接缝_合并设计.md）：
// 当前仅 justified 一种策略。useVirtualScroll 只依赖 source 的 totalHeight/totalRows/
// fetchRowsByY 抽象契约，与具体布局算法解耦——A2 将新增 grid 策略实现同一接口后由此切换。
const layoutSource = useGalleryLayoutSource(() => containerWidth.value)

// ── T16 方案B:bucket 分段引擎适用域(声明须先于两引擎构造——useVirtualScroll 内部
// watch(opts.enabled) 构造时即求值)──────────────────────────────────────────────
// B1.5 等高算术分段与分组语义无关 → 三种分组(date/folder/none)统一覆盖;B3 段级坐标
// 映射落地后**无总高上限**(>16M 进映射态:spacer 封顶、局部 1:1 + 低频重锚,见
// useBucketVirtualScroll 头注)——bucketActive 仅由开关与非空视图决定。两引擎常驻
// 实例化、各以 enabled() 休眠/接管,运行时即切即生效(滚动位保持见下方 watch)。
const bucketActive = computed(() => ui.bucketSegmentedScroll && media.totalRows > 0)

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
  // 方案 A 与 bucket 引擎互斥:bucket 接管时本引擎休眠(不取数、卸 wheel 补偿)。
  enabled: () => !bucketActive.value,
})

// bucket 分段引擎(T16 方案B B1.5):愿望窗口/取数自驱于 onScroll 算术同步与
// layoutVersion watch,宿主的各处 compute()+updateVisible() 调用无需逐一适配——
// compute 换 layoutVersion 即触发段表重建。
const bucketScroll = useBucketVirtualScroll({
  enabled: () => bucketActive.value,
  totalHeight: () => media.totalHeight,
  layoutVersion: () => media.layoutVersion,
  fetchBucketRows: (startY, endY) => media.fetchBucketRows(startY, endY),
  containerRef: () => gridRef.value,
})
const { segments: bucketSegments, anchorDelta: bucketAnchorDelta, spacerHeight: bucketSpacerHeight } = bucketScroll

// 双引擎统一读数面:逻辑滚动位(scrubber 高亮/分隔符联动)与「当前可视行」(可视项
// 就地 patch:缩略图/收藏/评分/色标/右键查找/行高锚点)。bucket 模式零映射,
// 物理 scrollTop 即逻辑 y。
const currentLogicalY = computed(() =>
  bucketActive.value ? bucketScroll.logicalScrollTop.value : logicalScrollTop.value,
)

function activeRows(): LayoutRow[] {
  return bucketActive.value ? bucketScroll.mountedRows() : visibleRows.value
}

// 引擎切换(一键开关/空视图翻转)时保持逻辑滚动位:两引擎的物理 scrollTop 语义不同
// (bucket 经 scrollToLogicalY 统一入口——B3 映射态的重锚自处理;方案 A 平移模式 =
// 压缩坐标)。切换瞬间读「离开方」的逻辑位,等 DOM(容器内容高度)换代后按「进入方」
// 语义回设。方案 A 的重取由其 enabled watch 自触发(scheduleUpdate(true) 的 rAF 晚于
// 本 nextTick,读到的已是回设后的 scrollTop)。
watch(bucketActive, async (nowBucket) => {
  const el = gridRef.value
  if (!el) return
  const logicalY = nowBucket ? logicalScrollTop.value : bucketScroll.logicalScrollTop.value
  await nextTick()
  if (nowBucket) {
    await bucketScroll.scrollToLogicalY(Math.max(0, logicalY))
  } else {
    el.scrollTop = logicalToPhysical(logicalY)
  }
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
  const vTop = currentLogicalY.value
  for (const row of activeRows()) {
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
      const targetY = Math.max(0, y - anchor.screenOffset)
      if (bucketActive.value) {
        // bucket:统一入口(B3 映射态重锚自处理);缓存存逻辑 y(映射态物理位不自足)。
        await bucketScroll.scrollToLogicalY(targetY)
        scrollCache.set(getViewKey(), targetY)
      } else {
        const physY = logicalToPhysical(targetY)
        gridRef.value.scrollTop = physY
        scrollCache.set(getViewKey(), physY)
      }
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

// B3.1 输入源分类转发(仅 bucket 引擎消费):wheel/滚动键/触屏盖 1:1 印记,与滚动条
// 拖动区分;wheel 另担映射态物理钉边后的边缘续滚。全部不阻断,对原生滚动零干预。
function onGridWheel(e: WheelEvent) {
  if (bucketActive.value) bucketScroll.onWheel(e)
}
function onGridKeydown(e: KeyboardEvent) {
  if (bucketActive.value) bucketScroll.onKeydown(e)
}
function onGridTouchmove() {
  if (bucketActive.value) bucketScroll.onTouchmove()
}

function onGridScroll() {
  if (bucketActive.value) bucketScroll.onScroll()
  else onScroll()
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
    const scrollTop = currentLogicalY.value
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
      // bucket 缓存逻辑 y(B3 映射态下物理 scrollTop 不自足);方案 A 仍缓存物理。
      scrollCache.set(
        getViewKey(),
        bucketActive.value ? currentLogicalY.value : gridRef.value.scrollTop,
      )
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
    // clientWidth 包含 padding，但布局计算需要的是内部内容区域宽度。实测容器自身的
    // 双侧 padding(bucket 模式右侧多一条 --scrollbar-width 的对称 padding,B3.2),
    // 与 ResizeObserver 路径的 contentRect 语义保持一致。
    const cs = getComputedStyle(gridRef.value)
    const pad = (parseFloat(cs.paddingLeft) || 0) + (parseFloat(cs.paddingRight) || 0)
    containerWidth.value = gridRef.value.clientWidth - pad
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
    for (const row of activeRows()) {
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
  // B2:行源引擎感知——bucket 模式喂已挂载段的行(与 activeRows() 同源;computed 亦是
  // Ref,内部 watch 随段挂载/换代自然触发)。refresh 在 bucket 模式为 no-op(方案 A 门控),
  // 重算后的段回填由 layoutVersion watch 自驱。
  visibleRows: computed(() => (bucketActive.value ? bucketScroll.mountedRows() : visibleRows.value)),
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
  for (const row of activeRows()) {
    if (row.rowType !== 'normal') continue
    for (const item of row.items) {
      if (ids.has(item.id)) item.rating = rating
    }
  }
}

// 乐观更新收藏态（镜像 patchVisibleRating）。供 in-grid 收藏 + 详情页回灌信号共用。
function patchVisibleFavorite(ids: Set<number>, isFavorited: boolean) {
  for (const row of activeRows()) {
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
  for (const row of activeRows()) {
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
  for (const row of activeRows()) {
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
// 一个根元素 getter）。仅用于删除/移除路径，绝不挂到滚动驱动的 updateVisible（避免与虚拟滚动 +
// renderAnchor 打架）。B2:根引擎感知——方案 A 用渲染层 layerRef,bucket 模式用段容器
// (两分支卡片同为 [data-item-id],FLIP 按 id 匹配跨段照常成立)。
const { flipReflow, fadeOutCells } = useGridFlipReflow(() =>
  bucketActive.value ? bucketContentRef.value : layerRef.value,
)

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
    // B2:bucket 模式的行数据在版本换代后异步回填——等愿望窗口内段全部落地,
    // FLIP 的 Last 快照才能读到重排后的真实位置(否则 DOM 为空,动画静默失效)。
    if (bucketActive.value) await bucketScroll.whenSettled()
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
        if (bucketActive.value) await bucketScroll.scrollToLogicalY(saved)
        else gridRef.value.scrollTop = saved
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
  // 第三源:bucket 模式的段挂载/卸载(段行深响应,mountedRows() 的依赖变化即触发)。
  [
    visibleRows,
    () => ui.showThumbInfo,
    () => (bucketActive.value ? bucketScroll.mountedRows() : null),
  ],
  () => {
    if (!ui.showThumbInfo) return
    const ids: number[] = []
    for (const row of activeRows()) {
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
      const targetY = Math.max(0, target.y)
      if (bucketActive.value) {
        void bucketScroll.scrollToLogicalY(targetY, { smooth: true })
        scrollCache.set(getViewKey(), targetY)
      } else {
        const physY = logicalToPhysical(targetY)
        gridRef.value.scrollTo({ top: physY, behavior: 'smooth' })
        scrollCache.set(getViewKey(), physY)
      }
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
  /* 画廊画布区专用底色(感知学红线):一切主题此 token 都是低饱和中性色,
     暖调/冷调主题的"花样"只落在 chrome,照片观感不被 UI 带偏。 */
  background: var(--color-bg-canvas);
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

/* T16 B3.2:bucket 引擎隐藏原生滚动条——原生拇指只跟随物理 spacer(映射态钳 16M),
   结构上无法表达逻辑比例,且停稳偿债挪 scrollTop 时会可见回跳;改由 MediaScrollbar
   渲染逻辑百分比。原生条槽位(--scrollbar-width)转为右 padding,与左侧对称,引擎
   切换零布局位移(内容宽度不变)。 */
.media-grid--bucket {
  scrollbar-width: none;
  padding-right: var(--scrollbar-width, 6px);
}
.media-grid--bucket::-webkit-scrollbar {
  display: none;
}

/* 行体抽入子组件 MediaGridRow 后(T16 收尾),行根类(.media-grid__row/.date-separator)
   继承本组件作用域属性、原选择器照常命中;卡片/分隔符内部类则须 :deep() 穿透——
   scoped 编译后特异性不变(同为一属性+一类),视觉零差异。 */
.media-grid.is-scrolling :deep(.media-card) {
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

/* compute_layout 首屏骨架(S5):flex 换行模拟 justified 网格的首屏落位 */
.media-grid__skeleton {
  display: flex;
  flex-wrap: wrap;
  gap: var(--grid-gap);
  padding: var(--spacing-sm);
}
.media-grid__skeleton-cell {
  height: calc(var(--grid-row-height) * 0.75);
  flex: 1 1 200px;
  max-width: 340px;
}

/* T16 方案B(B1.5):段行未到时的骨架条纹——远跳/横扫瞬间以行状占位替代白屏;
   单飞取数落地极快(数十 ms),常态几乎不可见。 */
.media-grid__segment--loading {
  background-image: repeating-linear-gradient(
    to bottom,
    var(--color-bg-surface) 0px,
    var(--color-bg-surface) 172px,
    transparent 172px,
    transparent 180px
  );
  opacity: 0.6;
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

:deep(.separator-content) {
  display: flex;
  align-items: center;
  gap: 8px;
  background: rgba(var(--color-bg-primary-rgb, 255, 255, 255), 0.85);
  backdrop-filter: blur(8px);
  padding: 4px 12px;
  border-radius: var(--radius-md);
  margin-top: 4px;
}

:deep(.separator-icon) {
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

:deep(.media-card) {
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
:deep(.media-card--pending-delete) {
  filter: grayscale(1) brightness(0.7);
  opacity: 0.45;
  transition:
    opacity 0.2s ease,
    filter 0.2s ease;
}
:deep(.media-card__pending-badge) {
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
:deep(.media-card--compact) {
  content-visibility: auto;
  will-change: auto;
  transition: none;
}

/* compact 模式下彻底禁用 hover 放大——60px 格子放大 6% 仅多 3.6px，
   视觉收益微乎其微但 800+ 合成层开销巨大 */
:deep(.media-card--compact:hover) {
  transform: none;
  box-shadow: none;
  z-index: 2;
  transition: none;
}

:deep(.media-card:hover) {
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
:deep(.media-card--selection-mode:hover) {
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
