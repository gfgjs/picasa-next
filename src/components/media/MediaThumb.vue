<template>
  <!-- ThumbHash placeholder -->
  <!-- ThumbHash 占位符 -->
  <div
    class="media-thumb"
    :style="thumbStyle"
    :class="{
      loaded: isLoaded,
      'media-thumb--placeholder': !isLoaded,
      'media-thumb--selected': isSelected,
      'media-thumb--selection-mode': isSelectionMode,
      'media-thumb--drag-hover': isDragHover,
    }"
  >
    <!-- Placeholder solid color + file format text -->
    <!-- 纯色占位符 + 文件格式文本 -->
    <div
      v-if="!isLoaded"
      class="media-thumb__placeholder"
      :style="{ backgroundColor: placeholderBgColor }"
    >
      <span v-if="fileFormat" class="media-thumb__ext">{{ fileFormat.toUpperCase() }}</span>
    </div>
    <!-- Actual image -->
    <!-- 实际图片 -->
    <img
      v-if="displaySrc"
      class="media-thumb__img thumb-loaded"
      :src="displaySrc"
      :width="w"
      :height="h"
      loading="lazy"
      @error="onError"
    />


    <!-- Overlays -->
    <!-- 覆盖层 -->
    <div class="media-thumb__overlays">
      <!-- Advanced Info Overlay & Badges -->
      <div 
        v-if="thumbInfoLines.length > 0 || (similarity == null && isLoaded && thumbStatus === 3 && ui.showThumbInfo && ui.thumbInfoElements.includes('status')) || (similarity == null && isLoaded && thumbStatus === 1 && ui.showThumbInfo && ui.thumbInfoElements.includes('status')) || (similarity == null && fileSize && ui.showThumbInfo && ui.thumbInfoElements.includes('size')) || similarity != null || isLivePhoto"
        class="media-thumb__info-overlay"
      >
        <div class="media-thumb__badges" style="display: flex; gap: 4px; margin-bottom: 2px; flex-wrap: wrap;">
          <span v-if="similarity == null && isLoaded && thumbStatus === 3 && ui.showThumbInfo && ui.thumbInfoElements.includes('status')" class="badge badge-source" title="直接渲染原图">ORIG</span>
          <span v-if="similarity == null && isLoaded && thumbStatus === 1 && ui.showThumbInfo && ui.thumbInfoElements.includes('status')" class="badge badge-thumb" title="渲染缩略图">THUMB</span>
          <span v-if="similarity == null && fileSize && ui.showThumbInfo && ui.thumbInfoElements.includes('size')" class="badge badge-size">{{ formatFileSize(fileSize) }}</span>
          <span v-if="similarity != null" class="badge badge-similarity">{{ Math.round(similarity * 100) }}%</span>
          <span v-if="isLivePhoto" class="badge badge-live">LIVE</span>
        </div>
        <div v-for="(line, idx) in thumbInfoLines" :key="idx" class="info-line">{{ line }}</div>
      </div>

      <!-- Video play -->
      <!-- 视频播放 -->
      <span v-if="mediaType === 'video'" class="badge badge-video"><Play :size="20" fill="#fff" /></span>
      <!-- Duration -->
      <!-- 时长 -->
      <span v-if="durationMs" class="badge badge-duration">{{ formatDuration(durationMs) }}</span>
      <!-- Favorite -->
      <!-- 收藏 -->
      <button
        class="media-thumb__fav"
        :class="{ active: isFavorited, 'fav-always-visible': isFavorited && ui.showThumbInfo && ui.thumbInfoElements.includes('favorite') }"
        @click.stop="toggleFav"
        title="收藏"
      ><Heart :size="14" :fill="isFavorited ? '#ff4757' : 'none'" :color="isFavorited ? '#ff4757' : '#fff'" :stroke-width="isFavorited ? 0 : 2" /></button>
      <!-- Selection checkbox -->
      <!-- 选择复选框 -->
      <div
        v-if="isSelected || isSelectionMode"
        class="media-thumb__checkbox"
        @click.stop="emit('select', id)"
      >
        <div class="checkbox" :class="{ checked: isSelected }">
          <Check v-if="isSelected" :size="12" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { Play, Heart, Check } from '@lucide/vue'
import { thumbhashToAverageColor } from '../../utils/thumbhash'
import { formatDuration, formatFileSize } from '../../utils/format'

import { useUiStore } from '../../stores/uiStore'

