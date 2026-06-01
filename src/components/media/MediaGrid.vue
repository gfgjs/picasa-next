<template>
  <div
    ref="gridRef"
    class="media-grid"
    @scroll.passive="onScroll"
  >
    <!-- Empty state -->
    <div v-if="media.totalRows === 0 && !media.isComputingLayout" class="empty-state">
      <div class="empty-state__icon">🖼️</div>
      <div class="empty-state__title">暂无媒体文件</div>
      <div class="empty-state__desc">在左侧边栏添加文件夹，点击扫描即可开始</div>
    </div>

    <!-- Loading -->
    <div v-if="media.isComputingLayout" class="media-grid__loading">
      <div class="spinner" />
      <span>正在计算布局...</span>
    </div>

    <!-- Virtual scroll spacer (top) -->
    <div :style="{ height: paddingTop + 'px' }" />

    <!-- Visible rows -->
    <div
      v-for="(row, ri) in visibleRows"
      :key="row.rowType === 'separator' ? `sep-${row.y}` : `row-${row.y}`"
      :style="{ marginBottom: GAP + 'px' }"
    >
      <!-- Date separator -->
      <div v-if="row.rowType === 'separator'" class="date-separator">
        {{ (row as any).separatorLabel }}
      </div>

      <!-- Normal row -->
      <div
        v-else
        class="media-grid__row"
        :style="{ height: (row as any).height + 'px', gap: GAP + 'px' }"
      >
        <div
          v-for="item in (row as any).items"
          :key="item.id"
          class="media-card"
          :style="{ width: item.w + 'px', height: item.h + 'px' }"
          @click="openDetail(item.id)"
        >
          <MediaThumb
            :id="item.id"
            :w="item.w"
            :h="item.h"
            :media-type="item.mediaType"
            :is-live-photo="item.isLivePhoto"
            :duration-ms="item.durationMs"
            :thumb-status="item.thumbStatus"
            :thumb-path="item.thumbPath"
            :thumbhash="item.thumbhash"
            :cache-dir="cacheDir"
          />
        </div>
      </div>
    </div>

    <!-- Virtual scroll spacer (bottom) -->
    <div :style="{ height: paddingBottom + 'px' }" />
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'

import { useMediaStore } from '../../stores/mediaStore'
import { useUiStore }    from '../../stores/uiStore'
import { useFilterStore } from '../../stores/filterStore'
import { useJustifiedLayout }  from '../../composables/useJustifiedLayout'
import { useVirtualScroll }    from '../../composables/useVirtualScroll'
import { useRequestQueue }     from '../../composables/useRequestQueue'

import MediaThumb from './MediaThumb.vue'
import type { LayoutRow } from '../../types/layout'
import { DEFAULTS, SEPARATOR_HEIGHT } from '../../constants/defaults'
import { IPC, EVENTS } from '../../constants/ipc'

const GAP = DEFAULTS.GRID_GAP

const media  = useMediaStore()
const ui     = useUiStore()
const filter = useFilterStore()
const queue  = useRequestQueue()

const gridRef  = ref<HTMLElement | null>(null)
const cacheDir = ref('')

// ── Virtual scroll ─────────────────────────────────────────────────────────

const {
  visibleRows, paddingTop, paddingBottom, scrollToTop, updateVisible,
} = useVirtualScroll({
  totalHeight:   () => media.totalHeight,
  totalRows:     () => media.totalRows,
  fetchRows:     (start, end) => media.fetchRows(start, end),
  containerRef:  () => gridRef.value,
})

function onScroll(e: Event) {
  updateVisible()
}

// ── Layout ─────────────────────────────────────────────────────────────────

const containerWidth = ref(0)
let resizeObserver: ResizeObserver | null = null

const { compute, onResize } = useJustifiedLayout(() => containerWidth.value)

onMounted(async () => {
  // Get cache dir from Tauri
  const { appDataDir } = await import('@tauri-apps/api/path')
  const dir = await appDataDir()
  cacheDir.value = `${dir}cache`.replace(/\\/g, '/')

  // Observe grid width
  resizeObserver = new ResizeObserver(entries => {
    const w = entries[0].contentRect.width
    if (w !== containerWidth.value) {
      containerWidth.value = w
      onResize(w)
    }
  })
  if (gridRef.value) {
    containerWidth.value = gridRef.value.clientWidth
    resizeObserver.observe(gridRef.value)
  }

  // Initial layout
  await compute()
  updateVisible()
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
})

// ── Detail ─────────────────────────────────────────────────────────────────

function openDetail(id: number) {
  media.openDetail(id)
}

// ── Listen to enrichment events ────────────────────────────────────────────

let unlistenEnriched: UnlistenFn | null = null

onMounted(async () => {
  unlistenEnriched = await listen(EVENTS.MEDIA_ENRICHED, () => {
    // Re-compute layout to pick up newly enriched items with correct dimensions
    compute()
  })
})

onBeforeUnmount(() => {
  unlistenEnriched?.()
})
</script>

<style scoped>
.media-grid {
  height: 100%;
  overflow-y: scroll;
  overflow-x: hidden;
  padding: var(--spacing-md);
  display: flex;
  flex-direction: column;
}

.media-grid__loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--spacing-sm);
  padding: var(--spacing-xl);
  color: var(--color-text-tertiary);
}

.media-grid__row {
  display: flex;
  flex-wrap: nowrap;
  overflow: hidden;
}

.date-separator {
  height: 36px;
  display: flex;
  align-items: center;
  font-size: var(--font-size-sm);
  font-weight: 600;
  color: var(--color-text-secondary);
  letter-spacing: -0.2px;
  padding: 0 2px;
}

.media-card {
  position: relative;
  overflow: hidden;
  border-radius: 2px;
  cursor: pointer;
  flex-shrink: 0;
}
/* Card hover defined in animations.css */
</style>
