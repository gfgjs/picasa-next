<template>
  <div class="media-grid-wrapper">
    <div
      ref="gridRef"
      class="media-grid"
      @scroll.passive="onGridScroll"
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

    <!-- Virtual scroll wrapper (absolute positioning) -->
    <div 
      v-if="media.totalRows > 0"
      class="media-grid__content"
      :style="{ height: media.totalHeight + 'px' }"
    >
      <div
        v-for="(row, ri) in visibleRows"
        :key="row.rowType === 'separator' ? `sep-${row.y}` : `row-${row.y}`"
        :class="row.rowType === 'separator' ? 'date-separator' : 'media-grid__row'"
        :style="{
          position: 'absolute',
          top: 0,
          transform: `translate3d(0, ${(row as any).y}px, 0)`,
          willChange: 'transform',
          left: 0,
          right: 0,
          height: (row as any).height + 'px',
          gap: row.rowType === 'separator' ? undefined : GAP + 'px'
        }"
      >
        <!-- Date separator -->
        <template v-if="row.rowType === 'separator'">
          {{ (row as any).separatorLabel }}
        </template>

        <!-- Normal row -->
        <template v-else>
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
              @request-thumb="onRequestThumb"
            />
          </div>
        </template>
      </div>
    </div>
  </div>

  <!-- Floating Scroll Buttons -->
    <div v-if="media.totalRows > 0" class="scroll-fab">
      <button class="fab-btn" @click="scrollGridToTop" title="回到顶部">
        ↑
      </button>
      <button class="fab-btn" @click="scrollGridToBottom" title="滚到底部">
        ↓
      </button>
    </div>
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

const scrollCache = new Map<string, number>()

function getViewKey() {
  return ui.activeDirectoryId ? `dir-${ui.activeDirectoryId}` : `album-${ui.activeSmartAlbum}`
}

const {
  visibleRows, paddingTop, paddingBottom, updateVisible, onScroll,
} = useVirtualScroll({
  totalHeight:   () => media.totalHeight,
  totalRows:     () => media.totalRows,
  fetchRowsByY:  (topY, bottomY) => media.fetchRowsByY(topY, bottomY),
  containerRef:  () => gridRef.value,
})

function onGridScroll(e: Event) {
  onScroll()
  if (gridRef.value) {
    scrollCache.set(getViewKey(), gridRef.value.scrollTop)
  }
}

function scrollGridToTop() {
  if (!gridRef.value) return
  gridRef.value.scrollTo({ top: 0, behavior: 'smooth' })
}

function scrollGridToBottom() {
  if (!gridRef.value) return
  gridRef.value.scrollTo({ top: gridRef.value.scrollHeight, behavior: 'smooth' })
}

// ── Layout ─────────────────────────────────────────────────────────────────

const containerWidth = ref(0)
let resizeObserver: ResizeObserver | null = null

const { compute, onResize } = useJustifiedLayout(() => containerWidth.value)

onMounted(async () => {
  // Get cache dir from Tauri
  const { appDataDir, join } = await import('@tauri-apps/api/path')
  const dir = await appDataDir()
  cacheDir.value = (await join(dir, 'cache')).replace(/\\/g, '/')

  // Read container width immediately
  if (gridRef.value) {
    containerWidth.value = gridRef.value.clientWidth
  } else {
    console.warn('[MediaGrid] onMounted: gridRef is null!')
  }

  resizeObserver = new ResizeObserver(entries => {
    const w = entries[0].contentRect.width
    // Ignore sub-pixel changes (often caused by scrollbar rendering glitches)
    if (w > 0 && Math.abs(w - containerWidth.value) > 1) {
      containerWidth.value = w
      onResize(w)
    }
  })
  if (gridRef.value) resizeObserver.observe(gridRef.value)

  // Initial layout compute — after width is known

  await compute()

  updateVisible()
})

// ── Thumbnail request handling ──────────────────────────────────────────────

async function onRequestThumb(id: number) {
  try {
    const result = await queue.request(id)
    // Find and patch the item in visibleRows
    for (const row of visibleRows.value) {
      if ((row as any).items) {
        const item = (row as any).items.find((it: any) => it.id === id)
        if (item) {
          item.thumbStatus = result.thumbStatus
          item.thumbPath   = result.thumbPath
          item.thumbhash   = result.thumbhash
          break
        }
      }
    }
  } catch {
    // request cancelled or failed — leave placeholder
  }
}

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
})

