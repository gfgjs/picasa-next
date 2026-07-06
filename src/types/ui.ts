// src/types/ui.ts
// UI-only state types
// 仅 UI 的状态类型

/**
 * 外观模式(多主题 S1 起与「主题包」正交):亮/暗/跟随系统。
 * 具体落到哪套主题由 uiStore 的 lightThemeId/darkThemeId 槽位决定。
 */
export type AppearanceMode = 'dark' | 'light' | 'system'

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
