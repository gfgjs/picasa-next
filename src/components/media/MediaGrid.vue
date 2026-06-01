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
    <!--
      IMPORTANT: Do NOT use marginBottom here.
      flex gap on .media-grid does not add trailing space after the last child,
      so scrollHeight == totalHeight. marginBottom WOULD add extra height,
      making scrollHeight > totalHeight, which causes the browser to clamp
      scrollTop at the bottom → scroll event → fetch → DOM change → loop.
    -->
    <div
      v-for="(row, ri) in visibleRows"
      :key="row.rowType === 'separator' ? `sep-${row.y}` : `row-${row.y}`"
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
            @request-thumb="onRequestThumb"
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
  visibleRows, paddingTop, paddingBottom, scrollToTop, updateVisible, onScroll,
} = useVirtualScroll({
  totalHeight:   () => media.totalHeight,
  totalRows:     () => media.totalRows,
  fetchRows:     (start, end) => media.fetchRows(start, end),
  containerRef:  () => gridRef.value,
})

// ── Layout ─────────────────────────────────────────────────────────────────

const containerWidth = ref(0)
let resizeObserver: ResizeObserver | null = null

const { compute, onResize } = useJustifiedLayout(() => containerWidth.value)

onMounted(async () => {
  // Get cache dir from Tauri
  const { appDataDir, join } = await import('@tauri-apps/api/path')
  const dir = await appDataDir()
  cacheDir.value = (await join(dir, 'cache')).replace(/\\/g, '/')

  // Read container width immediately — use offsetWidth for accuracy
  if (gridRef.value) {
    // Use ResizeObserver's contentRect when available (avoids padding confusion).
    // For the initial read before ResizeObserver fires, offsetWidth is reliable.
    containerWidth.value = gridRef.value.offsetWidth
    console.log('[MediaGrid] onMounted: offsetWidth=', gridRef.value.offsetWidth,
      'clientWidth=', gridRef.value.clientWidth,
      'clientHeight=', gridRef.value.clientHeight)
  } else {
    console.warn('[MediaGrid] onMounted: gridRef is null!')
  }

  resizeObserver = new ResizeObserver(entries => {
    const w = entries[0].contentRect.width
    console.log('[MediaGrid] ResizeObserver: w=', w, 'prev=', containerWidth.value)
    if (w > 0 && w !== containerWidth.value) {
      containerWidth.value = w
      onResize(w)
    }
  })
  if (gridRef.value) resizeObserver.observe(gridRef.value)

  // Initial layout compute — after width is known
  console.log('[MediaGrid] calling initial compute(), containerWidth=', containerWidth.value)
  await compute()
  console.log('[MediaGrid] initial compute done, calling updateVisible()')
  updateVisible()
})

// ── Thumbnail request handling ──────────────────────────────────────────────
//
// When a MediaThumb emits 'request-thumb', we enqueue the id in the request
// queue. When the batch resolves, we patch the item inside visibleRows in-place
// so the thumb component's watch fires and loads the image.
//
// For thumb_status=3 (small file direct display), the backend does NOT store
// an absolute path in thumb_path. We need to resolve the abs path via the
// get_media_detail call — but that's heavy. Instead we store abs_path on the
// item itself the first time we open detail. For the grid, we directly serve
// status=3 by passing the thumb_path through convertFileSrc (the path is
// already stored as abs in DB for status=3 — see generator.rs line 77:
// thumb_path is None for status=3). So for status=3 we need to open detail
// once to get absPath — too expensive.
//
// Simpler approach: batch_request_thumbnails returns ThumbResult with
// thumb_status=3 & thumb_path=null. For status=3, MediaThumb should use the
// original file. We patch a synthetic thumb_path = abs_path.
// Since we don't have abs_path in layout rows, we invoke get_item_path_info
// via an IPC that doesn't exist yet — instead we just accept that status=3
// items will show their ThumbHash until the user opens the detail.
// (Status=3 is only for files < 200KB, ThumbHash looks fine for those.)

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
      console.log('[MediaGrid] MEDIA_ENRICHED debounce fired, recomputing')
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
    console.log('[MediaGrid] totalItems changed:', oldVal, '->', newVal, 'containerWidth=', containerWidth.value)
    if (containerWidth.value < 100) {
      console.warn('[MediaGrid] totalItems watch: containerWidth not ready, skipping compute')
      return
    }
    await compute()
    updateVisible()
  }
)
</script>

<style scoped>
.media-grid {
  height: 100%;
  overflow-y: scroll;
  overflow-x: hidden;
  /* No gap, no margin, no padding — row-to-row spacing is already encoded in
     the Rust layout y coordinates (each row's y = prev_y + prev_h + gap).
     Adding any extra vertical space here would make scrollHeight > totalHeight,
     causing the browser to clamp scrollTop at the bottom and fire infinite
     scroll events. */
  padding: 0;
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
  /* overflow must be visible so hover-scaled cards can bleed outside the row */
  overflow: visible;
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
  /* shape clipping is handled inside .media-thumb; keep card visible */
  overflow: visible;
  border-radius: 2px;
  cursor: pointer;
  flex-shrink: 0;

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

</style>
