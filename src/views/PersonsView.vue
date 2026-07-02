<template>
  <!-- 人物墙（F6）：聚类出的人物簇卡片。点卡片进入该人物的照片；可命名/合并/隐藏。 -->
  <div class="persons-view">
    <div class="persons-header">
      <div class="persons-header__text">
        <h2 class="persons-title">{{ t('sidebar.persons') }}</h2>
        <p class="persons-subtitle">{{ t('persons.subtitle') }}</p>
      </div>
      <div class="persons-header__actions">
        <!-- 审批入口（T10）：有 likely-match 分组待审批时出现，打开批量审批面板。 -->
        <button
          v-if="store.likelyMatches.length > 0"
          class="btn btn-primary persons-approve"
          :title="t('persons.approveTitle')"
          @click="showApproval = true"
        >
          <UserCheck :size="14" />
          {{ t('persons.approveSuggestions', { count: store.likelyMatches.length }) }}
        </button>
        <button
          v-if="visiblePersons.length > 0"
          class="btn btn-secondary persons-recluster"
          :disabled="reclustering"
          :title="t('persons.reclusterTitle')"
          @click="onRecluster"
        >
          <RefreshCw :size="14" :class="{ spin: reclustering }" />
          {{ reclustering ? t('persons.reclustering') : t('persons.recluster') }}
        </button>
      </div>
    </div>

    <!-- 合并操作条：选中 ≥2 时出现 -->
    <div v-if="selectedIds.size >= 2" class="merge-bar">
      <span>{{ t('persons.selectedCount', { count: selectedIds.size }) }}</span>
      <button class="btn btn-primary" @click="mergeSelected">{{ t('persons.mergeIntoOne') }}</button>
      <button class="btn btn-secondary" @click="clearSelection">{{ t('common.cancel') }}</button>
    </div>

    <div v-if="!store.isLoading && visiblePersons.length === 0" class="persons-empty">
      {{ t('persons.empty') }}
    </div>

    <div class="persons-grid">
      <div
        v-for="p in visiblePersons"
        :key="p.id"
        class="person-card"
        :class="{ 'person-card--selected': selectedIds.has(p.id) }"
        @click="onCardClick(p)"
      >
        <!-- 选择复选框（hover 或已选时显示） -->
        <span
          class="person-card__check"
          :class="{ 'person-card__check--on': selectedIds.has(p.id) }"
          :title="t('persons.selectToMerge')"
          @click.stop="toggleSelect(p.id)"
        >
          <Check v-if="selectedIds.has(p.id)" :size="13" />
        </span>

        <!-- 隐藏按钮 -->
        <span
          class="person-card__hide"
          :title="t('persons.hidePerson')"
          @click.stop="store.setHidden(p.id, true)"
        >
          <EyeOff :size="13" />
        </span>

        <!-- 封面脸：整图缩略图 cover + 脸中心定位（v1 近似，未做精确 bbox 放大裁剪） -->
        <div class="person-card__avatar" :style="avatarStyle(p)">
          <ScanFace v-if="!coverSrc(p)" :size="32" class="person-card__avatar-fallback" />
        </div>

        <!-- 名字：双击编辑 -->
        <div class="person-card__name" @dblclick.stop="startEdit(p)">
          <input
            v-if="editingId === p.id"
            ref="nameInput"
            v-model="editName"
            class="person-card__input"
            :placeholder="t('persons.namePlaceholder')"
            maxlength="40"
            @keydown.enter="submitEdit(p)"
            @keydown.esc="cancelEdit"
            @blur="submitEdit(p)"
            @click.stop
          />
          <span v-else :class="{ 'person-card__name--unnamed': !p.isNamed }">
            {{ p.name || t('persons.unnamed') }}
          </span>
        </div>
        <div class="person-card__count">{{ t('persons.faceCount', { count: p.faceCount }) }}</div>
      </div>
    </div>

    <!-- 批量审批面板（T10）：模态覆盖层，按需打开。 -->
    <FaceApprovalPanel v-if="showApproval" @close="onApprovalClose" />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { convertFileSrc } from '@tauri-apps/api/core'
import { ScanFace, EyeOff, Check, RefreshCw, UserCheck } from '@lucide/vue'
import { useI18n } from 'vue-i18n'
import { usePersonStore } from '../stores/personStore'
import { useUiStore } from '../stores/uiStore'
import { useConfirm } from '../composables/useConfirm'
import FaceApprovalPanel from '../components/media/FaceApprovalPanel.vue'
import type { PersonSummary } from '../types/person'

