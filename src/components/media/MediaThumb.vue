<template>
  <!-- ThumbHash placeholder -->
  <div
    class="media-thumb"
    :style="thumbStyle"
    :class="{ loaded: isLoaded, 'media-thumb--placeholder': !isLoaded }"
  >
    <!-- Placeholder blur -->
    <div
      v-if="!isLoaded && placeholderBg"
      class="media-thumb__placeholder"
      :style="{ backgroundImage: placeholderBg }"
    />
    <!-- Actual image -->
    <img
      v-if="displaySrc"
      class="media-thumb__img thumb-loaded"
      :src="displaySrc"
      :width="w"
      :height="h"
      loading="lazy"
      @error="onError"
    />
    <!-- Empty state (no thumb yet) -->
    <div v-if="!displaySrc && !placeholderBg" class="media-thumb__fallback">
      <span>{{ mediaTypeIcon }}</span>
    </div>

    <!-- Overlays -->
    <div class="media-thumb__overlays">
      <!-- LIVE badge -->
      <span v-if="isLivePhoto" class="badge badge-live">LIVE</span>
      <!-- Video play -->
      <span v-if="mediaType === 'video'" class="badge badge-video">▶</span>
      <!-- Duration -->
      <span v-if="durationMs" class="badge badge-duration">{{ formatDuration(durationMs) }}</span>
      <!-- Favorite -->
      <button
        v-if="showFavorite"
        class="media-thumb__fav"
        :class="{ active: isFavorited, 'fav-animate': favAnimating }"
        @click.stop="toggleFav"
        title="收藏"
      >{{ isFavorited ? '❤️' : '🤍' }}</button>
      <!-- Selection checkbox -->
      <div
        v-if="isSelected || isSelectionMode"
        class="media-thumb__checkbox"
        @click.stop="emit('select', id)"
      >
        <div class="checkbox" :class="{ checked: isSelected }">
          <span v-if="isSelected">✓</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { thumbhashToBackgroundImage } from '../../utils/thumbhash'
import { formatDuration } from '../../utils/format'

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
  isFavorited?:    boolean
  isSelected?:     boolean
  isSelectionMode?: boolean
  cacheDir:        string
}

const props = withDefaults(defineProps<Props>(), {
  isLivePhoto:     false,
  durationMs:      null,
  thumbPath:       null,
  thumbhash:       null,
  isFavorited:     false,
  isSelected:      false,
  isSelectionMode: false,
})

const emit = defineEmits<{
  (e: 'click', id: number): void
  (e: 'select', id: number): void
  (e: 'favorite', id: number): void
}>()

const isLoaded    = ref(false)
const displaySrc  = ref('')
const showFavorite = ref(false)
const favAnimating = ref(false)

const thumbStyle = computed(() => ({
  width:  `${props.w}px`,
  height: `${props.h}px`,
}))

const placeholderBg = computed(() =>
  props.thumbhash ? thumbhashToBackgroundImage(props.thumbhash) : ''
)

const mediaTypeIcon = computed(() => {
  if (props.mediaType === 'video')    return '🎬'
  if (props.mediaType === 'audio')    return '🎵'
  if (props.mediaType === 'document') return '📄'
  return '🖼️'
})

async function loadThumb() {
  if (!props.thumbPath) return
  try {
    const { convertFileSrc } = await import('@tauri-apps/api/core')
    const abs = `${props.cacheDir}/thumbnails/${props.thumbPath}`.replace(/\\/g, '/')
    const src = convertFileSrc(abs)
    const img = new Image()
    img.src = src
    await img.decode()
    displaySrc.value = src
    isLoaded.value   = true
  } catch { /* leave placeholder */ }
}

async function toggleFav() {
  favAnimating.value = true
  setTimeout(() => { favAnimating.value = false }, 400)
  emit('favorite', props.id)
}

function onError() { displaySrc.value = '' }

onMounted(() => {
  if (props.thumbPath) loadThumb()
})
</script>

<style scoped>
.media-thumb {
  position: relative;
  overflow: hidden;
  border-radius: 2px;
  background: var(--color-bg-elevated);
  cursor: pointer;
  flex-shrink: 0;
}
.media-thumb:hover .media-thumb__fav,
.media-thumb:hover .media-thumb__checkbox {
  opacity: 1;
}

.media-thumb__placeholder {
  position: absolute;
  inset: 0;
  background-size: cover;
  background-position: center;
  filter: blur(8px);
  transform: scale(1.05); /* hide blur edges */
}

.media-thumb__img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.media-thumb__fallback {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 32px;
  opacity: 0.4;
}

/* ── Overlays ─────────────────────────────────────────────────────────── */
.media-thumb__overlays {
  position: absolute;
  inset: 0;
  pointer-events: none;
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
.badge-live {
  top: 6px;
  left: 6px;
  background: var(--color-badge-live);
  color: #fff;
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
  left: 4px;
  background: transparent;
  font-size: 16px;
  opacity: 0;
  pointer-events: auto;
  transition: opacity var(--transition-fast);
  padding: 2px;
  line-height: 1;
}
.media-thumb__fav.active { opacity: 1; }
.media-thumb__fav.fav-animate {
  animation: fav-spring 300ms cubic-bezier(0.34, 1.56, 0.64, 1);
}

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
</style>
