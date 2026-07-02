// src/composables/useViewDescriptor.ts
// R1-2（S4/T4c）· 当前画廊视图 → 后端 ViewDescriptor 的装配点。
//
// 🔴 与 useJustifiedLayout.compute() 的扁平 filters 装配**一一对应**（同一 UI 状态、两种投影：
// compute 喂扁平 MediaFilter 给 compute_layout；本函数拆成 scope + GalleryFilter 给 SelectAll
// 解析）。任何一侧新增视图维度（新的智能相册 / scope / 筛选字段），必须同步另一侧，否则
// 「全选批量操作的目标集」会与「画廊实际显示集」漂移（D1 单一事实源告诫）。

import { useMediaStore } from '../stores/mediaStore'
import { useFilterStore } from '../stores/filterStore'
import { useUiStore } from '../stores/uiStore'
import { useAiStore } from '../stores/aiStore'
import type { GalleryFilterDto, ViewDescriptorDto, ViewScopeDto } from '../types/view'

/**
 * 构造描述「当前画廊视图」的 ViewDescriptor（供 SelectAll 选区跨 IPC 传后端解析）。
 *
 * @returns 视图描述符；语义搜索模式返回 null（后端 view_to_sql 拒绝 SemanticSearch，
 *   调用方须回退 Explicit 物化路径）。
 */
export function buildCurrentViewDescriptor(): ViewDescriptorDto | null {
  const media = useMediaStore()
  const filter = useFilterStore()
  const ui = useUiStore()
  const ai = useAiStore()

  // 语义搜索：有序、非纯 SQL（v1 走 ai_search 既有路径），不可 SQL 描述。
  if (ai.isSemanticMode) return null

  const f: GalleryFilterDto = { ...filter.toApiFilter() }
  let scope: ViewScopeDto = { kind: 'all' }

  // scope 拆分优先级与 useJustifiedLayout.compute() 的分支一致（人物 > 收藏夹 > 智能相册）。
  // UI 侧已保证这些视图互斥（setActivePerson 等清兄弟状态）。
  if (ui.activePersonId) {
    scope = { kind: 'person', personId: ui.activePersonId }
  } else if (ui.activeCollection) {
    const c = ui.activeCollection
    if (c.kind === 'system' && c.mediaTypeFilter) {
      // 系统夹 ≈ 类型 + 收藏（scope 仍为 all，语义落在 filter）。
      f.mediaTypes = [c.mediaTypeFilter]
      f.favoritedOnly = true
    } else {
      scope = { kind: 'collection', albumId: c.id }
    }
  } else if (ui.activeSmartAlbum === 'favorites') {
    f.favoritedOnly = true
  } else if (ui.activeSmartAlbum === 'live-photos') {
    f.livePhotoOnly = true
  } else if (ui.activeSmartAlbum === 'recent') {
    f.recentOnly = true
  } else if (ui.activeSmartAlbum === 'trash') {
    scope = { kind: 'trash' }
  }

  // 目录树选中：仅当 scope 仍是 all 时降为 directory（compute 路径里 directoryId 与人物/
  // 收藏夹等由 UI 互斥保证不并存，这里显式让位避免 scope 单值丢维度）。
  if (scope.kind === 'all' && ui.activeDirectoryId != null) {
    scope = { kind: 'directory', directoryId: ui.activeDirectoryId }
  }

  if (ui.searchQuery && ui.searchQuery.trim() !== '') {
    f.searchQuery = ui.searchQuery.trim()
    f.searchScope = ui.searchScope
  }

  return {
    scope,
    filter: f,
    sort: {
      groupBy: ui.groupBy,
      sortWithinGroup: ui.sortWithinGroup,
      sortOrder: ui.sortOrder,
    },
    layoutVersion: media.layoutVersion,
  }
}
