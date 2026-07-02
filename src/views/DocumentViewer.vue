<template>
  <div class="doc-viewer">
    <!-- 工具栏：返回 / 标题 / 页码 / 翻页模式 / 外部打开 -->
    <div class="doc-viewer__toolbar">
      <button class="doc-viewer__btn" @click="goBack" :title="t('common.back')">
        <ChevronLeft :size="18" /> <span>{{ t('common.back') }}</span>
      </button>
      <span class="doc-viewer__title" :title="title">{{ title }}</span>
      <span v-if="pageInfo" class="doc-viewer__page"
        >{{ pageInfo.page }} / {{ pageInfo.pages }}</span
      >
      <div class="doc-viewer__spacer"></div>
      <label v-if="kind === 'pdf' || kind === 'text'" class="doc-viewer__mode">
        <span>{{ t('doc.pagerMode') }}</span>
        <select v-model="pagerMode" @change="savePagerMode">
          <option value="scroll">{{ t('doc.pagerScroll') }}</option>
          <option value="wheel-snap">{{ t('doc.pagerWheelSnap') }}</option>
          <option value="keyboard">{{ t('doc.pagerKeyboard') }}</option>
        </select>
      </label>
      <button
        v-if="supportsEdit && !editing"
        class="doc-viewer__btn"
        @click="startEdit"
        :title="t('doc.edit')"
        :aria-label="t('doc.edit')"
      >
        <Pencil :size="16" />
      </button>
      <button
        v-if="supportsEdit"
        class="doc-viewer__btn"
        :class="{ 'is-active': showVersions }"
        @click="showVersions = !showVersions"
        :title="t('doc.versions')"
        :aria-label="t('doc.versions')"
      >
        <History :size="16" />
      </button>
      <button
        v-if="supportsEdit"
        class="doc-viewer__btn"
        :class="{ 'is-active': showProofread }"
        @click="showProofread = !showProofread"
        :title="t('doc.proofread')"
        :aria-label="t('doc.proofread')"
      >
        <Sparkles :size="16" />
      </button>
      <button
        v-if="supportsReplace"
        class="doc-viewer__btn"
        :class="{ 'is-active': showRepl }"
        @click="showRepl = !showRepl"
        :title="t('doc.replace')"
        :aria-label="t('doc.replace')"
      >
        <Replace :size="16" />
      </button>
      <button
        v-if="detail"
        class="doc-viewer__btn"
        @click="openExternal"
        :title="t('common.openExternal')"
        :aria-label="t('common.openExternal')"
      >
        <ExternalLink :size="16" />
      </button>
    </div>

    <!-- 渲染区 + 可选侧栏（替换 / 版本） -->
    <div class="doc-viewer__body">
      <div class="doc-viewer__reader">
        <!-- 编辑态（仅文本）：纯文本编辑 + 保存目标 -->
        <div v-if="editing" class="doc-edit">
          <div class="doc-edit__bar">
            <input
              v-model="editLabel"
              class="doc-edit__label"
              :placeholder="t('doc.versionLabelPlaceholder')"
            />
            <button class="doc-viewer__btn doc-viewer__btn--primary" @click="saveNewVersion">
              <Save :size="14" /> {{ t('doc.saveNewVersion') }}
            </button>
            <button class="doc-viewer__btn" @click="overwriteSource">
              {{ t('doc.overwriteSource') }}
            </button>
            <button class="doc-viewer__btn" @click="cancelEdit">{{ t('common.cancel') }}</button>
          </div>
          <textarea v-model="editBuffer" class="doc-edit__area" spellcheck="false"></textarea>
        </div>

        <template v-else-if="detail">
          <TextReader
            v-if="kind === 'text'"
            :key="readerKey"
            ref="readerRef"
            :url="url"
            :format="detail.fileFormat"
            :initial="initialPos"
            :transform="replacer"
            :content="textContent"
            @ready="onReady"
            @progress="onProgress"
          />
          <PdfReader
            v-else-if="kind === 'pdf'"
            :key="readerKey"
            ref="readerRef"
            :url="url"
            :initial="initialPos"
            @ready="onReady"
            @progress="onProgress"
            @info="onInfo"
          />
          <EpubReader
            v-else-if="kind === 'epub'"
            :key="readerKey"
            ref="readerRef"
            :url="url"
            :initial="initialPos"
            :replacer="replacer"
            @ready="onReady"
            @progress="onProgress"
          />
          <div v-else class="doc-viewer__unsupported">
            <FileQuestion :size="48" />
            <p>{{ t('doc.unsupportedFormat', { format: detail.fileFormat }) }}</p>
            <button class="doc-viewer__btn doc-viewer__btn--primary" @click="openExternal">
              {{ t('common.openExternal') }}
            </button>
          </div>
        </template>
        <div v-else-if="error" class="doc-viewer__unsupported">{{ error }}</div>
      </div>

      <ReplacementPanel
        v-if="showRepl && supportsReplace"
        :item-id="id"
        @changed="onReplChanged"
        @close="showRepl = false"
      />

      <VersionPanel
        v-if="showVersions && supportsEdit"
        :item-id="id"
        @changed="refreshText"
        @close="showVersions = false"
      />

      <ProofreadPanel
        v-if="showProofread && supportsEdit"
        :item-id="id"
        :text="textContent ?? ''"
        :current-version-id="currentVersionId"
        @changed="refreshText"
        @close="showProofread = false"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
