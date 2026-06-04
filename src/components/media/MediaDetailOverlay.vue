<template>
  <Teleport to="body">
    <div v-if="media.isDetailOpen && media.detailItem" class="detail-overlay overlay-enter">
      <!-- Backdrop -->
      <!-- 背景幕 -->
      <div class="overlay-backdrop" @click="close" />

      <!-- Panel -->
      <!-- 面板 -->
      <div class="detail-panel">
        <!-- ── Viewer ───────────────────────────────────────────────────── -->
        <!-- ── 视图器 ───────────────────────────────────────────────────── -->
        <div
          class="detail-viewer"
          ref="viewerRef"
          @wheel.prevent="onWheelHandler"
          @mousedown="state.startDrag"
          @click="onImageClick"
        >
          <img
            v-if="detail.mediaType === 'image'"
            ref="imgRef"
            :src="absPath"
            class="detail-viewer__img"
            :class="{ 'is-dragging': state.isDragging.value }"
            :style="{ transform: state.transform.value }"
            draggable="false"
          />
          <video
            v-else-if="detail.mediaType === 'video'"
            :src="absPath"
            class="detail-viewer__video"
            :class="{ 'is-dragging': state.isDragging.value }"
            :style="{ transform: state.transform.value }"
            controls
            autoplay
          />
          <audio
            v-else-if="detail.mediaType === 'audio'"
            :src="absPath"
            class="detail-viewer__audio"
            controls
            autoplay
          />
          <div v-else class="detail-viewer__document">
            <FileText :size="48" />
            <p>{{ detail.fileName }}</p>
          </div>

          <!-- Live photo video overlay -->
          <!-- Live photo 视频覆盖层 -->
          <video
            v-if="state.isPlayingLive.value && state.liveVideoSrc.value"
            :src="state.liveVideoSrc.value"
            class="detail-viewer__live-video"
            autoplay
            loop
          />
        </div>

        <!-- ── Controls ────────────────────────────────────────────────── -->
        <!-- ── 控制器 ────────────────────────────────────────────────── -->
        <div class="detail-controls">
          <!-- Left -->
          <!-- 左侧 -->
          <div class="detail-controls__left">
            <button class="btn-icon" @click="close" :title="$t('detail.close')"><X :size="18" /></button>
            <button class="btn-icon" @click="state.zoomIn()" :title="$t('detail.zoomIn')"><ZoomIn :size="18" /></button>
            <!-- 缩放百分比显示 / zoom percentage indicator -->
            <span class="detail-zoom-pct">{{ Math.round(state.scale.value * 100) }}%</span>
            <button class="btn-icon" @click="state.zoomOut()" :title="$t('detail.zoomOut')"><ZoomOut :size="18" /></button>
            <button class="btn-icon" @click="handleToggleZoom" :title="zoomModeTitle">
              <Maximize v-if="state.zoomMode.value === 'auto'" :size="18" />
              <span v-else-if="state.zoomMode.value === 'original'" style="font-size: 12px; font-weight: 700;">1:1</span>
              <MoveHorizontal v-else-if="state.zoomMode.value === 'fit-width'" :size="18" />
              <MoveVertical v-else :size="18" />
            </button>
            <button
              v-if="detail.isLivePhoto"
              class="btn-icon"
              :class="{ active: state.isPlayingLive.value }"
              @click="toggleLive"
              title="Live 照片"
            >
              LIVE
            </button>
          </div>

          <!-- Center: file name -->
          <!-- 中间: 文件名 -->
          <div class="detail-controls__center">
            <span class="detail-controls__name" :title="detail.fileName">{{ detail.fileName }}</span>
          </div>

          <!-- Right -->
          <!-- 右侧 -->
          <div class="detail-controls__right">
            <button
              class="btn-icon"
              :class="{ active: detail.isFavorited }"
              @click="toggleFav"
              title="收藏"
            ><Heart :size="18" :fill="detail.isFavorited ? 'currentColor' : 'none'" :stroke-width="detail.isFavorited ? 0 : 2" /></button>
            <button class="btn-icon" @click="showInExplorer" title="在文件夹中显示"><FolderOpen :size="18" /></button>
            <button class="btn-icon" @click="state.toggleInfo()" :title="$t('detail.info')"><Info :size="18" /></button>
          </div>
        </div>

        <!-- ── Info sidebar (slide in when showInfo) ────────────────────── -->
        <!-- ── 信息侧边栏 (showInfo 开启时滑入) ────────────────────── -->
        <Transition name="slide">
          <div v-if="state.showInfo.value" class="detail-info">
            <div class="detail-info__header">
              <span>文件信息</span>
              <button class="btn-icon" @click="state.toggleInfo()"><X :size="16" /></button>
            </div>

            <div class="info-section">
              <div class="info-row">
                <span class="info-label">{{ $t('detail.fileName') }}</span>
                <span class="info-value" :title="detail.fileName">{{ detail.fileName }}</span>
              </div>
              <div class="info-row">
                <span class="info-label">{{ $t('detail.fileSize') }}</span>
                <span class="info-value">{{ formatFileSize(detail.fileSize) }}</span>
              </div>
              <div class="info-row">
                <span class="info-label">{{ $t('detail.dimensions') }}</span>
                <span class="info-value" v-if="detail.width">{{ detail.width }} × {{ detail.height }}</span>
              </div>
              <div class="info-row">
                <span class="info-label">{{ $t('detail.format') }}</span>
                <span class="info-value">{{ detail.fileFormat.toUpperCase() }}</span>
              </div>
            </div>

            <!-- EXIF -->
            <!-- EXIF -->
            <div v-if="detail.imageMeta" class="info-section">
              <div class="info-section__title">{{ $t('detail.exif') }}</div>
              <div v-if="detail.imageMeta.exifDatetime" class="info-row">
                <span class="info-label">{{ $t('detail.datetime') }}</span>
                <span class="info-value">{{ formatDateTime(detail.imageMeta.exifDatetime) }}</span>
              </div>
              <div v-if="detail.imageMeta.exifMake" class="info-row">
                <span class="info-label">{{ $t('detail.camera') }}</span>
                <span class="info-value">{{ detail.imageMeta.exifMake }} {{ detail.imageMeta.exifModel }}</span>
              </div>
              <div v-if="detail.imageMeta.exifFocalLength" class="info-row">
                <span class="info-label">{{ $t('detail.focalLength') }}</span>
                <span class="info-value">{{ formatFocalLength(detail.imageMeta.exifFocalLength) }}</span>
              </div>
              <div v-if="detail.imageMeta.exifAperture" class="info-row">
                <span class="info-label">{{ $t('detail.aperture') }}</span>
                <span class="info-value">{{ formatAperture(detail.imageMeta.exifAperture) }}</span>
              </div>
              <div v-if="detail.imageMeta.exifShutter" class="info-row">
                <span class="info-label">{{ $t('detail.exposure') }}</span>
                <span class="info-value">{{ detail.imageMeta.exifShutter }}s</span>
              </div>
              <div v-if="detail.imageMeta.exifIso" class="info-row">
                <span class="info-label">{{ $t('detail.iso') }}</span>
                <span class="info-value">{{ detail.imageMeta.exifIso }}</span>
              </div>
              <div v-if="detail.imageMeta.exifGpsLat" class="info-row">
                <span class="info-label">{{ $t('detail.location') }}</span>
                <span class="info-value">{{ formatGps(detail.imageMeta.exifGpsLat, detail.imageMeta.exifGpsLng!) }}</span>
              </div>
            </div>

            <!-- Rating -->
            <!-- 评分 -->
            <div class="info-section">
              <div class="info-section__title">评分</div>
              <div class="rating-stars">
                <button
                  v-for="n in 5"
                  :key="n"
                  class="star"
                  :class="{ filled: n <= (detail.rating ?? 0) }"
                  @click="setRating(n)"
                ><Star :size="20" :fill="n <= (detail.rating ?? 0) ? 'currentColor' : 'none'" :stroke-width="1.5" /></button>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import { useMediaStore } from '../../stores/mediaStore'
