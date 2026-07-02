// src/types/view.ts
// R1-2（S4/T4c）· 后端 ViewDescriptor 的前端镜像。
//
// wire 形状由后端锁测试钉死（queries.rs::selection_descriptor_wire_format_locks_camel_case）：
// tag 变体名小驼峰（'directory' / 'selectAll'），struct 变体字段小驼峰（directoryId / excludedIds）。
// 修改任何一侧都必须同步另一侧 + 锁测试。
//
// 注意：SemanticSearch scope 刻意不镜像 —— 后端 view_to_sql 拒绝解析它（v1 语义搜索非纯 SQL），
// 语义模式下的批量操作由调用方回退 Explicit（物化 id）。

/** 视图集合来源（scope 定来源，filter 在来源上再筛 —— 对齐后端 ViewScope，D1）。 */
export type ViewScopeDto =
  | { kind: 'all' }
  | { kind: 'directory'; directoryId: number }
  | { kind: 'collection'; albumId: number }
  | { kind: 'person'; personId: number }
  | { kind: 'trash' }

/** 附加筛选（对齐后端 GalleryFilter；全部可选，undefined 字段序列化时被丢弃）。 */
export interface GalleryFilterDto {
  mediaTypes?: string[]
  livePhotoOnly?: boolean
  favoritedOnly?: boolean
  minRating?: number
  colorLabel?: number
  dateRange?: { from: number; to: number }
  searchQuery?: string
  searchScope?: string
  /** 「最近导入」智能相册（R1-2 后端补的对应字段）。 */
  recentOnly?: boolean
}

/** 排序规格（对齐后端 SortSpec；与 uiStore.groupBy / sortWithinGroup / sortOrder 同源）。 */
export interface SortSpecDto {
  groupBy: string
  sortWithinGroup: string
  sortOrder: string
}

/** 不可变视图描述符：唯一确定「当前画廊视图全集 + 序」，SelectAll 解析的依据。 */
export interface ViewDescriptorDto {
  scope: ViewScopeDto
  filter: GalleryFilterDto
  sort: SortSpecDto
  /** 与后端 LayoutCache.layout_version 对齐；不一致时后端拒绝（ViewStale）。 */
  layoutVersion: number
}
