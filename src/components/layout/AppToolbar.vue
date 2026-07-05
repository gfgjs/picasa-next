<template>
  <div class="toolbar__left">
    <!-- Title / breadcrumb -->
    <!-- 标题 / 面包屑 -->
    <div class="toolbar__breadcrumb">
      <span class="toolbar__title">{{ title }}</span>
      <span v-if="media.stats" class="toolbar__count">{{
        $t('statusbar.items', { count: media.viewTotalItems.toLocaleString() })
      }}</span>
    </div>
  </div>

  <!-- Filter chips -->
  <!-- 筛选芯片 -->
  <div class="toolbar__filters">
    <button
      class="chip"
      :class="{ active: filter.mediaTypes.includes('image') }"
      @click="filter.toggleMediaType('image')"
    >
      <ImageIcon :size="14" /> {{ $t('toolbar.filterImages') }}
    </button>
    <button
      class="chip"
      :class="{ active: filter.mediaTypes.includes('video') }"
      @click="filter.toggleMediaType('video')"
    >
      <Video :size="14" /> {{ $t('sidebar.videos') }}
    </button>
    <button
      class="chip"
      :class="{ active: filter.livePhotoOnly }"
      @click="filter.livePhotoOnly = !filter.livePhotoOnly"
    >
      <Sparkles :size="14" /> Live
    </button>
    <button
      class="chip"
      :class="{ active: filter.favoritedOnly }"
      @click="filter.favoritedOnly = !filter.favoritedOnly"
    >
      <Heart :size="14" /> {{ $t('toolbar.filterFavorites') }}
    </button>
    <!-- 评分筛选：内联星级直接绑 minRating（"≥N 星"语义）；点当前星级清空。 -->
    <div
      class="chip chip--rating"
      :class="{ active: filter.minRating > 0 }"
      :title="$t('toolbar.filterByRating')"
    >
      <StarRating v-model="filter.minRating" :size="14" />
      <span v-if="filter.minRating > 0" class="chip__rating-suffix">+</span>
    </div>
    <!-- 颜色标签筛选（T16）：内联色块直接绑 colorLabel（精确匹配某色档）；点当前色清空。 -->
    <div
      class="chip chip--color"
      :class="{ active: filter.colorLabel > 0 }"
      :title="$t('toolbar.filterByColorLabel')"
    >
      <ColorLabelPicker v-model="filter.colorLabel" :size="13" />
    </div>
    <!-- 日期范围筛选（T15）：chip 切换弹层，弹层内两个原生日期输入绑 from/to。
         两端皆备方下发 date_range 谓词（见 filterStore.toApiFilter）；active 态同步。 -->
    <button
      ref="dateChipRef"
      class="chip chip--date"
      :class="{ active: isDateActive }"
      :title="$t('toolbar.filterByDateRange')"
      @click="toggleDatePopover"
    >
      <Calendar :size="14" />
      <span v-if="isDateActive" class="chip__date-label">{{ dateRangeLabel }}</span>
      <span v-else>{{ $t('toolbar.searchScopeDate') }}</span>
    </button>
    <button v-if="filter.hasActiveFilters" class="chip chip--clear" @click="clearAllFilters">
      <X :size="14" /> {{ $t('toolbar.clearFilters') }}
    </button>
  </div>

  <!-- 日期范围弹层：Teleport 到 body 以逃逸 .toolbar__filters 的 overflow 裁切；
       fixed 定位由 chip 的 getBoundingClientRect 计算。透明 backdrop 点击即关。 -->
  <Teleport to="body">
    <div v-if="showDatePopover" class="date-popover-backdrop" @click="showDatePopover = false">
      <Transition name="dropdown-fade" appear>
        <div
          class="date-popover"
          :style="{ top: popoverTop + 'px', left: popoverLeft + 'px' }"
          @click.stop
        >
          <label class="date-popover__field">
            <span>{{ $t('toolbar.dateFrom') }}</span>
            <input type="date" v-model="dateFromInput" :max="dateToInput || undefined" />
          </label>
          <label class="date-popover__field">
            <span>{{ $t('toolbar.dateTo') }}</span>
            <input type="date" v-model="dateToInput" :min="dateFromInput || undefined" />
          </label>
          <div class="date-popover__actions">
            <button class="date-popover__btn" @click="clearDateRange">
              {{ $t('toolbar.dateClear') }}
            </button>
            <button
              class="date-popover__btn date-popover__btn--primary"
              @click="showDatePopover = false"
            >
              {{ $t('onboarding.finish') }}
            </button>
          </div>
        </div>
      </Transition>
    </div>
  </Teleport>

  <!-- Right controls -->
  <!-- 右侧控件 -->
  <div class="toolbar__right">
    <!-- H-Lab 横向画廊实验室入口(多布局候选真人调研;plan-docs/2026-07-02-horizontal-gallery-lab.md) -->
    <button
      class="btn-icon"
      :title="$t('toolbar.hgalleryLab')"
      :aria-label="$t('toolbar.hgalleryLab')"
      @click="router.push('/hgallery-lab')"
    >
      <FlaskConical :size="18" />
    </button>
    <!-- Undo / Redo — folder move & copy | 撤销 / 重做 — 文件夹移动与复制 -->
    <button
      class="btn-icon"
      :disabled="!history.canUndo"
      :title="$t('toolbar.undo')"
      :aria-label="$t('toolbar.undo')"
      @click="history.undo()"
    >
      <Undo2 :size="18" />
    </button>
    <button
      class="btn-icon"
      :disabled="!history.canRedo"
      :title="$t('toolbar.redo')"
      :aria-label="$t('toolbar.redo')"
      @click="history.redo()"
    >
      <Redo2 :size="18" />
    </button>

    <!-- Fullscreen -->
    <!-- 全屏 -->
    <button
      class="btn-icon"
      :title="ui.isFullscreen ? $t('toolbar.exitFullscreen') : $t('toolbar.fullscreen')"
      :aria-label="ui.isFullscreen ? $t('toolbar.exitFullscreen') : $t('toolbar.fullscreen')"
      @click="ui.toggleFullscreen()"
    >
      <Minimize2 v-if="ui.isFullscreen" :size="18" />
      <Maximize2 v-else :size="18" />
    </button>

    <!-- Search -->
    <!-- 搜索 -->
    <div
      class="toolbar__search-wrap"
      :class="{ focused: isSearchFocused, 'semantic-mode': ai.isSemanticMode }"
      style="position: relative"
    >
      <!-- Mode toggle button -->
      <!-- 模式切换按钮 -->
      <button
        class="toolbar__search-mode-btn"
        :class="['mode-' + ai.searchMode]"
        :title="
          ai.searchMode === 'mixed'
            ? $t('toolbar.searchModeMixed')
            : ai.searchMode === 'semantic'
              ? $t('toolbar.searchModeSemantic')
              : $t('toolbar.searchModeNormal')
        "
        :aria-label="
          ai.searchMode === 'mixed'
            ? $t('toolbar.searchModeMixed')
            : ai.searchMode === 'semantic'
              ? $t('toolbar.searchModeSemantic')
              : $t('toolbar.searchModeNormal')
        "
        @click="toggleSearchMode"
      >
        <Sparkles v-if="ai.searchMode === 'mixed'" :size="14" />
        <Bot v-else-if="ai.searchMode === 'semantic'" :size="14" />
        <Search v-else :size="14" />
      </button>

      <input
        ref="searchInputRef"
        class="toolbar__search"
        v-model="currentSearchQuery"
        @keydown.esc.prevent="onEscape"
        @keydown.down="onKeydownDown"
        @keydown.up="onKeydownUp"
        @keydown.enter="onKeydownEnter"
        @focus="onSearchFocus"
        @blur="onSearchBlur"
        :placeholder="
          ai.searchMode === 'mixed'
            ? $t('toolbar.searchPlaceholderMixed')
            : ai.searchMode === 'semantic'
              ? $t('toolbar.searchPlaceholderSemantic')
              : $t('toolbar.searchPlaceholder')
        "
        type="search"
      />

      <!-- Scope selector (only for normal search mode) -->
      <!-- 搜索范围选择器（仅限普通搜索模式） -->
      <select
        v-show="ai.searchMode === 'normal'"
        class="toolbar__search-scope"
        v-model="ui.searchScope"
      >
        <option value="filename">{{ $t('toolbar.searchScopeFilename') }}</option>
        <option value="folder">{{ $t('toolbar.searchScopeFolder') }}</option>
        <option value="date">{{ $t('toolbar.searchScopeDate') }}</option>
        <option value="device">{{ $t('toolbar.searchScopeDevice') }}</option>
        <option value="location">{{ $t('toolbar.searchScopeLocation') }}</option>
        <option value="global">{{ $t('toolbar.searchScopeGlobal') }}</option>
      </select>

      <!-- AI searching indicator -->
      <!-- AI 搜索中指示器 -->
      <span v-if="ai.isSearching" class="toolbar__search-spinner" />

      <!-- Mixed mode dropdown -->
      <Transition name="dropdown-fade">
        <div v-if="isMixedDropdownOpen" class="mixed-search-dropdown" @mousedown.prevent>
          <div
            class="dropdown-item"
            :class="{ selected: mixedDropdownIndex === 0 }"
            @click="executeMixedSearch(0)"
          >
            <div class="dropdown-icon-wrap"><Sparkles :size="14" /></div>
            <div class="dropdown-text">
              <!-- 中英语序不同，前后缀直译无法成句：用 <i18n-t> 让高亮 query 落进各语言的正确语序位置 -->
              <i18n-t keypath="toolbar.mixedSearchAiHint" scope="global">
                <template #query>
                  <span class="query">{{ pendingMixedQuery }}</span>
                </template>
              </i18n-t>
            </div>
          </div>
          <div
            class="dropdown-item"
            :class="{ selected: mixedDropdownIndex === 1 }"
            @click="executeMixedSearch(1)"
          >
            <div class="dropdown-icon-wrap"><Search :size="14" /></div>
            <div class="dropdown-text">
              <i18n-t keypath="toolbar.mixedSearchFilenameHint" scope="global">
                <template #query>
                  <span class="query">{{ pendingMixedQuery }}</span>
                </template>
              </i18n-t>
            </div>
          </div>
        </div>
      </Transition>
    </div>

    <!-- Row height slider | 行高调节滑块 -->
    <div class="toolbar-row-height">
      <span class="toolbar-icon" :title="$t('toolbar.rowHeight')">
        <Rows3 :size="16" />
      </span>
      <input
        type="range"
        class="row-height-slider"
        :min="60"
        :max="960"
        :step="20"
        :value="sliderRowHeight"
        @input="onRowHeightInput"
      />
      <span class="row-height-value">{{ sliderRowHeight }}px</span>
    </div>

    <!-- Group by selector | 分组选择器 -->
    <div class="toolbar-group">
      <!-- 布局模式切换（T20）：宫格 ⇄ 等高行。显示当前模式图标（grid=方格 / justified=行），
           点击切换；驱动后端 compute_layout 换排版算法 + 前端方图裁切（cover）。 -->
      <button
        class="btn-icon"
        :title="
          ui.layoutMode === 'grid'
            ? $t('toolbar.layoutGridTitle')
            : $t('toolbar.layoutJustifiedTitle')
        "
        :aria-label="
          ui.layoutMode === 'grid'
            ? $t('toolbar.layoutGridTitle')
            : $t('toolbar.layoutJustifiedTitle')
        "
        @click="toggleLayoutMode"
      >
        <LayoutGrid v-if="ui.layoutMode === 'grid'" :size="16" />
        <GalleryVertical v-else :size="16" />
      </button>

      <select class="toolbar__select" :value="ui.groupBy" @change="onGroupByChange">
        <option value="date">{{ $t('toolbar.groupByDate') }}</option>
        <option value="folder">{{ $t('toolbar.groupByFolder') }}</option>
        <option value="none">{{ $t('toolbar.noGroup') }}</option>
      </select>

      <!-- Sort within group (visible only for folder grouping or AI mode) -->
      <!-- 组内排序（仅在文件夹分组或 AI 搜索时可见） -->
      <select
        v-if="ui.groupBy === 'folder' || ai.isSemanticMode"
        class="toolbar__select"
        :value="ui.sortWithinGroup"
        @change="onSortWithinGroupChange"
      >
        <option value="datetime">{{ $t('toolbar.sortByTime') }}</option>
        <option value="filename">{{ $t('toolbar.sortByName') }}</option>
        <option v-if="ai.isSemanticMode" value="similarity">
          {{ $t('toolbar.sortBySimilarity') }}
        </option>
      </select>

      <!-- Asc/Desc toggle -->
      <button
        class="btn-icon"
        :title="ui.sortOrder === 'desc' ? $t('toolbar.sortDescTitle') : $t('toolbar.sortAscTitle')"
        :aria-label="
          ui.sortOrder === 'desc' ? $t('toolbar.sortDescTitle') : $t('toolbar.sortAscTitle')
        "
        @click="toggleSortOrder"
      >
        <ArrowDown v-if="ui.sortOrder === 'desc'" :size="16" />
        <ArrowUp v-else :size="16" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import {
  ImageIcon,
  Video,
  Sparkles,
  Heart,
  X,
  Maximize2,
  Minimize2,
  Search,
  ArrowDown,
  ArrowUp,
  Rows3,
  Bot,
  Undo2,
  Redo2,
  Calendar,
  LayoutGrid,
  GalleryVertical,
  FlaskConical,
} from '@lucide/vue'
import StarRating from '../common/StarRating.vue'
import ColorLabelPicker from '../common/ColorLabelPicker.vue'
import { useUiStore } from '../../stores/uiStore'
import { useFilterStore } from '../../stores/filterStore'
import { useMediaStore } from '../../stores/mediaStore'
import { useAiStore } from '../../stores/aiStore'
import { useHistoryStore } from '../../stores/historyStore'
import { DEFAULTS } from '../../constants/defaults'

