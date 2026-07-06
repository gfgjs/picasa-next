<template>
  <div class="ver-panel">
    <div class="ver-panel__head">
      <span class="ver-panel__title">{{ t('doc.versions') }}</span>
      <button
        class="ver-panel__x"
        @click="emit('close')"
        :title="t('common.close')"
        :aria-label="t('common.close')"
      >
        <X :size="16" />
      </button>
    </div>

    <div class="ver-panel__list">
      <!-- 源文件基线 -->
      <div class="ver-row" :class="{ current: !currentId }">
        <div class="ver-row__main">
          <span class="ver-row__label">{{ t('doc.verOriginal') }}</span>
          <span v-if="!currentId" class="ver-badge">{{ t('doc.verCurrent') }}</span>
        </div>
        <div class="ver-row__actions">
          <button v-if="currentId" @click="setCurrent(null)" :title="t('doc.verRestoreOriginal')">
            {{ t('doc.verSetCurrent') }}
          </button>
        </div>
      </div>

      <!-- 各版本（最新在上） -->
      <div v-for="v in versionsDesc" :key="v.id" class="ver-row" :class="{ current: v.isCurrent }">
        <div class="ver-row__main">
          <span class="ver-row__label">{{ v.label || t('doc.verLabelFallback', { id: v.id }) }}</span>
          <span class="ver-row__meta">{{ srcLabel(v.source) }} · {{ fmtTime(v.createdAt) }}</span>
          <span v-if="v.isCurrent" class="ver-badge">{{ t('doc.verCurrent') }}</span>
        </div>
        <div class="ver-row__actions">
          <button
            @click="showDiff(v.id)"
            :title="t('doc.verDiffOriginal')"
            :aria-label="t('doc.verDiffOriginal')"
          >
            <GitCompare :size="14" />
          </button>
          <button v-if="!v.isCurrent" @click="setCurrent(v.id)" :title="t('doc.verSetCurrent')">
            {{ t('doc.verSetCurrent') }}
          </button>
          <button
            class="ver-row__del"
            @click="remove(v.id)"
            :title="t('selection.delete')"
            :aria-label="t('selection.delete')"
          >
            <Trash2 :size="14" />
          </button>
        </div>
      </div>

      <div v-if="!versions.length" class="ver-panel__empty">
        {{ t('doc.verEmpty') }}
      </div>
    </div>

    <!-- diff 视图 -->
    <div v-if="diff" class="ver-diff">
      <div class="ver-diff__head">
        <span>{{ t('doc.verDiffTitle', { id: diffVid }) }}</span>
        <button
          class="ver-panel__x"
          @click="diff = null"
          :title="t('common.close')"
          :aria-label="t('common.close')"
        >
          <X :size="14" />
        </button>
      </div>
      <div class="ver-diff__body">
        <div v-for="(op, i) in diff" :key="i" class="ver-diff__line" :class="'op-' + op.tag">
          <span class="ver-diff__sign">{{
            op.tag === 'insert' ? '+' : op.tag === 'delete' ? '−' : ' '
          }}</span>
          <span class="ver-diff__text">{{ op.value }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
// 文档版本时间线 + 差异（§5.3）。列出原始文件基线 + 各版本快照；设为当前 / 删除 / 与原始比较。
import { ref, computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@tauri-apps/api/core'
import { X, Trash2, GitCompare } from '@lucide/vue'
import { IPC } from '../../constants/ipc'

interface DocVersion {
  id: number
  itemId: number
  parentId: number | null
  label: string | null
  storage: string
  absPath: string
  source: string
  note: string | null
  contentHash: string | null
  isCurrent: boolean
  createdAt: number
}
interface DiffOp {
  tag: string
  value: string
}

const props = defineProps<{ itemId: number }>()
const emit = defineEmits<{ (e: 'changed'): void; (e: 'close'): void }>()
const { t } = useI18n()

const versions = ref<DocVersion[]>([])
const diff = ref<DiffOp[] | null>(null)
const diffVid = ref<number | null>(null)

const versionsDesc = computed(() => [...versions.value].reverse())
const currentId = computed(() => versions.value.find((v) => v.isCurrent)?.id ?? null)

function srcLabel(s: string) {
  return s === 'ai-remote'
    ? t('doc.verSrcAiRemote')
    : s === 'ai-local'
      ? t('doc.verSrcAiLocal')
      : t('doc.verSrcManual')
}
function fmtTime(t: number) {
  return new Date(t * 1000).toLocaleString()
}

async function reload() {
  versions.value = await invoke<DocVersion[]>(IPC.LIST_VERSIONS, { itemId: props.itemId }).catch(
    () => [],
  )
}

async function setCurrent(versionId: number | null) {
  await invoke(IPC.SET_CURRENT_VERSION, { itemId: props.itemId, versionId })
  await reload()
  emit('changed')
}

async function remove(versionId: number) {
  await invoke(IPC.DELETE_VERSION, { versionId })
  if (diffVid.value === versionId) diff.value = null
  await reload()
  emit('changed')
}

async function showDiff(versionId: number) {
  diffVid.value = versionId
  // a = null（原始基线），b = 该版本
  diff.value = await invoke<DiffOp[]>(IPC.DIFF_VERSIONS, {
    itemId: props.itemId,
    a: null,
    b: versionId,
  }).catch(() => [])
}

watch(() => props.itemId, reload, { immediate: true })
defineExpose({ reload })
</script>

<style scoped>
.ver-panel {
  display: flex;
  flex-direction: column;
  width: 340px;
  height: 100%;
  background: var(--color-bg-surface);
  border-left: 1px solid var(--color-border);
}
.ver-panel__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-bottom: 1px solid var(--color-border);
}
.ver-panel__title {
  font-weight: 600;
}
.ver-panel__x {
  background: transparent;
  border: none;
  color: var(--color-text-secondary);
  cursor: pointer;
}
.ver-panel__list {
  flex: 1;
  overflow-y: auto;
  padding: 6px 0;
}
.ver-panel__empty {
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  text-align: center;
  padding: 16px;
}
.ver-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 12px;
  border-left: 3px solid transparent;
}
.ver-row.current {
  border-left-color: var(--color-accent);
  background: var(--color-bg-elevated);
}
.ver-row__main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.ver-row__label {
  font-size: var(--font-size-sm);
  font-weight: 500;
}
.ver-row__meta {
  font-size: 10px;
  color: var(--color-text-secondary);
}
.ver-badge {
  align-self: flex-start;
  font-size: 9px;
  background: var(--color-accent);
  color: var(--color-text-inverse);
  border-radius: 3px;
  padding: 1px 5px;
}
.ver-row__actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 0 0 auto;
}
.ver-row__actions button {
  background: transparent;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  font-size: 11px;
  padding: 3px 6px;
  display: inline-flex;
  align-items: center;
}
.ver-row__actions button:hover {
  /* 原 --color-bg-base 幽灵 token 无 fallback 渲染透明;hover 态本有专用 token(S5 修) */
  background: var(--color-bg-hover);
  color: var(--color-text-primary);
}
.ver-row__del:hover {
  /* 原 var(--color-danger, #e5484d) 引用不存在的幽灵 token,一直走 fallback(S5 修) */
  color: var(--color-error);
}

.ver-diff {
  flex: 0 0 45%;
  display: flex;
  flex-direction: column;
  border-top: 1px solid var(--color-border);
  min-height: 0;
}
.ver-diff__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
}
.ver-diff__body {
  flex: 1;
  overflow: auto;
  font-family: var(--font-mono);
  font-size: 11px;
}
.ver-diff__line {
  display: flex;
  gap: 6px;
  padding: 0 8px;
  white-space: pre-wrap;
  word-break: break-word;
}
.ver-diff__sign {
  flex: 0 0 12px;
  color: var(--color-text-secondary);
}
.op-insert {
  background: color-mix(in srgb, var(--color-success) 18%, transparent);
}
.op-delete {
  background: color-mix(in srgb, var(--color-error) 18%, transparent);
}
.op-insert .ver-diff__sign {
  color: var(--color-success);
}
.op-delete .ver-diff__sign {
  color: var(--color-error);
}
</style>
