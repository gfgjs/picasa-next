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
      'media-thumb--compact': compact,
      'media-thumb--missing': availability === 'missing',
      'media-thumb--offline': availability === 'offline',
    }"
    @mouseenter="isHovering = true; onHoverEnter()"
    @mouseleave="isHovering = false; onHoverLeave()"
  >
    <!-- 可用态角标（缺失检测 Part2 §3.2）：missing/offline 始终可见（含 compact），一眼可辨。 -->
    <div
      v-if="availability !== 'online'"
      class="media-thumb__avail"
      :class="'avail--' + availability"
      :title="
        availability === 'missing' ? $t('media.availMissingTitle') : $t('media.availOfflineTitle')
      "
    >
      {{ availability === 'missing' ? $t('media.availMissing') : $t('settings.volOffline') }}
    </div>
    <!-- 颜色标签色条（T16）：缩略图顶缘细条（Lightroom 式）。直接挂 .media-thumb（非 overlays），
         故 compact 下也显（零 SVG、仅一个 div，利于快速 culling）。 -->
    <div v-if="colorStrip" class="media-thumb__color-strip" :style="{ background: colorStrip }"></div>
    <!-- Text-document card (txt/md/office) — CSS「文本卡」，零解码，比灰底占位更清晰（§3.4） -->
    <div
      v-if="!isLoaded && isTextCard"
      class="media-thumb__textcard"
      :class="'fmt-' + (fileFormat || '').toLowerCase()"
    >
      <div class="media-thumb__textcard-lines">
        <span></span>
        <span></span>
        <span></span>
        <span></span>
      </div>
      <span class="media-thumb__textcard-ext">{{ (fileFormat || '').toUpperCase() }}</span>
    </div>
    <!-- Placeholder solid color + file format text -->
    <!-- 纯色占位符 + 文件格式文本 -->
    <div
      v-else-if="!isLoaded"
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

    <!-- Hover auto-play preview (video / live photo) — 共享池，同时仅一个在播放（需求1） -->
    <!-- is-painted：首帧解码完成(loadeddata)后才渐显，之前 opacity:0 让封面图透出，避免「移入闪黑」 -->
    <video
      v-if="isHoverPreview"
      class="media-thumb__video"
      :class="{ 'is-painted': previewPainted }"
      :src="hoverSrc"
      loop
      autoplay
      playsinline
      preload="metadata"
      @loadedmetadata="onPreviewReady"
      @loadeddata="previewPainted = true"
    />

    <!-- Hover scrub sprite (超大视频降级 §3.3) — 鼠标横移切关键帧，不解码视频 -->
    <div
      v-if="isScrubbing"
      class="media-thumb__sprite"
      :style="scrubStyle"
      @mousemove="onScrubMove"
    />

    <!-- Overlays -->
    <!-- 覆盖层 -->
    <!-- compact 模式下仅在选择态时保留覆盖层容器，其余时候整个砍掉以减少 800+ 空 DOM 节点 -->
    <div v-if="!compact || isSelected || isSelectionMode" class="media-thumb__overlays">
      <!-- Advanced Info Overlay & Badges -->
      <div
        v-if="
          !compact &&
          (thumbInfoLines.length > 0 ||
            (similarity == null &&
              isLoaded &&
              thumbStatus === 3 &&
              ui.showThumbInfo &&
              ui.thumbInfoElements.includes('status')) ||
            (similarity == null &&
              isLoaded &&
              thumbStatus === 1 &&
              ui.showThumbInfo &&
              ui.thumbInfoElements.includes('status')) ||
            (similarity == null &&
              fileSize &&
              ui.showThumbInfo &&
              ui.thumbInfoElements.includes('size')) ||
            similarity != null ||
            isLivePhoto)
        "
        class="media-thumb__info-overlay"
      >
        <div
          class="media-thumb__badges"
          style="display: flex; gap: 4px; margin-bottom: 2px; flex-wrap: wrap"
        >
          <span
            v-if="
              similarity == null &&
              isLoaded &&
              thumbStatus === 3 &&
              ui.showThumbInfo &&
              ui.thumbInfoElements.includes('status')
            "
            class="badge badge-source"
            :title="$t('media.badgeOrigTitle')"
            >ORIG</span
          >
          <span
            v-if="
              similarity == null &&
              isLoaded &&
              thumbStatus === 1 &&
              ui.showThumbInfo &&
              ui.thumbInfoElements.includes('status')
            "
            class="badge badge-thumb"
            :title="$t('media.badgeThumbTitle')"
            >THUMB</span
          >
          <span
            v-if="
              similarity == null &&
              fileSize &&
              ui.showThumbInfo &&
              ui.thumbInfoElements.includes('size')
            "
            class="badge badge-size"
            >{{ formatFileSize(fileSize) }}</span
          >
          <span v-if="similarity != null" class="badge badge-similarity"
            >{{ Math.round(similarity * 100) }}%</span
          >
          <span v-if="isLivePhoto" class="badge badge-live">LIVE</span>
        </div>
        <div v-for="(line, idx) in thumbInfoLines" :key="idx" class="info-line">{{ line }}</div>
      </div>

      <!-- Video play (hidden while hover-previewing) -->
      <!-- 视频播放（悬停预览中隐藏） -->
      <span
        v-if="mediaType === 'video' && !compact && !isHoverPreview && !isScrubbing"
        class="badge badge-video"
        ><Play :size="20" fill="#fff"
      /></span>
      <!-- Duration -->
      <!-- 时长 -->
      <span v-if="durationMs && !compact" class="badge badge-duration">{{
        formatDuration(durationMs)
      }}</span>
      <!-- Favorite (hidden at compact sizes — unusable + a costly always-on SVG) -->
      <!-- 收藏（极小尺寸下隐藏 —— 点不动且是常驻的高成本 SVG） -->
      <button
        v-if="!compact"
        class="media-thumb__fav"
        :class="{
          active: isFavorited,
          'fav-always-visible':
            isFavorited && ui.showThumbInfo && ui.thumbInfoElements.includes('favorite'),
        }"
        @click.stop="toggleFav"
        :title="$t('selection.favorite')"
        :aria-label="$t('selection.favorite')"
      >
        <Heart
          :size="14"
          :fill="isFavorited ? '#ff4757' : 'none'"
          :color="isFavorited ? '#ff4757' : '#fff'"
          :stroke-width="isFavorited ? 0 : 2"
        />
      </button>
      <!-- Rating stars (bottom-left, classic-album-style). Read-only filled stars when rated;
           full interactive 5-star strip on hover for quick set/clear (点当前星=清零).
           Gated !compact — 5 SVGs are costly at tiny cell sizes (same discipline as 收藏红心). -->
      <!-- 评分星级（左下，经典桌面相册式）：已评分时显示只读填充星角标；hover 整格出完整 5 星
           交互条供快捷打分/清零。!compact 守门（5 个 SVG 在极小尺寸下昂贵，同收藏纪律）。 -->
      <div v-if="!compact" class="media-thumb__rating-slot">
        <StarRating
          v-if="isHovering"
          class="media-thumb__rating media-thumb__rating--edit"
          :model-value="rating"
          :size="13"
          @change="onRate"
          @click.stop
        />
        <StarRating
          v-else-if="rating > 0"
          class="media-thumb__rating"
          :model-value="rating"
          :max="rating"
          :size="12"
          readonly
        />
      </div>
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
import StarRating from '../common/StarRating.vue'
import { colorLabelHex } from '../../constants/colorLabels'