import { useUiStore }    from '../../stores/uiStore'
import { useMediaDetail } from '../../composables/useMediaDetail'
import { formatFileSize, formatDateTime, formatFocalLength, formatAperture, formatGps } from '../../utils/format'
import {
  X, ZoomIn, ZoomOut, Maximize, MoveHorizontal, MoveVertical,
  Heart, FolderOpen, Info, Star, FileText
} from '@lucide/vue'
import { IPC } from '../../constants/ipc'

const media = useMediaStore()
const ui    = useUiStore()
const { t } = useI18n()

const detail  = computed(() => media.detailItem!)
const absPath = computed(() => detail.value ? convertFileSrc(detail.value.absPath) : '')

// ── Viewer state — created ONCE, not inside computed() ─────────────────────
// ── 视图器状态 — 仅创建一次，不要放在 computed() 内部 ─────────────────────
// Calling useMediaDetail() inside computed() would recreate internal refs and
// re-register document event listeners every time a reactive dependency changes,
// permanently leaking mousemove/mouseup handlers.
// 如果在 computed() 内部调用 useMediaDetail()，每次响应式依赖变化时都会重新创建内部的 refs
// 并重新注册 document 事件监听器，从而导致 mousemove/mouseup 处理程序永久泄漏。
const state = useMediaDetail()