const emit = defineEmits<{
  (e: 'search', query: string): void
  (e: 'semantic-search', query: string): void
}>()

const { t } = useI18n()
const router = useRouter()
const ui = useUiStore()
const filter = useFilterStore()
const media = useMediaStore()
const ai = useAiStore()
const history = useHistoryStore()

// ── 日期范围筛选（T15）──────────────────────────────────────────────────────
// filterStore.dateFrom/dateTo 存 Unix epoch「秒」（与 sort_datetime 同单位）；原生
// <input type="date"> 用 'YYYY-MM-DD' 字符串。两者间按「本地民用日」互转：from 取当日 0 点、
// to 取当日 23:59:59，使范围对用户选的两天均为闭区间。
const showDatePopover = ref(false)
const dateChipRef = ref<HTMLElement>()
const popoverTop = ref(0)
const popoverLeft = ref(0)

const isDateActive = computed(() => filter.dateFrom !== null && filter.dateTo !== null)

/** epoch 秒 → 'YYYY-MM-DD'（本地）；null/非法 → ''。 */
function tsToInput(ts: number | null): string {
  if (ts == null) return ''
  const d = new Date(ts * 1000)
  if (Number.isNaN(d.getTime())) return ''
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

/** 'YYYY-MM-DD' → epoch 秒；endOfDay=true 取当日 23:59:59，否则 0 点。空/非法 → null。 */
function inputToTs(val: string, endOfDay: boolean): number | null {
  if (!val) return null
  const [y, m, d] = val.split('-').map(Number)
  if (!y || !m || !d) return null
  const date = endOfDay ? new Date(y, m - 1, d, 23, 59, 59) : new Date(y, m - 1, d, 0, 0, 0)
  const t = date.getTime()
  return Number.isNaN(t) ? null : Math.floor(t / 1000)
}

const dateFromInput = computed<string>({
  get: () => tsToInput(filter.dateFrom),
  set: (v: string) => {
    filter.dateFrom = inputToTs(v, false)
  },
})
const dateToInput = computed<string>({
  get: () => tsToInput(filter.dateTo),
  set: (v: string) => {
    filter.dateTo = inputToTs(v, true)
  },
})

// chip 上的紧凑区间标签：同年省略起始年份，仅显示「M/D – M/D」或「YYYY/M/D – M/D」。
const dateRangeLabel = computed(() => {
  const from = filter.dateFrom != null ? new Date(filter.dateFrom * 1000) : null
  const to = filter.dateTo != null ? new Date(filter.dateTo * 1000) : null
  if (!from || !to) return ''
  const sameYear = from.getFullYear() === to.getFullYear()
  const fromStr = sameYear
    ? `${from.getMonth() + 1}/${from.getDate()}`
    : `${from.getFullYear()}/${from.getMonth() + 1}/${from.getDate()}`
  const toStr = `${to.getMonth() + 1}/${to.getDate()}`
  return `${fromStr} – ${toStr}`
})

function toggleDatePopover() {
  if (!showDatePopover.value && dateChipRef.value) {
    // 打开前以 chip 的视口坐标定位弹层，规避 .toolbar__filters 的 overflow 裁切。
    const r = dateChipRef.value.getBoundingClientRect()
    popoverTop.value = r.bottom + 6
    popoverLeft.value = r.left
  }
  showDatePopover.value = !showDatePopover.value
}

function clearDateRange() {
  filter.dateFrom = null
  filter.dateTo = null
}

// 「清除筛选」统一出口：清空所有筛选并关闭日期弹层（否则弹层残留指向已清空状态）。
function clearAllFilters() {
  filter.clearFilters()
  showDatePopover.value = false
}

// ── Undo / Redo keyboard shortcuts (only when not typing in a field) ───────────
// ── 撤销 / 重做键盘快捷键（仅在非输入态时生效） ─────────────────────────────────
function onGlobalKeydown(e: KeyboardEvent) {
  const ctrl = e.ctrlKey || e.metaKey
  if (!ctrl) return
  const key = e.key.toLowerCase()
  if (key !== 'z' && key !== 'y') return
  const tgt = e.target as HTMLElement | null
  const tag = tgt?.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || tgt?.isContentEditable) return
  if (key === 'y' || (key === 'z' && e.shiftKey)) {
    e.preventDefault()
    history.redo()
  } else if (key === 'z') {
    e.preventDefault()
    history.undo()
  }
}
onMounted(() => document.addEventListener('keydown', onGlobalKeydown))
onBeforeUnmount(() => document.removeEventListener('keydown', onGlobalKeydown))

