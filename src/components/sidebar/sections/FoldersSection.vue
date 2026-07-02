<template>
  <AccordionSection id="folders" :order="order" :title="$t('sidebar.folders')">
    <!-- right-aligned header actions: show-all / import folder / new folder -->
    <!-- 右对齐的标题操作：全部 / 导入文件夹 / 新建文件夹 -->
    <template #actions>
      <button
        class="show-all-btn"
        :class="{ active: ui.activeSmartAlbum === 'all' && !ui.activeDirectoryId }"
        :title="$t('sidebar.showAllTitle')"
        @click="showAll"
      >
        {{ $t('sidebar.showAll') }}
      </button>
      <button
        class="btn-icon"
        :title="t('sidebar.newBlankFolder')"
        :aria-label="t('sidebar.newBlankFolder')"
        @click="createNewGlobalFolder"
      >
        <FolderPlus :size="16" />
      </button>
      <button
        class="btn-icon"
        :title="$t('sidebar.addFolder')"
        :aria-label="$t('sidebar.addFolder')"
        @click="addRoot"
      >
        <FolderSearch :size="16" />
      </button>
    </template>

    <!-- empty state | 空态 -->
    <div v-if="folderTree.nodes.value.length === 0 && !scan.hasScanRoots" class="empty">
      {{ $t('sidebar.noFolders') }}
    </div>

    <!-- folder tree | 文件夹树
         Rows are flattened by `displayRows`: each expanded directory is followed by
         its own files (lazy-loaded). Directory rows are `position: sticky` so the
         folder name stays pinned while scrolling through its (possibly long) file
         list — see the sticky note in <style>. | 行由 `displayRows` 拍平：每个展开
         的目录后紧跟其自身文件（懒加载）。目录行为 `position: sticky`，使滚动其
         （可能很长的）文件列表时文件夹名始终钉住——见 <style> 中的粘性说明。 -->
    <div
      v-if="folderTree.nodes.value.length > 0"
      class="tree"
      :style="{ '--tree-dir-sticky-top': stickyTop }"
    >
      <template
        v-for="row in displayRows"
        :key="row.kind === 'dir' ? 'd' + row.node.id : 'f' + row.file.id"
      >
        <!-- Directory row — sticky header that pins its files below it. -->
        <!-- 目录行——粘性标题，将其文件钉在下方。 -->
        <button
          v-if="row.kind === 'dir'"
          class="tree-item"
          :data-dir-id="row.node.id"
          :class="{
            active:
              ui.groupBy === 'folder'
                ? ui.scrolledDirectoryId === row.node.id
                : ui.activeDirectoryId === row.node.id,
            'drag-over': dropId === row.node.id || ui.mediaDragHoverDirId === row.node.id,
            'drag-source': dragId === row.node.id,
          }"
          :style="{ paddingLeft: row.node.depth * 16 + 8 + 'px' }"
          @click="onNodeClick(row.node)"
          @contextmenu.prevent="onNodeContextMenu($event, row.node)"
          @pointerdown="onTreePointerDown(row.node, $event)"
        >
          <span class="tree-arrow" @click.stop="folderTree.toggleNode(row.node)">
            <!-- Expandable if it has subfolders OR direct files. | 有子文件夹或直接文件即可展开。 -->
            <ChevronRight
              v-if="row.node.hasChildren || row.node.mediaCount > 0"
              :size="14"
              class="tree-chevron"
              :class="{ expanded: row.node.expanded }"
            />
            <span v-else class="tree-chevron-spacer" />
          </span>
          <span class="tree-icon"><Folder :size="15" /></span>
          <span class="tree-label" :title="row.node.relPath">{{ row.node.name }}</span>
          <span class="tree-count">{{ row.node.mediaCount }}</span>
        </button>

        <!-- File row — a leaf that scrolls under the pinned folder header. -->
        <!-- 文件行——叶子节点，从钉住的文件夹标题下方滚过。 -->
        <button
          v-else
          class="file-item"
          :data-file-id="row.file.id"
          :style="{ paddingLeft: row.depth * 16 + 8 + 'px' }"
          :title="row.file.fileName"
          @click="onFileClick(row.file)"
        >
          <span class="file-icon"><component :is="fileIcon(row.file.mediaType)" :size="14" /></span>
          <span class="file-label">{{ row.file.fileName }}</span>
          <Heart v-if="row.file.isFavorited" :size="11" class="file-fav" />
        </button>
      </template>
    </div>
  </AccordionSection>

  <!-- Context menu for a tree node (self-teleports to body). -->
  <!-- 树节点的右键菜单（自身 teleport 到 body）。 -->
  <ContextMenu
    :items="contextMenu.items"
    :visible="contextMenu.visible"
    :x="contextMenu.x"
    :y="contextMenu.y"
    @update:visible="contextMenu.visible = $event"
  />

  <!-- New-folder dialog | 新建文件夹对话框 -->
  <Teleport to="body">
    <FolderCreateDialog
      v-if="createDialog.isOpen"
      :base-path="createDialog.basePath"
      @close="createDialog.isOpen = false"
      @created="onFolderCreated"
    />
  </Teleport>

  <!-- Floating drag preview for move/copy — pointer-events:none so it never
       blocks elementFromPoint. | 移动/复制的浮动拖拽预览——pointer-events:none，
       绝不挡住 elementFromPoint。 -->
  <Teleport to="body">
    <div
      v-if="ghost.visible"
      class="drag-ghost"
      :style="{ left: ghost.x + 12 + 'px', top: ghost.y + 8 + 'px' }"
    >
      <Folder :size="13" />
      <span class="drag-ghost__name">{{ ghost.label }}</span>
      <span class="drag-ghost__mode">{{ ghost.copy ? t('common.copy') : t('common.move') }}</span>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, markRaw } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { open } from '@tauri-apps/plugin-dialog'