import { useUiStore } from '../../stores/uiStore'
import { useMediaStore } from '../../stores/mediaStore'
import { useHoverPreview } from '../../composables/useHoverPreview'
import type { LayoutRowItem } from '../../types/layout'

interface Props {
  id: number
  w: number
  h: number
  mediaType: string
  isLivePhoto?: boolean
  durationMs?: number | null
  thumbStatus: number
  thumbPath?: string | null
  thumbhash?: number[] | null
  fileFormat?: string
  fileSize?: number
  similarity?: number
  isFavorited?: boolean
  /** 用户评分 0-5（0 = 未评分）。扁平 prop（镜像 isFavorited），保证乐观更新的响应式与收藏路径一致。 */
  rating?: number
  /** 用户颜色标签 0-7（0 = 未标，T16）。扁平 prop，镜像 rating。 */
  colorLabel?: number
  isSelected?: boolean
  isSelectionMode?: boolean
  isDragHover?: boolean
  cacheDir: string
  item?: LayoutRowItem
}

const props = withDefaults(defineProps<Props>(), {
  isLivePhoto: false,
  durationMs: null,
  thumbPath: null,
  thumbhash: null,
  isFavorited: false,
  rating: 0,
  colorLabel: 0,
  isSelected: false,
  isSelectionMode: false,
  isDragHover: false,
})