const isSearchFocused = ref(false)
const searchInputRef = ref<HTMLInputElement>()
let searchTimer: ReturnType<typeof setTimeout> | null = null

// 标题复用侧栏词条以保持入口一致；map 在 computed 内重建并调 t()，locale 切换时自动重算。
const title = computed(() => {
  const map: Record<string, string> = {
    all: t('sidebar.allPhotos'),
    favorites: t('sidebar.favorites'),
    'live-photos': t('sidebar.livePhotos'),
    recent: t('sidebar.recentlyAdded'),
    trash: t('sidebar.trash'),
  }
  return map[ui.activeSmartAlbum] ?? t('toolbar.titleDefault')
})

const pendingMixedQuery = ref('')
const isMixedDropdownOpen = ref(false)
const mixedDropdownIndex = ref(0) // 0: AI, 1: Normal

function triggerMixedSearch(immediate = false) {
  const query = pendingMixedQuery.value

  if (searchTimer) clearTimeout(searchTimer)

  if (!query.trim()) {
    ai.setNormalSearchQueryInMixedMode('')
    return
  }

  const delay = immediate ? 0 : 500

  searchTimer = setTimeout(() => {
    if (mixedDropdownIndex.value === 0) {
      ai.runSemanticSearch(query)
      emit('semantic-search', query)
    } else {
      ai.setNormalSearchQueryInMixedMode(query)
      emit('search', query)
    }
  }, delay)
}

