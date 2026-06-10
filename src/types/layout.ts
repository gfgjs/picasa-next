// src/types/layout.ts
// Justified Layout row types (mirroring Rust LayoutRow enum)
// 两端对齐布局行类型（对应 Rust 的 LayoutRow 枚举）

// Resident per-item row data (kept small for million-item memory). Heavy metadata
// (fileName, dirPath, EXIF, GPS) is NOT here — fetch it lazily via MediaMeta.
// 常驻逐项行数据（为百万项内存保持精简）。重型元数据（fileName/dirPath/EXIF/GPS）
// 不在此处 —— 经 MediaMeta 按需拉取。
export interface LayoutRowItem {
  id:          number
  x:           number
  w:           number
  h:           number
  fileSize:    number
  fileFormat:  string
  mediaType:   string
  isLivePhoto: boolean
  durationMs:  number | null
  thumbStatus: number
  thumbPath:   string | null
  thumbhash:   number[] | null
  similarity?: number
  isFavorited: boolean
  originalWidth: number
  originalHeight: number
  sortDatetime: number
}

// Heavy per-item metadata fetched on demand for the visible viewport only.
// 仅为可视区按需拉取的逐项重型元数据。
export interface MediaMeta {
  id:              number
  fileName:        string
  dirPath:         string | null
  gpsLat:          number | null
  gpsLng:          number | null
  exifMake:        string | null
  exifModel:       string | null
  exifLens:        string | null
  exifFocalLength: number | null
  exifAperture:    number | null
  exifShutter:     string | null
  exifIso:         number | null
}

export interface LayoutRowNormal {
  rowType: 'normal'
  y:       number
  height:  number
  items:   LayoutRowItem[]
}

export interface LayoutRowSeparator {
  rowType:        'separator'
  y:              number
  height:         number
  separatorLabel: string
  groupId?:       string
}

export type LayoutRow = LayoutRowNormal | LayoutRowSeparator

export interface LayoutSummary {
  totalRows:     number
  totalHeight:   number
  layoutVersion: number
  totalItems:    number
  separators:    { label: string; y: number; groupId?: string }[]
}
