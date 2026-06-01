// src/types/layout.ts
// Justified Layout row types (mirroring Rust LayoutRow enum)

export interface LayoutRowItem {
  id:          number
  x:           number
  w:           number
  h:           number
  mediaType:   string
  isLivePhoto: boolean
  durationMs:  number | null
  thumbStatus: number
  thumbPath:   string | null
  thumbhash:   number[] | null
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
}

export type LayoutRow = LayoutRowNormal | LayoutRowSeparator

export interface LayoutSummary {
  totalRows:     number
  totalHeight:   number
  layoutVersion: number
}
