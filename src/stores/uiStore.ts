// src/stores/uiStore.ts
// Global UI state — persisted to app_config where noted.
// 全局 UI 状态 — 在有说明的地方持久化到 app_config。

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { Theme, SmartAlbum, ToastMessage, ToastAction } from '../types/ui'
import type { Collection } from '../types/media'
import { IPC } from '../constants/ipc'
import { invokeIpc } from '../utils/ipc'
import i18n from '../i18n'
import { useSelection } from '../composables/useSelection'

// get_startup_config 的载荷(R2-4:14 键单次往返;与后端 config_commands.rs StartupConfig 同步)。
export interface StartupConfig {
  language: string | null
  timelineScrollWidth: string | null
  uiFontSize: string | null
  enableThumbHoverScale: string | null
  gridRowHeight: string | null
  groupBy: string | null
  sortWithinGroup: string | null
  layoutMode: string | null
  closeBehavior: string | null
  pinnedSettings: string | null
  showThumbInfo: string | null
  thumbInfoElements: string | null
  hoverAutoplay: string | null
  bucketSegmentedScroll: string | null
  firstLaunch: string | null
}

export const useUiStore = defineStore('ui', () => {
  // ── Theme & Language ───────────────────────────────────────────────────
  // ── 主题与语言 ──────────────────────────────────────────────────────────
  const theme = ref<Theme>('system')
  const language = ref<string>('zh-CN')

  const systemIsDark = ref(window.matchMedia('(prefers-color-scheme: dark)').matches)

  // Listen for OS theme changes globally
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
    systemIsDark.value = e.matches
    if (theme.value === 'system') {
      applyTheme('system')
    }
  })

  const isDark = computed(() => {
    if (theme.value === 'system') {
      return systemIsDark.value
    }
    return theme.value === 'dark'
  })

  function applyLanguage(lang: string) {
    language.value = lang
    document.documentElement.setAttribute('lang', lang)
    if (i18n.global.locale.value !== lang) {
      i18n.global.locale.value = lang as typeof i18n.global.locale.value
    }
  }

  function setLanguage(lang: string) {
    applyLanguage(lang)
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'language', value: lang }).catch(console.error)
  }

  function applyTheme(t: Theme) {
    const resolved = t === 'system' ? (systemIsDark.value ? 'dark' : 'light') : t
    document.documentElement.setAttribute('data-theme', resolved)

    // Synchronize native window titlebar theme using our custom Rust IPC command
    // 强制同步原生窗口标题栏主题，绕过 Tauri 可能存在的无响应 BUG
    invokeIpc(IPC.SET_WINDOW_THEME, { theme: t, resolved }).catch(() => {})
  }

  function setTheme(t: Theme) {
    theme.value = t
    applyTheme(t)
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'theme', value: t }).catch(console.error)
  }

  function cycleTheme() {
    setTheme(isDark.value ? 'light' : 'dark')
  }

  // 注：thumbStrategy / gpuEngine 此前在此双持（configStore 也持有并镜像至此），但 uiStore 这份
  // 只被写、从不被读——已删，单一来源归 configStore（S5/T19 去重）。

  // ── Sidebar ────────────────────────────────────────────────────────────
  // ── 侧边栏 ────────────────────────────────────────────────────────────
  const sidebarWidth = ref(260)
  const sidebarCollapsed = ref(false)

  function setSidebarWidth(w: number) {
    sidebarWidth.value = Math.max(180, Math.min(400, w))
    document.documentElement.style.setProperty('--sidebar-width', `${sidebarWidth.value}px`)
  }

  function persistSidebarWidth() {
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'sidebar_width', value: String(sidebarWidth.value) }).catch(
      console.error,
    )
  }

  // ── Active view ────────────────────────────────────────────────────────
  // ── 当前视图 ────────────────────────────────────────────────────────
  const activeSmartAlbum = ref<SmartAlbum>('all')
  const activeDirectoryId = ref<number | null>(null)
  // Currently-opened collection (favorites folder), drives the grid filter when set.
  // Mutually exclusive with smart-album / directory views — opening any clears the others.
  // 当前打开的收藏夹，设置后驱动网格过滤。与智能相册/目录视图互斥——打开任一即清除其它。
  const activeCollection = ref<Collection | null>(null)
  // Currently-viewed person cluster (F6 people wall → person's photos). Fourth mutually-exclusive
  // view filter alongside smart-album / directory / collection.
  // 当前查看的人物簇（F6 人物墙 → 某人物的照片）。与 智能相册/目录/收藏夹 并列的第四个互斥视图筛选。
  const activePersonId = ref<number | null>(null)

  // 视图上下文切换即清空照片多选集（问题3，业界相册成熟做法）。选区是 useSelection 模块级单例，
  // 与具体视图无关，切换过滤维度后旧选区无意义且易误操作 → 统一在四个切视图入口清掉。
  const { clearSelection } = useSelection()

  function setSmartAlbum(album: SmartAlbum) {
    activeSmartAlbum.value = album
    activeDirectoryId.value = null
    activeCollection.value = null
    activePersonId.value = null
    clearSelection()
  }

  function setActiveDirectory(id: number | null) {
    activeDirectoryId.value = id
    activeSmartAlbum.value = 'all'
    activeCollection.value = null
    activePersonId.value = null
    clearSelection()
  }

  function setActiveCollection(c: Collection | null) {
    activeCollection.value = c
    activeSmartAlbum.value = 'all'
    activeDirectoryId.value = null
    activePersonId.value = null
    clearSelection()
  }

  function setActivePerson(id: number | null) {
    activePersonId.value = id
    activeSmartAlbum.value = 'all'
    activeDirectoryId.value = null
    activeCollection.value = null
    clearSelection()
  }

  // ── Sort ───────────────────────────────────────────────────────────────
  // ── 排序 ───────────────────────────────────────────────────────────────
  const sortOrder = ref<'asc' | 'desc'>('desc')

  // ── Grid Display Settings ────────────────────────────────────────────────
  // ── 网格显示设置 ────────────────────────────────────────────────
  const gridRowHeight = ref(200)

  function setGridRowHeight(h: number) {
    gridRowHeight.value = h
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'grid_row_height', value: String(h) }).catch(console.error)
  }

  // ── Group and sort settings ──────────────────────────────────────────────
  // ── 分组和排序设置 ──────────────────────────────────────────────
  const groupBy = ref<'date' | 'folder' | 'none'>('date')
  const sortWithinGroup = ref<'datetime' | 'filename' | 'similarity'>('datetime')

  function setGroupBy(mode: 'date' | 'folder' | 'none') {
    groupBy.value = mode
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'group_by', value: mode }).catch(console.error)
  }

  function setSortWithinGroup(sort: 'datetime' | 'filename' | 'similarity') {
    sortWithinGroup.value = sort
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'sort_within_group', value: sort }).catch(console.error)
  }

  // ── Layout mode（T20）：'justified' 等高行（默认）/ 'grid' 均匀宫格 ───────────────
  // 后端按此切换排版算法（compute_layout 的 layoutMode 参数）；前端据此切单元方图裁切。
  const layoutMode = ref<'justified' | 'grid'>('justified')

  function setLayoutMode(mode: 'justified' | 'grid') {
    layoutMode.value = mode
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'layout_mode', value: mode }).catch(console.error)
  }

  // ── Bucket 分段虚拟滚动(T16 方案B;B0-B3.2 真机验收后转默认引擎)──────────────
  // 开(默认)= 画廊用 bucket 分段引擎(零坐标平移,useBucketVirtualScroll + 自研逻辑
  // 滚动条);关 = 回退方案 A 线性平移。运行时即切即生效(MediaGrid 双引擎互斥)。
  const bucketSegmentedScroll = ref<boolean>(true)

  function setBucketSegmentedScroll(val: boolean) {
    bucketSegmentedScroll.value = val
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'bucket_segmented_scroll', value: String(val) }).catch(
      console.error,
    )
  }

  // ── Toasts ─────────────────────────────────────────────────────────────
  // ── 提示框 ─────────────────────────────────────────────────────────────
  const toasts = ref<ToastMessage[]>([])
  let toastSeq = 0

  function addToast(
    type: ToastMessage['type'],
    message: string,
    duration = 3000,
    actions?: ToastAction[],
  ) {
    const id = `toast-${++toastSeq}`
    toasts.value.push({ id, type, message, duration, actions })
    setTimeout(() => removeToast(id), duration + 300)
  }

  function removeToast(id: string) {
    const idx = toasts.value.findIndex((t) => t.id === id)
    if (idx >= 0) toasts.value.splice(idx, 1)
  }

  // ── Search ─────────────────────────────────────────────────────────────
  // ── 搜索 ─────────────────────────────────────────────────────────────
  const searchQuery = ref('')
  const searchScope = ref<string>('filename')
  const isSearching = ref(false)

  // ── Loading states ─────────────────────────────────────────────────────
  // ── 加载状态 ─────────────────────────────────────────────────────
  const isLayoutLoading = ref(false)

  // ── Scroll Target ──────────────────────────────────────────────────────
  // ── 滚动目标 ────────────────────────────────────────────────────────
  // Directory id to scroll the gallery to (set when a folder is clicked in the
  // sidebar). Using the unique id — not the name — so duplicate-named folders
  // jump to the right place. | 要滚动到的目录 id（点击侧边栏文件夹时设置）。
  // 用唯一 id 而非名字，使同名文件夹也能跳到正确位置。
  const pendingScrollDirId = ref<number | null>(null)
  const scrolledDirectoryId = ref<number | null>(null)

  // ── Media drag-to-folder ──────────────────────────────────────────────────
  // ── 媒体拖到文件夹 ────────────────────────────────────────────────────────
  // Directory id currently hovered while dragging gallery media onto the folder tree.
  // Set by MediaGrid during the drag; read by FoldersSection to highlight the drop
  // target (the drag starts in the grid but the target lives in the sidebar — 问题5).
  // 拖动画廊媒体到文件夹树时当前悬停的目录 id。MediaGrid 拖拽时设置；FoldersSection 读取以
  // 高亮放置目标（拖拽始于画廊、目标在侧栏 —— 问题5）。
  const mediaDragHoverDirId = ref<number | null>(null)

  // ── Fullscreen ─────────────────────────────────────────────────────────
  // ── 全屏 ─────────────────────────────────────────────────────────
  const isFullscreen = ref(false)

  async function initFullscreen() {
    try {
      const appWindow = getCurrentWindow()
      isFullscreen.value = await appWindow.isFullscreen()
    } catch {
      isFullscreen.value = !!document.fullscreenElement
    }
  }

  async function toggleFullscreen() {
    try {
      const appWindow = getCurrentWindow()
      const current = await appWindow.isFullscreen()
      await appWindow.setFullscreen(!current)
      isFullscreen.value = !current
    } catch {
      if (!document.fullscreenElement) {
        document.documentElement.requestFullscreen().catch(console.error)
        isFullscreen.value = true
      } else {
        document.exitFullscreen().catch(console.error)
        isFullscreen.value = false
      }
    }
  }

  // ── Close Behavior ───────────────────────────────────────────────────────
  const closeBehavior = ref<'ask' | 'minimize_to_tray' | 'exit'>('ask')
  const showCloseConfirmDialog = ref(false)

  function setCloseBehavior(behavior: 'ask' | 'minimize_to_tray' | 'exit') {
    closeBehavior.value = behavior
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'close_behavior', value: behavior }).catch(console.error)
  }

  // ── Settings ───────────────────────────────────────────────────────────
  const isSettingsOpen = ref(false)

  // ── Pinned Settings ──────────────────────────────────────────────────────
  const pinnedSettings = ref<string[]>([])

  // The "全量 AI 分析" tool is a permanent pinned entry (not a Settings-page item),
  // rendered specially. We keep it inside `pinnedSettings` so it can be drag-sorted
  // together with the other tools.
  // 「全量 AI 分析」是常驻置顶项（非设置页条目），特殊渲染。将其纳入 `pinnedSettings`
  // 以便与其他工具一起拖拽排序。
  const AI_FULL_ANALYSIS_KEY = 'aiFullAnalysis'
  // 「全量人脸识别」同为常驻置顶项（F5），与 AI 分析并列、可一起拖拽排序。
  const FACE_FULL_ANALYSIS_KEY = 'faceFullAnalysis'

  function persistPinned() {
    invokeIpc(IPC.SET_APP_CONFIG, {
      key: 'pinned_settings',
      value: JSON.stringify(pinnedSettings.value),
    }).catch(console.error)
  }

  function togglePinnedSetting(key: string) {
    const idx = pinnedSettings.value.indexOf(key)
    if (idx >= 0) {
      pinnedSettings.value.splice(idx, 1)
    } else {
      pinnedSettings.value.push(key)
    }
    persistPinned()
  }

  // Move a pinned tool from one position to another (drag-sort) and persist.
  // 将置顶工具从一个位置移动到另一个位置（拖拽排序）并持久化。
  function reorderPinnedSetting(fromIndex: number, toIndex: number) {
    const arr = pinnedSettings.value
    if (
      fromIndex < 0 ||
      fromIndex >= arr.length ||
      toIndex < 0 ||
      toIndex >= arr.length ||
      fromIndex === toIndex
    )
      return
    const [moved] = arr.splice(fromIndex, 1)
    arr.splice(toIndex, 0, moved)
    persistPinned()
  }

  // ── Thumbnail Info Overlays ──────────────────────────────────────────────
  const showThumbInfo = ref<boolean>(false)
  const thumbInfoElements = ref<string[]>([])

  function setShowThumbInfo(val: boolean) {
    showThumbInfo.value = val
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'show_thumb_info', value: String(val) }).catch(console.error)
  }

  function setThumbInfoElements(elements: string[]) {
    thumbInfoElements.value = elements
    invokeIpc(IPC.SET_APP_CONFIG, {
      key: 'thumb_info_elements',
      value: JSON.stringify(elements),
    }).catch(console.error)
  }

  // ── Hover auto-play (需求1) ───────────────────────────────────────────────
  // ── 悬停自动播放（需求1） ──────────────────────────────────────────────────
  // 鼠标移入视频/动态照片格子 → 自动静音循环预览。默认开启，持久化到 app_config。
  const hoverAutoplay = ref<boolean>(true)

  function setHoverAutoplay(val: boolean) {
    hoverAutoplay.value = val
    invokeIpc(IPC.SET_APP_CONFIG, { key: 'hover_autoplay', value: String(val) }).catch(console.error)
  }

  // ── 启动配置批量读(R2-4) ────────────────────────────────────────────────
  // 原 9 处模块初始化各发一次 get_app_config(N+1);现并入 get_startup_config 单次往返。
  // promise 共享给 App.vue(其全局项 language/字号/滚动条宽 + first_launch 同批),
  // 整个启动阶段的配置 IPC 由 11 次归 1 次。各键的解析与守卫逻辑原样保留。
  const startupConfigPromise = invokeIpc<StartupConfig>(IPC.GET_STARTUP_CONFIG)
  startupConfigPromise
    .then((cfg) => {
      if (cfg.gridRowHeight) gridRowHeight.value = parseInt(cfg.gridRowHeight, 10) || 200
      if (cfg.groupBy) groupBy.value = cfg.groupBy as typeof groupBy.value
      if (cfg.sortWithinGroup)
        sortWithinGroup.value = cfg.sortWithinGroup as typeof sortWithinGroup.value
      if (cfg.layoutMode === 'grid' || cfg.layoutMode === 'justified')
        layoutMode.value = cfg.layoutMode
      if (cfg.closeBehavior && ['ask', 'minimize_to_tray', 'exit'].includes(cfg.closeBehavior)) {
        closeBehavior.value = cfg.closeBehavior as typeof closeBehavior.value
      }
      if (cfg.pinnedSettings) {
        try {
          pinnedSettings.value = JSON.parse(cfg.pinnedSettings)
        } catch {}
      }
      // Back-compat:AI/人脸全量分析常驻项确保存在(老用户的持久化列表里没有此二键)。
      if (!pinnedSettings.value.includes(AI_FULL_ANALYSIS_KEY)) {
        pinnedSettings.value.push(AI_FULL_ANALYSIS_KEY)
      }
      if (!pinnedSettings.value.includes(FACE_FULL_ANALYSIS_KEY)) {
        pinnedSettings.value.push(FACE_FULL_ANALYSIS_KEY)
      }
      if (cfg.showThumbInfo) showThumbInfo.value = cfg.showThumbInfo === 'true'
      if (cfg.thumbInfoElements) {
        try {
          thumbInfoElements.value = JSON.parse(cfg.thumbInfoElements)
        } catch {}
      }
      if (cfg.hoverAutoplay != null) hoverAutoplay.value = cfg.hoverAutoplay !== 'false'
      // 默认开(T16 转正):仅显式 'false' 才回退方案 A——历史上显式开过的 'true'
      // 与未配置的新装置都落在 bucket 引擎。
      if (cfg.bucketSegmentedScroll != null)
        bucketSegmentedScroll.value = cfg.bucketSegmentedScroll !== 'false'
    })
    .catch(console.error)

  return {
    // theme & language
    // 主题与语言
    theme,
    isDark,
    setTheme,
    cycleTheme,
    applyTheme,
    language,
    applyLanguage,
    setLanguage,
    // R2-4:共享给 App.vue 的启动配置批(单次 IPC)。
    startupConfigPromise,
    // thumbnail strategy
    // 缩略图生成策略
    // sidebar
    // 侧边栏
    sidebarWidth,
    sidebarCollapsed,
    setSidebarWidth,
    persistSidebarWidth,
    // view
    // 视图
    activeSmartAlbum,
    activeDirectoryId,
    setSmartAlbum,
    setActiveDirectory,
    activeCollection,
    setActiveCollection,
    activePersonId,
    setActivePerson,
    // grid display
    // 网格显示
    gridRowHeight,
    setGridRowHeight,
    // grouping & sorting
    // 分组和排序
    groupBy,
    setGroupBy,
    sortWithinGroup,
    setSortWithinGroup,
    // layout mode（T20）
    layoutMode,
    setLayoutMode,
    // sort
    // 排序
    sortOrder,
    // toasts
    // 提示框
    toasts,
    addToast,
    removeToast,
    // search
    // 搜索
    searchQuery,
    searchScope,
    isSearching,
    // loading
    // 加载
    isLayoutLoading,
    // scroll target
    pendingScrollDirId,
    scrolledDirectoryId,
    // media drag-to-folder
    mediaDragHoverDirId,
    // fullscreen
    // 全屏
    isFullscreen,
    initFullscreen,
    toggleFullscreen,
    // close behavior
    closeBehavior,
    showCloseConfirmDialog,
    setCloseBehavior,
    // settings
    isSettingsOpen,
    // pinned settings
    pinnedSettings,
    togglePinnedSetting,
    reorderPinnedSetting,
    // thumb info
    showThumbInfo,
    setShowThumbInfo,
    thumbInfoElements,
    setThumbInfoElements,
    // hover auto-play
    // 悬停自动播放
    hoverAutoplay,
    setHoverAutoplay,
    // bucket 分段虚拟滚动(T16 方案B)
    bucketSegmentedScroll,
    setBucketSegmentedScroll,
  }
})
