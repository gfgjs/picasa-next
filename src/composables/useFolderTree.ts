// src/composables/useFolderTree.ts
// Folder tree lazy loading
// 文件夹树懒加载

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { invokeIpc } from '../utils/ipc'
import type { DirNode, DirFile, ScanRoot } from '../types/media'
import { IPC } from '../constants/ipc'

export function useFolderTree() {
  const roots = ref<ScanRoot[]>([])
  const nodes = ref<DirNode[]>([])
  const loading = ref(false)
  let loadingId = 0 // guard against concurrent loadRoots calls
  // 防止并发 loadRoots 调用的守卫

  // Load all root subtrees and replace `nodes` ATOMICALLY. We accumulate into a local
  // array and assign once at the end (only if still the winning call), so the tree is
  // never left blank or half-built when two reloads race — e.g. removing a root reloads
  // scanRoots while a separate reloadTreePreserveExpansion is in flight (问题6).
  // 加载所有根子树并「原子」替换 `nodes`。先累积到局部数组，最后只在仍是胜出调用时一次性
  // 赋值，使两次重载竞争时树不会变空或半成品——例如移除根目录触发的 scanRoots 重载与另一个
  // reloadTreePreserveExpansion 同时在途（问题6）。
  async function loadRoots(scanRoots: ScanRoot[]) {
    const myId = ++loadingId // if another call starts, this one's results are discarded
    // 如果另一个调用开始，这个调用的结果将被丢弃
    roots.value = scanRoots
    loading.value = true
    try {
      const acc: DirNode[] = []
      for (const root of scanRoots) {
        const children = await invoke<DirNode[]>(IPC.GET_DIRECTORY_TREE, { rootId: root.id })
        if (myId !== loadingId) return // superseded — discard WITHOUT touching nodes | 已被取代 — 丢弃且不动 nodes
        children.forEach((c) => {
          const r = scanRoots.find((rt) => rt.id === c.rootId)
          if (r) c.absPath = c.relPath ? `${r.path}/${c.relPath}` : r.path
        })
        acc.push(...children)
      }
      if (myId !== loadingId) return
      nodes.value = acc // single atomic swap — never blank mid-flight | 单次原子替换 — 中途不留空
    } finally {
      if (myId === loadingId) loading.value = false
    }
  }

  async function loadChildren(parentId: number | null, rootId?: number, loadId?: number) {
    loading.value = true
    try {
      if (parentId === null && rootId !== undefined) {
        const children = await invoke<DirNode[]>(IPC.GET_DIRECTORY_TREE, { rootId })
        if (loadId !== undefined && loadId !== loadingId) return // Race condition guard
        // 竞争条件守卫
        children.forEach((c) => {
          const r = roots.value.find((root) => root.id === c.rootId)
          if (r) c.absPath = c.relPath ? `${r.path}/${c.relPath}` : r.path
        })
        nodes.value = [...nodes.value, ...children]
      } else if (parentId !== null) {
        const children = await invoke<DirNode[]>(IPC.GET_DIRECTORY_CHILDREN, { parentId })
        children.forEach((c) => {
          const r = roots.value.find((root) => root.id === c.rootId)
          if (r) c.absPath = c.relPath ? `${r.path}/${c.relPath}` : r.path
        })
        // Inject children after their parent
        // 在其父节点之后注入子节点
        const idx = nodes.value.findIndex((n) => n.id === parentId)
        if (idx >= 0) {
          nodes.value.splice(idx + 1, 0, ...children)
        }
        // Mark parent as expanded
        // 标记父节点为展开状态
        const parent = nodes.value.find((n) => n.id === parentId)
        if (parent) parent.expanded = true
      }
    } finally {
      loading.value = false
    }
  }

  // Lazy-load a directory's own files for the tree's expandable file list. Cached on
  // the node (`filesLoaded`) so re-expanding is instant. Direct files only — subfolders
  // are separate nodes. | 懒加载某目录自身的文件，用于树的可展开文件列表。结果缓存在
  // 节点上（`filesLoaded`），使重新展开瞬时完成。仅直接文件——子文件夹是独立节点。
  async function loadFiles(node: DirNode) {
    if (node.filesLoaded) return
    const files = await invoke<DirFile[]>(IPC.LIST_DIRECTORY_FILES, { directoryId: node.id })
    node.files = files
    node.filesLoaded = true
  }

  async function toggleNode(node: DirNode) {
    if (node.expanded) {
      // Collapse: remove descendant DIR rows. Cached `files` stay on the node but the
      // template hides them while collapsed, so re-expanding shows them instantly.
      // 折叠：删除后代「目录」行。缓存的 `files` 仍留在节点上，但模板在折叠时隐藏它们，
      // 因此重新展开能瞬时显示。
      collapseNode(node)
      return
    }
    // Expand: reveal the body first (so cached files appear immediately), then lazily
    // load subfolders AND this dir's own files in parallel. A dir with only files
    // (hasChildren=false but mediaCount>0) still expands — it just has no subfolders.
    // 展开：先展开主体（使缓存文件立即出现），再并行懒加载子文件夹与本目录自身的文件。
    // 仅含文件的目录（hasChildren=false 但 mediaCount>0）同样可展开——只是没有子文件夹。
    node.expanded = true
    const tasks: Promise<unknown>[] = []
    if (node.hasChildren) tasks.push(loadChildren(node.id))
    if (!node.filesLoaded) tasks.push(loadFiles(node))
    if (tasks.length) {
      node.loading = true
      try {
        await Promise.all(tasks)
      } finally {
        node.loading = false
      }
    }
  }

  async function expandToNode(targetId: number) {
    if (nodes.value.find((n) => n.id === targetId)) {
      scrollToNode(targetId)
      return
    }

    try {
      const ancestors = await invokeIpc<number[]>(IPC.GET_DIRECTORY_ANCESTORS, { id: targetId })
      for (const id of ancestors) {
        if (id === targetId) continue
        const node = nodes.value.find((n) => n.id === id)
        if (node && !node.expanded) {
          await loadChildren(id)
        }
      }
      setTimeout(() => scrollToNode(targetId), 50)
    } catch (e) {
      console.error('[useFolderTree] expandToNode failed:', e)
    }
  }

  function scrollToNode(id: number) {
    const el = document.querySelector(`[data-dir-id="${id}"]`)
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
    }
  }

  function collapseNode(node: DirNode) {
    const descendants = getDescendantIds(node.id)
    nodes.value = nodes.value.filter((n) => !descendants.has(n.id))
    node.expanded = false
  }

  function getDescendantIds(parentId: number): Set<number> {
    const set = new Set<number>()
    const stack = [parentId]
    while (stack.length) {
      const pid = stack.pop()!
      nodes.value.forEach((n) => {
        if (n.parentId === pid) {
          set.add(n.id)
          stack.push(n.id)
        }
      })
    }
    return set
  }

  return { roots, nodes, loading, loadRoots, loadChildren, toggleNode, expandToNode }
}
