<template>
  <div ref="scrollEl" class="text-reader" :class="{ 'is-md': isMarkdown }">
    <div v-if="error" class="text-reader__error">{{ error }}</div>
    <div v-else class="text-reader__content" v-html="html"></div>
  </div>
</template>

<script setup lang="ts">
// 文本/Markdown 阅读器（§5.1）。前端原生渲染：txt → 预格式化；md → 轻量 markdown 渲染。
// 位置格式：滚动比例 "scroll:0.xx"。next/prev = 滚动约一屏（连续内容的「翻页」）。
import { ref, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { renderMarkdown, renderPlainText } from '../../utils/markdown'

const props = defineProps<{
  url: string
  format: string
  initial: string | null
  /** 替换规则等可选的文本变换钩子（§5.2 接入点）。 */
  transform?: (raw: string) => string
  /** 直接提供文本（如当前版本/编辑后内容，§5.3）；提供时不再 fetch url。 */
  content?: string | null
}>()

const emit = defineEmits<{
  (e: 'ready'): void
  (e: 'progress', pos: string): void
}>()

const { t } = useI18n()

const scrollEl = ref<HTMLElement | null>(null)
const html = ref('')
const error = ref('')
const isMarkdown = props.format === 'md' || props.format === 'markdown'

function emitProgress() {
  const el = scrollEl.value
  if (!el) return
  const max = el.scrollHeight - el.clientHeight
  const ratio = max > 0 ? el.scrollTop / max : 0
  emit('progress', `scroll:${ratio.toFixed(4)}`)
}

function restore() {
  const el = scrollEl.value
  if (!el || !props.initial) return
  const m = /^scroll:([\d.]+)$/.exec(props.initial)
  if (!m) return
  const ratio = parseFloat(m[1])
  const max = el.scrollHeight - el.clientHeight
  el.scrollTop = Math.max(0, ratio * max)
}

// next/prev：滚动约 0.9 屏，作为连续文本的「翻页」。
function next() {
  scrollEl.value?.scrollBy({ top: (scrollEl.value.clientHeight || 600) * 0.9, behavior: 'smooth' })
}
function prev() {
  scrollEl.value?.scrollBy({
    top: -((scrollEl.value.clientHeight || 600) * 0.9),
    behavior: 'smooth',
  })
}
function getScrollEl() {
  return scrollEl.value
}
defineExpose({ next, prev, getScrollEl })

onMounted(async () => {
  try {
    // 优先用直接提供的文本（当前版本/编辑后），否则 fetch 源文件。
    let text: string
    if (props.content != null) {
      text = props.content
    } else {
      const resp = await fetch(props.url)
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
      text = await resp.text()
    }
    if (props.transform) text = props.transform(text)
    html.value = isMarkdown ? renderMarkdown(text) : renderPlainText(text)
  } catch (e) {
    error.value = t('doc.textOpenFailed', { error: (e as Error)?.message ?? e })
  }
  await nextTick()
  restore()
  scrollEl.value?.addEventListener('scroll', emitProgress, { passive: true })
  emit('ready')
})

onBeforeUnmount(() => {
  scrollEl.value?.removeEventListener('scroll', emitProgress)
})
</script>

<style scoped>
.text-reader {
  height: 100%;
  overflow-y: auto;
  padding: 32px clamp(16px, 8vw, 120px);
  box-sizing: border-box;
}
.text-reader__error {
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
}
/* 纯文本：等宽、保留空白、自动换行 */
.text-reader__content :deep(.doc-plain) {
  font-family: var(--font-mono);
  font-size: var(--font-size-sm);
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-word;
  margin: 0;
  color: var(--color-text-primary);
}
/* Markdown：阅读排版 */
.text-reader.is-md .text-reader__content {
  max-width: 760px;
  margin: 0 auto;
  font-size: var(--font-size-base);
  line-height: 1.75;
  color: var(--color-text-primary);
}
.text-reader.is-md :deep(h1),
.text-reader.is-md :deep(h2),
.text-reader.is-md :deep(h3) {
  margin: 1.4em 0 0.6em;
  line-height: 1.3;
  font-weight: 700;
}
.text-reader.is-md :deep(h1) {
  font-size: 1.9em;
  border-bottom: 1px solid var(--color-border);
  padding-bottom: 0.3em;
}
.text-reader.is-md :deep(h2) {
  font-size: 1.5em;
}
.text-reader.is-md :deep(h3) {
  font-size: 1.25em;
}
.text-reader.is-md :deep(p) {
  margin: 0.8em 0;
}
.text-reader.is-md :deep(ul),
.text-reader.is-md :deep(ol) {
  margin: 0.6em 0;
  padding-left: 1.6em;
}
.text-reader.is-md :deep(li) {
  margin: 0.25em 0;
}
.text-reader.is-md :deep(a) {
  color: var(--color-accent);
}
.text-reader.is-md :deep(code) {
  font-family: var(--font-mono);
  font-size: 0.88em;
  background: var(--color-bg-elevated);
  padding: 0.1em 0.35em;
  border-radius: 3px;
}
.text-reader.is-md :deep(pre) {
  background: var(--color-bg-elevated);
  padding: 14px 16px;
  border-radius: var(--radius-md);
  overflow-x: auto;
}
.text-reader.is-md :deep(pre code) {
  background: none;
  padding: 0;
}
.text-reader.is-md :deep(blockquote) {
  margin: 0.8em 0;
  padding: 0.2em 1em;
  border-left: 3px solid var(--color-border);
  color: var(--color-text-secondary);
}
.text-reader.is-md :deep(hr) {
  border: none;
  border-top: 1px solid var(--color-border);
  margin: 1.5em 0;
}
</style>