const viewerRef = ref<HTMLElement | null>(null)
const imgRef = ref<HTMLImageElement | null>(null)

const zoomModeTitle = computed(() => {
  switch (state.zoomMode.value) {
    case 'auto': return '自适应屏幕 (当前) - 点击切换为 1:1'
    case 'original': return '1:1 原图 (当前) - 点击切换为铺满宽'
    case 'fit-width': return '铺满宽 (当前) - 点击切换为铺满高'
    case 'fit-height': return '铺满高 (当前) - 点击切换为自适应'
    default: return '重置缩放'
  }
})

function handleToggleZoom() {
  if (!viewerRef.value || !imgRef.value) {
    state.resetZoom()
    return
  }
  const iw = imgRef.value.naturalWidth
  const ih = imgRef.value.naturalHeight
  const cw = viewerRef.value.clientWidth
  const ch = viewerRef.value.clientHeight
  if (!iw || !ih || !cw || !ch) {
    state.resetZoom()
    return
  }
  state.cycleZoomMode(cw, ch, iw, ih)
}

// Reset zoom whenever the viewed item changes
// 当查看的项目更改时重置缩放
watch(() => media.detailItem, () => {
  state.resetZoom()
  state.isPlayingLive.value = false
  state.liveVideoSrc.value  = null
})

// ── Keyboard shortcuts ─────────────────────────────────────────────────────
// ── 快捷键 ─────────────────────────────────────────────────────
// Registered via onMounted / onBeforeUnmount to avoid accumulating listeners
// on each open/close cycle (Teleport keeps the component alive).
// 通过 onMounted / onBeforeUnmount 注册以避免在每次打开/关闭周期内积累监听器
// (Teleport 使组件保持活动状态)。
function onKeydown(e: KeyboardEvent) {
  if (!media.isDetailOpen) return
  if (e.key === 'Escape') { close(); return }
  if (e.key === '+' || e.key === '=') { state.zoomIn(); return }
  if (e.key === '-')                   { state.zoomOut(); return }
  if (e.key === 'i' || e.key === 'I') { state.toggleInfo(); return }
  if (e.key === 'ArrowLeft')  { media.navigateDetail(-1); return }
  if (e.key === 'ArrowRight') { media.navigateDetail(1); return }
}

let accumulatedDelta = 0
let deltaTimer: ReturnType<typeof setTimeout> | null = null

function onWheelHandler(e: WheelEvent) {
  const handledZoom = state.onWheel(e)
  
  // If useMediaDetail wasn't HMR'd properly, handledZoom might be undefined.
  // Explicitly check for true.
  // 如果 useMediaDetail 没有正确触发 HMR 更新，handledZoom 可能为 undefined。
  // 请显式检查是否为 true。
  if (handledZoom !== true) {
    accumulatedDelta += e.deltaY
    
    // Clear accumulator after scrolling stops (e.g. 50ms)
    // 停止滚动后清除累加器 (例如 50 毫秒)
    if (deltaTimer) clearTimeout(deltaTimer)
    deltaTimer = setTimeout(() => { accumulatedDelta = 0 }, 50)

    if (accumulatedDelta >= 50) {
      media.navigateDetail(1)
      accumulatedDelta = 0
    } else if (accumulatedDelta <= -50) {
      media.navigateDetail(-1)
      accumulatedDelta = 0
    }
  }
}

function onImageClick(e: MouseEvent) {
  // If user is dragging (scale > 1), we shouldn't close the info.
  // We can just close it if it's open.
  // 如果用户正在拖拽 (scale > 1)，我们不应该关闭信息栏。
  // 如果它打开着，我们可以直接关闭它。
  if (state.showInfo.value && state.scale.value <= 1) {
    state.showInfo.value = false
  }
}

onMounted(()        => document.addEventListener('keydown', onKeydown))
onBeforeUnmount(()  => {
  document.removeEventListener('keydown', onKeydown)
  state.cleanup()
})

function close() { media.closeDetail() }

async function toggleFav() {
  if (!detail.value) return
  const newVal = await media.toggleFavorite(detail.value.id)
  detail.value.isFavorited = newVal
}

async function setRating(n: number) {
  if (!detail.value) return
  await media.setRating(detail.value.id, n === detail.value.rating ? 0 : n)
  if (detail.value) detail.value.rating = n
}

async function showInExplorer() {
  if (!detail.value) return
  await invoke(IPC.SHOW_IN_EXPLORER, { itemId: detail.value.id })
}

