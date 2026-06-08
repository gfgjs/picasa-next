// src/types/media.ts
// Core media data types mirroring Rust models
// 核心媒体数据类型，对应 Rust 模型

export type MediaType = 'image' | 'video' | 'audio' | 'document'
export type ThumbStatus = 0 | 1 | 2 | 3 // pending | done | failed | source-direct
                                        // 等待中 | 完成 | 失败 | 直接源

export interface ScanRoot {
  id:           number
  path:         string
  alias:        string | null
  scanStatus:   string
  scanProgress: number
  totalFiles:   number
  lastScanAt:   number | null
  isActive:     boolean
  createdAt:    number
  updatedAt:    number
}

export interface DirNode {
  id:          number
  rootId:      number
  parentId:    number | null
  name:        string
  relPath:     string
  depth:       number
  mediaCount:  number
  hasChildren: boolean
  // UI state (not from DB)
  // UI 状态（非来自数据库）
  expanded?:   boolean
  loading?:    boolean
  children?:   DirNode[]
  absPath?:    string
}

export interface MediaItem {
  id:                number
  directoryId:       number
  fileName:          string
  fileSize:          number
  fileMtime:         number
  fileFormat:        string
  mediaType:         MediaType
  width:             number
  height:            number
  durationMs:        number | null
  sortDatetime:      number
  cacheKey:          number
  thumbStatus:       ThumbStatus
  thumbPath:         string | null
  thumbhash:         number[] | null   // Uint8Array from Rust BLOB
                                       // 来自 Rust BLOB 的 Uint8Array
  isFavorited:       boolean
  isDeleted:         boolean
  deletedAt:         number | null
  rating:            number
  isLivePhoto:       boolean
  hasEmbeddedVideo:  boolean
  companionOf:       number | null
  contentHash:       string | null
  createdAt:         number
  updatedAt:         number
}

export interface ImageMeta {
  itemId:           number
  orientation:      number
  exifDatetime:     number | null
  exifMake:         string | null
  exifModel:        string | null
  exifLens:         string | null
  exifFocalLength:  number | null
  exifAperture:     number | null
  exifShutter:      string | null
  exifIso:          number | null
  exifGpsLat:       number | null
  exifGpsLng:       number | null
  dominantHue:      number | null
  dominantSat:      number | null
  dominantLum:      number | null
  dominantHex:      string | null
  isMonochrome:     boolean
}

export interface MediaDetail extends MediaItem {
  absPath:    string
  imageMeta:  ImageMeta | null
}

export interface SearchResult {
  id:          number
  fileName:    string
  mediaType:   MediaType
  thumbPath:   string | null
  thumbhash:   number[] | null
  thumbStatus: ThumbStatus
}

export interface AppStats {
  totalItems:      number
  totalImages:     number
  totalVideos:     number
  totalAudios:     number
  totalDocuments:  number
  totalFavorited:  number
  totalDeleted:    number
  totalLivePhotos: number
}

export interface ThumbResult {
  itemId:      number
  thumbStatus: ThumbStatus
  thumbPath:   string | null
  thumbhash:   number[] | null
}

export interface DateRange {
  from: number   // unix timestamp
                 // unix 时间戳
  to:   number
}

export interface MediaFilter {
  directoryId?:   number | null
  mediaTypes?:    MediaType[] | null
  favoritedOnly?: boolean | null
  minRating?:     number | null
  dateRange?:     DateRange | null
  livePhotoOnly?: boolean | null
  searchQuery?:   string | null
  searchScope?:   string | null
  aiSearch?:      boolean | null
  aiThreshold?:   number | null
}
