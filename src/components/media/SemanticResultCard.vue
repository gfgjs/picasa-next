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

const props = defineProps<{ item: SemanticSearchResult }>()
const emit = defineEmits<{ (e: 'click', item: SemanticSearchResult): void }>()

const thumbSrc = computed(() => {
  if (!props.item.thumbPath) return null
  try {
    return convertFileSrc(props.item.thumbPath)
  } catch {
    return null
  }
})

const similarityPercent = computed(() => Math.round(props.item.similarity * 100))

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
