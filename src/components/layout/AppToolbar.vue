<template>
  <div class="toolbar__left">
    <!-- Title / breadcrumb -->
    <!-- 标题 / 面包屑 -->
    <div class="toolbar__breadcrumb">
      <span class="toolbar__title">{{ title }}</span>
      <span v-if="media.stats" class="toolbar__count">{{ media.viewTotalItems.toLocaleString() }} 个项目</span>
    </div>
  </div>

  <!-- Filter chips -->
  <!-- 筛选芯片 -->
  <div class="toolbar__filters">
    <button
      class="chip"
      :class="{ active: filter.mediaTypes.includes('image') }"
      @click="filter.toggleMediaType('image')"
    ><ImageIcon :size="14" /> 图片</button>
    <button
      class="chip"
      :class="{ active: filter.mediaTypes.includes('video') }"
      @click="filter.toggleMediaType('video')"
    ><Video :size="14" /> 视频</button>
    <button
      class="chip"
      :class="{ active: filter.livePhotoOnly }"
      @click="filter.livePhotoOnly = !filter.livePhotoOnly"
    ><Sparkles :size="14" /> Live</button>
    <button
      v-if="filter.hasActiveFilters"
      class="chip chip--clear"
      @click="filter.clearFilters()"
    ><X :size="14" /> 清除筛选</button>
  </div>

  <!-- Right controls -->
  <!-- 右侧控件 -->
  <div class="toolbar__right">
    <!-- Fullscreen -->
    <!-- 全屏 -->
    <button
      class="btn-icon"
      :title="ui.isFullscreen ? $t('toolbar.exitFullscreen') : $t('toolbar.fullscreen')"
      @click="ui.toggleFullscreen()"
    >
      <Minimize2 v-if="ui.isFullscreen" :size="18" />
      <Maximize2 v-else :size="18" />
    </button>

    <!-- Search -->
    <!-- 搜索 -->
    <div class="toolbar__search-wrap" :class="{ focused: isSearchFocused, 'semantic-mode': ai.isSemanticMode }" style="position: relative;">
      <!-- Mode toggle button -->
      <!-- 模式切换按钮 -->
      <button
        class="toolbar__search-mode-btn"
        :class="['mode-' + ai.searchMode]"
        :title="ai.searchMode === 'mixed' ? '混合搜索 (聚焦)' : (ai.searchMode === 'semantic' ? '纯 AI 语义搜索' : '纯普通搜索')"
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
        :placeholder="ai.searchMode === 'mixed' ? '搜索图片...' : (ai.searchMode === 'semantic' ? '用自然语言搜索图片…' : $t('toolbar.searchPlaceholder'))"
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
              <span class="prefix">使用 AI 寻找有关</span>
              <span class="query">"{{ pendingMixedQuery }}"</span>
              <span class="suffix">的画面</span>
            </div>
          </div>
          <div
            class="dropdown-item"
            :class="{ selected: mixedDropdownIndex === 1 }"
            @click="executeMixedSearch(1)"
          >
            <div class="dropdown-icon-wrap"><Search :size="14" /></div>
            <div class="dropdown-text">
              <span class="prefix">搜索文件名包含</span>
              <span class="query">"{{ pendingMixedQuery }}"</span>
              <span class="suffix">的图片</span>
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
        :value="ui.gridRowHeight"
        @input="onRowHeightInput"
      />
      <span class="row-height-value">{{ ui.gridRowHeight }}px</span>
    </div>

    <!-- Group by selector | 分组选择器 -->
    <div class="toolbar-group">
      <select
        class="toolbar__select"
        :value="ui.groupBy"
        @change="onGroupByChange"
      >
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
        <option v-if="ai.isSemanticMode" value="similarity">{{ $t('toolbar.sortBySimilarity') }}</option>
      </select>

      <!-- Asc/Desc toggle -->
      <button
        class="btn-icon"
        :title="ui.sortOrder === 'desc' ? '从新到旧 / Z-A' : '从旧到新 / A-Z'"
        @click="toggleSortOrder"
      >
        <ArrowDown v-if="ui.sortOrder === 'desc'" :size="16" />
        <ArrowUp v-else :size="16" />
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { ImageIcon, Video, Sparkles, X, Maximize2, Minimize2, Search, ArrowDown, ArrowUp, Rows3, Bot } from '@lucide/vue'
import { useUiStore } from '../../stores/uiStore'
import { useFilterStore } from '../../stores/filterStore'
import { useMediaStore } from '../../stores/mediaStore'
import { useAiStore } from '../../stores/aiStore'
import { DEFAULTS } from '../../constants/defaults'

const emit = defineEmits<{
  (e: 'search', query: string): void
  (e: 'semantic-search', query: string): void
}>()

const ui     = useUiStore()
const filter = useFilterStore()
const media  = useMediaStore()
const ai     = useAiStore()

const isSearchFocused = ref(false)
const searchInputRef  = ref<HTMLInputElement>()
let searchTimer: ReturnType<typeof setTimeout> | null = null

const title = computed(() => {
  const map: Record<string, string> = {
    all:         '全部媒体',
    favorites:   '收藏',
    'live-photos': 'Live 照片',
    recent:      '最近',
    trash:       '回收站',
  }
  return map[ui.activeSmartAlbum] ?? '媒体库'
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
  }
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

let rowHeightTimer: ReturnType<typeof setTimeout> | null = null

function onRowHeightInput(e: Event) {
  const value = parseInt((e.target as HTMLInputElement).value, 10)
  ui.gridRowHeight = value  // 即时更新 UI
  // Debounce the actual layout recomputation
  // 防抖实际的布局重新计算
  if (rowHeightTimer) clearTimeout(rowHeightTimer)
  rowHeightTimer = setTimeout(() => {
    ui.setGridRowHeight(value) // 持久化
  }, 300)
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
.toolbar__filters::-webkit-scrollbar { display: none; }

.chip--clear {
  color: var(--color-accent);
  border-color: var(--color-accent);
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
  transition: border-color var(--transition-fast), width var(--transition-normal);
}
.toolbar__search-wrap.focused {
  border-color: var(--color-accent);
  width: 280px;
}
.toolbar__search-icon { color: var(--color-text-tertiary); flex-shrink: 0; }
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
.toolbar__search::placeholder { color: var(--color-text-tertiary); }

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
.toolbar__select:hover { border-color: var(--color-border-strong); }

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
  transition: background var(--transition-fast), color var(--transition-fast);
  color: var(--color-text-secondary);
}

.dropdown-item:hover, .dropdown-item.selected {
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
  transition: opacity var(--transition-fast), transform var(--transition-fast);
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
  to { transform: rotate(360deg); }
}
</style>