const emit = defineEmits<{
  (e: 'click', id: number): void
  (e: 'select', id: number): void
  (e: 'favorite', id: number): void
  (e: 'rate', id: number, value: number): void
  (e: 'request-thumb', id: number): void
  (e: 'cancel-thumb', id: number): void
}>()

const ui = useUiStore()
const media = useMediaStore()

// Heavy metadata (fileName / dirPath / EXIF / GPS) is no longer carried on the
// layout row item; it is fetched lazily for the visible viewport into the store.
// 重型元数据（fileName/dirPath/EXIF/GPS）不再随布局行项携带；
// 改为按可视区懒加载到 store 中。
const meta = computed(() => (props.id != null ? media.viewportMeta.get(props.id) : undefined))

const urlParams = new URLSearchParams(window.location.search)
const cacheBuster = urlParams.get('clear') ? `?t=${urlParams.get('clear')}` : ''

const isLoaded = ref(false)
const displaySrc = ref('')
const favAnimating = ref(false)
// hover 态：驱动评分星条「只读角标 ↔ 交互 5 星」切换 —— 仅当前 hover 的那一格渲染交互条，
// 全屏同时只存在一组交互 SVG（同收藏红心的成本纪律）。未 hover 且 rating>0 时仅显填充星角标。
const isHovering = ref(false)
const hasRequested = ref(false) // guard: only request once per mount // 守卫：每次挂载仅请求一次
let decodingImg: HTMLImageElement | null = null

const thumbStyle = computed(() => ({
  width: `${props.w}px`,
  height: `${props.h}px`,
}))

// Below this cell size, drop non-essential overlays/badges — and crucially their
// `backdrop-filter` blur and the always-rendered favorite SVG. They're invisible/
// unusable at this scale yet very costly when hundreds are on screen (e.g. tiny
// grid + full-thumbnail generation). Selection checkbox is kept (still usable).
// 低于此单元尺寸时，砍掉非必要的覆盖层/徽章 —— 关键是其 `backdrop-filter` 模糊与常驻
// 的收藏 SVG。它们在此尺度下看不清/点不动，但数百个同屏时（如极小网格 + 全量缩略图
// 生成）开销极大。选择复选框保留（仍可用）。
const COMPACT_THUMB_PX = 100
const compact = computed(() => props.w < COMPACT_THUMB_PX || props.h < COMPACT_THUMB_PX)

// Hover auto-play preview (需求1). Shared pool of size 1 — only one cell plays at a time.
// 悬停自动播放预览（需求1）。共享池容量 1 —— 同一时刻只有一个格子在播放。
const {
  isPreviewing: isHoverPreview,
  previewSrc: hoverSrc,
  isScrubbing,
  scrubStyle,
  onMove: onScrub,
  onEnter: onHoverEnter,
  onLeave: onHoverLeave,
} = useHoverPreview({
  id: () => props.id,
  mediaType: () => props.mediaType,
  isLivePhoto: () => !!props.isLivePhoto,
  fileSize: () => props.fileSize ?? 0,
  compact: () => compact.value,
  isSelectionMode: () => !!props.isSelectionMode,
})

// 悬停预览「首帧已绘制」标志：解决「鼠标移入瞬间闪黑」。
// video 元素在 isHoverPreview 变 true 时立即渲染（z-index 盖住封面图），但首帧解码绘制有延迟，
// 这段空窗期会露出黑底 → 闪黑。改为：painted 前 opacity:0（封面图/占位透出），loadeddata 后渐显。
// 预览结束（isHoverPreview→false）即复位，保证下一次进入的新 video 仍从 opacity:0 开始。
const previewPainted = ref(false)
watch(isHoverPreview, (v) => {
  if (!v) previewPainted.value = false
})

// scrub 模式：鼠标横向位置（相对格宽）→ 关键帧切换。
function onScrubMove(e: MouseEvent) {
  const el = e.currentTarget as HTMLElement
  const w = el.clientWidth || 1
  onScrub(e.offsetX / w)
}