// ── Detail ─────────────────────────────────────────────────────────────────

function openDetail(id: number) {
  media.openDetail(id)
}

// ── Listen to enrichment events ────────────────────────────────────────────

let unlistenEnriched: UnlistenFn | null = null
let enrichedDebounceTimer: ReturnType<typeof setTimeout> | null = null

onMounted(async () => {
  unlistenEnriched = await listen(EVENTS.MEDIA_ENRICHED, () => {
    // Enrichment fires once per 500-item batch — debounce so we recompute
    // at most once every 2s during active enrichment instead of per-batch.
    if (enrichedDebounceTimer !== null) clearTimeout(enrichedDebounceTimer)
    enrichedDebounceTimer = setTimeout(async () => {
      enrichedDebounceTimer = null

      await compute()
      updateVisible()
    }, 2000)
  })
})

onBeforeUnmount(() => {
  unlistenEnriched?.()
  if (enrichedDebounceTimer !== null) clearTimeout(enrichedDebounceTimer)
})

// When totalItems changes (scan complete / clear data), recompute and refresh
watch(
  () => media.totalItems,
  async (newVal, oldVal) => {

    if (containerWidth.value < 100) return
    await compute()
    // updateVisible will be called by the layoutVersion watch below
  }
)

// When layout changes (due to resize, folder switch, filters, etc.), refresh visible rows
watch(
  () => media.layoutVersion,
  async () => {

    // Wait for the DOM to allow setting scrollTop before layout renders
    if (gridRef.value) {
      const saved = scrollCache.get(getViewKey()) || 0
      gridRef.value.scrollTop = saved
    }
    updateVisible(true)
  }
)
</script>

<style scoped>
.media-grid-wrapper {
  position: relative;
  height: 100%;
  width: 100%;
}

.media-grid {
  height: 100%;
  overflow-y: scroll;
  overflow-x: hidden;
  padding: 0;
  position: relative;
  overflow-anchor: none;
}

.media-grid__content {
  position: relative;
  width: 100%;
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
  /* overflow must be visible so hover-scaled cards can bleed outside the row */
  overflow: visible;
}

.date-separator {
  display: flex;
  font-size: 16px;
  font-weight: 600;
  color: var(--color-text-primary);
  align-items: center;
  padding-left: var(--spacing-sm);
}

.media-card {
  position: relative;
  /* shape clipping is handled inside .media-thumb; keep card visible */
  overflow: visible;
  border-radius: 2px;
  cursor: pointer;
  flex: 0 0 auto;
  box-sizing: border-box;

  /* base: sits behind neighbours */
  z-index: 0;

  /* On hover-out, delay z-index reset until the scale-down finishes (220ms) */
  transition:
    transform 220ms cubic-bezier(0.34, 1.18, 0.64, 1),
    box-shadow 220ms ease,
    z-index 0ms 220ms;
}

.media-card:hover {
  transform: scale(1.06);
  z-index: 10;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.5), 0 2px 8px rgba(0, 0, 0, 0.25);

  /* On hover-in, apply z-index immediately (no delay) */
  transition:
    transform 220ms cubic-bezier(0.34, 1.18, 0.64, 1),
    box-shadow 220ms ease;
}

.scroll-fab {
  position: absolute;
  bottom: 32px;
  right: 32px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  z-index: 100;
}

.fab-btn {
  width: 44px;
  height: 44px;
  border-radius: 50%;
  background: var(--color-surface-elevated);
  border: 1px solid var(--color-border);
  color: var(--color-text);
  font-size: 20px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 4px 12px rgba(0,0,0,0.2);
  transition: transform 0.2s cubic-bezier(0.34, 1.18, 0.64, 1), background 0.2s, box-shadow 0.2s;
  opacity: 0.8;
}

.fab-btn:hover {
  transform: scale(1.1);
  background: var(--color-surface-hover);
  opacity: 1;
  box-shadow: 0 6px 16px rgba(0,0,0,0.3);
}

.fab-btn:active {
  transform: scale(0.95);
}

</style>