interface Props {
  id:              number
  w:               number
  h:               number
  mediaType:       string
  isLivePhoto?:    boolean
  durationMs?:     number | null
  thumbStatus:     number
  thumbPath?:      string | null
  thumbhash?:      number[] | null
  fileFormat?:     string
  fileSize?:       number
  similarity?:     number
  isFavorited?:    boolean
  isSelected?:     boolean
  isSelectionMode?: boolean
  isDragHover?:    boolean
  cacheDir:        string
  item?:           any
}

const props = withDefaults(defineProps<Props>(), {
  isLivePhoto:     false,
  durationMs:      null,
  thumbPath:       null,
  thumbhash:       null,
  isFavorited:     false,
  isSelected:      false,
  isSelectionMode: false,
  isDragHover:     false,
})

const emit = defineEmits<{
  (e: 'click', id: number): void
  (e: 'select', id: number): void
  (e: 'favorite', id: number): void
  (e: 'request-thumb', id: number): void
  (e: 'cancel-thumb', id: number): void
}>()

const ui = useUiStore()

const urlParams = new URLSearchParams(window.location.search)
const cacheBuster = urlParams.get('clear') ? `?t=${urlParams.get('clear')}` : ''

const isLoaded      = ref(false)
const displaySrc    = ref('')
const favAnimating  = ref(false)
const hasRequested  = ref(false)  // guard: only request once per mount // 守卫：每次挂载仅请求一次
let decodingImg: HTMLImageElement | null = null

const thumbStyle = computed(() => ({
  width:  `${props.w}px`,
  height: `${props.h}px`,
}))

const thumbInfoLines = computed(() => {
  if (!ui.showThumbInfo || !props.item) return []
  const lines: string[] = []
  const elements = ui.thumbInfoElements
  const it = props.item

  if (elements.includes('filename') && it.fileName) {
    lines.push(it.fileName)
  }
  if (elements.includes('date') && it.sortDatetime) {
    lines.push(new Date(it.sortDatetime * 1000).toLocaleString())
  }
  if (elements.includes('resolution') && it.originalWidth && it.originalHeight) {
    lines.push(`${it.originalWidth} × ${it.originalHeight}`)
  }
  if (elements.includes('path') && it.dirPath) {
    lines.push(it.dirPath)
  }
  if (elements.includes('geo') && it.gpsLat != null && it.gpsLng != null) {
    lines.push(`${it.gpsLat.toFixed(4)}, ${it.gpsLng.toFixed(4)}`)
  }
  if (elements.includes('camera') && (it.exifMake || it.exifModel)) {
    const make = it.exifMake || ''
    const model = it.exifModel || ''
    lines.push(`${make} ${model}`.trim())
  }
  if (elements.includes('params')) {
    const params = []
    if (it.exifFocalLength) params.push(`${it.exifFocalLength}mm`)
    if (it.exifAperture) params.push(`f/${it.exifAperture}`)
    if (it.exifShutter) params.push(`${it.exifShutter}s`)
    if (it.exifIso) params.push(`ISO${it.exifIso}`)
    if (params.length > 0) lines.push(params.join(' '))
  }

  return lines
})

const placeholderBgColor = computed(() =>
  props.thumbhash ? thumbhashToAverageColor(props.thumbhash) : 'var(--color-bg-elevated)'
)