import { invokeIpc } from '../../../utils/ipc'
import { IPC } from '../../../constants/ipc'
import {
  ChevronRight,
  Folder,
  FolderSearch,
  FolderPlus,
  Image,
  Film,
  Music,
  FileText,
  Heart,
} from '@lucide/vue'
import AccordionSection from '../AccordionSection.vue'
import ContextMenu, { type ContextMenuItem } from '../../common/ContextMenu.vue'
import FolderCreateDialog from '../../common/FolderCreateDialog.vue'
import { useUiStore } from '../../../stores/uiStore'
import { useScanStore } from '../../../stores/scanStore'
import { useMediaStore } from '../../../stores/mediaStore'
import { useHistoryStore } from '../../../stores/historyStore'
import { useFolderTree } from '../../../composables/useFolderTree'
import { useSidebarSections } from '../../../composables/useSidebarSections'
import { beginPointerDrag, DRAG_THRESHOLD } from '../../../composables/usePointerDrag'
import { useConfirm } from '../../../composables/useConfirm'
import type { DirNode, DirFile, MediaType } from '../../../types/media'

defineProps<{ order: number }>()

const ui = useUiStore()
const scan = useScanStore()
const media = useMediaStore()
const history = useHistoryStore()
const folderTree = useFolderTree()
const sections = useSidebarSections()
const { confirm } = useConfirm()
const router = useRouter()
const route = useRoute()
const { t } = useI18n()

// ── Flattened display rows: dirs interleaved with their (lazy-loaded) files ──────
// Folders-first (VSCode-style): within a directory, subfolders (and their expanded
// subtrees) come first, then the directory's OWN files. `folderTree.nodes` is already
// a pre-order DFS of dirs; we emit a dir's files only once its whole subtree has been
// walked — i.e. when we hit the next node at the same-or-shallower depth — so a
// folder's structure stays navigable even when it holds many loose files.
// 拍平的显示行：文件夹优先（VSCode 风格）——某目录内先列子文件夹（及其已展开子树），
// 再列该目录「自身」的文件。`folderTree.nodes` 已是目录的前序 DFS；我们仅在某目录整个
// 子树遍历完毕后（即遇到同级或更浅深度的下一个节点）才发射其文件，使文件夹结构在含大量
// 散落文件时仍可导航。
interface DirRow {
  kind: 'dir'
  node: DirNode
}
interface FileRow {
  kind: 'file'
  file: DirFile
  depth: number
  dirId: number
}
type TreeRow = DirRow | FileRow

