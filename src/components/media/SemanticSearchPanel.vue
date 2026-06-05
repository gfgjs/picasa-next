<template>
  <!-- Semantic search results overlay panel -->
  <!-- 语义搜索结果覆盖面板 -->
  <Transition name="semantic-panel">
    <div v-if="ai.isSemanticMode" 
         class="semantic-panel" 
         :class="{ 'semantic-panel--has-results': ai.semanticQuery && !ai.isSearching && (media.layoutSummary?.totalItems || 0) > 0 }"
         role="region" aria-label="AI 语义搜索结果">

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

      <!-- Results state (query exists, not searching) -->
      <!-- 结果状态（存在查询，非搜索中） -->
      <template v-else-if="ai.semanticQuery">
        <!-- Results meta (ALWAYS shown if we have a query, so slider is available) -->
        <div class="semantic-panel__results-meta">
          <span class="semantic-panel__results-count">找到 {{ media.layoutSummary?.totalItems || 0 }} 张相关图片</span>
          <div class="threshold-slider">
            <label>相似度 &ge; {{ (localThreshold * 100).toFixed(0) }}%</label>
            <input type="range" min="0.1" max="0.5" step="0.01" :value="localThreshold" @input="onSliderInput" @change="onSliderChange" />
          </div>
        </div>

        <!-- Empty state (shown if 0 items) -->
        <div v-if="(media.layoutSummary?.totalItems || 0) === 0" class="semantic-panel__empty">
          <Search :size="32" class="semantic-panel__empty-icon" />
          <p>未找到匹配的图片</p>
          <p class="semantic-panel__hint">尝试换一种描述方式，或降低相似度阈值</p>
        </div>
      </template>

    </div>
  </Transition>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { Sparkles, Zap, Square, RefreshCw, AlertCircle, Search } from '@lucide/vue'
import { useAiStore } from '../../stores/aiStore'
import { useMediaStore } from '../../stores/mediaStore'

const ai = useAiStore()
const media = useMediaStore()
const isInitialising = ref(false)

// Debounce logic for the slider
const localThreshold = ref(ai.similarityThreshold)
let sliderTimeout: ReturnType<typeof setTimeout> | null = null

watch(() => ai.similarityThreshold, (val) => {
  localThreshold.value = val
})

function onSliderInput(e: Event) {
  const val = parseFloat((e.target as HTMLInputElement).value)
  localThreshold.value = val

  if (sliderTimeout) clearTimeout(sliderTimeout)
  sliderTimeout = setTimeout(() => {
    ai.similarityThreshold = val
  }, 300) // 300ms debounce
}

function onSliderChange(e: Event) {
  const val = parseFloat((e.target as HTMLInputElement).value)
  localThreshold.value = val
  if (sliderTimeout) clearTimeout(sliderTimeout)
  ai.similarityThreshold = val
}

async function initAndStart() {
  isInitialising.value = true
  try {
    await ai.initEngine()
    await ai.startAnalysis()
  } finally {
    isInitialising.value = false
  }
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
  pointer-events: auto;
}

.semantic-panel--has-results {
  position: relative;
  inset: auto;
  flex: 0 0 auto;
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
  pointer-events: auto;
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
  pointer-events: auto;
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
  pointer-events: auto;
  background: var(--color-bg-primary);
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

/* ── Results meta ────────────────────────────────────────────────────────── */
.semantic-panel__results-meta {
  padding: 10px var(--spacing-md);
  background: var(--color-bg-primary);
  border-bottom: 1px solid var(--color-border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  pointer-events: auto;
}
.semantic-panel__results-count {
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  font-weight: 500;
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
</style>