// Muted + play on metadata load — setting `muted` via property avoids Vue's
// well-known `muted` attribute binding quirk, and satisfies autoplay policy.
// 元数据加载后置静音并播放 —— 用属性方式设 muted 规避 Vue 对 `muted` 特性绑定的已知问题，
// 同时满足浏览器自动播放策略。
function onPreviewReady(e: Event) {
  const v = e.target as HTMLVideoElement
  v.muted = true
  v.play().catch(() => {})
}

const thumbInfoLines = computed(() => {
  // compact 模式下直接跳过，避免 800+ 组件追踪 6+ 响应式依赖
  if (compact.value || !ui.showThumbInfo || !props.item) return []
  const lines: string[] = []
  const elements = ui.thumbInfoElements
  const it = props.item // cheap, resident fields (date / resolution)
  const m = meta.value // heavy fields, lazily fetched per viewport

  if (elements.includes('filename') && m?.fileName) {
    lines.push(m.fileName)
  }
  if (elements.includes('date') && it.sortDatetime) {
    lines.push(new Date(it.sortDatetime * 1000).toLocaleString())
  }
  if (elements.includes('resolution') && it.originalWidth && it.originalHeight) {
    lines.push(`${it.originalWidth} × ${it.originalHeight}`)
  }
  if (elements.includes('path') && m?.dirPath) {
    lines.push(m.dirPath)
  }
  if (elements.includes('geo') && m?.gpsLat != null && m?.gpsLng != null) {
    lines.push(`${m.gpsLat.toFixed(4)}, ${m.gpsLng.toFixed(4)}`)
  }
  if (elements.includes('camera') && (m?.exifMake || m?.exifModel)) {
    const make = m?.exifMake || ''
    const model = m?.exifModel || ''
    lines.push(`${make} ${model}`.trim())
  }
  if (elements.includes('params') && m) {
    const params = []
    if (m.exifFocalLength) params.push(`${m.exifFocalLength}mm`)
    if (m.exifAperture) params.push(`f/${m.exifAperture}`)
    if (m.exifShutter) params.push(`${m.exifShutter}s`)
    if (m.exifIso) params.push(`ISO${m.exifIso}`)
    if (params.length > 0) lines.push(params.join(' '))
  }

  return lines
})

// 系统可用态（缺失检测 Part2 §3.2）：从布局行项读取，旧缓存项缺该字段时默认 'online'。
const availability = computed<string>(() => props.item?.availability ?? 'online')

// 颜色标签色条颜色（T16）：0/未标 → null（不渲染色条）。色档→hex 映射在前端 colorLabels。
const colorStrip = computed<string | null>(() => colorLabelHex(props.colorLabel))

const placeholderBgColor = computed(() =>
  props.thumbhash ? thumbhashToAverageColor(props.thumbhash) : 'var(--color-bg-elevated)',
)

// Text-document formats that are NOT rasterised (pdf/svg/epub get real thumbnails). These
// render as a CSS "text card" (faux lines + extension badge) instead of a gray placeholder
// (§3.4). Skipped at compact sizes where the plain centered ext is enough.
// 不栅格化的文本文档格式（pdf/svg/epub 有真实缩略图）。用 CSS「文本卡」（仿文本行 + 扩展名角标）
// 呈现，替代灰底占位（§3.4）。compact 尺寸下退回居中扩展名占位即可。
const TEXT_CARD_FORMATS = [
  'txt',
  'md',
  'rtf',
  'doc',
  'docx',
  'xls',
  'xlsx',
  'ppt',
  'pptx',
  'odt',
  'ods',
  'odp',
]
const isTextCard = computed(
  () =>
    !compact.value &&
    props.mediaType === 'document' &&
    TEXT_CARD_FORMATS.includes((props.fileFormat || '').toLowerCase()),
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
      } catch {
        // decode() 失败回退到 DOM 加载（吞错，无需错误对象）
      }
      if (decodingImg !== img) return
      displaySrc.value = src
      isLoaded.value = true
    } catch {
      // status 1 外层吞错（无需错误对象）
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
        } catch {
          // decode() 失败回退到 DOM 加载（吞错，无需错误对象）
        }
        if (decodingImg !== img) return
        displaySrc.value = src
        isLoaded.value = true
      } catch {
        // status 3 外层吞错（无需错误对象）
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
  ([, newStatus]) => {
    if (newStatus === 1 || newStatus === 3) {
      loadThumb()
    }
  },
)

