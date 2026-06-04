<template>
  <!-- Semantic search results overlay panel -->
  <!-- 语义搜索结果覆盖面板 -->
  <Transition name="semantic-panel">
    <div v-if="ai.isSemanticMode" class="semantic-panel" role="region" aria-label="AI 语义搜索结果">

      <!-- Header -->
      <div class="semantic-panel__header">
        <div class="semantic-panel__title">
          <Sparkles :size="16" class="semantic-panel__icon" />
          <span>AI 语义搜索</span>
          <span class="semantic-panel__provider" v-if="ai.status.provider">
            {{ ai.providerLabel }}
          </span>
        </div>

        <!-- Analysis progress badge (shown during background indexing) -->
        <!-- 分析进度徽章（后台索引期间显示） -->
        <div
          v-if="ai.status.totalItems > 0"
          class="semantic-panel__progress"
          :title="`已分析 ${ai.status.analyzedItems} / ${ai.status.totalItems} 张图片`"
        >
          <div
            class="semantic-panel__progress-bar"
            :style="{ width: ai.analyzeProgress + '%' }"
          />
          <span class="semantic-panel__progress-text">
            {{ ai.analyzeProgress }}%
          </span>
        </div>

        <!-- Controls -->
        <div class="semantic-panel__controls">
          <!-- Start analysis if not yet running -->
          <!-- 如果尚未运行则启动分析 -->
          <button
            v-if="!ai.status.clipLoaded || (!ai.status.isAnalyzing && ai.status.pendingItems > 0)"
            class="semantic-panel__btn"
            @click="initAndStart"
            :disabled="isInitialising"
          >
            <Zap v-if="!isInitialising" :size="13" />
            <span v-if="isInitialising" class="btn-spinner" />
            {{ isInitialising ? '初始化中…' : '开始 AI 分析' }}
          </button>
          <!-- Stop analysis -->
          <!-- 停止分析 -->
          <button
            v-else-if="ai.status.isAnalyzing"
            class="semantic-panel__btn semantic-panel__btn--danger"
            @click="ai.stopAnalysis()"
          >
            <Square :size="13" />
            停止
          </button>
          <!-- Rebuild -->
          <!-- 重建 -->
          <button
            class="semantic-panel__btn semantic-panel__btn--ghost"
            @click="ai.rebuildEmbeddings()"
            title="重新分析所有图片"
          >
            <RefreshCw :size="13" />
          </button>
        </div>
      </div>

      <!-- Error state -->
      <!-- 错误状态 -->
      <div v-if="ai.searchError" class="semantic-panel__error">
        <AlertCircle :size="14" />
        {{ ai.searchError }}
      </div>

      <!-- Empty state: no query yet -->
      <!-- 空状态：尚无查询 -->
      <div v-else-if="!ai.semanticQuery && !ai.isSearching" class="semantic-panel__empty">
        <Sparkles :size="40" class="semantic-panel__empty-icon" />
        <p>在上方搜索框中用自然语言描述图片</p>
        <p class="semantic-panel__hint">例：「海边日落」「两人自拍」「猫咪玩耍」</p>
      </div>

      <!-- Searching spinner -->
      <!-- 搜索中旋转器 -->
      <div v-else-if="ai.isSearching" class="semantic-panel__loading">
        <div class="semantic-panel__spinner" />
        <span>语义分析中…</span>
      </div>

      <!-- No results -->
      <!-- 无结果 -->
      <div v-else-if="ai.semanticResults.length === 0 && ai.semanticQuery" class="semantic-panel__empty">
        <Search :size="32" class="semantic-panel__empty-icon" />
        <p>未找到匹配的图片</p>
        <p class="semantic-panel__hint">尝试换一种描述方式，或先完成 AI 分析（{{ ai.status.analyzedItems }}/{{ ai.status.totalItems }} 张）</p>
      </div>

      <!-- Results grid -->
      <!-- 结果网格 -->
      <div v-else class="semantic-panel__results" ref="semanticResultsOuterRef">
        <div class="semantic-panel__results-meta">
          <span>找到 {{ ai.visibleSemanticResults.length }} 张相关图片</span>
          <div class="threshold-slider" v-if="ai.semanticResults.length > 0">
            <label>相似度 &ge; {{ (ai.similarityThreshold * 100).toFixed(0) }}%</label>
            <input type="range" min="0.1" max="0.5" step="0.01" v-model.number="ai.similarityThreshold" />
          </div>
        </div>
        <div
          v-if="layoutItems.length > 0"
          ref="resultsContainerRef"
          class="search-results-justified"
          :style="{ height: layoutTotalHeight + 'px', position: 'relative', width: '100%' }"
        >
          <div
            v-for="item in layoutItems"
            :key="item.id"
            class="search-result-positioned"
            :style="{
              position: 'absolute',
              left: item.x + 'px',
              top: item.y + 'px',
              width: item.w + 'px',
              height: item.h + 'px',
              transition: 'all 0.3s ease'
            }"
          >
            <SemanticResultCard
              :item="getResultById(item.id)!"
              :cache-dir="cacheDir"
              :is-selected="selection.isSelected(item.id)"
              :is-selection-mode="selection.isSelectionMode.value"
              @click="(i, e) => handleCardClick(i, e)"
              @select="(i) => selection.toggleSelect(i.id)"
              @pointerdown="selection.onPointerDown(item.id, $event)"
            />
          </div>
        </div>
      </div>

      <!-- Selection toolbar -->
      <SelectionToolbar
        @batch-favorite="batchFavorite"
        @batch-unfavorite="batchUnfavorite"
        @batch-delete="batchDelete"
        @select-all="selection.selectAll(getAllVisibleItemIds())"
        @invert-selection="selection.invertSelection(getAllVisibleItemIds())"
      />
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { Sparkles, Zap, Square, RefreshCw, AlertCircle, Search } from '@lucide/vue'
import { appDataDir, join } from '@tauri-apps/api/path'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { useAiStore } from '../../stores/aiStore'
import { useUiStore } from '../../stores/uiStore'
import { useSelection } from '../../composables/useSelection'
import { useMediaStore } from '../../stores/mediaStore'
import SemanticResultCard from './SemanticResultCard.vue'
import SelectionToolbar from './SelectionToolbar.vue'
import type { SemanticSearchResult } from '../../types/ai'
import { computeJustifiedLayout, type PositionedItem } from '../../utils/justifiedLayout'
import { watch } from 'vue'