async function toggleLive() {
  if (!detail.value) return
  if (state.isPlayingLive.value) {
    state.isPlayingLive.value = false
    state.liveVideoSrc.value  = null
  } else {
    try {
      const path = await invoke<string>(IPC.GET_COMPANION_VIDEO_URL, { itemId: detail.value.id })
      state.liveVideoSrc.value  = convertFileSrc(path)
      state.isPlayingLive.value = true
    } catch (e) {
      ui.addToast('error', t('detail.livePhotoError'))
    }
  }
}
</script>

<style scoped>
.detail-overlay {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: flex;
}

.detail-panel {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  z-index: 201;
}

/* ── Viewer ───────────────────────────────────────────────────────────── */
/* ── 视图器 ───────────────────────────────────────────────────────────── */
.detail-viewer {
  flex: 1;
  position: relative;
  overflow: hidden;
  background: #000;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: grab;
}
.detail-viewer:active { cursor: grabbing; }

.detail-viewer__img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  transform-origin: center;
  user-select: none;
  pointer-events: none;
  transition: transform 0.2s cubic-bezier(0.25, 0.46, 0.45, 0.94);
}
.detail-viewer__img.is-dragging,
.detail-viewer__video.is-dragging {
  transition: none;
}
.detail-viewer__video,
.detail-viewer__audio {
  max-width: 90%;
  max-height: 90%;
  transform-origin: center;
  transition: transform 0.2s cubic-bezier(0.25, 0.46, 0.45, 0.94);
}
.detail-viewer__document {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--spacing-md);
  color: var(--color-text-secondary);
  font-size: 48px;
}

.detail-viewer__live-video {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: contain;
}

/* ── Controls ─────────────────────────────────────────────────────────── */
/* ── 控制器 ─────────────────────────────────────────────────────────── */
.detail-controls {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 52px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--spacing-md);
  background: linear-gradient(transparent, rgba(0,0,0,0.75));
  color: #fff;
  z-index: 10;
}
.detail-controls__left,
.detail-controls__right {
  display: flex;
  align-items: center;
  gap: 4px;
}
.detail-controls .btn-icon {
  color: rgba(255,255,255,0.8);
  font-size: 14px;
}
.detail-controls .btn-icon:hover { color: #fff; background: rgba(255,255,255,0.12); }
.detail-controls .btn-icon.active { color: var(--color-accent); }
/* 缩放百分比指示器 / zoom percentage indicator */
.detail-zoom-pct {
  font-size: var(--font-size-xs);
  color: rgba(255,255,255,0.75);
  font-variant-numeric: tabular-nums;
  min-width: 44px;
  text-align: center;
  letter-spacing: 0.01em;
  user-select: none;
}
.detail-controls__center {
  flex: 1;
  text-align: center;
  overflow: hidden;
}
.detail-controls__name {
  font-size: var(--font-size-sm);
  color: rgba(255,255,255,0.85);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  display: block;
  user-select: text;
  -webkit-user-select: text;
  cursor: text;
}

/* ── Info sidebar ─────────────────────────────────────────────────────── */
/* ── 信息侧边栏 ─────────────────────────────────────────────────────── */
.detail-info {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: 300px;
  background: var(--color-bg-secondary);
  border-left: 1px solid var(--color-border);
  overflow-y: auto;
  z-index: 12;
  display: flex;
  flex-direction: column;
  user-select: text;
  -webkit-user-select: text;
}
.detail-info__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--spacing-md);
  border-bottom: 1px solid var(--color-border);
  font-weight: 600;
  font-size: var(--font-size-sm);
  flex-shrink: 0;
}

.info-section {
  padding: var(--spacing-md);
  border-bottom: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}
.info-section__title {
  font-size: var(--font-size-xs);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--color-text-tertiary);
  margin-bottom: 4px;
}
.info-row {
  display: flex;
  justify-content: space-between;
  gap: var(--spacing-sm);
}
.info-label {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}
.info-value {
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: text;
  -webkit-user-select: text;
  cursor: text;
}

.rating-stars {
  display: flex;
  gap: 4px;
}
.star {
  font-size: 22px;
  color: var(--color-border);
  transition: color var(--transition-fast);
  cursor: pointer;
}
.star.filled { color: #ffc107; }
.star:hover  { color: #ffd54f; }

/* ── Slide transition ─────────────────────────────────────────────────── */
/* ── 滑动过渡 ─────────────────────────────────────────────────── */
.slide-enter-from,
.slide-leave-to  { transform: translateX(100%); }
.slide-enter-active,
.slide-leave-active { transition: transform var(--transition-normal); }
</style>