const currentSearchQuery = computed({
  get() {
    if (ai.searchMode === 'mixed') return pendingMixedQuery.value
    return ai.searchMode === 'semantic' ? ai.semanticQuery : ui.searchQuery
  },
  set(val: string) {
    if (ai.searchMode === 'mixed') {
      pendingMixedQuery.value = val
      isMixedDropdownOpen.value = val.trim().length > 0
      triggerMixedSearch(false)
    } else if (ai.searchMode === 'semantic') {
      ai.semanticQuery = val
      // Debounce semantic search | 对语义搜索进行防抖
      if (searchTimer) clearTimeout(searchTimer)
      searchTimer = setTimeout(() => {
        ai.runSemanticSearch(val)
        emit('semantic-search', val)
      }, 500)
    } else {
      ui.searchQuery = val
      if (searchTimer) clearTimeout(searchTimer)
      searchTimer = setTimeout(() => emit('search', val), DEFAULTS.SEARCH_DEBOUNCE_MS)
    }
  },
})

function onSearchFocus() {
  isSearchFocused.value = true
  if (ai.searchMode === 'mixed' && pendingMixedQuery.value.trim().length > 0) {
    isMixedDropdownOpen.value = true
  }
}

function onSearchBlur() {
  isSearchFocused.value = false
  isMixedDropdownOpen.value = false
}

