// src/types/ui.ts
// UI-only state types
// 仅 UI 的状态类型

export type Theme = 'dark' | 'light' | 'system'

export type SortBy = 'sort_datetime' | 'file_name' | 'file_size' | 'created_at'
export type SortOrder = 'asc' | 'desc'

/** An interactive chip rendered inside a toast (e.g. "加入收藏夹" shortcuts). */
/** toast 内的交互式快捷 chip（如「加入收藏夹」）。 */
export interface ToastAction {
  label: string
  onClick: () => void | Promise<void>
}

export interface ToastMessage {
  id: string
  type: 'success' | 'error' | 'warning' | 'info'
  message: string
  duration: number
  /** Optional action chips (e.g. add-to-collection). | 可选动作 chips（如加入收藏夹）。 */
  actions?: ToastAction[]
}

export type SmartAlbum = 'all' | 'favorites' | 'trash' | 'live-photos' | 'recent'

export interface MediaFilter {
  mediaTypes?: string[]
  livePhotoOnly?: boolean
  favoritedOnly?: boolean
  minRating?: number
  dateRange?: { from: number; to: number }
  directoryId?: number | null
  albumId?: number | null
}