const emit = defineEmits<{
  (e: 'item-click', item: SemanticSearchResult): void
}>()

const ai = useAiStore()
const ui = useUiStore()
const { t } = useI18n()
const selection = useSelection()
const media = useMediaStore()
const isInitialising = ref(false)

// Resolve cache directory (same logic as MediaGrid.vue)
// 解析缓存目录（与 MediaGrid.vue 相同逻辑）
const cacheDir = ref('')
onMounted(async () => {
  try {
    const dir = await appDataDir()
    cacheDir.value = (await join(dir, 'cache')).replace(/\\/g, '/')
  } catch (e) {
    console.warn('[SemanticSearchPanel] Failed to resolve cacheDir:', e)
  }
})

async function initAndStart() {
  isInitialising.value = true
  try {
    await ai.initEngine()
    await ai.startAnalysis()
  } finally {
    isInitialising.value = false
  }
}

// ── Justified Layout ─────────────────────────────────────────────────────
const resultsContainerRef = ref<HTMLElement | null>(null)
const containerWidth = ref(0)
const layoutItems = ref<PositionedItem[]>([])
const layoutTotalHeight = ref(0)
let resizeObserver: ResizeObserver | null = null

function updateLayout() {
  if (containerWidth.value <= 0 || ai.visibleSemanticResults.length === 0) {
    layoutItems.value = []
    layoutTotalHeight.value = 0
    return
  }
  const result = computeJustifiedLayout(
    ai.visibleSemanticResults,
    containerWidth.value,
    180, // targetRowHeight
    8    // gap
  )
  layoutItems.value = result.items
  layoutTotalHeight.value = result.totalHeight
}