function onEscape() {
  if (isMixedDropdownOpen.value) {
    isMixedDropdownOpen.value = false
  } else {
    // maybe clear search?
  }
}

function onKeydownDown(e: KeyboardEvent) {
  if (ai.searchMode === 'mixed' && isMixedDropdownOpen.value) {
    e.preventDefault()
    mixedDropdownIndex.value = (mixedDropdownIndex.value + 1) % 2
    triggerMixedSearch(true)
  }
}

function onKeydownUp(e: KeyboardEvent) {
  if (ai.searchMode === 'mixed' && isMixedDropdownOpen.value) {
    e.preventDefault()
    mixedDropdownIndex.value = (mixedDropdownIndex.value + 1) % 2
    triggerMixedSearch(true)
  }
}

function onKeydownEnter(e: KeyboardEvent) {
  if (ai.searchMode === 'mixed' && isMixedDropdownOpen.value) {
    e.preventDefault()
    isMixedDropdownOpen.value = false
    triggerMixedSearch(true)
  }
}

function executeMixedSearch(index: number) {
  mixedDropdownIndex.value = index
  isMixedDropdownOpen.value = false
  triggerMixedSearch(true)
}

function toggleSearchMode() {
  ai.toggleSearchMode()
  if (ai.searchMode === 'mixed') {
    pendingMixedQuery.value = ''
    isMixedDropdownOpen.value = false
  }
}

