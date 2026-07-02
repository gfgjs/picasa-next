<template>
  <!-- 收藏夹总览：4 个系统类型夹 + 用户自定义夹 + 新建（需求7, §3.7） -->
  <div class="collections-view">
    <div class="collections-header">
      <h2 class="collections-title">{{ t('sidebar.collections') }}</h2>
      <p class="collections-subtitle">{{ t('collections.subtitle') }}</p>
    </div>

    <div class="collections-grid">
      <!-- 收藏夹卡片（clickable div，与 PersonsView person-card 同款；用 div 而非 button 以便
           编辑时内嵌 input 重命名，避免 input-in-button 的焦点/事件问题）。 -->
      <div
        v-for="c in store.collections"
        :key="c.id"
        class="collection-card"
        :class="{ 'collection-card--system': c.kind === 'system' }"
        @click="onCardClick(c)"
      >
        <span class="collection-card__icon">
          <component :is="iconFor(c)" :size="28" />
        </span>

        <!-- 名字：用户夹可双击或点铅笔重命名（系统夹只读） -->
        <input
          v-if="editingId === c.id"
          ref="renameInput"
          v-model="editName"
          class="collection-card__input"
          :placeholder="t('collections.namePlaceholder')"
          maxlength="40"
          @keydown.enter="submitRename(c)"
          @keydown.esc="cancelRename"
          @blur="submitRename(c)"
          @click.stop
        />
        <span
          v-else
          class="collection-card__name"
          @dblclick.stop="startRename(c)"
        >
          {{ c.name }}
        </span>

        <span class="collection-card__count">{{ t('settings.volItems', { n: c.itemCount }) }}</span>

        <!-- 用户夹：重命名（左上）+ 删除（右上）；系统夹受保护无此二者 -->
        <template v-if="c.kind === 'user'">
          <span class="collection-card__edit" :title="t('settings.volRename')" @click.stop="startRename(c)">
            <Pencil :size="13" />
          </span>
          <span class="collection-card__del" :title="t('collections.deleteCollection')" @click.stop="onDelete(c)">
            <Trash2 :size="14" />
          </span>
        </template>
      </div>

      <!-- 新建收藏夹卡片 -->
      <div v-if="!creating" class="collection-card collection-card--new" @click="startCreate">
        <span class="collection-card__icon"><Plus :size="28" /></span>
        <span class="collection-card__name">{{ t('collections.newCollection') }}</span>
      </div>
      <div v-else class="collection-card collection-card--new collection-card--editing">
        <input
          ref="nameInput"
          v-model="newName"
          class="collection-card__input"
          :placeholder="t('collections.namePlaceholder')"
          maxlength="40"
          @keydown.enter="submitCreate"
          @keydown.esc="cancelCreate"
          @blur="submitCreate"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue'
import type { Component } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { ImageIcon, Video, Music, FileText, FolderHeart, Plus, Trash2, Pencil } from '@lucide/vue'
import { useCollectionStore } from '../stores/collectionStore'
import { useUiStore } from '../stores/uiStore'
import { useConfirm } from '../composables/useConfirm'
import type { Collection } from '../types/media'

const store = useCollectionStore()
const ui = useUiStore()
const router = useRouter()
const { confirm } = useConfirm()
const { t } = useI18n()

// 系统夹按类型映射图标；用户夹统一用「收藏文件夹」图标。
const SYS_ICON: Record<string, Component> = {
  image: ImageIcon,
  video: Video,
  audio: Music,
  document: FileText,
}
function iconFor(c: Collection) {
  if (c.kind === 'system' && c.mediaTypeFilter) return SYS_ICON[c.mediaTypeFilter] ?? FolderHeart
  return FolderHeart
}

function onCardClick(c: Collection) {
  if (editingId.value === c.id) return // 编辑中不导航
  openCollection(c)
}

function openCollection(c: Collection) {
  ui.setActiveCollection(c)
  router.push('/')
}

// ── 重命名（用户夹）─────────────────────────────────────────────────────────
const editingId = ref<number | null>(null)
const editName = ref('')
const renameInput = ref<HTMLInputElement | HTMLInputElement[] | null>(null)
// 防 blur 与 enter 重复提交（同 create 流程）。
let renameSubmitted = false