const displayRows = computed<TreeRow[]>(() => {
  const out: TreeRow[] = []
  // Stack of expanded dirs whose own files are queued to emit after their subtree.
  // 待发射文件的展开目录栈：其文件在自身子树之后发射。
  const pending: DirNode[] = []
  const flush = (depth: number) => {
    while (pending.length && pending[pending.length - 1].depth >= depth) {
      const dir = pending.pop()!
      for (const f of dir.files!) {
        out.push({ kind: 'file', file: f, depth: dir.depth + 1, dirId: dir.id })
      }
    }
  }
  for (const node of folderTree.nodes.value) {
    flush(node.depth) // close subtrees that ended before this node | 冲刷在此节点前结束的子树
    out.push({ kind: 'dir', node })
    if (node.expanded && node.files && node.files.length) pending.push(node)
  }
  flush(-1) // flush all remaining (depth < any real depth) | 冲刷剩余全部
  return out
})

// Sticky `top` for directory rows: pin them right BELOW the stack of pinned section
// headers. The FOLDERS section header itself pins at `H * foldersIndex`, occupying
// one header height, so directory rows start at `H * (foldersIndex + 1)`. Derived
// from the live visible-section order so it stays correct if sections are hidden.
// 目录行的粘性 `top`：把它们钉在已固定的区块标题栈「正下方」。FOLDERS 区块标题本身
// 钉在 `H * foldersIndex`，占一个标题高度，故目录行从 `H * (foldersIndex + 1)` 起。
// 由实时可见区块顺序推导，使区块被隐藏时仍正确。
const stickyTop = computed(() => {
  const i = sections.visibleIds.value.indexOf('folders')
  return `calc(var(--sidebar-header-h) * ${(i < 0 ? 0 : i) + 1})`
})

// Type icon for a file leaf. | 文件叶子的类型图标。
function fileIcon(type: MediaType) {
  switch (type) {
    case 'video':
      return Film
    case 'audio':
      return Music
    case 'document':
      return FileText
    default:
      return Image
  }
}

// Open a file from the tree — same routing as a gallery card click (handleCardClick):
// documents/audio get dedicated routes, everything else opens the detail overlay.
// 从树中打开文件——与画廊卡片单击（handleCardClick）一致：文档/音频走专用路由，
// 其余打开详情覆盖层。
function onFileClick(file: DirFile) {
  if (file.mediaType === 'document') {
    router.push(`/doc/${file.id}`)
    return
  }
  if (file.mediaType === 'audio') {
    router.push(`/audio/${file.id}`)
    return
  }
  media.openDetail(file.id, true)
}

// ── Folder tree: single source of (re)loads ─────────────────────────────────
// This watch is the ONLY place the tree loads from scan roots. It fires on init
// (immediate) and whenever scan.scanRoots is reassigned (add / remove / clear),
// so add/remove/empty are all handled in one place. Count refreshes after a scan
// come through the `folder-stats-changed` event below instead.
// ── 文件夹树：唯一的加载来源 ─────────────────────────────────────────────────
// 该 watch 是树从扫描根目录加载的唯一入口。它在初始化（immediate）以及每次
// scan.scanRoots 被重新赋值（添加/移除/清空）时触发，使增删空都集中处理。扫描后的
// 计数刷新改由下方的 `folder-stats-changed` 事件驱动。
let pendingSelectRootId: number | null = null
watch(
  () => scan.scanRoots,
  async (roots) => {
    await folderTree.loadRoots(roots)
    // Honour a selection requested by addRoot once the tree is actually loaded.
    // 树真正加载后，再执行 addRoot 请求的选中。
    if (pendingSelectRootId != null) {
      const node = folderTree.nodes.value.find(
        (n) => n.rootId === pendingSelectRootId && n.parentId === null,
      )
      if (node) {
        ui.setActiveDirectory(node.id)
        if (route.path !== '/') router.push('/')
      }
      pendingSelectRootId = null
    }
  },
  { immediate: true },
)