function toggleSortOrder() {
  ui.sortOrder = ui.sortOrder === 'desc' ? 'asc' : 'desc'
}

// 布局模式切换（T20）：宫格 ⇄ 等高行。setLayoutMode 持久化；relayout watch 监听 layoutMode 触发重算。
function toggleLayoutMode() {
  ui.setLayoutMode(ui.layoutMode === 'grid' ? 'justified' : 'grid')
}

// S4（Part2 重排提速）：拖动中只更新本地显示值，**不写 ui.gridRowHeight** ——
// useJustifiedLayout 的 relayout watch 吃的是 store 值，原实现每 20px 步进即触发一次
// 全量重排（百万库下串行排队数次 × 秒级）；原 300ms 防抖只防了持久化、没防重排。
// 停手 250ms 后一次性提交 store + 持久化（setGridRowHeight），重排恰好一次。
const sliderRowHeight = ref(ui.gridRowHeight)
// 外部变化（启动读配置等）→ 回同步本地显示值。
watch(
  () => ui.gridRowHeight,
  (v) => {
    sliderRowHeight.value = v
  },
)
let rowHeightTimer: ReturnType<typeof setTimeout> | null = null

function onRowHeightInput(e: Event) {
  const value = parseInt((e.target as HTMLInputElement).value, 10)
  sliderRowHeight.value = value // 即时更新滑块与数值显示（纯本地，不触发重排）
  if (rowHeightTimer) clearTimeout(rowHeightTimer)
  rowHeightTimer = setTimeout(() => {
    ui.setGridRowHeight(value) // 停手后一次：写 store（触发唯一一次重排）+ 持久化
  }, 250)
}

function onGroupByChange(e: Event) {
  const value = (e.target as HTMLSelectElement).value as 'date' | 'folder' | 'none'
  ui.setGroupBy(value)
}

