<template>
  <!-- 真·时间 scrubber（Part5 §3.3）：按时间均布的月刻度 + 密度热力条 + 年份 label + 点击/拖拽跳转。
       与旧 mini-timeline 的本质区别：刻度位置按「月序均布」（每月等高），而非按逻辑高度比例——
       让稀疏月与海量月在 scrubber 上占同样空间，符合人对「时间」的直觉；密度由热力条长度体现。 -->
  <div
    ref="trackRef"
    class="tl-scrubber"
    :class="{ 'tl-scrubber--dragging': dragging }"
    @pointerdown="onPointerDown"
    @pointermove="onPointerMove"
    @pointerup="onPointerUp"
    @pointercancel="onPointerUp"
    @pointerleave="onTrackLeave"
  >
    <!-- 月密度模式（date 分组才有 monthBuckets） -->
    <template v-if="hasMonths">
      <div
        v-for="(b, i) in monthBuckets"
        :key="b.groupId"
        class="tl-month"
        :class="{ 'tl-month--active': i === activeIndex, 'tl-month--year-start': isYearStart(i) }"
        :style="{ top: `${(i / monthCount) * 100}%`, height: `${(1 / monthCount) * 100}%` }"
        @click.stop="emit('jump', b.y)"
        @pointerenter="hoverIndex = i"
      >
        <!-- 密度热力条：长度 = 该月项数占最热月的比例；右对齐，越长越热。 -->
        <span class="tl-month__bar" :style="{ width: `${barWidth(b.count)}%` }"></span>
        <!-- 年份标记：仅每年首月（最新→最旧排列，故年份变化处）显示，浮在轨道左侧。 -->
        <span v-if="isYearStart(i)" class="tl-month__year">{{ b.year }}</span>
      </div>
    </template>

    <!-- 回退模式：folder/none 分组无 monthBuckets → 沿用「按逻辑高度均布的分隔符圆点」旧行为，
         保证非 date 视图不回归（仍可点分隔符跳转）。 -->
    <template v-else>
      <div
        v-for="sep in separators"
        :key="sep.y"
        class="tl-sep-node"
        :style="{ top: `${(sep.y / Math.max(1, totalHeight)) * 100}%` }"
        :title="sep.label"
        @click.stop="emit('jump', sep.y)"
      ></div>
    </template>

    <!-- hover/拖拽浮层：显示当前指向的「年-月 · 张数」，浮在轨道左侧跟随光标月。 -->
    <div
      v-if="hasMonths && hoverIndex !== null"
      class="tl-flyout"
      :style="{ top: `${((hoverIndex + 0.5) / monthCount) * 100}%` }"
    >
      {{ monthBuckets[hoverIndex].year }}-{{ String(monthBuckets[hoverIndex].month).padStart(2, '0') }}
      <span class="tl-flyout__count">· {{ monthBuckets[hoverIndex].count }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
// 独立时间轴 scrubber 子组件（Part5 §3.3 要求从 MediaGrid 巨组件抽离）。
// 纯展示 + 交互：消费父层传入的 monthBuckets/separators，跳转意图经 emit('jump', y) 上抛，
// 不内嵌 store/IPC——与 MediaThumb 评分一致的「视图组件不内嵌副作用」约定。
import { ref, computed } from 'vue'
import type { MonthBucket } from '../../types/layout'
import {
  maxBucketCount,
  densityBarWidth,
  findActiveMonthIndex,
  isYearBoundary,
  fractionToMonthIndex,
} from './timelineScrubber.helpers'

const props = withDefaults(
  defineProps<{
    /** 月密度桶（date 分组才非空）：年/月/张数/逻辑 y/groupId。 */
    monthBuckets: MonthBucket[]
    /** 分隔符（回退用）：monthBuckets 为空时按逻辑高度均布圆点。 */
    separators: { label: string; y: number; groupId?: string }[]
    /** 布局总逻辑高度（回退模式按比例定位用）。 */
    totalHeight: number
    /** 当前逻辑滚动位置（高亮所在月）。 */
    currentY?: number
  }>(),
  { currentY: 0 },
)

const emit = defineEmits<{
  /** 跳转到某逻辑 y（父层 scrollToY 映射物理坐标 + smooth scroll）。 */
  jump: [y: number]
}>()

const trackRef = ref<HTMLElement | null>(null)
const dragging = ref(false)
const hoverIndex = ref<number | null>(null)

const monthCount = computed(() => props.monthBuckets.length)
const hasMonths = computed(() => monthCount.value > 0)

// 纯映射逻辑抽到 timelineScrubber.helpers（带单测），此处仅做响应式包裹。
const maxCount = computed(() => maxBucketCount(props.monthBuckets))
const barWidth = (count: number): number => densityBarWidth(count, maxCount.value)
const activeIndex = computed(() => findActiveMonthIndex(props.monthBuckets, props.currentY))
const isYearStart = (i: number): boolean => isYearBoundary(props.monthBuckets, i)

// ── 点击/拖拽跳转 ───────────────────────────────────────────────────────────
// 把光标在轨道上的纵向比例映射到目标：有月桶则映射到「时间均布」的月索引（跳到该月真实 y），
// 无月桶则回退按比例映射到 totalHeight。视觉均布、跳转用真实逻辑 y —— 二者解耦是本设计的要点。
function jumpToPointer(e: PointerEvent) {
  const el = trackRef.value
  if (!el) return
  const rect = el.getBoundingClientRect()
  const frac = Math.min(1, Math.max(0, (e.clientY - rect.top) / Math.max(1, rect.height)))
  if (hasMonths.value) {
    const idx = fractionToMonthIndex(frac, monthCount.value)
    hoverIndex.value = idx
    emit('jump', props.monthBuckets[idx].y)
  } else {
    emit('jump', frac * props.totalHeight)
  }
}

function onPointerDown(e: PointerEvent) {
  dragging.value = true
  // 捕获指针：拖出轨道边界仍持续收到 move/up，scrub 不中断。
  trackRef.value?.setPointerCapture(e.pointerId)
  jumpToPointer(e)
}

function onPointerMove(e: PointerEvent) {
  // 更新 hover 浮层（即使未拖拽）：移到哪个月就显哪个月的标签。
  if (!dragging.value && hasMonths.value) {
    const el = trackRef.value
    if (el) {
      const rect = el.getBoundingClientRect()
      const frac = Math.min(1, Math.max(0, (e.clientY - rect.top) / Math.max(1, rect.height)))
      hoverIndex.value = fractionToMonthIndex(frac, monthCount.value)
    }
  }
  if (dragging.value) jumpToPointer(e)
}

function onPointerUp(e: PointerEvent) {
  if (dragging.value) {
    dragging.value = false
    trackRef.value?.releasePointerCapture(e.pointerId)
  }
}

function onTrackLeave() {
  // 非拖拽时离开轨道才清 hover 浮层；拖拽中即便移出也保留（配合指针捕获持续 scrub）。
  if (!dragging.value) hoverIndex.value = null
}
</script>

<style scoped>
.tl-scrubber {
  position: absolute;
  left: 0;
  right: 0;
  top: 10px;
  bottom: 10px;
  /* 整轨可点/可拖（旧 mini-timeline 是 pointer-events:none，仅圆点可点；scrubber 需整轨拖拽）。 */
  cursor: pointer;
  touch-action: none; /* 防止移动端拖拽被浏览器手势抢走 */
}

/* ── 月刻度（时间均布）─────────────────────────────────────────────── */
.tl-month {
  position: absolute;
  left: 0;
  right: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
}
.tl-month__bar {
  height: 3px;
  border-radius: 2px;
  background: var(--color-border);
  transition:
    background var(--transition-fast),
    height var(--transition-fast);
}
.tl-month--active .tl-month__bar {
  background: var(--color-accent);
  height: 5px;
}
.tl-scrubber:hover .tl-month:hover .tl-month__bar {
  background: var(--color-text-secondary);
}
/* 年首月：热力条上方叠一根更亮的年分隔线，并显年份。 */
.tl-month--year-start .tl-month__bar {
  background: var(--color-text-secondary);
}
.tl-month__year {
  position: absolute;
  right: 100%;
  margin-right: 4px;
  font-size: 9px;
  line-height: 1;
  color: var(--color-text-secondary);
  white-space: nowrap;
  pointer-events: none;
  opacity: 0.75;
}

/* ── 回退：分隔符圆点（旧行为）────────────────────────────────────── */
.tl-sep-node {
  position: absolute;
  left: 50%;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--color-border);
  transform: translate(-50%, -50%);
  cursor: pointer;
}
.tl-sep-node:hover {
  background: var(--color-accent);
}

/* ── hover/拖拽浮层 ─────────────────────────────────────────────────── */
.tl-flyout {
  position: absolute;
  right: 100%;
  margin-right: 8px;
  transform: translateY(-50%);
  padding: 2px 7px;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  font-size: 11px;
  line-height: 1.4;
  color: var(--color-text-primary);
  white-space: nowrap;
  pointer-events: none;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.18);
  z-index: 2;
}
.tl-flyout__count {
  color: var(--color-text-secondary);
}
</style>