// 文档浏览器（§5.1）：路由 /doc/:id。按格式分发到 pdf.js / epub.js / 文本渲染器；翻页逻辑
// 由 usePager 解耦（三种模式，存配置）；阅读进度按位置字符串保存/恢复（reading_progress 表）。
import { ref, computed, watch, onBeforeUnmount, defineAsyncComponent } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { open as shellOpen } from '@tauri-apps/plugin-shell'
import {
  ChevronLeft,
  ExternalLink,
  FileQuestion,
  Replace,
  Pencil,
  History,
  Save,
  Sparkles,
} from '@lucide/vue'
import { confirm } from '@tauri-apps/plugin-dialog'
import { IPC } from '../constants/ipc'
import { usePager, type PagerMode } from '../composables/usePager'
import { buildReplacer, type ReplacementRule } from '../utils/replacements'
import type { MediaDetail } from '../types/media'

// 渲染器懒加载：epubjs/pdfjs 仅在真正打开对应格式时才进入对应 chunk。
const TextReader = defineAsyncComponent(() => import('../components/doc/TextReader.vue'))
const PdfReader = defineAsyncComponent(() => import('../components/doc/PdfReader.vue'))
const EpubReader = defineAsyncComponent(() => import('../components/doc/EpubReader.vue'))
const ReplacementPanel = defineAsyncComponent(
  () => import('../components/doc/ReplacementPanel.vue'),
)
const VersionPanel = defineAsyncComponent(() => import('../components/doc/VersionPanel.vue'))
const ProofreadPanel = defineAsyncComponent(() => import('../components/doc/ProofreadPanel.vue'))

interface ReaderApi {
  next(): void
  prev(): void
  getScrollEl(): HTMLElement | null
}

const route = useRoute()
const router = useRouter()
const { t } = useI18n()

const id = computed(() => Number(route.params.id))
const detail = ref<MediaDetail | null>(null)
const error = ref('')
const initialPos = ref<string | null>(null)
const pageInfo = ref<{ page: number; pages: number } | null>(null)
const readerRef = ref<ReaderApi | null>(null)
const pagerMode = ref<PagerMode>('scroll')

// 替换规则（§5.2）：生效规则 → 替换函数；面板开关；reloadToken 用于规则变更后重渲染。
const replacer = ref<(t: string) => string>((t) => t)
const showRepl = ref(false)
const reloadToken = ref(0)

// 编辑/版本（§5.3，仅文本）：当前生效文本、编辑态与缓冲、当前版本 id、版本面板开关。
const textContent = ref<string | null>(null)
const editing = ref(false)
const editBuffer = ref('')
const editLabel = ref('')
const currentVersionId = ref<number | null>(null)
const showVersions = ref(false)
const showProofread = ref(false)

const url = computed(() => (detail.value ? convertFileSrc(detail.value.absPath) : ''))
const title = computed(() => detail.value?.fileName ?? t('routes.doc'))
// :key 含 reloadToken，使替换规则/版本变更后重建渲染器以重新套用。
const readerKey = computed(() => `${id.value}-${reloadToken.value}`)
// 替换仅支持 txt/epub（§5.2 首期）。
const supportsReplace = computed(() => kind.value === 'text' || kind.value === 'epub')
// 编辑 + 版本管理仅支持文本（§5.3）。
const supportsEdit = computed(() => kind.value === 'text')

