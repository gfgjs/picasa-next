<template>
  <AccordionSection id="tools" :order="order" :title="$t('sidebar.tools')">
    <ul class="tool-list">
      <li
        v-for="(key, index) in ui.pinnedSettings"
        :key="key"
        class="tool"
        :class="{ 'tool--drop': dropIndex === index && dragIndex !== null && dragIndex !== index }"
        :data-tool-index="index"
      >
        <!-- Drag handle — only this initiates reorder, so card controls stay usable. -->
        <!-- 拖拽手柄——仅此处发起排序，使卡片内控件仍可使用。 -->
        <span class="tool__handle" title="拖拽排序 | Drag to reorder" @pointerdown="onPointerDown(index, $event)">
          <GripVertical :size="14" />
        </span>

        <!-- Special: full thumbnail generation | 特殊：全量生成缩略图 -->
        <div v-if="key === 'fullThumbGen'" class="tool__card">
          <div class="tool__row">
            <div class="tool__main">
              <span class="tool__icon"><Zap :size="18" /></span>
              <span class="tool__label">全量生成缩略图</span>
            </div>
            <button class="btn-icon" :title="scan.thumbGenProgress.isRunning ? '停止生成' : '开始生成'" @click="toggleThumbGen">
              <Square v-if="scan.thumbGenProgress.isRunning" :size="14" color="var(--color-error)" fill="var(--color-error)" />
              <Play v-else :size="14" />
            </button>
          </div>
          <div v-if="scan.thumbGenProgress.isRunning || scan.thumbGenProgress.status === 'completed'" class="tool__progress">
            <div v-if="scan.thumbGenProgress.isRunning" class="progress-bar">
              <div class="progress-bar__fill" :style="{ width: thumbPercent + '%' }" />
            </div>
            <div class="tool__progress-meta">
              <span>{{ scan.thumbGenProgress.generated }} / {{ scan.thumbGenProgress.total }}</span>
              <span v-if="thumbElapsedStr" class="mono">{{ thumbElapsedStr }}</span>
            </div>
          </div>
        </div>

        <!-- Special: full AI analysis (permanent core tool) | 特殊：全量 AI 分析（常驻核心项） -->
        <div v-else-if="key === 'aiFullAnalysis'" class="tool__card">
          <div class="tool__row">
            <div class="tool__main">
              <span class="tool__icon"><Sparkles :size="18" /></span>
              <span class="tool__label">全量 AI 分析</span>
            </div>
            <button class="btn-icon" :disabled="isAiInitialising" :title="ai.status.isAnalyzing ? '停止分析' : '开始分析'" @click="toggleAiAnalysis">
              <Square v-if="ai.status.isAnalyzing" :size="14" color="var(--color-error)" fill="var(--color-error)" />
              <RefreshCw v-else-if="isAiInitialising" :size="14" class="spin-anim" />
              <Play v-else :size="14" />
            </button>
          </div>
          <div v-if="ai.status.isAnalyzing || ai.status.totalItems > 0" class="tool__progress">
            <div v-if="ai.status.isAnalyzing" class="progress-bar">
              <div class="progress-bar__fill" :style="{ width: ai.analyzeProgress + '%' }" />
            </div>
            <div class="tool__progress-meta">
              <span>{{ ai.status.analyzedItems }} / {{ ai.status.totalItems }}</span>
              <span v-if="aiElapsedStr" class="mono push">{{ aiElapsedStr }}</span>
              <span class="mono">{{ ai.analyzeProgress }}%</span>
            </div>
          </div>
        </div>

        <!-- Generic: a pinned Settings item rendered with its compact control. -->
        <!-- 通用：置顶的设置项，使用其紧凑控件渲染。 -->
        <div v-else-if="SETTINGS_MAP[key]" class="tool__card">
          <div class="tool__main">
            <span class="tool__icon"><component :is="SETTINGS_MAP[key].icon" :size="18" /></span>
            <span class="tool__label tool__label--ellipsis">{{ $t(SETTINGS_MAP[key].label) }}</span>
          </div>
          <DynamicSettingControl :setting-key="key" compact />
        </div>
      </li>
    </ul>
  </AccordionSection>