const { t } = useI18n()
const store = usePersonStore()
const ui = useUiStore()
const router = useRouter()
const { confirm } = useConfirm()

// 隐藏的人物不上墙（隐藏=误检/不想看；F6 暂不提供"显示已隐藏"开关，留待后续）。
const visiblePersons = computed(() => store.persons.filter((p) => !p.isHidden))

store.load()
// 进页拉一次 likely-match 分组（限 50）以显示「审批建议 (N)」入口；失败非致命。
store.loadLikelyMatches(undefined, 50).catch(() => {})

// ── 批量审批面板（T10）─────────────────────────────────────────────────────────
const showApproval = ref(false)

function onApprovalClose() {
  showApproval.value = false
  // 审批改了脸归属/计数 → 重载人物墙；likelyMatches 已在 store 内乐观更新（入口计数随之刷新）。
  store.load()
}

// ── 重新聚类（全量复核）─────────────────────────────────────────────────────────
const reclustering = ref(false)

async function onRecluster() {
  const { confirmed } = await confirm({
    title: t('persons.recluster'),
    message: t('persons.reclusterConfirmMsg'),
    confirmText: t('persons.recluster'),
    cancelText: t('common.cancel'),
  })
  if (!confirmed) return
  reclustering.value = true
  try {
    await store.recluster()
    ui.addToast('success', t('persons.reclusterDone'))
  } catch (e) {
    // 后端在分析运行中会拒绝 → 提示先暂停。
    ui.addToast('error', String(e))
  } finally {
    reclustering.value = false
  }
}

// ── 封面脸缩略图 URL（仿 SemanticResultCard：status=1 相对、status=3 绝对）─────────
function coverSrc(p: PersonSummary): string | null {
  const path = p.coverThumbPath
  if (!path) return null
  try {
    if (p.coverThumbStatus === 1) {
      if (!store.cacheDir) return null
      return convertFileSrc(`${store.cacheDir}/thumbnails/${path}`.replace(/\\/g, '/'))
    }
    if (p.coverThumbStatus === 3) {
      return convertFileSrc(path.replace(/\\/g, '/'))
    }
    return null
  } catch {
    return null
  }
}

// 整图 cover + 把脸中心对齐到容器中心区域（background-position 百分比）。bbox=[x,y,w,h] 归一化。
function avatarStyle(p: PersonSummary): Record<string, string> {
  const src = coverSrc(p)
  if (!src) return {}
  const bb = p.coverBbox
  const cx = bb ? (bb[0] + bb[2] / 2) * 100 : 50
  const cy = bb ? (bb[1] + bb[3] / 2) * 100 : 50
  return {
    backgroundImage: `url("${src}")`,
    backgroundPosition: `${cx}% ${cy}%`,
  }
}

// ── 进入该人物的照片 ─────────────────────────────────────────────────────────
function onCardClick(p: PersonSummary) {
  if (editingId.value === p.id) return
  ui.setActivePerson(p.id)
  router.push('/')
}

// ── inline 命名 ───────────────────────────────────────────────────────────────
const editingId = ref<number | null>(null)
const editName = ref('')
const nameInput = ref<HTMLInputElement | HTMLInputElement[] | null>(null)
let submitted = false

function startEdit(p: PersonSummary) {
  editingId.value = p.id
  editName.value = p.name ?? ''
  submitted = false
  nextTick(() => {
    const el = Array.isArray(nameInput.value) ? nameInput.value[0] : nameInput.value
    el?.focus()
    el?.select()
  })
}

async function submitEdit(p: PersonSummary) {
  if (submitted) return
  submitted = true
  const name = editName.value
  editingId.value = null
  await store.rename(p.id, name)
}

function cancelEdit() {
  submitted = true
  editingId.value = null
}

// ── 多选合并 ─────────────────────────────────────────────────────────────────
const selectedIds = ref<Set<number>>(new Set())

function toggleSelect(id: number) {
  const s = new Set(selectedIds.value)
  if (s.has(id)) s.delete(id)
  else s.add(id)
  selectedIds.value = s
}

function clearSelection() {
  selectedIds.value = new Set()
}

