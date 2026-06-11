// src/stores/historyStore.ts
// Undo/redo history for folder move & copy operations (session-only, in-memory).
// 文件夹移动 & 复制操作的撤销/重做历史（仅会话内，内存存储）。
//
// Filesystem-level operations are NOT persisted across restarts — the disk state
// may have changed between sessions, making a stale undo dangerous.
// 文件系统级操作不跨重启持久化 —— 两次会话间磁盘状态可能已变，过期的撤销有风险。

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { IPC } from '../constants/ipc'
import { useUiStore } from './uiStore'
import { useScanStore } from './scanStore'

interface MoveRecord {
  type: 'move'
  dirId: number
  name: string
  fromParentId: number
  toParentId: number
}

interface CopyRecord {
  type: 'copy'
  sourceDirId: number
  targetDirId: number
  name: string
  createdRootId: number
  createdRelPath: string
  createdAbsPath: string
}

type OpRecord = MoveRecord | CopyRecord

interface CopyDirResult {
  createdRootId: number
  createdRelPath: string
  createdAbsPath: string
}

export const useHistoryStore = defineStore('history', () => {
  const undoStack = ref<OpRecord[]>([])
  const redoStack = ref<OpRecord[]>([])
  const busy = ref(false)

  const canUndo = computed(() => undoStack.value.length > 0 && !busy.value)
  const canRedo = computed(() => redoStack.value.length > 0 && !busy.value)

  // Notify the rest of the UI to refresh. The sidebar listens for `folder-stats-changed`
  // (reloads the folder tree, preserving expansion, and selects `detail.selectDirId` if
  // given); the media grid recomputes its layout. One event → one reload.
  // 通知界面其余部分刷新。侧边栏监听 `folder-stats-changed`（重载文件夹树、保留展开态，
  // 并在给出 `detail.selectDirId` 时选中它）；媒体网格重新计算布局。一个事件 → 一次重载。
  function refresh(selectDirId?: number | null) {
    window.dispatchEvent(new CustomEvent('folder-stats-changed', { detail: { selectDirId } }))
  }

  // ── Raw operations (shared by initial action + redo) ───────────────────────
  // ── 原子操作（初始动作 + 重做共用） ───────────────────────────────────────
  async function execMove(dirId: number, targetId: number): Promise<void> {
    await invoke(IPC.MOVE_DIRECTORY, { sourceDirId: dirId, targetDirId: targetId })
    refresh(dirId) // auto-select the moved folder | 自动选中移动后的文件夹
  }

  async function execCopy(sourceDirId: number, targetDirId: number): Promise<CopyDirResult> {
    const res = await invoke<CopyDirResult>(IPC.COPY_DIRECTORY, { sourceDirId, targetDirId })
    // Ingest the copied files as fresh assets via a background re-scan of the target root.
    // 通过对目标根目录的后台重扫，将复制出的文件作为全新资产引入。
    await useScanStore().startScan(res.createdRootId, () => {
      window.dispatchEvent(new CustomEvent('folder-stats-changed'))
    })
    refresh()
    return res
  }

  async function execDeleteCopy(rec: CopyRecord): Promise<void> {
    await invoke(IPC.DELETE_DIRECTORY_TO_TRASH, {
      absPath: rec.createdAbsPath,
      rootId: rec.createdRootId,
      relPath: rec.createdRelPath,
    })
    refresh()
  }

  // ── Public: perform a NEW move/copy (pushes an undo record) ────────────────
  // ── 对外：执行一次新的移动/复制（压入撤销记录） ─────────────────────────────
  async function move(dirId: number, name: string, fromParentId: number, toParentId: number): Promise<void> {
    busy.value = true
    try {
      await execMove(dirId, toParentId)
      undoStack.value.push({ type: 'move', dirId, name, fromParentId, toParentId })
      redoStack.value = []
    } finally {
      busy.value = false
    }
  }

  async function copy(sourceDirId: number, name: string, targetDirId: number): Promise<void> {
    busy.value = true
    try {
      const res = await execCopy(sourceDirId, targetDirId)
      undoStack.value.push({ type: 'copy', sourceDirId, targetDirId, name, ...res })
      redoStack.value = []
    } finally {
      busy.value = false
    }
  }

  // ── Undo / Redo ────────────────────────────────────────────────────────────
  async function undo(): Promise<void> {
    if (!canUndo.value) return
    const ui = useUiStore()
    const rec = undoStack.value[undoStack.value.length - 1]
    busy.value = true
    try {
      if (rec.type === 'move') {
        await execMove(rec.dirId, rec.fromParentId)
      } else {
        await execDeleteCopy(rec)
      }
      undoStack.value.pop()
      redoStack.value.push(rec)
      ui.addToast('success', rec.type === 'move' ? `已撤销移动「${rec.name}」` : `已撤销复制「${rec.name}」`)
    } catch (e) {
      // The recorded operation is no longer valid (target changed externally etc.).
      // Drop it so the user isn't stuck on a broken history entry.
      // 记录的操作已失效（目标被外部改动等）。丢弃它，避免卡在损坏的历史项上。
      undoStack.value.pop()
      ui.addToast('error', `撤销失败 | Undo failed: ${e}`)
    } finally {
      busy.value = false
    }
  }

  async function redo(): Promise<void> {
    if (!canRedo.value) return
    const ui = useUiStore()
    const rec = redoStack.value[redoStack.value.length - 1]
    busy.value = true
    try {
      if (rec.type === 'move') {
        await execMove(rec.dirId, rec.toParentId)
      } else {
        const res = await execCopy(rec.sourceDirId, rec.targetDirId)
        rec.createdRootId = res.createdRootId
        rec.createdRelPath = res.createdRelPath
        rec.createdAbsPath = res.createdAbsPath
      }
      redoStack.value.pop()
      undoStack.value.push(rec)
      ui.addToast('success', rec.type === 'move' ? `已重做移动「${rec.name}」` : `已重做复制「${rec.name}」`)
    } catch (e) {
      redoStack.value.pop()
      ui.addToast('error', `重做失败 | Redo failed: ${e}`)
    } finally {
      busy.value = false
    }
  }

  return {
    undoStack, redoStack, busy,
    canUndo, canRedo,
    move, copy, undo, redo,
  }
})