</template>

<script setup lang="ts">
import { ref, computed, watch, onUnmounted } from 'vue'
import { GripVertical, Zap, Square, Play, Sparkles, RefreshCw } from '@lucide/vue'
import AccordionSection from '../AccordionSection.vue'
import DynamicSettingControl from '../../settings/DynamicSettingControl.vue'
import { useUiStore } from '../../../stores/uiStore'
import { useScanStore } from '../../../stores/scanStore'
import { useAiStore } from '../../../stores/aiStore'
import { SETTINGS_MAP } from '../../../constants/settingsMap'
import { beginPointerDrag, DRAG_THRESHOLD } from '../../../composables/usePointerDrag'

defineProps<{ order: number }>()

const ui = useUiStore()
const scan = useScanStore()
const ai = useAiStore()

// ── Drag-to-reorder (pointer-based, see usePointerDrag) ─────────────────────
// ── 拖拽排序（基于指针，见 usePointerDrag） ─────────────────────────────────
const dragIndex = ref<number | null>(null)
const dropIndex = ref<number | null>(null)

function onPointerDown(index: number, e: PointerEvent) {
  if (e.button !== 0) return
  e.preventDefault() // suppress text selection on the handle | 抑制手柄上的文本选择
  const startX = e.clientX, startY = e.clientY
  let dragging = false

  beginPointerDrag(
    (ev) => {
      if (!dragging) {
        if (Math.abs(ev.clientX - startX) + Math.abs(ev.clientY - startY) < DRAG_THRESHOLD) return
        dragging = true
        dragIndex.value = index
        document.body.style.userSelect = 'none'
        document.body.style.cursor = 'grabbing'
      }
      const li = (document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null)?.closest('[data-tool-index]') as HTMLElement | null
      dropIndex.value = li ? Number(li.dataset.toolIndex) : null
    },
    (_ev, cancelled) => {
      const from = dragIndex.value, to = dropIndex.value
      dragIndex.value = null
      dropIndex.value = null
      if (!cancelled && dragging && from != null && to != null && from !== to) {
        ui.reorderPinnedSetting(from, to)
      }
    },
  )
}

// ── Thumbnail-gen controls + elapsed timer ──────────────────────────────────
// ── 缩略图生成控制 + 计时 ───────────────────────────────────────────────────
const thumbPercent = computed(() =>
  (scan.thumbGenProgress.generated / Math.max(scan.thumbGenProgress.total, 1)) * 100,
)

function toggleThumbGen() {
  if (scan.thumbGenProgress.isRunning) scan.stopFullThumbnailGeneration()
  else scan.startFullThumbnailGeneration()
}

const { elapsedStr: thumbElapsedStr } = useElapsedTimer(
  () => scan.thumbGenProgress.isRunning,
  () => scan.thumbGenProgress.status === 'completed',
)

// ── AI analysis controls + elapsed timer ────────────────────────────────────
// ── AI 分析控制 + 计时 ───────────────────────────────────────────────────────
const isAiInitialising = ref(false)

async function toggleAiAnalysis() {
  if (ai.status.isAnalyzing) {
    await ai.stopAnalysis()
  } else {
    if (isAiInitialising.value) return
    isAiInitialising.value = true
    try {
      if (!ai.status.clipLoaded) await ai.initEngine()
      await ai.startAnalysis()
    } finally {
      isAiInitialising.value = false
    }
  }
}

const { elapsedStr: aiElapsedStr } = useElapsedTimer(
  () => ai.status.isAnalyzing,
  () => ai.status.totalItems > 0,
)