onMounted(() => {
  resizeObserver = new ResizeObserver((entries) => {
    const w = entries[0].contentRect.width
    if (w > 0 && Math.abs(w - containerWidth.value) > 1) {
      containerWidth.value = w
      updateLayout()
    }
  })
  
  // Observe a parent element of the layout if we want, or the results wrapper.
  // We'll observe the results parent meta div or we can watch the panel itself.
  // Since resultsContainerRef might not exist initially, we should observe the document or wait.
})

// Use a ref for the outer container so we can measure its width even if it's empty
const semanticResultsOuterRef = ref<HTMLElement | null>(null)

watch([() => ai.visibleSemanticResults, () => ai.similarityThreshold], () => {
  updateLayout()
}, { deep: true })

onMounted(() => {
  if (resizeObserver && semanticResultsOuterRef.value) {
    resizeObserver.observe(semanticResultsOuterRef.value)
  }
})

watch(semanticResultsOuterRef, (el) => {
  if (resizeObserver) {
    resizeObserver.disconnect()
    if (el) {
      resizeObserver.observe(el)
      // trigger immediate layout
      containerWidth.value = el.clientWidth || 0
      updateLayout()
    }
  }
})

function getResultById(id: number): SemanticSearchResult | undefined {
  return ai.visibleSemanticResults.find(r => r.id === id)
}

// ── Detail & Selection ───────────────────────────────────────────────────

function getAllVisibleItemIds(): number[] {
  return ai.visibleSemanticResults.map(i => i.id)
}

function handleCardClick(item: SemanticSearchResult, event: MouseEvent) {
  if (selection.wasDrag()) return

  if (event.ctrlKey || event.metaKey) {
    selection.toggleSelect(item.id)
    return
  }

  if (selection.isSelectionMode.value && event.shiftKey) {
    selection.selectRange(selection.lastClickedId.value, item.id, getAllVisibleItemIds())
    return
  }

  emit('item-click', item)
}

function onKeyDown(e: KeyboardEvent) {
  selection.onKeyDown(e, getAllVisibleItemIds)
}

onMounted(() => document.addEventListener('keydown', onKeyDown))
onBeforeUnmount(() => document.removeEventListener('keydown', onKeyDown))

// ── Batch Actions ────────────────────────────────────────────────────────

async function batchFavorite() {
  const ids = Array.from(selection.selectedIds.value)
  if (ids.length === 0) return
  await invoke('batch_toggle_favorite', { itemIds: ids, value: true })
  
  // Update visible items
  for (const item of ai.visibleSemanticResults) {
    if (selection.isSelected(item.id)) {
      // It's possible SemanticSearchResult doesn't have isFavorited, but if it did, we'd update it here.
    }
  }
  await media.loadStats()
  
  if (typeof (ui as any).showToast === 'function') {
    ;(ui as any).showToast(t('selection.favorited', { count: ids.length }))
  }
  selection.clearSelection()
}

async function batchUnfavorite() {
  const ids = Array.from(selection.selectedIds.value)
  if (ids.length === 0) return
  await invoke('batch_toggle_favorite', { itemIds: ids, value: false })
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
  
  // Update local results view
  ai.semanticResults = ai.semanticResults.filter(item => !selection.isSelected(item.id))
  
  if (typeof (ui as any).showToast === 'function') {
    ;(ui as any).showToast(t('selection.deleted', { count: ids.length }))
  }
  selection.clearSelection()
}
</script>

