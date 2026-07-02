<template>
  <div class="epub-reader">
    <div v-if="error" class="epub-reader__error">{{ error }}</div>
    <button
      class="epub-nav epub-nav--prev"
      @click="prev"
      :title="t('doc.prevPage')"
      :aria-label="t('doc.prevPage')"
    >
      ‹
    </button>
    <div ref="hostEl" class="epub-reader__host"></div>
    <button
      class="epub-nav epub-nav--next"
      @click="next"
      :title="t('doc.nextPage')"
      :aria-label="t('doc.nextPage')"
    >
      ›
    </button>
  </div>
</template>

<script setup lang="ts">
// EPUB 阅读器（§5.1），基于 epub.js（分页流）。位置格式 "cfi:<epubcfi>"。
// 说明：本期固定 paginated 流 —— 滚轮/箭头翻页（epub 自带文字层、章节、CSS）。
// 连续滚动（flow:scrolled-doc）作为后续增强。
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'
import ePub from 'epubjs'
import type { Book, Rendition } from 'epubjs'
import { applyReplacerToDom } from '../../utils/replacements'

const props = defineProps<{
  url: string
  initial: string | null
  /** 替换规则函数（§5.2）；每章渲染后对其文本节点就地应用。 */
  replacer?: (t: string) => string
}>()
const emit = defineEmits<{
  (e: 'ready'): void
  (e: 'progress', pos: string): void
}>()

const { t } = useI18n()

const hostEl = ref<HTMLElement | null>(null)
const error = ref('')

let book: Book | null = null
let rendition: Rendition | null = null
let destroyed = false
let wheelLock = 0

function next() {
  rendition?.next()
}
function prev() {
  rendition?.prev()
}
// epub 为分页流，无原生滚动容器 → 返回 null（usePager 仅接键盘；滚轮由本组件处理）。
function getScrollEl(): HTMLElement | null {
  return null
}
defineExpose({ next, prev, getScrollEl })

// 滚轮翻页（节流）：分页 epub 无原生滚动，故由本组件统一处理（不经 usePager）。
function onWheel(e: WheelEvent) {
  e.preventDefault()
  const now = Date.now()
  if (now < wheelLock) return
  wheelLock = now + 350
  if (e.deltaY > 0) next()
  else if (e.deltaY < 0) prev()
}

onMounted(async () => {
  try {
    book = ePub(props.url)
    rendition = book.renderTo(hostEl.value!, {
      width: '100%',
      height: '100%',
      flow: 'paginated',
      spread: 'none',
    })
    // 替换规则（§5.2）：每章内容渲染进 iframe 后，对其文本节点就地应用。
    if (props.replacer) {
      rendition.hooks.content.register((contents: { document?: { body?: HTMLElement } }) => {
        try {
          if (contents?.document?.body) applyReplacerToDom(contents.document.body, props.replacer!)
        } catch {
          /* ignore per-chapter replace errors */
        }
      })
    }

    // 恢复到上次 CFI（无则从头）。
    const cfi =
      props.initial && props.initial.startsWith('cfi:') ? props.initial.slice(4) : undefined
    await rendition.display(cfi)
    if (destroyed) return
    rendition.on('relocated', (location: { start?: { cfi?: string } }) => {
      const c = location?.start?.cfi
      if (c) emit('progress', `cfi:${c}`)
    })
    hostEl.value?.addEventListener('wheel', onWheel, { passive: false })
    emit('ready')
  } catch (e) {
    error.value = t('doc.epubOpenFailed', { error: (e as Error)?.message ?? e })
    emit('ready')
  }
})

onBeforeUnmount(() => {
  destroyed = true
  hostEl.value?.removeEventListener('wheel', onWheel)
  try {
    rendition?.destroy()
    book?.destroy()
  } catch {
    /* ignore */
  }
})
</script>

<style scoped>
.epub-reader {
  position: relative;
  height: 100%;
  display: flex;
  align-items: stretch;
  background: var(--color-bg-surface, #fff);
}
.epub-reader__host {
  flex: 1;
  min-width: 0;
  height: 100%;
}
.epub-reader__error {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-secondary);
}
.epub-nav {
  width: 48px;
  flex: 0 0 auto;
  border: none;
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 28px;
  cursor: pointer;
  transition:
    background var(--transition-fast),
    color var(--transition-fast);
}
.epub-nav:hover {
  background: var(--color-bg-elevated);
  color: var(--color-text-primary);
}
</style>