function onSortWithinGroupChange(e: Event) {
  const value = (e.target as HTMLSelectElement).value as 'datetime' | 'filename' | 'similarity'
  ui.setSortWithinGroup(value)
}
</script>

<style scoped>
.toolbar__left {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  min-width: 0;
  flex-shrink: 0;
}
.toolbar__breadcrumb {
  display: flex;
  align-items: baseline;
  gap: var(--spacing-sm);
  overflow: hidden;
}
.toolbar__title {
  font-size: var(--font-size-md);
  font-weight: 600;
  color: var(--color-text-primary);
  white-space: nowrap;
}
.toolbar__count {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  white-space: nowrap;
}

.toolbar__filters {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
  flex: 1;
  overflow-x: auto;
  padding: 0 var(--spacing-sm);
  scrollbar-width: none;
}
.toolbar__filters::-webkit-scrollbar {
  display: none;
}

.chip--clear {
  color: var(--color-accent);
  border-color: var(--color-accent);
}

/* 评分筛选 chip：容纳内联星级 + "≥N" 的 "+" 后缀；active 高亮沿用全局 .chip.active。 */
.chip--rating {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
.chip__rating-suffix {
  font-size: var(--font-size-xs);
  font-weight: 600;
  color: #ffc107;
  margin-left: 1px;
}

/* 颜色标签筛选 chip：容纳内联色块；active 高亮沿用全局 .chip.active。 */
.chip--color {
  display: inline-flex;
  align-items: center;
}

/* 日期范围筛选 chip：图标 + 文案/区间标签；active 高亮沿用全局 .chip.active。 */
.chip--date {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  white-space: nowrap;
}
.chip__date-label {
  font-variant-numeric: tabular-nums;
}

/* 日期弹层：Teleport 到 body，透明 backdrop 铺满视口拦截外部点击；弹层本体 fixed 定位。 */
.date-popover-backdrop {
  position: fixed;
  inset: 0;
  z-index: 300;
}
.date-popover {
  position: fixed;
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
  min-width: 220px;
  padding: var(--spacing-md);
  background: var(--color-bg-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.2);
}
.date-popover__field {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}
.date-popover__field > span {
  width: 1.5em;
  flex-shrink: 0;
}
.date-popover__field input[type='date'] {
  flex: 1;
  padding: 4px 8px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg-primary);
  color: var(--color-text-primary);
  font-size: var(--font-size-sm);
  outline: none;
  color-scheme: dark light; /* 让原生日期控件的弹出日历跟随主题明暗 */
}
.date-popover__field input[type='date']:focus {
  border-color: var(--color-accent);
}
.date-popover__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--spacing-sm);
  margin-top: 2px;
}
.date-popover__btn {
  padding: 4px 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition:
    border-color var(--transition-fast),
    color var(--transition-fast),
    background var(--transition-fast);
}
.date-popover__btn:hover {
  border-color: var(--color-border-strong);
  color: var(--color-text-primary);
}
.date-popover__btn--primary {
  background: var(--color-accent);
  border-color: var(--color-accent);
  color: #fff;
}
.date-popover__btn--primary:hover {
  opacity: 0.9;
  color: #fff;
}

.toolbar__right {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  flex-shrink: 0;
}