async function toggleFav() {
  favAnimating.value = true
  setTimeout(() => {
    favAnimating.value = false
  }, 400)
  emit('favorite', props.id)
}

// 缩略图内 hover 快捷评分：把用户点选值上抛父层（MediaGrid 落 setRating + 乐观更新布局行）。
// 与收藏一致走 emit 而非直接调 store，保持「视图组件不内嵌 IPC」的一致性。
function onRate(value: number) {
  emit('rate', props.id, value)
}

function onError() {
  displaySrc.value = ''
  isLoaded.value = false
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
  transition:
    transform 0.25s cubic-bezier(0.34, 1.18, 0.64, 1),
    border-radius 0.25s ease;
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

/* ── Text-document card (§3.4) — paper-like card with faux lines + ext badge ── */
/* ── 文本文档卡（§3.4）—— 纸张感卡片 + 仿文本行 + 扩展名角标 ── */
/* 硬编码色豁免说明(S5,设计 §6.2):本组件内叠在照片/视频/彩色徽章之上的
   #fff、黑系渐变与 drop-shadow 语义为「媒体上的永远白字黑纱」,属画布内容区,
   刻意不随主题——主题化会破坏照片观感中性红线。可主题化的(纸面/徽章底色)
   已全部收敛为 --color-doc-paper(-line) 与 --color-badge-doc-xxx token。
   (注意:注释内写 token 通配不能用「…paper* / …」——「*」贴着「/」会拼出「星斜杠」提前终止块注释) */
.media-thumb__textcard {
  position: absolute;
  inset: 0;
  background: var(--color-doc-paper);
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 14% 12%;
  /* 顶端细装订线，强化「文档」观感 */
  border-top: 3px solid rgba(0, 0, 0, 0.06);
}
.media-thumb__textcard-lines {
  display: flex;
  flex-direction: column;
  gap: 9%;
}
.media-thumb__textcard-lines span {
  display: block;
  height: 5px;
  border-radius: 2px;
  background: var(--color-doc-paper-line);
}
.media-thumb__textcard-lines span:nth-child(1) {
  width: 65%;
}
.media-thumb__textcard-lines span:nth-child(2) {
  width: 100%;
}
.media-thumb__textcard-lines span:nth-child(3) {
  width: 92%;
}
.media-thumb__textcard-lines span:nth-child(4) {
  width: 55%;
}
.media-thumb__textcard-ext {
  align-self: flex-start;
  font-family: var(--font-mono);
  font-size: 11px;
  font-weight: 700;
  color: #fff;
  background: var(--color-badge-doc-generic);
  padding: 2px 6px;
  border-radius: 3px;
  letter-spacing: 0.04em;
}
/* 按类型着色角标（Office 沿用其品牌色，便于一眼区分） */
.media-thumb__textcard.fmt-md .media-thumb__textcard-ext {
  background: var(--color-badge-doc-md);
}
.media-thumb__textcard.fmt-doc .media-thumb__textcard-ext,
.media-thumb__textcard.fmt-docx .media-thumb__textcard-ext,
.media-thumb__textcard.fmt-odt .media-thumb__textcard-ext {
  background: var(--color-badge-doc-word);
}
.media-thumb__textcard.fmt-xls .media-thumb__textcard-ext,
.media-thumb__textcard.fmt-xlsx .media-thumb__textcard-ext,
.media-thumb__textcard.fmt-ods .media-thumb__textcard-ext {
  background: var(--color-badge-doc-excel);
}
.media-thumb__textcard.fmt-ppt .media-thumb__textcard-ext,
.media-thumb__textcard.fmt-pptx .media-thumb__textcard-ext,
.media-thumb__textcard.fmt-odp .media-thumb__textcard-ext {
  background: var(--color-badge-doc-ppt);
}

.media-thumb__img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
}

