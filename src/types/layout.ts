// src/types/layout.ts
// Justified Layout row types (mirroring Rust LayoutRow enum)
// 两端对齐布局行类型（对应 Rust 的 LayoutRow 枚举）

// Resident per-item row data (kept small for million-item memory). Heavy metadata
// (fileName, dirPath, EXIF, GPS) is NOT here — fetch it lazily via MediaMeta.
// 常驻逐项行数据（为百万项内存保持精简）。重型元数据（fileName/dirPath/EXIF/GPS）
// 不在此处 —— 经 MediaMeta 按需拉取。
export interface LayoutRowItem {
  id: number
  x: number
  w: number
  h: number
  fileSize: number
  fileFormat: string
  mediaType: string
  isLivePhoto: boolean
  durationMs: number | null
  thumbStatus: number
  thumbPath: string | null
  thumbhash: number[] | null
  similarity?: number
  isFavorited: boolean
  /** 用户评分 0-5（0 = 未评分）。与 isFavorited 同类的逐项小标量，供网格星级显示 + hover 快捷评分 + 「≥N 星」筛选。 */
  rating: number
  /** 用户颜色标签 0-7（0 = 未标）。与 rating 同类的逐项小标量，供网格 swatch 显示 + 按色筛选（T16）。 */
  colorLabel: number
  /** 系统可用态 'online'|'offline'|'missing'（缺失检测 Part2 §3.2）：前端置灰+角标。 */
  availability: string
  originalWidth: number
  originalHeight: number
  sortDatetime: number
}

// Heavy per-item metadata fetched on demand for the visible viewport only.
// 仅为可视区按需拉取的逐项重型元数据。
export interface MediaMeta {
  id: number
  fileName: string
  dirPath: string | null
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
  y: number
  height: number
  items: LayoutRowItem[]
}

export interface LayoutRowSeparator {
  rowType: 'separator'
  y: number
  height: number
  separatorLabel: string
  groupId?: string
}

export type LayoutRow = LayoutRowNormal | LayoutRowSeparator

/// 月密度桶（T14 §3.8.3）：date 分组下同月日分隔符合并而成，供时间轴 scrubber 按时间均布 +
/// 密度条渲染。仅 date 分组非空（folder/none 为空数组）。`groupId="YYYY-MM"` 可按月→y 定向滚动。
export interface MonthBucket {
  year: number
  month: number // 1-12
  count: number // 该月媒体项数（密度条高度依据）
  y: number // 该月首个分隔符逻辑 y（scrubber 跳转定位）
  groupId: string // "YYYY-MM"
}

export interface LayoutSummary {
  totalRows: number
  totalHeight: number
  layoutVersion: number
  totalItems: number
  separators: { label: string; y: number; groupId?: string }[]
  /// date 分组才非空（见 MonthBucket）。
  monthBuckets: MonthBucket[]
}
