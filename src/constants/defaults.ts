// src/constants/defaults.ts
// Application default values

export const DEFAULTS = {
  THUMB_SIZE:         300,
  THUMB_SKIP_MAX_KB:  200,
  THUMB_QUALITY:      80,
  THUMB_FORMAT:       'webp',
  SIDEBAR_WIDTH:      260,
  GRID_ROW_HEIGHT:    200,
  GRID_GAP:           4,
  SEARCH_DEBOUNCE_MS: 150,
  RESIZE_DEBOUNCE_MS: 300,
  SCROLL_BUFFER_ROWS: 3,
  THUMB_BATCH_SIZE:   24,
  ENRICHMENT_BATCH:   500,
  SCAN_PROGRESS_INTERVAL: 500,
} as const

export const SEPARATOR_HEIGHT = 36  // px — fixed DateSeparator row height