<style scoped>
/* ── Panel container ─────────────────────────────────────────────────────── */
.semantic-panel {
  position: absolute;
  inset: 0;
  z-index: 10;
  background: var(--color-bg-primary);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* Transition */
.semantic-panel-enter-active,
.semantic-panel-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.semantic-panel-enter-from,
.semantic-panel-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}

/* ── Header ─────────────────────────────────────────────────────────────── */
.semantic-panel__header {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: 10px var(--spacing-md);
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg-surface);
  flex-shrink: 0;
}

.semantic-panel__title {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--color-text-primary);
}

.semantic-panel__icon {
  color: var(--color-accent);
}

.semantic-panel__provider {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  font-weight: 400;
  background: var(--color-bg-overlay);
  padding: 1px 6px;
  border-radius: 99px;
  border: 1px solid var(--color-border);
}

.semantic-panel__progress {
  position: relative;
  height: 18px;
  background: var(--color-bg-overlay);
  border-radius: 99px;
  overflow: hidden;
  flex: 1;
  max-width: 120px;
  border: 1px solid var(--color-border);
}
.semantic-panel__progress-bar {
  position: absolute;
  inset: 0 auto 0 0;
  background: linear-gradient(90deg, var(--color-accent), color-mix(in srgb, var(--color-accent) 70%, #a78bfa));
  border-radius: inherit;
  transition: width 0.4s ease;
}
.semantic-panel__progress-text {
  position: relative;
  z-index: 1;
  font-size: 10px;
  font-weight: 600;
  color: var(--color-text-secondary);
  padding: 0 6px;
  line-height: 18px;
}

.semantic-panel__controls {
  display: flex;
  align-items: center;
  gap: var(--spacing-xs);
  margin-left: auto;
}

.semantic-panel__btn {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: var(--font-size-xs);
  font-weight: 500;
  padding: 5px 10px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-bg-surface);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: all var(--transition-fast);
}
.semantic-panel__btn:hover {
  background: var(--color-accent);
  border-color: var(--color-accent);
  color: #fff;
}
.semantic-panel__btn--danger:hover {
  background: hsl(0 70% 50%);
  border-color: hsl(0 70% 50%);
}
.semantic-panel__btn--ghost {
  padding: 5px;
}
.semantic-panel__btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-spinner {
  display: inline-block;
  width: 12px;
  height: 12px;
  border: 2px solid rgba(255,255,255,0.3);
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

/* ── Body states ─────────────────────────────────────────────────────────── */
.semantic-panel__error {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-md);
  color: hsl(0 70% 60%);
  font-size: var(--font-size-sm);
  background: color-mix(in srgb, hsl(0 70% 60%) 8%, transparent);
  border-bottom: 1px solid color-mix(in srgb, hsl(0 70% 60%) 20%, transparent);
}

.semantic-panel__empty,
.semantic-panel__loading {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-sm);
  color: var(--color-text-tertiary);
  font-size: var(--font-size-sm);
  text-align: center;
  padding: var(--spacing-xl);
}
.semantic-panel__empty-icon {
  opacity: 0.25;
  margin-bottom: var(--spacing-sm);
  color: var(--color-accent);
}
.semantic-panel__hint {
  font-size: var(--font-size-xs);
  opacity: 0.6;
}

.semantic-panel__spinner {
  width: 32px;
  height: 32px;
  border: 3px solid color-mix(in srgb, var(--color-accent) 20%, transparent);
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

/* ── Results ─────────────────────────────────────────────────────────────── */
.semantic-panel__results {
  flex: 1;
  overflow-y: auto;
  padding: var(--spacing-md);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-md);
}
.semantic-panel__results-meta {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  padding: 0 4px;
}
.threshold-slider {
  display: flex;
  align-items: center;
  gap: 8px;
}
.threshold-slider label {
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
.search-results-justified {
  /* wrapper for absolute elements */
  margin-top: 8px;
}
.search-result-positioned {
  /* Ensure the hovered element sits above the others */
}
.search-result-positioned:hover {
  z-index: 10;
}
</style>
