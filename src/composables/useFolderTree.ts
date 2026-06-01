// src/composables/useFolderTree.ts
// Folder tree lazy loading

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { DirNode, ScanRoot } from '../types/media'
import { IPC } from '../constants/ipc'

export function useFolderTree() {
  const roots     = ref<ScanRoot[]>([])
  const nodes     = ref<DirNode[]>([])
  const loading   = ref(false)

  async function loadRoots(scanRoots: ScanRoot[]) {
    roots.value = scanRoots
    nodes.value = []
    for (const root of scanRoots) {
      await loadChildren(null, root.id)
    }
  }

  async function loadChildren(parentId: number | null, rootId?: number) {
    loading.value = true
    try {
      if (parentId === null && rootId !== undefined) {
        const children = await invoke<DirNode[]>(IPC.GET_DIRECTORY_TREE, { rootId })
        nodes.value = [...nodes.value, ...children]
      } else if (parentId !== null) {
        const children = await invoke<DirNode[]>(IPC.GET_DIRECTORY_CHILDREN, { parentId })
        // Inject children after their parent
        const idx = nodes.value.findIndex(n => n.id === parentId)
        if (idx >= 0) {
          nodes.value.splice(idx + 1, 0, ...children)
        }
        // Mark parent as expanded
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
      collapseNode(node)
    } else {
      loadChildren(node.id)
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

  return { roots, nodes, loading, loadRoots, loadChildren, toggleNode }
}