// Keep the tree in sync with the active / scrolled directory (expand + scroll to).
// 让树与当前选中 / 滚动到的目录保持同步（展开并滚动到）。
watch(
  () => ui.activeDirectoryId,
  (id) => {
    if (id !== null) folderTree.expandToNode(id)
  },
)
watch(
  () => ui.scrolledDirectoryId,
  (id) => {
    if (ui.groupBy === 'folder' && id !== null) folderTree.expandToNode(id)
  },
)

// Reload preserving expansion (used after a scan refreshes media counts, and to
// auto-select a folder after a move). | 保留展开态重载（扫描刷新计数后、移动后自动选中时使用）。
async function reloadTreePreserveExpansion(selectDirId?: number | null) {
  const expandedIds = folderTree.nodes.value.filter((n) => n.expanded).map((n) => n.id)
  await folderTree.loadRoots(scan.scanRoots)
  for (const id of expandedIds) {
    const node = folderTree.nodes.value.find((n) => n.id === id)
    if (node && !node.expanded) await folderTree.loadChildren(id)
  }
  if (selectDirId != null) {
    await folderTree.expandToNode(selectDirId)
    ui.setActiveDirectory(selectDirId)
  }
}

function onFolderStatsChanged(e: Event) {
  const selectDirId = (e as CustomEvent).detail?.selectDirId ?? null
  reloadTreePreserveExpansion(selectDirId)
}
onMounted(() => window.addEventListener('folder-stats-changed', onFolderStatsChanged))
onBeforeUnmount(() => window.removeEventListener('folder-stats-changed', onFolderStatsChanged))

// ── Tree node click / selection ─────────────────────────────────────────────
// ── 树节点点击 / 选择 ─────────────────────────────────────────────────────────
let suppressClick = false // set when a press became a drag, so the trailing click is ignored
// 当按下变成拖拽时置位，以忽略尾随的 click
function onNodeClick(node: DirNode) {
  if (suppressClick) {
    suppressClick = false
    return
  }
  if (ui.groupBy === 'folder') {
    ui.pendingScrollDirId = node.id
    if (ui.activeSmartAlbum !== 'all' || ui.activeDirectoryId !== null) {
      ui.setSmartAlbum('all')
      ui.setActiveDirectory(null)
    }
  } else {
    ui.setActiveDirectory(node.id)
  }
  if (route.path !== '/') router.push('/')
}

function showAll() {
  ui.setSmartAlbum('all')
  ui.setActiveDirectory(null)
}

// ── Tree drag move / copy (pointer-based) ───────────────────────────────────
// ── 树拖拽移动 / 复制（基于指针） ────────────────────────────────────────────
const dragId = ref<number | null>(null)
const dropId = ref<number | null>(null)
const ghost = ref<{ visible: boolean; x: number; y: number; label: string; copy: boolean }>({
  visible: false,
  x: 0,
  y: 0,
  label: '',
  copy: false,
})

/** Is `nodeId` inside the subtree rooted at `ancestorId`? | `nodeId` 是否在 `ancestorId` 子树内？ */
function isDescendant(ancestorId: number, nodeId: number): boolean {
  let cur = folderTree.nodes.value.find((n) => n.id === nodeId)
  while (cur && cur.parentId != null) {
    if (cur.parentId === ancestorId) return true
    cur = folderTree.nodes.value.find((n) => n.id === cur!.parentId)
  }
  return false
}

/** Whether dragged dir `srcId` may drop onto dir `targetId`. | 被拖目录 `srcId` 能否落到 `targetId`。 */
function canDropOnId(srcId: number, targetId: number): boolean {
  if (targetId === srcId) return false
  const src = folderTree.nodes.value.find((n) => n.id === srcId)
  if (!src) return false
  if (src.parentId === targetId) return false // already there | 已在目标中
  if (isDescendant(srcId, targetId)) return false // would create a cycle | 会成环
  return true
}

