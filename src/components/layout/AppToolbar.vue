<template>
  <div class="toolbar__left">
    <!-- Title / breadcrumb -->
    <!-- 标题 / 面包屑 -->
    <div class="toolbar__breadcrumb">
      <span class="toolbar__title">{{ title }}</span>
      <span v-if="media.stats" class="toolbar__count">{{ media.totalItems.toLocaleString() }} 个项目</span>
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
    <div class="toolbar__search-wrap" :class="{ focused: isSearchFocused, 'semantic-mode': ai.isSemanticMode }">
      <!-- Mode toggle button -->
      <!-- 模式切换按钮 -->
      <button
        class="toolbar__search-mode-btn"
        :class="{ active: ai.isSemanticMode }"
        :title="ai.isSemanticMode ? '切换到普通搜索' : '切换到 AI 语义搜索'"
        @click="toggleSearchMode"
      >
        <Sparkles :size="13" />
      </button>

      <Search :size="14" class="toolbar__search-icon" />
      <input
        ref="searchInputRef"
        class="toolbar__search"
        :value="ai.isSemanticMode ? ai.semanticQuery : ui.searchQuery"
        @input="onSearchInput"
        @focus="isSearchFocused = true"
        @blur="isSearchFocused = false"
        :placeholder="ai.isSemanticMode ? '用自然语言搜索图片…' : $t('toolbar.searchPlaceholder')"
        type="search"
      />
      <!-- AI searching indicator -->
      <!-- AI 搜索中指示器 -->
      <span v-if="ai.isSearching" class="toolbar__search-spinner" />
    </div>

    <!-- View sort -->
    <!-- 视图排序 -->
    <div class="toolbar__sort">
      <select class="toolbar__select" v-model="ui.sortBy" @change="onSortChange">
        <option value="sort_datetime">{{ $t('toolbar.sortByDate') }}</option>
        <option value="file_mtime">按修改时间</option>
        <option value="file_name">{{ $t('toolbar.sortByName') }}</option>
        <option value="file_size">{{ $t('toolbar.sortBySize') }}</option>
      </select>
      <button
        class="btn-icon"
        :title="ui.sortOrder === 'desc' ? '从新到旧' : '从旧到新'"
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
import { ImageIcon, Video, Sparkles, X, Maximize2, Minimize2, Search, ArrowDown, ArrowUp } from '@lucide/vue'
import { useUiStore } from '../../stores/uiStore'
import { useFilterStore } from '../../stores/filterStore'
import { useMediaStore } from '../../stores/mediaStore'
import { useAiStore } from '../../stores/aiStore'
import { DEFAULTS } from '../../constants/defaults'

const emit = defineEmits<{
  (e: 'search', query: string): void
  (e: 'semantic-search', query: string): void
  (e: 'sort-change'): void
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

function onSearchInput(e: Event) {
  const val = (e.target as HTMLInputElement).value
  if (ai.isSemanticMode) {
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

function toggleSearchMode() {
  ai.toggleSearchMode()
  // Clear current search when switching modes | 切换模式时清除当前搜索
  if (ai.isSemanticMode) {
    ui.searchQuery = ''
  } else {
    ai.semanticQuery === '' // reset handled in store
  }
}

function toggleSortOrder() {
  ui.sortOrder = ui.sortOrder === 'desc' ? 'asc' : 'desc'
  emit('sort-change')
}

function onSortChange() {
  emit('sort-change')
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

/* ── AI semantic search toggle ────────────────────────────────────────────── */
.toolbar__search-wrap.semantic-mode {
  border-color: color-mix(in srgb, var(--color-accent) 60%, transparent);
  background: color-mix(in srgb, var(--color-accent) 6%, var(--color-bg-surface));
}

.toolbar__search-mode-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
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
.toolbar__search-mode-btn:hover {
  border-color: var(--color-accent);
  color: var(--color-accent);
}
.toolbar__search-mode-btn.active {
  background: var(--color-accent);
  border-color: var(--color-accent);
  color: #fff;
  box-shadow: 0 0 8px color-mix(in srgb, var(--color-accent) 40%, transparent);
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
