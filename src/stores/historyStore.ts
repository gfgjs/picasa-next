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
import i18n from '../i18n'
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

/** Move of one or more MEDIA items into a folder (drag-to-folder). Each item carries its
 *  original directory so undo restores exact positions (问题5). */
/** 一个或多个媒体项移动到文件夹（拖到文件夹）。每项带原目录，撤销时精确还原（问题5）。 */
interface MoveMediaRecord {
  type: 'moveMedia'
  items: { id: number; fromDirId: number }[]
  targetDirId: number
  label: string
}

/** Copy of one or more MEDIA items into a folder (drag-to-folder with Shift / right-drag).
 *  We record the created row ids so undo deletes exactly those copies (问题2). */
/** 一个或多个媒体项复制到文件夹（Shift / 右键拖拽）。记录新建行 id，撤销时精确删除这些副本（问题2）。 */
interface CopyMediaRecord {
  type: 'copyMedia'
  srcIds: number[]
  targetDirId: number
  createdIds: number[]
  label: string
}

/** Backend result of relocate_media_items | relocate_media_items 的后端结果 */
interface MediaRelocationResult {
  id: number
  fromDirId: number
  targetDirId: number
}

/** Backend result of copy_media_items_db | copy_media_items_db 的后端结果 */
interface MediaCopyResult {
  srcId: number
  newId: number
}

type OpRecord = MoveRecord | CopyRecord | MoveMediaRecord | CopyMediaRecord

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
  async function move(
    dirId: number,
    name: string,
    fromParentId: number,
    toParentId: number,
  ): Promise<void> {
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

  // ── Public: move MEDIA items into a folder (drag-to-folder, undoable) ───────────
  // ── 对外：把媒体项移动到文件夹（拖到文件夹，可撤销） ────────────────────────────
  /** Relocate a set of items (shared by undo + redo). | 重定位一组项（撤销+重做共用）。 */
  async function relocateMedia(moves: { id: number; targetDirId: number }[]): Promise<void> {
    await invoke(IPC.RELOCATE_MEDIA_ITEMS, { moves })
    refresh() // reload tree (live counts) + recompute grid | 重载树（实时计数）+ 重算网格
  }

  async function moveMedia(itemIds: number[], targetDirId: number, label: string): Promise<number> {
    if (itemIds.length === 0) return 0
    busy.value = true
    try {
      const results = await invoke<MediaRelocationResult[]>(IPC.RELOCATE_MEDIA_ITEMS, {
        moves: itemIds.map((id) => ({ id, targetDirId })),
      })
      if (results.length === 0) return 0 // nothing actually moved (already in target) | 无实际移动
      undoStack.value.push({
        type: 'moveMedia',
        items: results.map((r) => ({ id: r.id, fromDirId: r.fromDirId })),
        targetDirId,
        label,
      })
      redoStack.value = []
      refresh()
      return results.length
    } finally {
      busy.value = false
    }
  }

  async function copyMedia(itemIds: number[], targetDirId: number, label: string): Promise<number> {
    if (itemIds.length === 0) return 0
    busy.value = true
    try {
      const results = await invoke<MediaCopyResult[]>(IPC.COPY_MEDIA_ITEMS_DB, {
        moves: itemIds.map((id) => ({ id, targetDirId })),
      })
      if (results.length === 0) return 0 // nothing copied (collision / same dir) | 无实际复制
      undoStack.value.push({
        type: 'copyMedia',
        srcIds: itemIds,
        targetDirId,
        createdIds: results.map((r) => r.newId),
        label,
      })
      redoStack.value = []
      refresh()
      return results.length
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
      let msg: string
      if (rec.type === 'move') {
        await execMove(rec.dirId, rec.fromParentId)
        msg = i18n.global.t('history.undoMoveFolder', { name: rec.name })
      } else if (rec.type === 'copy') {
        await execDeleteCopy(rec)
        msg = i18n.global.t('history.undoCopyFolder', { name: rec.name })
      } else if (rec.type === 'moveMedia') {
        // moveMedia: relocate each item back to its ORIGINAL directory.
        // 媒体移动：把每项移回原目录。
        await relocateMedia(rec.items.map((i) => ({ id: i.id, targetDirId: i.fromDirId })))
        msg = i18n.global.t('history.undoMoveItems', { count: rec.items.length })
      } else {
        // copyMedia: delete exactly the copies we created (file → trash + row).
        // 媒体复制：精确删除我们创建的副本（文件→回收站 + 行）。
        await invoke(IPC.REMOVE_MEDIA_ITEMS_HARD, { ids: rec.createdIds })
        refresh()
        msg = i18n.global.t('history.undoCopyItems', { count: rec.createdIds.length })
      }
      undoStack.value.pop()
      redoStack.value.push(rec)
      ui.addToast('success', msg)
    } catch (e) {
      // The recorded operation is no longer valid (target changed externally etc.).
      // Drop it so the user isn't stuck on a broken history entry.
      // 记录的操作已失效（目标被外部改动等）。丢弃它，避免卡在损坏的历史项上。
      undoStack.value.pop()
      ui.addToast('error', i18n.global.t('common.undoFailed', { error: e }))
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
      let msg: string
      if (rec.type === 'move') {
        await execMove(rec.dirId, rec.toParentId)
        msg = i18n.global.t('history.redoMoveFolder', { name: rec.name })
      } else if (rec.type === 'copy') {
        const res = await execCopy(rec.sourceDirId, rec.targetDirId)
        rec.createdRootId = res.createdRootId
        rec.createdRelPath = res.createdRelPath
        rec.createdAbsPath = res.createdAbsPath
        msg = i18n.global.t('history.redoCopyFolder', { name: rec.name })
      } else if (rec.type === 'moveMedia') {
        // moveMedia: relocate each item to the target directory again.
        // 媒体移动：把每项重新移动到目标目录。
        await relocateMedia(rec.items.map((i) => ({ id: i.id, targetDirId: rec.targetDirId })))
        msg = i18n.global.t('history.redoMoveItems', { count: rec.items.length })
      } else {
        // copyMedia: re-copy from the original sources; capture the fresh new ids so a
        // subsequent undo still deletes the right rows.
        // 媒体复制：从原始源重新复制；记录新的 id，使随后的撤销仍能删除正确的行。
        const results = await invoke<MediaCopyResult[]>(IPC.COPY_MEDIA_ITEMS_DB, {
          moves: rec.srcIds.map((id) => ({ id, targetDirId: rec.targetDirId })),
        })
        rec.createdIds = results.map((r) => r.newId)
        refresh()
        msg = i18n.global.t('history.redoCopyItems', { count: rec.createdIds.length })
      }
      redoStack.value.pop()
      undoStack.value.push(rec)
      ui.addToast('success', msg)
    } catch (e) {
      redoStack.value.pop()
      ui.addToast('error', i18n.global.t('history.redoFailed', { error: e }))
    } finally {
      busy.value = false
    }
  }

  return {
    undoStack,
    redoStack,
    busy,
    canUndo,
    canRedo,
    move,
    copy,
    moveMedia,
    copyMedia,
    undo,
    redo,
  }
})
