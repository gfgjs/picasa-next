<template>
  <!--
    ⚠️ TWO-ROOT FRAGMENT — a sticky header + a collapsible body, with NO wrapping
    element. This is load-bearing, do not "tidy" it into a single <div>.
    ⚠️ 两根片段——一个粘性标题 + 一个可折叠主体，外层无包裹元素。这是承重设计，
    请勿「整理」成单个 <div>。

    Because component boundaries are transparent in the DOM, every section's
    header ends up a *direct child* of `.sidebar__scroll-area`. That is what lets
    `position: sticky` pin AND stack all headers across the whole scroll range
    (dual top + bottom). Wrapping header+body in a <div> would scope each header's
    sticky to that wrapper, so headers could only stick within their own section
    and cross-section stacking would break.
    由于组件边界在 DOM 中是透明的，每个区块的标题最终都是 `.sidebar__scroll-area`
    的*直接子元素*。这正是 `position: sticky` 能在整个滚动范围内粘住并堆叠所有标题
    （top + bottom 双向）的原因。若把 标题+主体 包进 <div>，会把每个标题的 sticky
    限制在该包裹元素内，导致标题只能在各自区块内粘住，破坏跨区块堆叠。
  -->
  <div class="acc-header" :style="{ top: stickyTop, bottom: stickyBottom }" @click="sections.toggle(id)">
    <ChevronRight :size="14" class="acc-header__chevron" :class="{ expanded }" />
    <span class="acc-header__title">{{ title }}</span>
    <!-- optional right-aligned actions; clicks here must not toggle the section -->
    <!-- 可选的右侧操作区；此处点击不应触发区块折叠 -->
    <span v-if="$slots.actions" class="acc-header__actions" @click.stop>
      <slot name="actions" />
    </span>
  </div>

  <transition name="acc-collapse" @enter="onEnter" @after-enter="onAfterEnter" @leave="onLeave">
    <!-- v-show (NOT v-if): collapsing hides the body but keeps its DOM, so nested
         state (e.g. folder-tree expansion) survives — 「多级展开状态记忆」. -->
    <!-- v-show（非 v-if）：折叠仅隐藏主体但保留其 DOM，使嵌套状态（如文件夹树展开）
         得以保留——「多级展开状态记忆」。 -->
    <div v-show="expanded" class="acc-body">
      <!-- Inner wrapper carries the padding — see .acc-body__inner note below. -->
      <!-- 内层包裹元素承载 padding——见下方 .acc-body__inner 说明。 -->
      <div class="acc-body__inner">
        <slot />
      </div>
    </div>
  </transition>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue'
import { ChevronRight } from '@lucide/vue'
import { useSidebarSections } from '../../composables/useSidebarSections'

const props = defineProps<{
  /** stable section key for expand-state persistence | 用于持久化展开状态的稳定区块键 */
  id: string
  /** display order among sections; drives sticky stacking | 区块间的显示顺序；驱动粘性堆叠 */
  order: number
  title: string
}>()

const sections = useSidebarSections()
const expanded = computed(() => sections.isExpanded(props.id))

// Register only while mounted, so conditional sections (mounted via v-if) take
// part in the sticky offsets exactly when they're visible — no gaps.
// 仅在挂载期间登记，使条件区块（通过 v-if 挂载）恰好在可见时参与粘性偏移——不留空档。
onMounted(() => sections.register(props.id, props.order))
onUnmounted(() => sections.unregister(props.id))

const index = computed(() => {
  const i = sections.visibleIds.value.indexOf(props.id)
  return i < 0 ? 0 : i
})
const total = computed(() => sections.visibleIds.value.length || 1)

// Stacked sticky offsets, all expressed against ONE CSS var so the arithmetic
// can never drift from the header's real height:
//   top    = H * index            → pin to top, stacked downward
//   bottom = H * (total-1-index)   → pin to bottom, stacked upward
// 堆叠粘性偏移，全部以单个 CSS 变量表达，使算术永不与标题真实高度脱节：
//   top    = H * index            → 粘顶，向下堆叠
//   bottom = H * (total-1-index)  → 粘底，向上堆叠
const stickyTop = computed(() => `calc(var(--sidebar-header-h) * ${index.value})`)
const stickyBottom = computed(() => `calc(var(--sidebar-header-h) * ${total.value - 1 - index.value})`)

// Height/opacity transition for collapse & expand (works with v-show).
// 折叠与展开的高度/透明度过渡（配合 v-show 使用）。
function onEnter(el: Element) {
  const h = el as HTMLElement
  h.style.height = '0'
  h.style.opacity = '0'
  void h.offsetHeight // force reflow | 强制回流
  h.style.height = h.scrollHeight + 'px'
  h.style.opacity = '1'
}
function onAfterEnter(el: Element) {
  const h = el as HTMLElement
  h.style.height = ''   // back to auto so content can grow freely | 恢复 auto，使内容自由增长
  h.style.opacity = ''
}
function onLeave(el: Element) {
  const h = el as HTMLElement
  h.style.height = h.offsetHeight + 'px'
  h.style.opacity = '1'
  void h.offsetHeight // force reflow | 强制回流
  h.style.height = '0'
  h.style.opacity = '0'
}
</script>

<style scoped>
.acc-collapse-enter-active,
.acc-collapse-leave-active {
  transition: height 0.28s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.28s cubic-bezier(0.4, 0, 0.2, 1);
  overflow: hidden;
}

/* ── Sticky header ─────────────────────────────────────────────────────────
   双向粘性：滚出顶部时粘顶、滚出底部时粘底，按 index 堆叠互不遮挡。 */
.acc-header {
  position: sticky;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 4px;
  height: var(--sidebar-header-h);
  padding: 0 var(--spacing-md);
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.02em;
  color: var(--color-text-secondary);
  cursor: pointer;
  user-select: none;
  /* MUST stay opaque in EVERY state — body content scrolls UNDER pinned headers.
     A translucent bg (e.g. --color-bg-hover) would let that content bleed through.
     必须在任何状态下保持不透明——主体内容会从已固定的标题下方滚过。半透明背景
     （如 --color-bg-hover）会让下方内容透出来。 */
  background: var(--color-bg-secondary);
  border-bottom: 1px solid var(--color-divider);
  transition: color 0.15s, background-color 0.15s;
}
.acc-header:hover {
  color: var(--color-text-primary);
  /* opaque, theme-aware hover — NOT the translucent --color-bg-hover */
  /* 不透明、随主题变化的 hover 背景——非半透明的 --color-bg-hover */
  background: var(--color-bg-elevated);
}
.acc-header__chevron {
  flex-shrink: 0;
  transition: transform 0.2s;
}
.acc-header__chevron.expanded {
  transform: rotate(90deg);
}
.acc-header__title {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.acc-header__actions {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: default;
}

/* The animated element itself MUST stay zero-padding so that height→0 collapses
   to a true 0. A content-box with padding can't shrink below its padding height,
   so height→0 leaves a residual band that display:none snaps away in the last
   frame — a visible "顿一下" at the end of the collapse. Put the breathing room
   on the inner wrapper instead. | 被动画的元素本身必须零内边距，使 height→0 能干净
   收到真正的 0。带 padding 的盒子无法收缩到小于其 padding 高度，height→0 会残留一条，
   在 display:none 时于末帧瞬间消失——即折叠收尾那一「顿」。留白改放到内层包裹元素。 */
.acc-body__inner {
  padding: var(--spacing-sm) 0;
}
</style>
