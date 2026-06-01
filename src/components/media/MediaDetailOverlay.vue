<template>
  <Teleport to="body">
    <div v-if="media.isDetailOpen && media.detailItem" class="detail-overlay overlay-enter">
      <!-- Backdrop -->
      <div class="overlay-backdrop" @click="close" />

      <!-- Panel -->
      <div class="detail-panel">
        <!-- ── Viewer ───────────────────────────────────────────────────── -->
        <div
          class="detail-viewer"
          @wheel.prevent="state.onWheel"
          @mousedown="state.startDrag"
        >
          <img
            v-if="detail.mediaType === 'image'"
            :src="absPath"
            class="detail-viewer__img"
            :style="{ transform: state.transform.value }"
            draggable="false"
          />
          <video
            v-else-if="detail.mediaType === 'video'"
            :src="absPath"
            class="detail-viewer__video"
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
            <span>📄</span>
            <p>{{ detail.fileName }}</p>
          </div>

          <!-- Live photo video overlay -->
          <video
            v-if="state.isPlayingLive.value && state.liveVideoSrc.value"
            :src="state.liveVideoSrc.value"
            class="detail-viewer__live-video"
            autoplay
            loop
          />
        </div>

        <!-- ── Controls ────────────────────────────────────────────────── -->
        <div class="detail-controls">
          <!-- Left -->
          <div class="detail-controls__left">
            <button class="btn-icon" @click="close" title="关闭 (Esc)">✕</button>
            <button class="btn-icon" @click="state.zoomIn()">＋</button>
            <button class="btn-icon" @click="state.zoomOut()">－</button>
            <button class="btn-icon" @click="state.resetZoom()" title="重置缩放">◎</button>
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
          <div class="detail-controls__center">
            <span class="detail-controls__name">{{ detail.fileName }}</span>
          </div>

          <!-- Right -->
          <div class="detail-controls__right">
            <button
              class="btn-icon"
              :class="{ active: detail.isFavorited }"
              @click="toggleFav"
              title="收藏"
            >{{ detail.isFavorited ? '❤️' : '🤍' }}</button>
            <button class="btn-icon" @click="showInExplorer" title="在文件夹中显示">📂</button>
            <button class="btn-icon" @click="state.toggleInfo()" title="详细信息">ℹ️</button>
          </div>
        </div>

        <!-- ── Info sidebar (slide in when showInfo) ────────────────────── -->
        <Transition name="slide">
          <div v-if="state.showInfo.value" class="detail-info">
            <div class="detail-info__header">
              <span>文件信息</span>
              <button class="btn-icon" @click="state.toggleInfo()">✕</button>
            </div>

            <div class="info-section">
              <div class="info-row">
                <span class="info-label">文件名</span>
                <span class="info-value">{{ detail.fileName }}</span>
              </div>
              <div class="info-row">
                <span class="info-label">文件大小</span>
                <span class="info-value">{{ formatFileSize(detail.fileSize) }}</span>
              </div>
              <div class="info-row">
                <span class="info-label">尺寸</span>
                <span class="info-value" v-if="detail.width">{{ detail.width }} × {{ detail.height }}</span>
              </div>
              <div class="info-row">
                <span class="info-label">格式</span>
                <span class="info-value">{{ detail.fileFormat.toUpperCase() }}</span>
              </div>
            </div>

            <!-- EXIF -->
            <div v-if="detail.imageMeta" class="info-section">
              <div class="info-section__title">EXIF</div>
              <div v-if="detail.imageMeta.exifDatetime" class="info-row">
                <span class="info-label">拍摄时间</span>
                <span class="info-value">{{ formatDateTime(detail.imageMeta.exifDatetime) }}</span>
              </div>
              <div v-if="detail.imageMeta.exifMake" class="info-row">
                <span class="info-label">相机</span>
                <span class="info-value">{{ detail.imageMeta.exifMake }} {{ detail.imageMeta.exifModel }}</span>
              </div>
              <div v-if="detail.imageMeta.exifFocalLength" class="info-row">
                <span class="info-label">焦距</span>
                <span class="info-value">{{ formatFocalLength(detail.imageMeta.exifFocalLength) }}</span>
              </div>
              <div v-if="detail.imageMeta.exifAperture" class="info-row">
                <span class="info-label">光圈</span>
                <span class="info-value">{{ formatAperture(detail.imageMeta.exifAperture) }}</span>
              </div>
              <div v-if="detail.imageMeta.exifShutter" class="info-row">
                <span class="info-label">快门</span>
                <span class="info-value">{{ detail.imageMeta.exifShutter }}s</span>
              </div>
              <div v-if="detail.imageMeta.exifIso" class="info-row">
                <span class="info-label">ISO</span>
                <span class="info-value">{{ detail.imageMeta.exifIso }}</span>
              </div>
              <div v-if="detail.imageMeta.exifGpsLat" class="info-row">
                <span class="info-label">GPS</span>
                <span class="info-value">{{ formatGps(detail.imageMeta.exifGpsLat, detail.imageMeta.exifGpsLng!) }}</span>
              </div>
            </div>

            <!-- Rating -->
            <div class="info-section">
              <div class="info-section__title">评分</div>
              <div class="rating-stars">
                <button
                  v-for="n in 5"
                  :key="n"
                  class="star"
                  :class="{ filled: n <= (detail.rating ?? 0) }"
                  @click="setRating(n)"
                >★</button>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useMediaStore } from '../../stores/mediaStore'
import { useUiStore }    from '../../stores/uiStore'
import { useMediaDetail } from '../../composables/useMediaDetail'
import { formatFileSize, formatDateTime, formatFocalLength, formatAperture, formatGps } from '../../utils/format'
import { IPC } from '../../constants/ipc'

const media = useMediaStore()
const ui    = useUiStore()

const detail  = computed(() => media.detailItem!)
const state   = computed(() => useMediaDetail(detail.value))
const absPath = computed(() => detail.value ? convertFileSrc(detail.value.absPath) : '')

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
  const s = state.value
  if (s.isPlayingLive.value) {
    s.isPlayingLive.value = false
    s.liveVideoSrc.value  = null
  } else {
    try {
      const path = await invoke<string>(IPC.GET_COMPANION_VIDEO_URL, { itemId: detail.value.id })
      s.liveVideoSrc.value  = convertFileSrc(path)
      s.isPlayingLive.value = true
    } catch (e) {
      ui.addToast('error', '无法加载 Live 视频')
    }
  }
}

// Close on Escape
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape' && media.isDetailOpen) close()
})
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
}
.detail-viewer__video,
.detail-viewer__audio {
  max-width: 90%;
  max-height: 90%;
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
}

/* ── Info sidebar ─────────────────────────────────────────────────────── */
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
.slide-enter-from,
.slide-leave-to  { transform: translateX(100%); }
.slide-enter-active,
.slide-leave-active { transition: transform var(--transition-normal); }
</style>