// 格式 → 渲染器类别。
const TEXT_FORMATS = [
  'txt',
  'md',
  'markdown',
  'rtf',
  'log',
  'json',
  'csv',
  'xml',
  'yaml',
  'yml',
  'ini',
]
const kind = computed<'pdf' | 'epub' | 'text' | 'unsupported'>(() => {
  const f = (detail.value?.fileFormat ?? '').toLowerCase()
  if (f === 'pdf') return 'pdf'
  if (f === 'epub') return 'epub'
  if (TEXT_FORMATS.includes(f)) return 'text'
  return 'unsupported'
})

const pager = usePager({
  mode: () => pagerMode.value,
  next: () => readerRef.value?.next(),
  prev: () => readerRef.value?.prev(),
  container: () => readerRef.value?.getScrollEl() ?? null,
})

function onReady() {
  // epub 自管滚轮（分页流，无原生滚动）→ usePager 仅接键盘；text/pdf 绑定其滚动容器。
  pager.attach(kind.value === 'epub' ? null : (readerRef.value?.getScrollEl() ?? null))
}

// ── 阅读进度：去抖保存 ────────────────────────────────────────────────────────
let lastPos: string | null = null
let saveTimer: ReturnType<typeof setTimeout> | null = null
function onProgress(pos: string) {
  lastPos = pos
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(flushProgress, 1200)
}
function flushProgress() {
  if (saveTimer) {
    clearTimeout(saveTimer)
    saveTimer = null
  }
  if (lastPos != null && Number.isFinite(id.value)) {
    invoke(IPC.SET_READING_PROGRESS, { itemId: id.value, position: lastPos }).catch(() => {})
  }
}

function onInfo(info: { page: number; pages: number }) {
  pageInfo.value = info
}

async function load() {
  // 切换文档：先冲刷上一篇进度，拆掉旧监听。
  flushProgress()
  pager.detach()
  lastPos = null
  detail.value = null
  pageInfo.value = null
  initialPos.value = null
  textContent.value = null
  editing.value = false
  error.value = ''
  try {
    const d = await invoke<MediaDetail>(IPC.GET_MEDIA_DETAIL, { id: id.value })
    initialPos.value = await invoke<string | null>(IPC.GET_READING_PROGRESS, {
      itemId: id.value,
    }).catch(() => null)
    await loadReplacer()
    // 文本文档：取生效文本（当前版本或源）+ 当前版本 id（供编辑父版本）。
    const f = (d.fileFormat ?? '').toLowerCase()
    if (TEXT_FORMATS.includes(f)) {
      textContent.value = await invoke<string>(IPC.GET_DOCUMENT_TEXT, { itemId: id.value }).catch(
        () => null,
      )
      const cur = await invoke<{ id: number } | null>(IPC.GET_CURRENT_VERSION, {
        itemId: id.value,
      }).catch(() => null)
      currentVersionId.value = cur?.id ?? null
    }
    detail.value = d
  } catch (e) {
    error.value = t('doc.openFailed', { error: (e as Error)?.message ?? e })
  }
}

// ── 编辑 + 版本（§5.3，仅文本）─────────────────────────────────────────────────
function startEdit() {
  editBuffer.value = textContent.value ?? ''
  editLabel.value = ''
  editing.value = true
}
function cancelEdit() {
  editing.value = false
}

// 另存为新版本（默认，不进画廊）→ 设为当前 → 重渲染。
async function saveNewVersion() {
  const newId = await invoke<number>(IPC.SAVE_VERSION, {
    itemId: id.value,
    content: editBuffer.value,
    label: editLabel.value || null,
    parentId: currentVersionId.value,
    target: 'version',
  })
  await invoke(IPC.SET_CURRENT_VERSION, { itemId: id.value, versionId: newId })
  editing.value = false
  await refreshText()
}

// 覆盖源文件（高级，二次确认；后端自动先备份旧源为一个版本）。
async function overwriteSource() {
  const ok = await confirm(t('doc.overwriteConfirmMsg'), {
    title: t('doc.overwriteSource'),
    kind: 'warning',
  })
  if (!ok) return
  await invoke(IPC.SAVE_VERSION, {
    itemId: id.value,
    content: editBuffer.value,
    label: null,
    parentId: currentVersionId.value,
    target: 'overwrite',
  })
  editing.value = false
  await refreshText()
}

