<template>
  <!-- Individual semantic search result card -->
  <!-- 单个语义搜索结果卡片 -->
  <button
    class="result-card"
    :title="`${item.fileName} · 相似度 ${similarityPercent}%`"
    @click="emit('click', item)"
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
      <div class="result-card__badge" :class="badgeClass">
        {{ similarityPercent }}%
      </div>
    </div>

    <!-- File name -->
    <div class="result-card__name">{{ item.fileName }}</div>
  </button>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { ImageIcon } from '@lucide/vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import type { SemanticSearchResult } from '../../types/ai'

const props = defineProps<{
  item: SemanticSearchResult
  /** Absolute path to app cache dir (e.g. C:/Users/.../AppData/.../cache) */
  /** 应用缓存目录的绝对路径 */
  cacheDir?: string
}>()
const emit = defineEmits<{ (e: 'click', item: SemanticSearchResult): void }>()

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

// Map raw cosine similarity [0.20, 0.35] to [10, 99] for better UX UX
// 映射原始的余弦相似度到对用户更友好的百分比，避免误解
const similarityPercent = computed(() => {
  const raw = props.item.similarity
  // Clip's raw cosine similarity rarely exceeds 0.35 for cross-modal tasks
  const minVal = 0.20
  const maxVal = 0.35
  let mapped = ((raw - minVal) / (maxVal - minVal)) * 100
  mapped = Math.max(10, Math.min(99, mapped))
  return Math.round(mapped)
})

const badgeClass = computed(() => {
  const pct = similarityPercent.value
  if (pct >= 80) return 'badge--high'
  if (pct >= 60) return 'badge--mid'
  return 'badge--low'
})
</script>

<style scoped>
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
}
.result-card:hover {
  background: var(--color-bg-surface);
  border-color: var(--color-border);
}

.result-card__thumb-wrap {
  position: relative;
  aspect-ratio: 1;
  border-radius: var(--radius-sm);
  overflow: hidden;
  background: var(--color-bg-overlay);
}
.result-card__thumb {
  width: 100%;
  height: 100%;
  object-fit: cover;
  transition: transform var(--transition-fast);
}
.result-card:hover .result-card__thumb {
  transform: scale(1.04);
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
</style>
