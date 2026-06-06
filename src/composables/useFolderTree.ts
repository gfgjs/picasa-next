// src/composables/useFolderTree.ts
// Folder tree lazy loading
// 文件夹树懒加载

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { DirNode, ScanRoot } from '../types/media'
import { IPC } from '../constants/ipc'

export function useFolderTree() {
  const roots     = ref<ScanRoot[]>([])
  const nodes     = ref<DirNode[]>([])
  const loading   = ref(false)
  let   loadingId = 0          // guard against concurrent loadRoots calls
                               // 防止并发 loadRoots 调用的守卫

  async function loadRoots(scanRoots: ScanRoot[]) {
    const myId = ++loadingId   // if another call starts, this one's results are discarded
                               // 如果另一个调用开始，这个调用的结果将被丢弃
    roots.value = scanRoots
    nodes.value = []
    for (const root of scanRoots) {
      if (myId !== loadingId) return   // superseded — bail out
                                       // 已被取代 — 退出
      await loadChildren(null, root.id, myId)
    }
  }

  async function loadChildren(parentId: number | null, rootId?: number, loadId?: number) {
    loading.value = true
    try {
      if (parentId === null && rootId !== undefined) {
        const children = await invoke<DirNode[]>(IPC.GET_DIRECTORY_TREE, { rootId })
        if (loadId !== undefined && loadId !== loadingId) return // Race condition guard
                                                                 // 竞争条件守卫
        nodes.value = [...nodes.value, ...children]
      } else if (parentId !== null) {
        const children = await invoke<DirNode[]>(IPC.GET_DIRECTORY_CHILDREN, { parentId })
        // Inject children after their parent
        // 在其父节点之后注入子节点
        const idx = nodes.value.findIndex(n => n.id === parentId)
        if (idx >= 0) {
          nodes.value.splice(idx + 1, 0, ...children)
        }
        // Mark parent as expanded
        // 标记父节点为展开状态
        const parent = nodes.value.find(n => n.id === parentId)
        if (parent) parent.expanded = true
      }

    } finally {
      loading.value = false
    }
  }

  function toggleNode(node: DirNode) {
    if (node.expanded) {
      // Collapse: remove all descendants
      // 折叠：删除所有后代节点
      collapseNode(node)
    } else {
      loadChildren(node.id)
    }
  }

  async function expandToNode(targetId: number) {
    if (nodes.value.find(n => n.id === targetId)) {
      scrollToNode(targetId)
      return
    }
    
    try {
      const ancestors = await invoke<number[]>('get_directory_ancestors', { id: targetId })
      for (const id of ancestors) {
        if (id === targetId) continue
        const node = nodes.value.find(n => n.id === id)
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
    nodes.value = nodes.value.filter(n => !descendants.has(n.id))
    node.expanded = false
  }

  function getDescendantIds(parentId: number): Set<number> {
    const set = new Set<number>()
    const stack = [parentId]
    while (stack.length) {
      const pid = stack.pop()!
      nodes.value.forEach(n => {
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