async function mergeSelected() {
  const ids = [...selectedIds.value]
  if (ids.length < 2) return
  // 合并目标 = 选中里脸数最多的人物（保留它的身份/名字）；其余并入它。
  const chosen = ids
    .map((id) => store.persons.find((p) => p.id === id)!)
    .filter(Boolean)
    .sort((a, b) => b.faceCount - a.faceCount)
  const dst = chosen[0]
  const srcIds = chosen.slice(1).map((p) => p.id)
  const dstLabel = dst.name || t('persons.unnamedWithCount', { count: dst.faceCount })
  const { confirmed } = await confirm({
    title: t('persons.mergeTitle'),
    message: t('persons.mergeConfirmMsg', { count: ids.length, name: dstLabel }),
    confirmText: t('persons.merge'),
    cancelText: t('common.cancel'),
  })
  if (!confirmed) return
  await store.merge(srcIds, dst.id)
  clearSelection()
}
</script>

<style scoped>
.persons-view {
  flex: 1;
  overflow-y: auto;
  padding: var(--spacing-lg);
}
.persons-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--spacing-md);
  margin-bottom: var(--spacing-lg);
}
.persons-header__actions {
  display: flex;
  align-items: center;
  gap: var(--spacing-sm);
  flex-shrink: 0;
}
.persons-approve,
.persons-recluster {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  white-space: nowrap;
}
.persons-recluster:disabled {
  opacity: 0.6;
  cursor: default;
}
.persons-recluster .spin {
  animation: persons-spin 1s linear infinite;
}
@keyframes persons-spin {
  to {
    transform: rotate(360deg);
  }
}
.persons-title {
  font-size: var(--font-size-xl);
  font-weight: 600;
  color: var(--color-text-primary);
  margin: 0 0 4px;
}
.persons-subtitle {
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  margin: 0;
}
.merge-bar {
  display: flex;
  align-items: center;
  gap: var(--spacing-md);
  padding: 10px var(--spacing-md);
  margin-bottom: var(--spacing-md);
  background: var(--color-bg-secondary);
  border: 1px solid var(--color-accent);
  border-radius: var(--radius-md);
  font-size: var(--font-size-sm);
  color: var(--color-text-primary);
}
.persons-empty {
  padding: var(--spacing-lg);
  color: var(--color-text-tertiary);
  font-size: var(--font-size-sm);
}
.persons-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: var(--spacing-md);
}
.person-card {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: var(--spacing-md);
  border-radius: var(--radius-lg);
  background: var(--color-bg-secondary);
  border: 1px solid var(--color-border);
  cursor: pointer;
  transition:
    background var(--transition-fast),
    border-color var(--transition-fast),
    transform var(--transition-fast);
}
.person-card:hover {
  background: var(--color-sidebar-hover-bg);
  border-color: var(--color-accent);
  transform: translateY(-2px);
}
.person-card--selected {
  border-color: var(--color-accent);
  background: var(--color-bg-hover);
}
.person-card__avatar {
  width: 96px;
  height: 96px;
  border-radius: 50%;
  background-color: var(--color-bg-primary);
  background-size: cover;
  background-repeat: no-repeat;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
.person-card__avatar-fallback {
  color: var(--color-text-tertiary);
}
.person-card__name {
  font-size: var(--font-size-sm);
  font-weight: 500;
  color: var(--color-text-primary);
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.person-card__name--unnamed {
  color: var(--color-text-tertiary);
  font-style: italic;
}
.person-card__count {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}
.person-card__input {
  width: 100%;
  padding: 4px 6px;
  border-radius: var(--radius-md);
  border: 1px solid var(--color-accent);
  background: var(--color-bg-primary);
  color: var(--color-text-primary);
  font-size: var(--font-size-sm);
  text-align: center;
  outline: none;
}
/* 复选框 + 隐藏按钮：默认隐藏，hover 卡片时显现 */
.person-card__check,
.person-card__hide {
  position: absolute;
  top: 8px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: var(--radius-md);
  color: var(--color-text-tertiary);
  background: var(--color-bg-primary);
  opacity: 0;
  transition:
    opacity var(--transition-fast),
    color var(--transition-fast),
    background var(--transition-fast);
}
.person-card__check {
  left: 8px;
  border: 1px solid var(--color-border);
}
.person-card__hide {
  right: 8px;
}
.person-card:hover .person-card__check,
.person-card:hover .person-card__hide,
.person-card__check--on {
  opacity: 1;
}
.person-card__check--on {
  background: var(--color-accent);
  color: #fff;
  border-color: var(--color-accent);
}
.person-card__hide:hover {
  color: var(--color-error);
}
</style>
