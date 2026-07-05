<template>
  <!-- T16 B3.2 自研逻辑滚动条:拇指 = 当前逻辑位 / 逻辑总高 的纯百分比渲染,与画廊
       逐帧同步;不参与物理 scrollTop 账务(映射态停稳偿债只动原生 scrollTop,本条
       零感知——这正是它替代原生条的理由,见 mediaScrollbar.helpers.ts 头注)。
       轨道常驻为透明命中区,空闲时拇指淡出;拖拽经 pointer capture,轨道点击直达定位
       并可无缝转拖。注:覆盖层锚定 .media-grid-wrapper 全高,人物返回栏可见时轨道顶
       与其重叠 ~34px(几何误差 <4%,特例场景可接受)。 -->
  <div
    ref="trackRef"
    class="media-scrollbar"
    :class="{ 'is-active': lingering || dragging || hovering }"
    role="scrollbar"
    aria-orientation="vertical"
    :aria-valuemin="0"
    :aria-valuemax="Math.round(maxY)"
    :aria-valuenow="Math.round(clampedY)"
    @pointerdown="onTrackPointerdown"
    @pointermove="onPointermove"
    @pointerup="endDrag"
    @pointercancel="endDrag"
    @pointerenter="hovering = true"
    @pointerleave="hovering = false"
  >
    <div
      v-if="geom"
      class="media-scrollbar__thumb"
      :style="{ transform: `translateY(${geom.top}px)`, height: geom.height + 'px' }"
      @pointerdown.stop="onThumbPointerdown"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue'
import { thumbGeometry, thumbTopToLogicalY } from './mediaScrollbar.helpers'

const props = defineProps<{
  /** 逻辑总高(px)——与引擎 totalHeight 同源,非物理 spacer。 */
  totalHeight: number
  /** 当前逻辑滚动位(px)——双引擎统一的 currentLogicalY。 */
  currentY: number
  /** 宿主滚动进行中(isScrolling)→ 拇指显形。 */
  active: boolean
}>()

const emit = defineEmits<{ (e: 'jump', y: number): void }>()

const trackRef = ref<HTMLElement | null>(null)
const trackH = ref(0)
const hovering = ref(false)
const dragging = ref(false)

/// 显形滞回(B3.2.1 真机回报修复):active(宿主 isScrolling)在滚动收尾会抖动——
/// 惯性尾梢的稀疏滚动事件(间隔超过宿主 150ms 复位阈值)与停稳偿债的内部 scrollTop
/// 写各翻转一轮 false→true→false,拇指随 opacity/颜色过渡明暗频闪。显形立即、
/// 隐没延迟合并:短暂重激活并入同一次可见期,任凭信号抖动也只有一次淡出。
const LINGER_MS = 700
const lingering = ref(false)
let lingerTimer: ReturnType<typeof setTimeout> | null = null

watch(
  () => props.active,
  (on) => {
    if (lingerTimer !== null) {
      clearTimeout(lingerTimer)
      lingerTimer = null
    }
    if (on) {
      lingering.value = true
      return
    }
    lingerTimer = setTimeout(() => {
      lingerTimer = null
      lingering.value = false
    }, LINGER_MS)
  },
  { immediate: true },
)
let resizeObserver: ResizeObserver | null = null
/// 抓点 = 指针在拇指内的相对位置(px),拖动全程保持,拇指不会跳到指针下。
let grabOffset = 0
let rafId: number | null = null
let pendingY: number | null = null

const maxY = computed(() => Math.max(0, props.totalHeight - trackH.value))
const clampedY = computed(() => Math.min(maxY.value, Math.max(0, props.currentY)))
const geom = computed(() => thumbGeometry(clampedY.value, props.totalHeight, trackH.value))

/// 拖拽跳转按 rAF 节流:pointermove 可达 120-240Hz,每帧只发最后一个目标位——
/// 引擎侧远跳/近跳都是 O(1) 算术,但没必要一帧多次。
function emitJumpThrottled(y: number) {
  pendingY = y
  if (rafId !== null) return
  rafId = requestAnimationFrame(() => {
    rafId = null
    if (pendingY !== null) emit('jump', pendingY)
    pendingY = null
  })
}

function trackTopScreen(): number {
  return trackRef.value?.getBoundingClientRect().top ?? 0
}

function beginDrag(e: PointerEvent, offset: number) {
  dragging.value = true
  grabOffset = offset
  // capture 设在轨道上:后续 move/up 无论指针滑到哪都回到本组件。
  trackRef.value?.setPointerCapture(e.pointerId)
}

function moveTo(e: PointerEvent) {
  const g = geom.value
  if (!g) return
  const thumbTop = e.clientY - trackTopScreen() - grabOffset
  emitJumpThrottled(thumbTopToLogicalY(thumbTop, props.totalHeight, trackH.value, g.height))
}

function onThumbPointerdown(e: PointerEvent) {
  if (e.button !== 0 || !geom.value) return
  beginDrag(e, e.clientY - trackTopScreen() - geom.value.top)
  e.preventDefault()
}

function onTrackPointerdown(e: PointerEvent) {
  if (e.button !== 0 || !geom.value) return
  // 轨道点击 = 直达定位(拇指中心对齐点击点),随即可无缝继续拖动。
  beginDrag(e, geom.value.height / 2)
  moveTo(e)
  e.preventDefault()
}

function onPointermove(e: PointerEvent) {
  if (!dragging.value) return
  moveTo(e)
}

function endDrag() {
  dragging.value = false
}

onMounted(() => {
  const el = trackRef.value
  if (!el || typeof ResizeObserver === 'undefined') return
  resizeObserver = new ResizeObserver((entries) => {
    trackH.value = entries[0].contentRect.height
  })
  resizeObserver.observe(el)
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
  if (rafId !== null) cancelAnimationFrame(rafId)
  if (lingerTimer !== null) clearTimeout(lingerTimer)
})
</script>

<style scoped>
.media-scrollbar {
  position: absolute;
  top: 0;
  bottom: 0;
  right: 0;
  width: 12px; /* 命中区比可视拇指宽,好抓 */
  z-index: 5;
  user-select: none;
  touch-action: none; /* 触屏上拖本条 = 拖拇指,不触发页面滚动 */
}

.media-scrollbar__thumb {
  position: absolute;
  top: 0;
  right: 2px;
  width: var(--scrollbar-width, 6px);
  border-radius: 3px;
  background: var(--color-scrollbar-thumb);
  opacity: 0.35;
  transition:
    opacity var(--transition-fast),
    background var(--transition-fast);
  will-change: transform;
}

.media-scrollbar.is-active .media-scrollbar__thumb,
.media-scrollbar:hover .media-scrollbar__thumb {
  opacity: 1;
  background: var(--color-scrollbar-thumb-hover);
}
</style>