function onTreePointerDown(node: DirNode, e: PointerEvent) {
  if (e.button !== 0 || node.parentId === null) return // left button only; scan roots aren't movable
  suppressClick = false
  const startX = e.clientX,
    startY = e.clientY
  let dragging = false

  beginPointerDrag(
    (ev) => {
      if (!dragging) {
        if (Math.abs(ev.clientX - startX) + Math.abs(ev.clientY - startY) < DRAG_THRESHOLD) return
        dragging = true
        suppressClick = true
        dragId.value = node.id
        document.body.style.userSelect = 'none'
        document.body.style.cursor = 'grabbing'
        ghost.value = {
          visible: true,
          x: ev.clientX,
          y: ev.clientY,
          label: node.name,
          copy: ev.ctrlKey || ev.metaKey,
        }
      }
      ghost.value.x = ev.clientX
      ghost.value.y = ev.clientY
      ghost.value.copy = ev.ctrlKey || ev.metaKey
      const item = (
        document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null
      )?.closest('[data-dir-id]') as HTMLElement | null
      const targetId = item ? Number(item.dataset.dirId) : null
      dropId.value = targetId != null && canDropOnId(node.id, targetId) ? targetId : null
    },
    (ev, cancelled) => {
      const srcId = dragId.value,
        targetId = dropId.value
      const copy = ev.ctrlKey || ev.metaKey
      dragId.value = null
      dropId.value = null
      ghost.value.visible = false
      if (!cancelled && dragging && srcId != null && targetId != null) {
        performTreeDrop(srcId, targetId, copy)
      }
    },
  )
}

async function performTreeDrop(srcId: number, targetId: number, copy: boolean) {
  const src = folderTree.nodes.value.find((n) => n.id === srcId)
  const target = folderTree.nodes.value.find((n) => n.id === targetId)
  if (!src || !target || src.parentId == null || !canDropOnId(srcId, targetId)) return
  try {
    if (copy) {
      await history.copy(src.id, src.name, target.id)
      ui.addToast('success', t('sidebar.copiedTo', { name: src.name, target: target.name }))
    } else {
      await history.move(src.id, src.name, src.parentId, target.id)
      ui.addToast('success', t('sidebar.movedTo', { name: src.name, target: target.name }))
    }
  } catch (err) {
    const e = err as { code?: string; message?: string } | null | undefined
    if (e && e.code === 'DirectoryExists') {
      ui.addToast('error', t('sidebar.dirExistsNoMerge', { name: e.message }))
    } else {
      ui.addToast('error', t('sidebar.opFailed', { error: e?.message ?? err }))
    }
  }
}

// ── Context menu + folder creation ──────────────────────────────────────────
// ── 右键菜单 + 文件夹创建 ────────────────────────────────────────────────────
const contextMenu = ref({ visible: false, x: 0, y: 0, items: [] as ContextMenuItem[] })
const createDialog = ref({ isOpen: false, basePath: '' })

function onNodeContextMenu(event: MouseEvent, node: DirNode) {
  contextMenu.value.items = [
    {
      id: 'new_subfolder',
      label: t('sidebar.newSubfolder'),
      icon: markRaw(FolderPlus),
      action: () => {
        createDialog.value.basePath = node.absPath || ''
        createDialog.value.isOpen = true
      },
    },
  ]
  contextMenu.value.x = event.clientX
  contextMenu.value.y = event.clientY
  contextMenu.value.visible = true
}

function createNewGlobalFolder() {
  createDialog.value.basePath = ''
  createDialog.value.isOpen = true
}

function onFolderCreated() {
  // A new (sub)folder may not change scan.scanRoots, so reload the tree directly.
  // 新建（子）文件夹可能不改变 scan.scanRoots，因此直接重载树。
  reloadTreePreserveExpansion()
}

