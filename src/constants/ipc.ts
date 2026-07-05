// src/constants/ipc.ts
// IPC command and event name constants
// IPC 命令和事件名称常量

export const IPC = {
  // ── Scan ──────────────────────────────────────────────────────────────
  // ── 扫描 ──────────────────────────────────────────────────────────────
  ADD_SCAN_ROOT: 'add_scan_root',
  REMOVE_SCAN_ROOT: 'remove_scan_root',
  // 移除扫描根并可选清理其缩略图（侧栏文件夹管理用）。
  REMOVE_SCAN_ROOT_WITH_OPTIONS: 'remove_scan_root_with_options',
  // 新增扫描根前检测与既有根的路径包含/重叠冲突。
  CHECK_FOLDER_OVERLAP: 'check_folder_overlap',
  LIST_SCAN_ROOTS: 'list_scan_roots',
  START_SCAN: 'start_scan',
  STOP_SCAN: 'stop_scan',
  CLEAR_DATABASE: 'clear_database',
  CLEAR_SETTINGS: 'clear_settings',

  // ── Layout ────────────────────────────────────────────────────────────
  // ── 布局 ────────────────────────────────────────────────────────────
  COMPUTE_LAYOUT: 'compute_layout',
  GET_LAYOUT_ROWS_BY_Y: 'get_layout_rows_by_y',
  // T16 方案B:按段取行(半开区间 [startY,endY) 精确归属,区别于 by_y 的视口相交语义)
  GET_BUCKET_ROWS: 'get_bucket_rows',
  GET_SEPARATOR_Y_BY_GROUP_ID: 'get_separator_y_by_group_id',
  GET_ITEM_Y_BY_ID: 'get_item_y_by_id',
  GET_SUBTREE_SCROLL_TARGET: 'get_subtree_scroll_target',
  // 按布局序的视图全集 id（Part5 T4 选区脱离 DOM 的顺序来源；后端 layout_commands.rs:128 已注册）
  GET_VIEW_IDS: 'get_view_ids',
  // 选择描述符解析/计数（Part5 S4，纯新增）：SelectAll 在后端 SQL 解析全集,不经前端整包传 id
  RESOLVE_SELECTION: 'resolve_selection',
  COUNT_SELECTION: 'count_selection',

  // ── H-Lab 横向画廊实验(plan-docs/2026-07-02-horizontal-gallery-lab.md)────────
  // 独立缓存/版本,与生产布局命令平行互不可见。
  COMPUTE_H_LAYOUT: 'compute_h_layout',
  GET_H_BLOCKS_BY_X: 'get_h_blocks_by_x',

  // ── Media ─────────────────────────────────────────────────────────────
  // ── 媒体 ─────────────────────────────────────────────────────────────
  GET_MEDIA_DETAIL: 'get_media_detail',
  GET_ADJACENT_MEDIA: 'get_adjacent_media',
  GET_META_FOR_VIEWPORT: 'get_meta_for_viewport',
  GET_COMPANION_VIDEO_URL: 'get_companion_video_url',
  GET_KEYFRAME_SPRITE: 'get_keyframe_sprite',
  GET_AUDIO_DETAIL: 'get_audio_detail',
  TOGGLE_FAVORITE: 'toggle_favorite',
  // 批量设/取消收藏（选区批量操作用）。
  BATCH_TOGGLE_FAVORITE: 'batch_toggle_favorite',
  SET_RATING: 'set_rating',
  // 批量评分（0-5,选区键盘 1-5 快捷评分用,单条 UPDATE+IN,避免逐项 N 次 IPC）。
  BATCH_SET_RATING: 'batch_set_rating',
  // 颜色标签（0=无 / 1-7 色档,T16）。set 单项,batch 选区批量(单条 UPDATE+IN)。
  SET_COLOR_LABEL: 'set_color_label',
  BATCH_SET_COLOR_LABEL: 'batch_set_color_label',
  SOFT_DELETE_ITEMS: 'soft_delete_items',
  RESTORE_ITEMS: 'restore_items',
  GET_TRASH: 'get_trash',
  GET_STATS: 'get_stats',
  GET_DIRECTORY_TREE: 'get_directory_tree',
  GET_DIRECTORY_CHILDREN: 'get_directory_children',
  // 由目标项 id 反查其目录祖先链（定位/展开到指定项用）。
  GET_DIRECTORY_ANCESTORS: 'get_directory_ancestors',
  LIST_DIRECTORY_FILES: 'list_directory_files',

  // ── Thumbnail ─────────────────────────────────────────────────────────
  // ── 缩略图 ─────────────────────────────────────────────────────────
  BATCH_REQUEST_THUMBNAILS: 'batch_request_thumbnails',
  REQUEST_THUMBNAIL: 'request_thumbnail',
  START_FULL_THUMBNAIL_GENERATION: 'start_full_thumbnail_generation',
  STOP_FULL_THUMBNAIL_GENERATION: 'stop_full_thumbnail_generation',
  CANCEL_THUMBNAIL_REQUEST: 'cancel_thumbnail_request',
  // 优先测量给定项的真实尺寸（滚动到锚点前抢先算其 y，避免跳动）。
  PRIORITIZE_DIMENSIONS: 'prioritize_dimensions',
  // 清空全部缩略图缓存。
  CLEAR_ALL_THUMBNAILS: 'clear_all_thumbnails',

  // ── Derivation pipeline (P2/P3/P4, §3.2/§3.3/§3.6) ─────────────────────
  // ── 派生流水线（视频封面/关键帧、音频封面、epub 封面） ────────────────────
  START_DERIVATION: 'start_derivation',
  PAUSE_DERIVATION: 'pause_derivation',
  STOP_DERIVATION: 'stop_derivation',
  DERIVATION_STATUS: 'derivation_status',

  // ── Collections (favorites, 需求7) ─────────────────────────────────────
  // ── 收藏夹（需求7） ─────────────────────────────────────────────────────
  LIST_COLLECTIONS: 'list_collections',
  RECENT_COLLECTIONS: 'recent_collections',
  CREATE_COLLECTION: 'create_collection',
  DELETE_COLLECTION: 'delete_collection',
  RENAME_COLLECTION: 'rename_collection',
  ADD_TO_COLLECTION: 'add_to_collection',
  REMOVE_FROM_COLLECTION: 'remove_from_collection',

  // ── Documents (P4, §3.4/§3.5) ─────────────────────────────────────────
  // ── 文档（P4） ─────────────────────────────────────────────────────────
  ENSURE_DOC_THUMB_QUEUE: 'ensure_doc_thumb_queue',
  LIST_PENDING_DOC_THUMBS: 'list_pending_doc_thumbs',
  STORE_DOC_THUMBNAIL: 'store_doc_thumbnail',
  GET_READING_PROGRESS: 'get_reading_progress',
  SET_READING_PROGRESS: 'set_reading_progress',
  LIST_REPLACEMENTS: 'list_replacements',
  GET_EFFECTIVE_REPLACEMENTS: 'get_effective_replacements',
  UPSERT_REPLACEMENT: 'upsert_replacement',
  DELETE_REPLACEMENT: 'delete_replacement',
  LIST_VERSIONS: 'list_versions',
  GET_CURRENT_VERSION: 'get_current_version',
  GET_DOCUMENT_TEXT: 'get_document_text',
  GET_VERSION_CONTENT: 'get_version_content',
  SAVE_VERSION: 'save_version',
  SET_CURRENT_VERSION: 'set_current_version',
  DELETE_VERSION: 'delete_version',
  DIFF_VERSIONS: 'diff_versions',
  DIFF_TEXTS: 'diff_texts',
  GET_PROOFREAD_CONFIG: 'get_proofread_config',
  SET_PROOFREAD_CONFIG: 'set_proofread_config',
  SET_PROOFREAD_KEY: 'set_proofread_key',
  CLEAR_PROOFREAD_KEY: 'clear_proofread_key',
  PROOFREAD_CHUNK: 'proofread_chunk',

  // ── Storage backends (network drives, 需求8 8B, §3.8) ──────────────────
  // ── 存储后端（网络盘, 需求8 8B, §3.8） ─────────────────────────────────
  LIST_BACKENDS: 'list_backends',
  ADD_BACKEND: 'add_backend',
  TEST_BACKEND: 'test_backend',
  REMOVE_BACKEND: 'remove_backend',

  // ── Search ────────────────────────────────────────────────────────────
  // ── 搜索 ────────────────────────────────────────────────────────────
  SEARCH_MEDIA: 'search_media',

  // ── Config ────────────────────────────────────────────────────────────
  // ── 配置 ────────────────────────────────────────────────────────────
  GET_APP_CONFIG: 'get_app_config',
  SET_APP_CONFIG: 'set_app_config',
  // 启动时一次性取多项配置（合并 4 次 get_app_config 为 1 次往返）。
  GET_STARTUP_CONFIG: 'get_startup_config',
  // 应用日志目录路径（设置页"打开日志目录"用）。
  GET_LOG_DIR: 'get_log_dir',

  // ── File ops ──────────────────────────────────────────────────────────
  // ── 文件操作 ──────────────────────────────────────────────────────────
  MOVE_MEDIA_ITEMS: 'move_media_items',
  COPY_MEDIA_ITEMS: 'copy_media_items',
  RELOCATE_MEDIA_ITEMS: 'relocate_media_items',
  COPY_MEDIA_ITEMS_DB: 'copy_media_items_db',
  REMOVE_MEDIA_ITEMS_HARD: 'remove_media_items_hard',
  MOVE_DIRECTORY: 'move_directory',
  COPY_DIRECTORY: 'copy_directory',
  DELETE_DIRECTORY_TO_TRASH: 'delete_directory_to_trash',
  // 在磁盘上新建物理文件夹（侧栏新建文件夹对话框用）。
  CREATE_PHYSICAL_FOLDER: 'create_physical_folder',

  // ── System ────────────────────────────────────────────────────────────
  // ── 系统 ────────────────────────────────────────────────────────────
  SHOW_IN_EXPLORER: 'show_in_explorer',
  MOVE_TO_TRASH: 'move_to_trash',
  // 用系统文件管理器打开指定目录。
  OPEN_DIRECTORY: 'open_directory',
  // 关闭启动闪屏并显示主窗口（前端 onMounted 就绪后调用）。
  CLOSE_SPLASHSCREEN: 'close_splashscreen',
  // 隐藏主窗口（关闭行为=最小化到托盘时）。
  HIDE_WINDOW: 'hide_window',
  // 退出应用进程。
  EXIT_APP: 'exit_app',
  // 清空应用日志文件。
  CLEAR_LOGS: 'clear_logs',
  // 把指定图片复制到系统剪贴板。
  COPY_IMAGE_TO_CLIPBOARD: 'copy_image_to_clipboard',
  // 把指定图片设为桌面壁纸。
  SET_AS_WALLPAPER: 'set_as_wallpaper',

  // ── AI ────────────────────────────────────────────────────────────────
  // ── AI ────────────────────────────────────────────────────────────────
  DETECT_AI_PROVIDER: 'detect_ai_provider',
  GET_AI_STATUS: 'get_ai_status',
  SEMANTIC_SEARCH_CMD: 'semantic_search_cmd',
  START_AI_ANALYSIS: 'start_ai_analysis',
  STOP_AI_ANALYSIS: 'stop_ai_analysis',
  PAUSE_AI_ANALYSIS: 'pause_ai_analysis',
  RESTART_AI_ANALYSIS: 'restart_ai_analysis',
  REBUILD_EMBEDDINGS: 'rebuild_embeddings',
  LIST_AI_MODELS: 'list_ai_models',
  IMPORT_AI_MODEL: 'import_ai_model',
  RELOAD_AI_ENGINE: 'reload_ai_engine',
  LIST_MODEL_REGISTRY: 'list_model_registry',
  SET_ACTIVE_MODEL: 'set_active_model',
  DOWNLOAD_MODEL: 'download_model',

  // ── Face（人脸分析 / 人物管理）─────────────────────────────────────────
  // ── 人脸 ─────────────────────────────────────────────────────────────
  START_FACE_ANALYSIS: 'start_face_analysis',
  STOP_FACE_ANALYSIS: 'stop_face_analysis',
  PAUSE_FACE_ANALYSIS: 'pause_face_analysis',
  RESTART_FACE_ANALYSIS: 'restart_face_analysis',
  GET_FACE_STATUS: 'get_face_status',
  GET_ITEM_FACES: 'get_item_faces',
  LIST_FACE_PERSONS: 'list_face_persons',
  RENAME_FACE_PERSON: 'rename_face_person',
  MERGE_FACE_PERSONS: 'merge_face_persons',
  SET_FACE_PERSON_HIDDEN: 'set_face_person_hidden',
  RECLUSTER_FACES: 'recluster_faces',
  LIST_FACE_MODEL_REGISTRY: 'list_face_model_registry',
  DOWNLOAD_FACE_MODEL: 'download_face_model',
  // 批量审批（T10）：likely-match 分组 + 整组确认/改派/移出/拒绝/建新人物。
  LIST_LIKELY_FACE_MATCHES: 'list_likely_face_matches',
  CONFIRM_FACES: 'confirm_faces',
  REASSIGN_FACES: 'reassign_faces',
  UNASSIGN_FACES: 'unassign_faces',
  REJECT_FACES: 'reject_faces',
  CREATE_PERSON: 'create_person',

  // ── Exotic 插件平台（Part5 T11/T12，消费 Part6）────────────────────────
  // ── Exotic plugin platform (Part5 T11/T12, consuming Part6) ────────────
  // 某插件的授权判定（gate / 购买引导用）；判定全在后端 EntitlementProvider，前端不持验签逻辑。
  GET_PLUGIN_ENTITLEMENT: 'get_plugin_entitlement',
  // 单个媒体项的 exotic 状态（可用态 + 任务态）；resolution=null 即普通格式，触点据此决定是否 gate。
  GET_EXOTIC_ITEM_STATE: 'get_exotic_item_state',
  // Catalog 全部格式解析（前端据此缓存"哪些格式属 exotic"，避免为普通格式空跑 item-state IPC）。
  LIST_EXOTIC_FORMAT_RESOLUTIONS: 'list_exotic_format_resolutions',
  // 激活插件：用可信 Catalog 的 sku 验证 token→存 keyring。参数只接受 pluginId+token（后端红线）。
  ACTIVATE_EXOTIC_PLUGIN: 'activate_exotic_plugin',
  // 移除授权（卸载时的独立操作，不影响安装目录）。
  DEACTIVATE_EXOTIC_PLUGIN: 'deactivate_exotic_plugin',
  // ── 插件商店（T11）：registry 浏览 / 安装生命周期 / 处理进度 ──────────────
  // 拉取远程签名 Registry（验签+防回滚+原子写缓存）；返回本次可装条目摘要。
  FETCH_EXOTIC_REGISTRY: 'fetch_exotic_registry',
  // 列出本地缓存的可安装条目（无缓存→空列表）。
  LIST_EXOTIC_REGISTRY: 'list_exotic_registry',
  // 列出已安装插件（安装真相投影）。
  LIST_INSTALLED_EXOTIC_PLUGINS: 'list_installed_exotic_plugins',
  // 安装 / 卸载 / 修复 / 回滚（参数只接受已校验 pluginId，绝不接受 URL/路径/hash——后端红线）。
  INSTALL_EXOTIC_PLUGIN: 'install_exotic_plugin',
  UNINSTALL_EXOTIC_PLUGIN: 'uninstall_exotic_plugin',
  REPAIR_EXOTIC_PLUGIN: 'repair_exotic_plugin',
  ROLLBACK_EXOTIC_PLUGIN: 'rollback_exotic_plugin',
  // 处理进度摘要 + 控制（恢复/暂停/停止本次运行/重试某插件全部失败）。
  GET_EXOTIC_PROCESSING_STATUS: 'get_exotic_processing_status',
  START_EXOTIC_PROCESSING: 'start_exotic_processing',
  PAUSE_EXOTIC_PROCESSING: 'pause_exotic_processing',
  STOP_EXOTIC_PROCESSING: 'stop_exotic_processing',
  RETRY_EXOTIC_PLUGIN_FAILURES: 'retry_exotic_plugin_failures',

  // ── Volume（已知卷面板，T13 离线 UX）──────────────────────────────────
  // 列出已知卷（含在线态 + 媒体数）/ 重命名 / 忘记（FK SET NULL，媒体保留仅解绑）。
  LIST_VOLUMES: 'list_volumes',
  RENAME_VOLUME: 'rename_volume',
  FORGET_VOLUME: 'forget_volume',

  // ── Window / Thumbnail cache（窗口主题 / 缩略图缓存目录）────────────────
  SET_WINDOW_THEME: 'set_window_theme',
  GET_THUMB_CACHE_DIR: 'get_thumb_cache_dir',
} as const

// ── Tauri events ──────────────────────────────────────────────────────────
// ── Tauri 事件 ──────────────────────────────────────────────────────────
export const EVENTS = {
  MEDIA_ENRICHED: 'db:media_enriched',
  ENRICHMENT_COMPLETED: 'enrichment:completed',
  MEDIA_UPDATED: 'db:media_updated',
  /** 卷插拔监听（Part2 T2）：卷在线态变化 → 画廊刷新离线徽标显隐。 */
  VOLUMES_CHANGED: 'volumes:changed',
} as const
