<template>
  <!-- Semantic search results overlay panel -->
  <!-- 搜索结果覆盖面板 -->
  <Transition name="semantic-panel">
    <div v-if="uiStore.searchQuery || ai.semanticQuery || ai.isSearching" class="semantic-panel" role="region" aria-label="搜索结果">

      <!-- Header -->
      <div class="semantic-panel__header">
        <div class="semantic-panel__title">
          <Sparkles v-if="ai.isSemanticMode" :size="16" class="semantic-panel__icon" />
          <Search v-else :size="16" class="semantic-panel__icon" />
          <span>{{ ai.isSemanticMode ? 'AI 语义搜索' : '文件名搜索' }}</span>
          <span class="semantic-panel__provider" v-if="ai.isSemanticMode && ai.status.provider">
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

        <!-- Controls (Semantic mode only) -->
        <div class="semantic-panel__controls" v-if="ai.isSemanticMode">
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
      <div v-else-if="(ai.isSemanticMode && !ai.semanticQuery) || (!ai.isSemanticMode && !uiStore.searchQuery)" class="semantic-panel__empty">
        <Sparkles v-if="ai.isSemanticMode" :size="40" class="semantic-panel__empty-icon" />
        <Search v-else :size="40" class="semantic-panel__empty-icon" />
        <p v-if="ai.isSemanticMode">在上方搜索框中用自然语言描述图片</p>
        <p v-else>在上方搜索框中输入文件名以查找图片</p>
        <p class="semantic-panel__hint" v-if="ai.isSemanticMode">例：「海边日落」「两人自拍」「猫咪玩耍」</p>
      </div>

      <!-- Searching spinner -->
      <!-- 搜索中旋转器 -->
      <div v-else-if="ai.isSearching" class="semantic-panel__loading">
        <div class="semantic-panel__spinner" />
        <span>{{ ai.isSemanticMode ? '语义分析中…' : '搜索中…' }}</span>
      </div>

      <!-- No results -->
      <!-- 无结果 -->
      <div v-else-if="currentResults.length === 0" class="semantic-panel__empty">
        <Search :size="32" class="semantic-panel__empty-icon" />
        <p>未找到匹配的图片</p>
        <p class="semantic-panel__hint" v-if="ai.isSemanticMode">尝试换一种描述方式，或先完成 AI 分析（{{ ai.status.analyzedItems }}/{{ ai.status.totalItems }} 张）</p>
        <p class="semantic-panel__hint" v-else>尝试使用其他关键字搜索</p>
      </div>

      <!-- Results grid -->
      <!-- 结果网格 -->
      <div v-else class="semantic-panel__results" ref="resultsContainerRef">
        <div class="semantic-panel__results-meta">
          找到 {{ currentResults.length }} 张相关图片
        </div>
        <div class="semantic-panel__justified" :style="{ height: totalHeight + 'px' }">
          <template v-for="row in layoutRows" :key="row.y">
            <SemanticResultCard
              v-for="item in row.items"
              :key="item.id"
              :item="item"
              :cache-dir="cacheDir"
              :style="{
                position: 'absolute',
                top: `${item.offsetY}px`,
                left: `${item.offsetX}px`,
                width: `${item.scaledWidth}px`,
                height: `${item.scaledHeight}px`
              }"
              @click="emit('item-click', item)"
            />
          </template>
        </div>
      </div>

    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, shallowRef, computed } from 'vue'
import { Sparkles, Zap, Square, RefreshCw, AlertCircle, Search } from '@lucide/vue'
import { appDataDir, join } from '@tauri-apps/api/path'
import { useAiStore } from '../../stores/aiStore'
import { useUiStore } from '../../stores/uiStore'
import SemanticResultCard from './SemanticResultCard.vue'
import type { SemanticSearchResult } from '../../types/ai'
import type { SearchResult } from '../../types/media'
import { computeJustifiedLayout, type LayoutRowResult } from '../../utils/justifiedLayout'

const emit = defineEmits<{
  (e: 'item-click', item: SemanticSearchResult | SearchResult): void
}>()

const ai = useAiStore()
const uiStore = useUiStore()
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

// ── Justified Layout ──────────────────────────────────────────────────────
type CombinedSearchResult = SemanticSearchResult | SearchResult

const resultsContainerRef = ref<HTMLElement | null>(null)
const layoutRows = shallowRef<LayoutRowResult<CombinedSearchResult & { w: number, h: number }>[]>([])
const totalHeight = ref(0)
let resizeObserver: ResizeObserver | null = null

const currentResults = computed<CombinedSearchResult[]>(() => {
  return ai.isSemanticMode ? ai.semanticResults : ai.standardResults
})

function updateLayout() {
  if (!resultsContainerRef.value || currentResults.value.length === 0) {
    layoutRows.value = []
    totalHeight.value = 0
    return
  }
  const cw = resultsContainerRef.value.clientWidth - 32 // padding
  if (cw <= 0) return

  const items = currentResults.value.map((item: CombinedSearchResult) => ({
    ...item,
    w: item.width || 1,
    h: item.height || 1,
  }))

  const rows = computeJustifiedLayout(items, cw, uiStore.gridRowHeight, 4)
  layoutRows.value = rows
  
  if (rows.length > 0) {
    const lastRow = rows[rows.length - 1]
    totalHeight.value = lastRow.y + lastRow.height
  } else {
    totalHeight.value = 0
  }
}

watch(currentResults, updateLayout, { deep: false })
watch(() => uiStore.gridRowHeight, updateLayout)

onMounted(() => {
  resizeObserver = new ResizeObserver(() => {
    updateLayout()
  })
  if (resultsContainerRef.value) {
    resizeObserver.observe(resultsContainerRef.value)
  }
})

watch(resultsContainerRef, (el) => {
  if (resizeObserver) {
    resizeObserver.disconnect()
    if (el) resizeObserver.observe(el)
  }
})

onUnmounted(() => {
  if (resizeObserver) resizeObserver.disconnect()
})
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
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  margin-bottom: var(--spacing-md);
}
.semantic-panel__justified {
  position: relative;
  width: 100%;
}
</style>
