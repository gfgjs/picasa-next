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
import { ref, computed, watch, onMounted } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
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
  (e: 'request-thumb', id: number): void
}>()

const isLoaded      = ref(false)
const displaySrc    = ref('')
const showFavorite  = ref(false)
const favAnimating  = ref(false)
const hasRequested  = ref(false)  // guard: only request once per mount

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
  // thumb_status meanings:
  //   0 = pending generation
  //   1 = generated WebP on disk  → load from cache dir
  //   2 = failed
  //   3 = small file direct display → load the original file via absPath
  //       (absPath is not available here; parent supplies the thumb_path as the abs path in this case)

  if (props.thumbStatus === 1 && props.thumbPath) {
    // Load the generated thumbnail from the cache directory
    try {
      const abs = `${props.cacheDir}/thumbnails/${props.thumbPath}`.replace(/\\/g, '/')
      const src = convertFileSrc(abs)
      const img = new Image()
      img.src = src
      await img.decode()
      displaySrc.value = src
      isLoaded.value   = true
    } catch { /* leave placeholder */ }
    return
  }

  if (props.thumbStatus === 3 && props.thumbPath) {
    // Small file: thumbPath holds the absolute path to the original file
    try {
      const src = convertFileSrc(props.thumbPath.replace(/\\/g, '/'))
      const img = new Image()
      img.src = src
      await img.decode()
      displaySrc.value = src
      isLoaded.value   = true
    } catch { /* leave placeholder */ }
    return
  }

  if (props.thumbStatus === 0) {
    // Not yet generated — ask the parent/grid to request generation.
    // Guard: only emit once per mount lifecycle to prevent infinite loops
    // when the backend fails and keeps returning status=2.
    if (!hasRequested.value) {
      hasRequested.value = true
      emit('request-thumb', props.id)
    }
  }
}

// Re-run loadThumb only when thumbPath/thumbStatus actually gets a usable value
// (status transitions from 0→1 or 0→3 after the parent receives batch results).
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
</script>

<style scoped>
.media-thumb {
  /* position:relative so thumbStyle width/height props are respected */
  position: relative;
  overflow: hidden;
  border-radius: 2px;
  background: var(--color-bg-elevated);
  /* cursor and flex-shrink live on the parent .media-card */
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

@keyframes fav-spring {
  0%   { transform: scale(1); }
  40%  { transform: scale(1.5); }
  70%  { transform: scale(0.9); }
  100% { transform: scale(1.2); }
}
</style>