// ── Shared elapsed-time helper ──────────────────────────────────────────────
// A running rAF clock that ticks while `isRunning()` is true and keeps its last
// value while `keepAfter()` is true (e.g. a completed run). Returns "Xm Y.ZZZs".
// ── 共享的计时助手 ───────────────────────────────────────────────────────────
// 一个 rAF 时钟：`isRunning()` 为真时走动，`keepAfter()` 为真时保留最后的值
//（如已完成的任务）。返回 "Xm Y.ZZZs"。
function useElapsedTimer(isRunning: () => boolean, keepAfter: () => boolean) {
  const ms = ref(0)
  let startedAt: number | null = null
  let frame: number | null = null

  function tick() {
    if (startedAt != null && isRunning()) {
      ms.value = Date.now() - startedAt
      frame = requestAnimationFrame(tick)
    }
  }

  watch(isRunning, (running) => {
    if (running) {
      startedAt = Date.now()
      ms.value = 0
      if (frame) cancelAnimationFrame(frame)
      frame = requestAnimationFrame(tick)
    } else {
      if (frame) { cancelAnimationFrame(frame); frame = null }
      if (startedAt != null) ms.value = Date.now() - startedAt
    }
  })

  onUnmounted(() => { if (frame) cancelAnimationFrame(frame) })

  const elapsedStr = computed(() => {
    if (ms.value === 0 && !isRunning() && !keepAfter()) return ''
    const total = ms.value
    const secs = Math.floor(total / 1000)
    const m = Math.floor(secs / 60)
    const s = secs % 60
    const msPart = String(total % 1000).padStart(3, '0')
    return `${m}m ${s}.${msPart}s`
  })

  return { elapsedStr }
}
</script>

<style scoped>
.tool-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 4px var(--spacing-xs) 8px;
  list-style: none;
  margin: 0;
}

/* ── Tool row (handle + card) ────────────────────────────────────────────── */
.tool {
  display: flex;
  align-items: stretch;
  gap: 4px;
  position: relative;
}
.tool__handle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  flex-shrink: 0;
  color: var(--color-text-tertiary);
  cursor: grab;
  opacity: 0;
  transition: opacity var(--transition-fast);
}
.tool:hover .tool__handle { opacity: 0.6; }
.tool__handle:hover { opacity: 1; }
.tool__handle:active { cursor: grabbing; }

/* drop indicator line above the hovered target | 悬停目标上方的落点指示线 */
.tool--drop::before {
  content: '';
  position: absolute;
  top: -5px;
  left: 0;
  right: 0;
  height: 2px;
  border-radius: 1px;
  background: var(--color-accent);
}

/* ── Card ─────────────────────────────────────────────────────────────────
   每个工具是一张卡片：可含标题行 + 进度 / 控件。整张卡片不可点击，仅内部控件可交互。 */
.tool__card {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px var(--spacing-sm);
  background: var(--color-bg-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03);
}
.tool__card:hover {
  background: var(--color-bg-hover);
  border-color: var(--color-border-strong);
}
.tool__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--spacing-sm);
}
.tool__main {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  min-width: 0;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
}
.tool__icon {
  width: 20px;
  flex-shrink: 0;
  display: inline-flex;
  justify-content: center;
}
.tool__label { white-space: nowrap; }
.tool__label--ellipsis {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── Progress ─────────────────────────────────────────────────────────────── */
.tool__progress {
  font-size: 12px;
  color: var(--color-text-tertiary);
}
.tool__progress-meta {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.mono { font-family: monospace; }
.push { margin-right: auto; margin-left: 8px; }
.progress-bar {
  height: 3px;
  border-radius: 2px;
  background: var(--color-border);
  overflow: hidden;
  margin-bottom: 4px;
}
.progress-bar__fill {
  height: 100%;
  border-radius: 2px;
  background: var(--color-accent);
  transition: width 100ms linear;
}

.spin-anim { animation: spin 1s linear infinite; }
</style>
