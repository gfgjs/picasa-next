// src/types/ui.ts
// UI-only state types
// 仅 UI 的状态类型

export type Theme = 'dark' | 'light' | 'system'

export type SortBy = 'sort_datetime' | 'file_name' | 'file_size' | 'created_at'
export type SortOrder = 'asc' | 'desc'

export interface ToastMessage {
  id:       string
  type:     'success' | 'error' | 'warning' | 'info'
  message:  string
  duration: number
}

export type SmartAlbum = 'all' | 'favorites' | 'trash' | 'live-photos' | 'recent'

export interface MediaFilter {
  mediaTypes?:     string[]
  livePhotoOnly?:  boolean
  favoritedOnly?:  boolean
  minRating?:      number
  dateRange?:      { from: number; to: number }
  directoryId?:    number | null
}
