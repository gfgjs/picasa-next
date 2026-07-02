// src/stores/personStore.ts
// People-wall state (F6) — person clusters list + rename/merge/hide + per-item face boxes.
// 人物墙状态（F6）—— 人物簇列表 + 命名/合并/隐藏 + 单图人脸框。

import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invokeIpc } from '../utils/ipc'
import { IPC } from '../constants/ipc'
import type { PersonSummary, FaceBox, LikelyMatchGroup } from '../types/person'

export const usePersonStore = defineStore('person', () => {
  const persons = ref<PersonSummary[]>([])
  const isLoading = ref(false)
  // App cache dir (for resolving cover-face thumbnail URLs); fetched once.
  // 应用缓存目录（用于解析封面脸缩略图 URL）；只取一次。
  const cacheDir = ref('')

  async function ensureCacheDir() {
    if (cacheDir.value) return
    try {
      cacheDir.value = (await invokeIpc<string>(IPC.GET_THUMB_CACHE_DIR)).replace(/\\/g, '/')
    } catch (e) {
      console.error('[Person] get cache dir failed:', e)
    }
  }

  /** Load all person clusters for the wall. | 加载人物墙的全部人物簇。 */
  async function load() {
    isLoading.value = true
    try {
      await ensureCacheDir()
      persons.value = await invokeIpc<PersonSummary[]>(IPC.LIST_FACE_PERSONS)
    } catch (e) {
      console.error('[Person] load failed:', e)
    } finally {
      isLoading.value = false
    }
  }

  /** Rename a person (empty → unnamed); updates the row in place. | 命名（空→未命名）；原地更新。 */
  async function rename(personId: number, name: string) {
    try {
      await invokeIpc(IPC.RENAME_FACE_PERSON, { personId, name })
      const p = persons.value.find((p) => p.id === personId)
      if (p) {
        const trimmed = name.trim()
        p.name = trimmed || null
        p.isNamed = !!trimmed
      }
    } catch (e) {
      console.error('[Person] rename failed:', e)
    }
  }

  /** Show/hide a person on the wall; updates in place. | 显示/隐藏；原地更新。 */
  async function setHidden(personId: number, hidden: boolean) {
    try {
      await invokeIpc(IPC.SET_FACE_PERSON_HIDDEN, { personId, hidden })
      const p = persons.value.find((p) => p.id === personId)
      if (p) p.isHidden = hidden
    } catch (e) {
      console.error('[Person] setHidden failed:', e)
    }
  }

  /** Merge `srcIds` into `dstId`, then reload (counts/centroids changed). | 合并后重载。 */
  async function merge(srcIds: number[], dstId: number) {
    try {
      await invokeIpc(IPC.MERGE_FACE_PERSONS, { srcIds, dstId })
      await load()
    } catch (e) {
      console.error('[Person] merge failed:', e)
    }
  }

  /** Faces detected in one image (detail-viewer overlay). | 一张图中的人脸（详情叠加）。 */
  async function getFacesForItem(itemId: number): Promise<FaceBox[]> {
    try {
      return await invokeIpc<FaceBox[]>(IPC.GET_ITEM_FACES, { itemId })
    } catch (e) {
      console.error('[Person] getFacesForItem failed:', e)
      return []
    }
  }

  /** Full re-cluster: fix fragmentation (same person split across unnamed clusters) without
   *  breaking confirmed faces / named persons. Throws if analysis is running (surfaced by caller).
   *  全量重新聚类：修碎片化（同一人散成多个未命名簇），不打散已确认脸/已命名人物。分析运行中会抛错
   *  （由调用方提示）。 */
  async function recluster() {
    await invokeIpc(IPC.RECLUSTER_FACES)
    await load() // 簇/计数已变 → 重载
  }

  // ── 批量审批（T10, §3.6.2）────────────────────────────────────────────────
  // likely-match 分组：未确认脸按候选 person 分组，用户对整组/选中脸一次性确认/改派/移出/拒绝/建人。
  // 审批动作的副作用直接传播 invokeIpc 的 reject（含后端中文错误消息，如跨模型改派），由调用方 toast。
  const likelyMatches = ref<LikelyMatchGroup[]>([])

  /** 加载批量审批的 likely-match 分组。可选按 `personId` 聚焦 / `limit` 限量。 */
  async function loadLikelyMatches(personId?: number, limit?: number) {
    await ensureCacheDir()
    likelyMatches.value = await invokeIpc<LikelyMatchGroup[]>(IPC.LIST_LIKELY_FACE_MATCHES, {
      personId: personId ?? null,
      limit: limit ?? null,
    })
  }

  /** 乐观更新：从内存分组移除已处理的脸，并丢弃清空的组。重排新数组以触发响应式刷新。 */
  function dropResolvedFaces(faceIds: number[]) {
    const ids = new Set(faceIds)
    likelyMatches.value = likelyMatches.value
      .map((g) => ({ ...g, candidateFaces: g.candidateFaces.filter((f) => !ids.has(f.faceId)) }))
      .filter((g) => g.candidateFaces.length > 0)
  }

  /** 确认：接受这些脸归属其候选 person（锁定 is_confirmed）。 */
  async function confirmFaces(faceIds: number[]) {
    await invokeIpc(IPC.CONFIRM_FACES, { faceIds })
    dropResolvedFaces(faceIds)
  }

  /** 改派：把这些脸改归 `personId` 并锁定（纠正聚类错误）。后端拒绝跨模型改派。 */
  async function reassignFaces(faceIds: number[], personId: number) {
    await invokeIpc(IPC.REASSIGN_FACES, { faceIds, personId })
    dropResolvedFaces(faceIds)
  }

  /** 移出：清这些脸的 person 归属与确认态（误检/归错）。 */
  async function unassignFaces(faceIds: number[]) {
    await invokeIpc(IPC.UNASSIGN_FACES, { faceIds })
    dropResolvedFaces(faceIds)
  }

  /** 拒绝：标记这些脸不属于候选 `personId`（不再作为其 likely-match）。 */
  async function rejectFaces(faceIds: number[], personId: number) {
    await invokeIpc(IPC.REJECT_FACES, { faceIds, personId })
    dropResolvedFaces(faceIds)
  }

  /** 建新人物：从这些脸新建 person（可选命名），返回新 person id。 */
  async function createPerson(faceIds: number[], name?: string): Promise<number> {
    const newId = await invokeIpc<number>(IPC.CREATE_PERSON, { faceIds, name: name ?? null })
    dropResolvedFaces(faceIds)
    return newId
  }

  return {
    persons,
    isLoading,
    cacheDir,
    load,
    ensureCacheDir,
    rename,
    setHidden,
    merge,
    getFacesForItem,
    recluster,
    likelyMatches,
    loadLikelyMatches,
    confirmFaces,
    reassignFaces,
    unassignFaces,
    rejectFaces,
    createPerson,
  }
})