// ── Import an existing folder as a scan root ────────────────────────────────
// ── 导入一个已有文件夹作为扫描根目录 ─────────────────────────────────────────
interface OverlapInfo {
  id: number
  path: string
  alias: string | null
}
interface FolderOverlapResult {
  children: OverlapInfo[]
  parents: OverlapInfo[]
}

async function addRoot() {
  try {
    const selected = await open({ directory: true, multiple: false, title: t('sidebar.chooseDir') })
    if (!selected) return
    const path = typeof selected === 'string' ? selected : selected[0]
    if (!path) return

    // Step 1: check for overlaps with existing roots. | 第一步：检查与现有根目录是否重叠。
    const overlap = await invokeIpc<FolderOverlapResult>(IPC.CHECK_FOLDER_OVERLAP, { newPath: path })
    if (overlap.children.length > 0) {
      const childNames = overlap.children.map((c) => c.alias || c.path).join(', ')
      const { confirmed: merge } = await confirm({
        title: t('sidebar.overlapDetected'),
        message: t('sidebar.overlapParentMsg', { path, children: childNames }),
        confirmText: t('sidebar.mergeAndReplace'),
        cancelText: t('sidebar.addAnyway'),
      })
      if (merge) {
        for (const child of overlap.children) {
          await invokeIpc(IPC.REMOVE_SCAN_ROOT_WITH_OPTIONS, { id: child.id, clearThumbnails: false })
        }
      }
    } else if (overlap.parents.length > 0) {
      const parentNames = overlap.parents.map((p) => p.alias || p.path).join(', ')
      const { confirmed: proceed } = await confirm({
        title: t('sidebar.overlapDetected'),
        message: t('sidebar.overlapChildMsg', { path, parents: parentNames }),
        confirmText: t('sidebar.addAnyway'),
        cancelText: t('common.cancel'),
      })
      if (!proceed) return
    }

    // Step 2: add the root, then let the scanRoots watch load the tree and select
    // the new root node once it's loaded. | 第二步：添加根目录，随后由 scanRoots 的
    // watch 加载树，并在加载完成后选中新根节点。
    try {
      const root = await scan.addScanRoot(path)
      pendingSelectRootId = root.id
      await scan.loadScanRoots() // reassigns scanRoots → triggers the watch above | 重建 scanRoots → 触发上方 watch
      await scan.startScan(root.id, () => {
        media.loadStats()
        window.dispatchEvent(new CustomEvent('folder-stats-changed'))
      })
    } catch (e) {
      ui.addToast('error', t('sidebar.addFolderFailed') + ' ' + e)
    }
  } catch (e) {
    ui.addToast('error', t('sidebar.chooseDirFailed') + ' ' + e)
  }
}
</script>

<style scoped>
/* ── Header "show all" pill ────────────────────────────────────────────────── */
.show-all-btn {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-tertiary);
  border: 1px solid var(--color-border);
  cursor: pointer;
  transition: all var(--transition-fast);
}
.show-all-btn:hover {
  background: var(--color-sidebar-hover-bg, var(--color-bg-hover));
  color: var(--color-text-primary);
}
.show-all-btn.active {
  background: var(--color-accent);
  color: #fff;
  border-color: var(--color-accent);
}

/* ── Empty state ───────────────────────────────────────────────────────────── */
.empty {
  padding: var(--spacing-md);
  font-size: var(--font-size-sm);
  color: var(--color-text-tertiary);
  text-align: center;
}

