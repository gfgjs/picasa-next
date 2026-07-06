<template>
  <!-- Individual semantic search result card -->
  <!-- 单个语义搜索结果卡片 -->
  <button
    class="result-card"
    :class="{
      'result-card--selected': isSelected,
      'result-card--selection-mode': isSelectionMode,
    }"
    :data-item-id="item.id"
    :title="$t('semantic.resultTitle', { fileName: item.fileName, percent: similarityPercent })"
    @click="emit('click', item, $event)"
    @pointerdown="emit('pointerdown', $event)"
  >
    <!-- Thumbnail -->
    <div class="result-card__thumb-wrap">
      <img
        v-if="thumbSrc"
        :src="thumbSrc"
        :alt="item.fileName"
        class="result-card__thumb"
        loading="lazy"
        decoding="async"
      />
      <div v-else class="result-card__thumb-placeholder">
        <ImageIcon :size="24" />
      </div>

      <!-- Similarity badge -->
      <!-- 相似度徽章 -->
      <div class="result-card__badge" :class="badgeClass">{{ similarityPercent }}%</div>

      <!-- Selection Overlay -->
      <!-- 选择遮罩 -->
      <div v-if="isSelected" class="result-card__overlay"></div>

      <!-- Selection Indicator (CheckCircle) -->
      <!-- 选择指示器 (勾选圆圈) -->
      <div
        class="result-card__select-btn"
        :class="{ 'is-selected': isSelected }"
        @click.stop="emit('select', item, $event)"
      >
        <div class="select-icon-bg"></div>
        <CheckCircle2 :size="20" class="select-icon" />
      </div>
    </div>

    <!-- File name -->
    <div class="result-card__name">{{ item.fileName }}</div>
  </button>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { ImageIcon, CheckCircle2 } from '@lucide/vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import type { SemanticSearchResult } from '../../types/ai'

const props = defineProps<{
  item: SemanticSearchResult
  /** Absolute path to app cache dir (e.g. C:/Users/.../AppData/.../cache) */
  /** 应用缓存目录的绝对路径 */
  cacheDir?: string
  isSelected?: boolean
  isSelectionMode?: boolean
}>()
const emit = defineEmits<{
  (e: 'click', item: SemanticSearchResult, event: MouseEvent): void
  (e: 'select', item: SemanticSearchResult, event: MouseEvent): void
  (e: 'pointerdown', event: PointerEvent): void
}>()

/**
 * Resolve thumb_path to a displayable URL, mirroring MediaThumb.vue's logic:
 *   status=1 → relative path under cacheDir/thumbnails/ (e.g. "300/a3/xxx.webp")
 *   status=3 → absolute path already resolved by backend SQL JOIN
 *   others   → null (show placeholder)
 *
 * thumbPath 路径解析逻辑（与 MediaThumb.vue 完全一致）：
 *   status=1 → 相对路径，需要拼接 cacheDir/thumbnails/
 *   status=3 → 后端已通过 SQL JOIN 解析为绝对路径
 *   其他     → null（显示占位图）
 */
const thumbSrc = computed(() => {
  const { thumbPath, thumbStatus } = props.item
  if (!thumbPath) return null
  try {
    if (thumbStatus === 1) {
      // Relative path stored in DB — must prepend full cache dir
      // 数据库中存储的相对路径 — 必须拼接完整缓存目录
      if (!props.cacheDir) return null
      const abs = `${props.cacheDir}/thumbnails/${thumbPath}`.replace(/\\/g, '/')
      return convertFileSrc(abs)
    }
    if (thumbStatus === 3) {
      // Absolute path resolved by backend SQL (via scan_roots JOIN)
      // 后端通过 SQL JOIN 解析好的绝对路径
      return convertFileSrc(thumbPath.replace(/\\/g, '/'))
    }
    return null
  } catch {
    return null
  }
})