// 重新拉取生效文本 + 当前版本，并重建渲染器。
async function refreshText() {
  textContent.value = await invoke<string>(IPC.GET_DOCUMENT_TEXT, { itemId: id.value }).catch(
    () => null,
  )
  const cur = await invoke<{ id: number } | null>(IPC.GET_CURRENT_VERSION, {
    itemId: id.value,
  }).catch(() => null)
  currentVersionId.value = cur?.id ?? null
  reloadToken.value++
}

// 拉取该项生效的替换规则（global + item）并构建替换函数。
async function loadReplacer() {
  const rules = await invoke<ReplacementRule[]>(IPC.GET_EFFECTIVE_REPLACEMENTS, {
    itemId: id.value,
  }).catch(() => [])
  replacer.value = buildReplacer(rules)
}

// 规则变更：重建替换函数并重渲染当前文档（key 变化 → 渲染器重挂载重新套用）。
async function onReplChanged() {
  await loadReplacer()
  reloadToken.value++
}

function goBack() {
  if (window.history.length > 1) router.back()
  else router.push('/')
}

async function openExternal() {
  if (detail.value) await shellOpen(detail.value.absPath).catch(() => {})
}

function savePagerMode() {
  invoke(IPC.SET_APP_CONFIG, { key: 'doc_pager_mode', value: pagerMode.value }).catch(() => {})
}

// 初始化翻页模式（持久化），并随路由 id 变化重载文档。
invoke<string | null>(IPC.GET_APP_CONFIG, { key: 'doc_pager_mode' })
  .then((v) => {
    if (v === 'scroll' || v === 'wheel-snap' || v === 'keyboard') pagerMode.value = v
  })
  .catch(() => {})

watch(id, load, { immediate: true })

onBeforeUnmount(() => {
  flushProgress()
  pager.detach()
})
</script>

<style scoped>
.doc-viewer {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  background: var(--color-bg-base);
  z-index: 5;
}
.doc-viewer__toolbar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg-surface);
}
.doc-viewer__btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: transparent;
  border: 1px solid var(--color-border);
  color: var(--color-text-primary);
  padding: 5px 10px;
  border-radius: var(--radius-md);
  cursor: pointer;
  font-size: var(--font-size-sm);
}
.doc-viewer__btn:hover {
  background: var(--color-bg-elevated);
}
.doc-viewer__btn.is-active {
  background: var(--color-accent);
  color: #fff;
  border-color: transparent;
}
.doc-viewer__btn--primary {
  background: var(--color-accent);
  color: #fff;
  border-color: transparent;
}
.doc-viewer__title {
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 40vw;
}
.doc-viewer__page {
  font-family: var(--font-mono);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
.doc-viewer__spacer {
  flex: 1;
}
.doc-viewer__mode {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
.doc-viewer__mode select {
  background: var(--color-bg-elevated);
  color: var(--color-text-primary);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  padding: 3px 6px;
}
.doc-viewer__body {
  flex: 1;
  min-height: 0;
  display: flex;
}
.doc-viewer__reader {
  flex: 1;
  min-width: 0;
  height: 100%;
  position: relative;
}
.doc-viewer__reader > * {
  height: 100%;
}
.doc-viewer__unsupported {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  color: var(--color-text-secondary);
}

/* ── 编辑态 ─────────────────────────────────────────────────────────── */
.doc-edit {
  height: 100%;
  display: flex;
  flex-direction: column;
}
.doc-edit__bar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg-surface);
}
.doc-edit__label {
  flex: 1;
  min-width: 0;
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  color: var(--color-text-primary);
  padding: 5px 8px;
  font-size: var(--font-size-sm);
}
.doc-edit__area {
  flex: 1;
  min-height: 0;
  width: 100%;
  box-sizing: border-box;
  resize: none;
  border: none;
  outline: none;
  padding: 24px clamp(16px, 8vw, 120px);
  background: var(--color-bg-base);
  color: var(--color-text-primary);
  font-family: var(--font-mono);
  font-size: var(--font-size-sm);
  line-height: 1.7;
}
</style>