function startRename(c: Collection) {
  if (c.kind !== 'user') return // 系统夹只读
  editingId.value = c.id
  editName.value = c.name
  renameSubmitted = false
  nextTick(() => {
    const el = Array.isArray(renameInput.value) ? renameInput.value[0] : renameInput.value
    el?.focus()
    el?.select()
  })
}

async function submitRename(c: Collection) {
  if (renameSubmitted) return
  renameSubmitted = true
  const name = editName.value.trim()
  editingId.value = null
  if (name && name !== c.name) await store.rename(c.id, name)
}

function cancelRename() {
  renameSubmitted = true
  editingId.value = null
}

// ── 新建 ───────────────────────────────────────────────────────────────────
const creating = ref(false)
const newName = ref('')
const nameInput = ref<HTMLInputElement | null>(null)
// 防止 blur 与 enter 同时触发导致重复提交。
let submitted = false

function startCreate() {
  creating.value = true
  newName.value = ''
  submitted = false
  nextTick(() => nameInput.value?.focus())
}

async function submitCreate() {
  if (submitted) return
  submitted = true
  const name = newName.value.trim()
  creating.value = false
  if (name) await store.create(name)
}

function cancelCreate() {
  submitted = true
  creating.value = false
}

// ── 删除（用户夹） ──────────────────────────────────────────────────────────
async function onDelete(c: Collection) {
  const { confirmed } = await confirm({
    title: t('collections.deleteCollection'),
    message: t('collections.deleteConfirmMsg', { name: c.name }),
    confirmText: t('selection.delete'),
    cancelText: t('common.cancel'),
  })
  if (confirmed) await store.remove(c.id)
}

onMounted(() => {
  store.load()
})
</script>

<style scoped>
.collections-view {
  flex: 1;
  overflow-y: auto;
  padding: var(--spacing-lg);
}
.collections-header {
  margin-bottom: var(--spacing-lg);
}
.collections-title {
  font-size: var(--font-size-xl);
  font-weight: 600;
  color: var(--color-text-primary);
  margin: 0 0 4px;
}
.collections-subtitle {
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  margin: 0;
}
.collections-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: var(--spacing-md);
}
.collection-card {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  aspect-ratio: 4 / 3;
  padding: var(--spacing-md);
  border-radius: var(--radius-lg);
  background: var(--color-bg-secondary);
  border: 1px solid var(--color-border);
  color: var(--color-text-primary);
  cursor: pointer;
  text-align: center;
  transition:
    background var(--transition-fast),
    border-color var(--transition-fast),
    transform var(--transition-fast);
}
.collection-card:hover {
  background: var(--color-sidebar-hover-bg);
  border-color: var(--color-accent);
  transform: translateY(-2px);
}
.collection-card--system .collection-card__icon {
  color: var(--color-accent);
}
.collection-card__icon {
  display: inline-flex;
  color: var(--color-text-secondary);
}
.collection-card__name {
  font-size: var(--font-size-sm);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}
.collection-card__count {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}
.collection-card__del,
.collection-card__edit {
  position: absolute;
  top: 6px;
  display: inline-flex;
  padding: 4px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  opacity: 0;
  cursor: pointer;
  transition:
    opacity var(--transition-fast),
    color var(--transition-fast),
    background var(--transition-fast);
}
.collection-card__del {
  right: 6px;
}
.collection-card__edit {
  left: 6px;
}
.collection-card:hover .collection-card__del,
.collection-card:hover .collection-card__edit {
  opacity: 1;
}
.collection-card__del:hover {
  color: var(--color-error);
  background: var(--color-bg-primary);
}
.collection-card__edit:hover {
  color: var(--color-accent);
  background: var(--color-bg-primary);
}
.collection-card--new {
  border-style: dashed;
  color: var(--color-text-secondary);
}
.collection-card--editing {
  cursor: default;
}
.collection-card__input {
  width: 100%;
  padding: 6px 8px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-accent);
  background: var(--color-bg-primary);
  color: var(--color-text-primary);
  font-size: var(--font-size-sm);
  text-align: center;
  outline: none;
}
</style>
