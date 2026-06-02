<template>
  <div class="toolbar__left">
    <!-- Title / breadcrumb -->
    <div class="toolbar__breadcrumb">
      <span class="toolbar__title">{{ title }}</span>
      <span v-if="media.stats" class="toolbar__count">{{ media.totalItems.toLocaleString() }} 个项目</span>
    </div>
  </div>

  <!-- Filter chips -->
  <div class="toolbar__filters">
    <button
      class="chip"
      :class="{ active: filter.mediaTypes.includes('image') }"
      @click="filter.toggleMediaType('image')"
    >🖼️ 图片</button>
    <button
      class="chip"
      :class="{ active: filter.mediaTypes.includes('video') }"
      @click="filter.toggleMediaType('video')"
    >🎬 视频</button>
    <button
      class="chip"
      :class="{ active: filter.livePhotoOnly }"
      @click="filter.livePhotoOnly = !filter.livePhotoOnly"
    >✨ Live</button>
    <button
      v-if="filter.hasActiveFilters"
      class="chip chip--clear"
      @click="filter.clearFilters()"
    >✕ 清除筛选</button>
  </div>

  <!-- Right controls -->
  <div class="toolbar__right">
    <!-- Fullscreen -->
    <button
      class="btn-icon"
      :title="ui.isFullscreen ? '退出全屏 (F11)' : '全屏 (F11)'"
      @click="ui.toggleFullscreen()"
    >
      {{ ui.isFullscreen ? '🗗' : '🖵' }}
    </button>

    <!-- Search -->
    <div class="toolbar__search-wrap" :class="{ focused: isSearchFocused }">
      <span class="toolbar__search-icon">🔍</span>
      <input
        ref="searchInputRef"
        class="toolbar__search"
        :value="ui.searchQuery"
        @input="onSearchInput"
        @focus="isSearchFocused = true"
        @blur="isSearchFocused = false"
        placeholder="搜索文件名..."
        type="search"
      />
    </div>

    <!-- View sort -->
    <div class="toolbar__sort">
      <select class="toolbar__select" v-model="ui.sortBy" @change="onSortChange">
        <option value="sort_datetime">按拍摄时间</option>
        <option value="file_mtime">按修改时间</option>
        <option value="file_name">按文件名</option>
        <option value="file_size">按文件大小</option>
      </select>
      <button
        class="btn-icon"
        :title="ui.sortOrder === 'desc' ? '从新到旧' : '从旧到新'"
        @click="toggleSortOrder"
      >
        {{ ui.sortOrder === 'desc' ? '↓' : '↑' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useUiStore } from '../../stores/uiStore'
import { useFilterStore } from '../../stores/filterStore'
import { useMediaStore } from '../../stores/mediaStore'
import { DEFAULTS } from '../../constants/defaults'

const emit = defineEmits<{
  (e: 'search', query: string): void
  (e: 'sort-change'): void
}>()

const ui     = useUiStore()
const filter = useFilterStore()
const media  = useMediaStore()

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
  ui.searchQuery = val
  if (searchTimer) clearTimeout(searchTimer)
  searchTimer = setTimeout(() => emit('search', val), DEFAULTS.SEARCH_DEBOUNCE_MS)
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
.toolbar__search-icon { font-size: 13px; }
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
</style>