// Display the true cosine similarity percentage
// 显示真实的余弦相似度百分比
const similarityPercent = computed(() => {
  const raw = props.item.similarity
  return (raw * 100).toFixed(1)
})

const badgeClass = computed(() => {
  const pct = props.item.similarity * 100
  if (pct >= 30) return 'badge--high'
  if (pct >= 25) return 'badge--mid'
  return 'badge--low'
})
</script>

<style scoped>
/* 硬编码色豁免说明(S5,设计 §6.2):本卡内 #fff/黑纱系均为照片上的恒定浮层
   (相似度徽章/播放钮/黑底渐变),语义=媒体上的永远白字黑纱,刻意不随主题。 */
.result-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  padding: 6px;
  cursor: pointer;
  text-align: left;
  transition: all var(--transition-fast);
  width: 100%;
  height: 100%;
}
.result-card:hover {
  background: var(--color-bg-surface);
  border-color: var(--color-border);
  transform: scale(1.03);
  z-index: 10;
  box-shadow: var(--shadow-md);
}
.result-card--selection-mode:hover {
  background: transparent;
  border-color: transparent;
}
.result-card--selected {
  background: transparent;
}

.result-card__thumb-wrap {
  position: relative;
  width: 100%;
  flex: 1;
  min-height: 0;
  border-radius: var(--radius-sm);
  overflow: hidden;
  background: var(--color-bg-overlay);
  transition:
    transform 0.25s cubic-bezier(0.34, 1.18, 0.64, 1),
    border-radius 0.25s ease;
}
.result-card--selected .result-card__thumb-wrap {
  transform: scale(0.85);
  border-radius: var(--radius-lg);
}
.result-card__thumb {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.result-card--selection-mode:hover .result-card__thumb {
  transform: none;
}
.result-card__thumb-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-tertiary);
}

.result-card__badge {
  position: absolute;
  bottom: 4px;
  right: 4px;
  font-size: 10px;
  font-weight: 700;
  padding: 1px 5px;
  border-radius: 99px;
  backdrop-filter: blur(4px);
}
.badge--high {
  background: color-mix(in srgb, hsl(142 70% 45%) 80%, transparent);
  color: #fff;
}
.badge--mid {
  background: color-mix(in srgb, hsl(38 90% 55%) 80%, transparent);
  color: #fff;
}
.badge--low {
  background: color-mix(in srgb, hsl(0 0% 50%) 60%, transparent);
  color: #fff;
}

.result-card__name {
  font-size: 11px;
  color: var(--color-text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 1.3;
}

/* Selection UI */
/* 选择 UI */
.result-card__overlay {
  position: absolute;
  inset: 0;
  background: color-mix(in srgb, var(--color-bg-surface) 20%, transparent);
  z-index: 5;
  pointer-events: none;
  border-radius: inherit;
}

.result-card__select-btn {
  position: absolute;
  top: 4px;
  left: 4px;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
  cursor: pointer;
  opacity: 0;
  transition:
    opacity 0.2s,
    transform 0.2s;
  pointer-events: auto; /* Capture clicks directly */
}

/* Show the checkbox when hovered (even not in selection mode) or when in selection mode */
.result-card:hover .result-card__select-btn,
.result-card--selection-mode .result-card__select-btn {
  opacity: 1;
}

.select-icon {
  color: rgba(255, 255, 255, 0.8);
  position: relative;
  z-index: 2;
  transition: color 0.2s;
}

/* Background circle to ensure contrast */
.select-icon-bg {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 16px;
  height: 16px;
  background: rgba(0, 0, 0, 0.3);
  border-radius: 50%;
  z-index: 1;
  transition: background 0.2s;
}

.result-card__select-btn:hover .select-icon {
  color: #fff;
}

.result-card__select-btn.is-selected .select-icon {
  color: var(--color-accent);
  fill: #fff;
}

.result-card__select-btn.is-selected .select-icon-bg {
  background: #fff;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
}
</style>