/* ── Folder tree ───────────────────────────────────────────────────────────── */
.tree {
  padding: 0 var(--spacing-xs);
}
.tree-item {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  height: 28px;
  border-radius: var(--radius-sm);
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
  cursor: pointer;
  transition: background-color var(--transition-fast);
  overflow: hidden;
  /* ── Sticky folder header ───────────────────────────────────────────────────
     Every directory row pins at `--tree-dir-sticky-top` (just below the section
     headers). As you scroll a folder's file list, the folder name stays pinned;
     when the next folder reaches the same top it paints over the previous one
     (later in DOM ⇒ on top), so the pinned name always reflects the folder you're
     currently inside — including nested folders, with no stale-header artifacts
     (all rows share ONE top, so nothing lingers at a deeper offset).
     每个目录行都钉在 `--tree-dir-sticky-top`（区块标题正下方）。滚动某文件夹的文件
     列表时其名称保持钉住；下一个文件夹到达同一 top 时覆盖前者（DOM 中更靠后⇒在上），
     故钉住的名称始终反映当前所在文件夹——含嵌套文件夹，且无残留标题（所有行共用一个
     top，不会有东西滞留在更深的偏移处）。 */
  position: sticky;
  top: var(--tree-dir-sticky-top, calc(var(--sidebar-header-h) * 3));
  z-index: 2;
  /* Opaque base so a PINNED header hides files scrolling underneath. The state
     tints below are translucent tokens, so we layer them OVER this base via
     background-image (using them as `background` alone would let content bleed
     through a pinned header — the same lesson as the section headers).
     不透明底色，使「钉住」的标题遮住下方滚动的文件。下方状态着色是半透明 token，
     故经 background-image 叠加在此底色之上（仅作 `background` 会让内容从钉住标题
     下透出——与区块标题同一教训）。 */
  background-color: var(--color-bg-secondary);
}
.tree-item:hover {
  background-image: linear-gradient(var(--color-sidebar-hover-bg), var(--color-sidebar-hover-bg));
  color: var(--color-text-primary);
}
.tree-item.active {
  background-image: linear-gradient(var(--color-sidebar-active-bg), var(--color-sidebar-active-bg));
  color: var(--color-sidebar-active-text);
}
.tree-item.drag-over {
  background-image: linear-gradient(var(--color-sidebar-active-bg), var(--color-sidebar-active-bg));
  box-shadow: inset 0 0 0 1px var(--color-accent);
}
.tree-item.drag-source {
  opacity: 0.45;
}

/* ── File leaf row ─────────────────────────────────────────────────────────────
   Not sticky — scrolls under the pinned folder header. Slightly smaller/dimmer
   than folder rows. | 文件叶子行——非粘性，从钉住的文件夹标题下方滚过。比目录行
   略小、略暗。 */
.file-item {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  height: 26px;
  border-radius: var(--radius-sm);
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: background-color var(--transition-fast);
  overflow: hidden;
}
.file-item:hover {
  background: var(--color-sidebar-hover-bg);
  color: var(--color-text-primary);
}
.file-icon {
  width: 18px;
  display: inline-flex;
  justify-content: center;
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}
.file-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.file-fav {
  flex-shrink: 0;
  margin-right: 4px;
  color: var(--color-accent);
}

.tree-arrow {
  width: 16px;
  display: inline-flex;
  justify-content: center;
  color: var(--color-text-tertiary);
}
.tree-chevron {
  transition: transform var(--transition-fast);
  flex-shrink: 0;
}
.tree-chevron.expanded {
  transform: rotate(90deg);
}
.tree-chevron-spacer {
  width: 14px;
  flex-shrink: 0;
}
.tree-icon {
  width: 18px;
  display: inline-flex;
  justify-content: center;
}
.tree-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tree-count {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  margin-right: 4px;
  font-variant-numeric: tabular-nums;
}

/* ── Floating drag preview ─────────────────────────────────────────────────── */
.drag-ghost {
  position: fixed;
  z-index: 10001;
  pointer-events: none; /* critical: must not block elementFromPoint | 关键：不能挡住 elementFromPoint */
  display: flex;
  align-items: center;
  gap: 6px;
  max-width: 240px;
  padding: 4px 10px;
  font-size: 12px;
  color: var(--color-text-primary);
  background: var(--color-bg-elevated);
  border: 1px solid var(--color-border-strong, var(--color-border));
  border-radius: var(--radius-md);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
}
.drag-ghost__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.drag-ghost__mode {
  flex-shrink: 0;
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
  background: var(--color-accent);
  color: #fff;
}
</style>