.toolbar__search-wrap {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
  background: var(--color-bg-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: 4px 10px;
  width: 200px;
  transition:
    border-color var(--transition-fast),
    width var(--transition-normal);
}
.toolbar__search-wrap.focused {
  border-color: var(--color-accent);
  width: 280px;
}
.toolbar__search-icon {
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}
.toolbar__search-scope {
  appearance: none;
  background: transparent;
  border: none;
  border-left: 1px solid var(--color-border);
  color: var(--color-text-secondary);
  font-size: var(--font-size-xs);
  padding: 0 2px 0 6px;
  margin-left: 4px;
  cursor: pointer;
  outline: none;
  transition: color var(--transition-fast);
}
.toolbar__search-scope:hover {
  color: var(--color-text-primary);
}
.toolbar__search {
  flex: 1;
  font-size: var(--font-size-sm);
  color: var(--color-text-primary);
  background: transparent;
  border: none;
  outline: none;
}
.toolbar__search::placeholder {
  color: var(--color-text-tertiary);
}

.toolbar__sort {
  display: flex;
  align-items: center;
  gap: 2px;
}
.toolbar__select {
  appearance: none;
  background: var(--color-bg-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  color: var(--color-text-secondary);
  font-size: var(--font-size-xs);
  padding: 4px 8px;
  cursor: pointer;
  outline: none;
  transition: border-color var(--transition-fast);
}
.toolbar__select:hover {
  border-color: var(--color-border-strong);
}

.toolbar-row-height {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
}
.toolbar-icon {
  display: flex;
  color: var(--color-text-secondary);
}
.row-height-slider {
  width: 80px;
  height: 4px;
  -webkit-appearance: none;
  appearance: none;
  background: var(--color-border);
  border-radius: 2px;
  cursor: pointer;
}
.row-height-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--color-accent);
  cursor: pointer;
  transition: transform var(--transition-fast);
}
.row-height-slider::-webkit-slider-thumb:hover {
  transform: scale(1.2);
}
.row-height-value {
  font-size: 11px;
  color: var(--color-text-tertiary);
  min-width: 40px;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

.toolbar-group {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
}

/* ── AI semantic search toggle ────────────────────────────────────────────── */
.toolbar__search-wrap.semantic-mode {
  border-color: color-mix(in srgb, var(--color-accent) 60%, transparent);
  background: color-mix(in srgb, var(--color-accent) 6%, var(--color-bg-surface));
}

.toolbar__search-mode-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 22px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border);
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  flex-shrink: 0;
  transition: all var(--transition-fast);
  padding: 0;
}
.toolbar__search-mode-btn .mode-text {
  font-size: 11px;
  font-weight: 800;
  letter-spacing: 0.5px;
}
.toolbar__search-mode-btn:hover {
  border-color: var(--color-accent);
  color: var(--color-accent);
}
.toolbar__search-mode-btn.mode-mixed {
  background: color-mix(in srgb, var(--color-accent) 15%, transparent);
  border-color: transparent;
  color: var(--color-accent);
}
.toolbar__search-mode-btn.mode-semantic {
  background: var(--color-accent);
  border-color: var(--color-accent);
  color: #fff;
  box-shadow: 0 0 8px color-mix(in srgb, var(--color-accent) 40%, transparent);
}
.toolbar__search-mode-btn.mode-normal {
  border-color: var(--color-border);
  background: transparent;
  color: var(--color-text-tertiary);
}
.toolbar__search-mode-btn:hover {
  opacity: 0.8;
}

/* Mixed Search Dropdown */
.mixed-search-dropdown {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  right: 0;
  background: var(--color-bg-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.2);
  padding: var(--spacing-xs);
  z-index: 100;
  /* fallback for backdrop-filter */
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
}

.dropdown-item {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: 8px 12px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition:
    background var(--transition-fast),
    color var(--transition-fast);
  color: var(--color-text-secondary);
}

.dropdown-item:hover,
.dropdown-item.selected {
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
}

.dropdown-item.selected {
  background: color-mix(in srgb, var(--color-accent) 15%, transparent);
  color: var(--color-accent);
}
.dropdown-item.selected .query {
  color: var(--color-accent);
}

.dropdown-icon-wrap {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--color-bg-surface);
  flex-shrink: 0;
}
.dropdown-item.selected .dropdown-icon-wrap {
  background: color-mix(in srgb, var(--color-accent) 25%, transparent);
}

.dropdown-text {
  flex: 1;
  font-size: var(--font-size-sm);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.query {
  font-weight: 600;
  margin: 0 4px;
}

.dropdown-fade-enter-active,
.dropdown-fade-leave-active {
  transition:
    opacity var(--transition-fast),
    transform var(--transition-fast);
}
.dropdown-fade-enter-from,
.dropdown-fade-leave-to {
  opacity: 0;
  transform: translateY(-5px);
}

.toolbar__search-spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid color-mix(in srgb, var(--color-accent) 25%, transparent);
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: toolbar-spin 0.6s linear infinite;
  flex-shrink: 0;
}
@keyframes toolbar-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
