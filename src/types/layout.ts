// src/types/layout.ts
// Justified Layout row types (mirroring Rust LayoutRow enum)
// 两端对齐布局行类型（对应 Rust 的 LayoutRow 枚举）

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
  fileName: string
  dirPath: string | null
  sortDatetime: number
  gpsLat: number | null
  gpsLng: number | null
  exifMake: string | null
  exifModel: string | null
  exifLens: string | null
  exifFocalLength: number | null
  exifAperture: number | null
  exifShutter: string | null
  exifIso: number | null
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
