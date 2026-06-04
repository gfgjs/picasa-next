// src/constants/defaults.ts
// Application default values
// 应用程序默认值

/** 缩略图固定四档（像素）/ Fixed four thumbnail size tiers (pixels) */
export const THUMB_SIZE_TIERS = [120, 240, 480, 960] as const
export type ThumbSizeTier = typeof THUMB_SIZE_TIERS[number]

export const DEFAULTS = {
  THUMB_SIZE:         240,   // 默认档位 / Default tier: 240px (matches THUMB_SIZE_TIERS[1])
  THUMB_SKIP_MAX_KB:  200,
  THUMB_QUALITY:      80,
  THUMB_FORMAT:       'webp',
  THUMB_STRATEGY:     'cpu',
  GPU_ENGINE:         'wic',
  SIDEBAR_WIDTH:      260,
  GRID_ROW_HEIGHT:    200,
  GRID_GAP:           4,
  SEARCH_DEBOUNCE_MS: 150,
  RESIZE_DEBOUNCE_MS: 300,
  SCROLL_BUFFER_ROWS: 15,
  THUMB_BATCH_SIZE:   24,
  ENRICHMENT_BATCH:   500,
  SCAN_PROGRESS_INTERVAL: 500,
} as const

export const SEPARATOR_HEIGHT = 36  // px — fixed DateSeparator row height
// px — 固定的 DateSeparator 行高