async function loadThumb() {
  // thumb_status meanings:
  // thumb_status 的含义:
  //   0 = pending generation
  //   0 = 待生成
  //   1 = generated WebP on disk  → load from cache dir
  //   1 = 已在磁盘上生成 WebP → 从缓存目录加载
  //   2 = failed
  //   2 = 失败
  //   3 = small file direct display → load the original file via absPath
  //   3 = 小文件直接显示 → 通过 absPath 加载原文件
  //       (absPath is not available here; parent supplies the thumb_path as the abs path in this case)
  //       (这里没有 absPath；在这种情况下，父组件会将 thumb_path 作为绝对路径提供)

  if (props.thumbStatus === 1 && props.thumbPath) {
    // Load the generated thumbnail from the cache directory
    // 从缓存目录加载生成的缩略图
    try {
      const abs = `${props.cacheDir}/thumbnails/${props.thumbPath}`.replace(/\\/g, '/')
      const src = convertFileSrc(abs) + cacheBuster
      const img = new Image()
      decodingImg = img
      img.src = src
      try {
        await img.decode()
      } catch (e) {
        // console.warn('MediaThumb decode() failed, falling back to DOM load', e)
        // console.warn('MediaThumb decode() 失败，回退到 DOM 加载', e)
      }
      if (decodingImg !== img) return
      displaySrc.value = src
      isLoaded.value   = true
    } catch (e) {
      // console.warn('Outer catch caught error for status 1:', e)
      // console.warn('Outer catch 捕获了状态 1 的错误:', e)
    }
    return
  }

  if (props.thumbStatus === 3) {
    if (props.thumbPath) {
      // Small file: thumbPath holds the absolute path to the original file
      // 小文件: thumbPath 保存了原始文件的绝对路径
      try {
        const src = convertFileSrc(props.thumbPath.replace(/\\/g, '/')) + cacheBuster
        const img = new Image()
        decodingImg = img
        img.src = src
        try {
          await img.decode()
        } catch (e) {
          // console.warn('MediaThumb decode() failed, falling back to DOM load', e)
          // console.warn('MediaThumb decode() 失败，回退到 DOM 加载', e)
        }
        if (decodingImg !== img) return
        displaySrc.value = src
        isLoaded.value   = true
      } catch (e) {
      // console.warn('Outer catch caught error for status 3:', e)
      // console.warn('Outer catch 捕获了状态 3 的错误:', e)
      }
      return
    } else {
      // We know it's status 3 but we don't have the absPath in the layout row.
      // Ask the queue for it! (The backend get_thumb_by_item_ids will resolve it)
      // 我们知道它是状态 3，但我们在布局行中没有 absPath。
      // 向队列请求它！(后端的 get_thumb_by_item_ids 会解决这个问题)
      if (!hasRequested.value) {
        hasRequested.value = true
        emit('request-thumb', props.id)
      }
      return
    }
  }

  if (props.thumbStatus === 0) {
    // Not yet generated — ask the parent/grid to request generation.
    // 尚未生成 — 请求父组件/网格发出生成请求。
    // Guard: only emit once per mount lifecycle to prevent infinite loops
    // when the backend fails and keeps returning status=2.
    // 守卫：在每个挂载生命周期中仅发出一次事件，以防止
    // 后端失败并持续返回 status=2 时出现无限循环。
    if (!hasRequested.value) {
      hasRequested.value = true
      emit('request-thumb', props.id)
    }
  }
}

// Re-run loadThumb only when thumbPath/thumbStatus actually gets a usable value
// (status transitions from 0→1 or 0→3 after the parent receives batch results).
// 仅当 thumbPath/thumbStatus 确实获得可用值时才重新运行 loadThumb
// (在父组件接收到批处理结果后，状态从 0→1 或 0→3 过渡)。
watch(
  () => [props.thumbPath, props.thumbStatus] as const,
  ([newPath, newStatus]) => {
    if (newStatus === 1 || newStatus === 3) {
      loadThumb()
    }
  },
)

async function toggleFav() {
  favAnimating.value = true
  setTimeout(() => { favAnimating.value = false }, 400)
  emit('favorite', props.id)
}

function onError() {
  displaySrc.value = ''
  isLoaded.value   = false
}

onMounted(() => loadThumb())

onBeforeUnmount(() => {
  if (decodingImg) {
    decodingImg.src = ''
    decodingImg = null
  }
  if (hasRequested.value && !isLoaded.value) {
    emit('cancel-thumb', props.id)
  }
})
</script>

<style scoped>
.media-thumb {
  /* position:relative so thumbStyle width/height props are respected */
  /* position:relative 以便遵守 thumbStyle 的宽度/高度属性 */
  position: relative;
  overflow: hidden;
  border-radius: 2px;
  background: var(--color-bg-elevated);
  /* cursor and flex-shrink live on the parent .media-card */
  /* cursor 和 flex-shrink 存在于父组件 .media-card 上 */
  transition: transform 0.25s cubic-bezier(0.34, 1.18, 0.64, 1), border-radius 0.25s ease;
}
.media-thumb:hover .media-thumb__fav,
.media-thumb:hover .media-thumb__checkbox {
  opacity: 1;
}


.media-thumb__placeholder {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.media-thumb__ext {
  font-family: var(--font-mono);
  font-size: 14px;
  font-weight: 700;
  color: rgba(255, 255, 255, 0.4);
  letter-spacing: 1px;
}

.media-thumb__img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
}



/* ── Overlays ─────────────────────────────────────────────────────────── */
/* ── 覆盖层 ─────────────────────────────────────────────────────────── */
.media-thumb__overlays {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 10;
}