/* Hover preview video sits above the still image, below the overlays (z-index:10). */
/* 悬停预览视频位于静态图之上、覆盖层（z-index:10）之下。 */
.media-thumb__video {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  z-index: 5;
  /* 首帧解码完成（loadeddata → .is-painted）前 opacity:0，让下层封面图/占位透出，
     移入不再闪黑；就绪后短暂渐显切到视频。不再用 background:#000（空窗期会盖成黑块）。 */
  opacity: 0;
  transition: opacity 0.18s ease;
}
.media-thumb__video.is-painted {
  opacity: 1;
}

/* Hover scrub sprite — fills the cell; frame chosen via background-position (§3.3). */
/* 悬停 scrub 雪碧图 —— 铺满格子；通过 background-position 选帧（§3.3）。 */
.media-thumb__sprite {
  position: absolute;
  inset: 0;
  z-index: 5;
  /* 不用黑底：sprite 图加载前让下层封面图透出，与视频预览一致，避免移入闪黑。 */
  cursor: ew-resize;
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
  transition:
    opacity var(--transition-fast),
    transform var(--transition-fast);
  padding: 4px;
  border: none;
  filter: drop-shadow(0 1px 3px rgba(0, 0, 0, 0.6));
}
.media-thumb__fav:hover {
  transform: scale(1.1);
}
.media-thumb__fav.fav-always-visible {
  opacity: 1;
}

/* ── Rating stars (bottom-left, classic-album-style) ─────────────────────────── */
/* ── 评分星级（左下，经典桌面相册式）─────────────────────────────────────── */
.media-thumb__rating-slot {
  position: absolute;
  bottom: 4px;
  left: 6px;
  /* 容器不挡事件；仅交互星条（--edit）显式开启 pointer-events，避免只读角标拦截下层 hover 预览。 */
  pointer-events: none;
  /* 琥珀星在亮图上易糊 —— 投影增强对比，与收藏红心同处理。 */
  filter: drop-shadow(0 1px 3px rgba(0, 0, 0, 0.7));
  z-index: 11;
}
.media-thumb__rating--edit {
  pointer-events: auto;
}

/* 颜色标签色条（T16）：顶缘 4px 细条，盖过角标层但不挡交互。 */
.media-thumb__color-strip {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 4px;
  z-index: 11;
  pointer-events: none;
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

/* ── Selection visual states | 选择视觉状态 ─────────────────────── */

.media-thumb--selected {
  transform: scale(0.85);
  border-radius: var(--radius-lg);
  /* accent 描边环(§6.3):暗色主题下仅缩小+压暗遮罩不可辨,补一圈零模糊
     box-shadow(零模糊环渲染开销极低,批量选中数百格也安全)。 */
  box-shadow: 0 0 0 2px var(--color-accent);
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
  background: linear-gradient(
    to top,
    rgba(0, 0, 0, 0.85) 0%,
    rgba(0, 0, 0, 0.5) 70%,
    transparent 100%
  );
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
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.8);
}

/* ── 可用态：missing/offline 置灰 + 角标（缺失检测 Part2 §3.2）─────────────── */
/* 整格去色 + 降透明，一眼区分「不在场」；filter 作用于整个 thumb（含图/占位）。 */
.media-thumb--missing {
  filter: grayscale(1) brightness(0.85);
  opacity: 0.5;
}
.media-thumb--offline {
  filter: grayscale(0.7);
  opacity: 0.72;
}

.media-thumb__avail {
  position: absolute;
  top: 4px;
  left: 4px;
  z-index: 11; /* 盖过 overlays，保证可见 */
  font-size: 9px;
  font-weight: 700;
  line-height: 1;
  padding: 2px 5px;
  border-radius: var(--radius-sm);
  color: #fff;
  letter-spacing: 0.04em;
  pointer-events: auto; /* 允许 hover 出 title 提示 */
}
.media-thumb__avail.avail--missing {
  background: rgba(220, 53, 69, 0.92);
} /* 红：文件没了 */
.media-thumb__avail.avail--offline {
  background: rgba(108, 117, 125, 0.92);
} /* 灰：卷离线 */

/* ── Compact 模式极限优化 ─────────────────────────────────────────── */
/* 当单元尺寸 < 100px 时，禁用 transition 和额外合成层以极大减少 GPU 开销。
   contain: strict 告诉浏览器此元素内部变化不影响外部布局。 */
.media-thumb--compact {
  transition: none;
  contain: strict;
}
</style>
