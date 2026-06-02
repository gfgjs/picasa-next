// src/constants/ipc.ts
// IPC command and event name constants
// IPC 命令和事件名称常量

export const IPC = {
  // ── Scan ──────────────────────────────────────────────────────────────
  // ── 扫描 ──────────────────────────────────────────────────────────────
  ADD_SCAN_ROOT:    'add_scan_root',
  REMOVE_SCAN_ROOT: 'remove_scan_root',
  LIST_SCAN_ROOTS:  'list_scan_roots',
  START_SCAN:             'start_scan',
  STOP_SCAN:              'stop_scan',
  CLEAR_DATABASE:         'clear_database',
  CLEAR_SETTINGS:         'clear_settings',

  // ── Layout ────────────────────────────────────────────────────────────
  // ── 布局 ────────────────────────────────────────────────────────────
  COMPUTE_LAYOUT:   'compute_layout',
  GET_LAYOUT_ROWS:  'get_layout_rows',
  GET_LAYOUT_ROWS_BY_Y: 'get_layout_rows_by_y',

  // ── Media ─────────────────────────────────────────────────────────────
  // ── 媒体 ─────────────────────────────────────────────────────────────
  GET_MEDIA_DETAIL:          'get_media_detail',
  GET_COMPANION_VIDEO_URL:   'get_companion_video_url',
  TOGGLE_FAVORITE:           'toggle_favorite',
  SET_RATING:                'set_rating',
  SOFT_DELETE_ITEMS:         'soft_delete_items',
  RESTORE_ITEMS:             'restore_items',
  GET_TRASH:                 'get_trash',
  GET_STATS:                 'get_stats',
  GET_DIRECTORY_TREE:        'get_directory_tree',
  GET_DIRECTORY_CHILDREN:    'get_directory_children',

  // ── Thumbnail ─────────────────────────────────────────────────────────
  // ── 缩略图 ─────────────────────────────────────────────────────────
  BATCH_REQUEST_THUMBNAILS:  'batch_request_thumbnails',
  REQUEST_THUMBNAIL:         'request_thumbnail',

  // ── Search ────────────────────────────────────────────────────────────
  // ── 搜索 ────────────────────────────────────────────────────────────
  SEARCH_MEDIA:     'search_media',

  // ── Config ────────────────────────────────────────────────────────────
  // ── 配置 ────────────────────────────────────────────────────────────
  GET_APP_CONFIG:   'get_app_config',
  SET_APP_CONFIG:   'set_app_config',

  // ── System ────────────────────────────────────────────────────────────
  // ── 系统 ────────────────────────────────────────────────────────────
  SHOW_IN_EXPLORER: 'show_in_explorer',
  MOVE_TO_TRASH:    'move_to_trash',

  // ── Dev / maintenance ─────────────────────────────────────────────────
  // ── 开发 / 维护 ─────────────────────────────────────────────────
  CLEAR_ALL_DATA:   'clear_all_data',
} as const

// ── Tauri events ──────────────────────────────────────────────────────────
// ── Tauri 事件 ──────────────────────────────────────────────────────────
export const EVENTS = {
  MEDIA_ENRICHED:        'db:media_enriched',
  ENRICHMENT_COMPLETED:  'enrichment:completed',
  MEDIA_UPDATED:         'db:media_updated',
} as const