.badge {
  position: absolute;
  border-radius: var(--radius-sm);
  font-size: 10px;
  font-weight: 700;
  padding: 2px 5px;
  line-height: 1;
  letter-spacing: 0.04em;
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
}
.badge-source {
  position: static;
  background: rgba(30, 136, 229, 0.85); /* Blue for orig */
  color: #fff;
  font-size: 9px;
}
.badge-thumb {
  position: static;
  background: rgba(67, 160, 71, 0.85); /* Green for thumb */
  color: #fff;
  font-size: 9px;
}
.media-thumb__top-left {
  position: absolute;
  top: 6px;
  left: 6px;
  display: flex;
  gap: 4px;
}
.badge-live {
  position: static;
  background: var(--color-badge-live);
  color: #fff;
}
.badge-size {
  position: static;
  background: var(--color-badge-size);
  color: #fff;
}
.badge-similarity {
  position: static;
  background: rgba(138, 43, 226, 0.85); /* Purple for AI */
  color: #fff;
  font-size: 9px;
}
.badge-video {
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  background: rgba(0, 0, 0, 0.5);
  color: #fff;
  font-size: 20px;
  padding: 8px 10px;
  border-radius: var(--radius-md);
  pointer-events: none;
}
.badge-duration {
  bottom: 6px;
  right: 6px;
  background: rgba(0, 0, 0, 0.55);
  color: #fff;
  font-family: var(--font-mono);
  font-size: 10px;
}

.media-thumb__fav {
  position: absolute;
  bottom: 4px;
  right: 4px;
  background: transparent;
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  pointer-events: auto;
  transition: opacity var(--transition-fast), transform var(--transition-fast);
  padding: 4px;
  border: none;
  filter: drop-shadow(0 1px 3px rgba(0,0,0,0.6));
}
.media-thumb__fav:hover {
  transform: scale(1.1);
}
.media-thumb__fav.fav-always-visible { opacity: 1; }

.media-thumb__checkbox {
  position: absolute;
  top: 4px;
  right: 4px;
  opacity: 0;
  pointer-events: auto;
  transition: opacity var(--transition-fast);
}
.checkbox {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.9);
  background: rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  color: #fff;
  font-weight: 700;
  backdrop-filter: blur(2px);
  transition: background var(--transition-fast);
}
.checkbox.checked {
  background: var(--color-accent);
  border-color: var(--color-accent);
}

/* ── Selection visual states | 选择视觉状态 ─────────────────────── */

.media-thumb--selected {
  transform: scale(0.85);
  border-radius: var(--radius-lg);
}

/* Selected overlay — subtle dimming mask */
/* 选中遮罩 — 轻微变暗蒙版 */
.media-thumb--selected::after {
  content: '';
  position: absolute;
  inset: 0;
  background: color-mix(in srgb, var(--color-bg-surface) 20%, transparent);
  pointer-events: none;
  z-index: 2;
  border-radius: inherit;
  transition: all 150ms ease;
}

.media-thumb--drag-hover {
  transform: scale(0.92);
  border-radius: var(--radius-md);
}

/* Drag hover overlay */
.media-thumb--drag-hover::after {
  content: '';
  position: absolute;
  inset: 0;
  background: color-mix(in srgb, var(--color-bg-surface) 35%, transparent);
  pointer-events: none;
  z-index: 2;
  border-radius: inherit;
  transition: all 150ms ease;
}

/* In selection mode: always show checkbox (not just on hover) */
/* 选择模式：始终显示 checkbox（不仅是 hover 时） */
.media-thumb--selection-mode .media-thumb__checkbox {
  opacity: 1;
}

/* In selection mode: always show favorite button too */
/* 选择模式：也始终显示收藏按钮 */
.media-thumb--selection-mode .media-thumb__fav {
  opacity: 1;
}

.media-thumb__info-overlay {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  background: linear-gradient(to top, rgba(0,0,0,0.85) 0%, rgba(0,0,0,0.5) 70%, transparent 100%);
  color: #fff;
  padding: 24px 6px 6px 6px;
  font-size: 10px;
  font-family: var(--font-mono);
  display: flex;
  flex-direction: column;
  gap: 2px;
  pointer-events: none;
  opacity: 1;
  transition: opacity var(--transition-fast);
}

.info-line {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-shadow: 0 1px 2px rgba(0,0,0,0.8);
}
</style>
