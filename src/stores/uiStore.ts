// src/stores/uiStore.ts
// Global UI state — persisted to app_config where noted.
// 全局 UI 状态 — 在有说明的地方持久化到 app_config。

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { Theme, SmartAlbum, ToastMessage } from '../types/ui'
import { IPC } from '../constants/ipc'
import i18n from '../i18n'

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
      i18n.global.locale.value = lang as any
    }
  }

  function setLanguage(lang: string) {
    applyLanguage(lang)
    invoke(IPC.SET_APP_CONFIG, { key: 'language', value: lang }).catch(console.error)
  }

  function applyTheme(t: Theme) {
    const resolved = t === 'system'
      ? (systemIsDark.value ? 'dark' : 'light')
      : t
    document.documentElement.setAttribute('data-theme', resolved)
    
    // Synchronize native window titlebar theme using our custom Rust IPC command
    // 强制同步原生窗口标题栏主题，绕过 Tauri 可能存在的无响应 BUG
    invoke('set_window_theme', { theme: t, resolved }).catch(() => {})
  }

  function setTheme(t: Theme) {
    theme.value = t
    applyTheme(t)
    invoke(IPC.SET_APP_CONFIG, { key: 'theme', value: t }).catch(console.error)
  }

  function cycleTheme() {
    setTheme(isDark.value ? 'light' : 'dark')
  }

  // ── Thumbnail Strategy ───────────────────────────────────────────────────
  // ── 缩略图生成策略 ────────────────────────────────────────────────────────
  const thumbStrategy = ref<string>('cpu')
  const gpuEngine = ref<string>('wic')

  function setThumbStrategy(strategy: string) {
    thumbStrategy.value = strategy
    invoke(IPC.SET_APP_CONFIG, { key: 'thumb_strategy', value: strategy }).catch(console.error)
  }

  function setGpuEngine(engine: string) {
    gpuEngine.value = engine
    invoke(IPC.SET_APP_CONFIG, { key: 'gpu_engine', value: engine }).catch(console.error)
  }

  // ── Sidebar ────────────────────────────────────────────────────────────
  // ── 侧边栏 ────────────────────────────────────────────────────────────
  const sidebarWidth = ref(260)
  const sidebarCollapsed = ref(false)

  function setSidebarWidth(w: number) {
    sidebarWidth.value = Math.max(180, Math.min(400, w))
    document.documentElement.style.setProperty('--sidebar-width', `${sidebarWidth.value}px`)
  }

  function persistSidebarWidth() {
    invoke(IPC.SET_APP_CONFIG, { key: 'sidebar_width', value: String(sidebarWidth.value) })
      .catch(console.error)
  }

  // ── Active view ────────────────────────────────────────────────────────
  // ── 当前视图 ────────────────────────────────────────────────────────
  const activeSmartAlbum = ref<SmartAlbum>('all')
  const activeDirectoryId = ref<number | null>(null)

  function setSmartAlbum(album: SmartAlbum) {
    activeSmartAlbum.value = album
    activeDirectoryId.value = null
  }

  function setActiveDirectory(id: number | null) {
    activeDirectoryId.value = id
    activeSmartAlbum.value  = 'all'
  }

  // ── Sort ───────────────────────────────────────────────────────────────
  // ── 排序 ───────────────────────────────────────────────────────────────
  const sortOrder = ref<'asc' | 'desc'>('desc')

  // ── Grid Display Settings ────────────────────────────────────────────────
  // ── 网格显示设置 ────────────────────────────────────────────────
  const gridRowHeight = ref(200)

  function setGridRowHeight(h: number) {
    gridRowHeight.value = h
    invoke(IPC.SET_APP_CONFIG, { key: 'grid_row_height', value: String(h) }).catch(console.error)
  }

  // Load saved grid row height
  invoke<string | null>(IPC.GET_APP_CONFIG, { key: 'grid_row_height' })
    .then(savedRowHeight => {
      if (savedRowHeight) {
        gridRowHeight.value = parseInt(savedRowHeight, 10) || 200
      }
    }).catch(console.error)

  // ── Group and sort settings ──────────────────────────────────────────────
  // ── 分组和排序设置 ──────────────────────────────────────────────
  const groupBy = ref<'date' | 'folder' | 'none'>('date')
  const sortWithinGroup = ref<'datetime' | 'filename' | 'similarity'>('datetime')

  function setGroupBy(mode: 'date' | 'folder' | 'none') {
    groupBy.value = mode
    invoke(IPC.SET_APP_CONFIG, { key: 'group_by', value: mode }).catch(console.error)
  }

  function setSortWithinGroup(sort: 'datetime' | 'filename' | 'similarity') {
    sortWithinGroup.value = sort
    invoke(IPC.SET_APP_CONFIG, { key: 'sort_within_group', value: sort }).catch(console.error)
  }

  // Load saved group and sort settings
  invoke<string | null>(IPC.GET_APP_CONFIG, { key: 'group_by' })
    .then(saved => { if (saved) groupBy.value = saved as any }).catch(console.error)
  invoke<string | null>(IPC.GET_APP_CONFIG, { key: 'sort_within_group' })
    .then(saved => { if (saved) sortWithinGroup.value = saved as any }).catch(console.error)

  // ── Toasts ─────────────────────────────────────────────────────────────
  // ── 提示框 ─────────────────────────────────────────────────────────────
  const toasts = ref<ToastMessage[]>([])
  let toastSeq = 0

  function addToast(type: ToastMessage['type'], message: string, duration = 3000) {
    const id = `toast-${++toastSeq}`
    toasts.value.push({ id, type, message, duration })
    setTimeout(() => removeToast(id), duration + 300)
  }

  function removeToast(id: string) {
    const idx = toasts.value.findIndex(t => t.id === id)
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
  const pendingScrollLabel = ref<string | null>(null)
  const scrolledDirectoryId = ref<number | null>(null)

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

  invoke<string | null>(IPC.GET_APP_CONFIG, { key: 'close_behavior' })
    .then(saved => {
      if (saved && ['ask', 'minimize_to_tray', 'exit'].includes(saved)) {
        closeBehavior.value = saved as any
      }
    }).catch(console.error)

  function setCloseBehavior(behavior: 'ask' | 'minimize_to_tray' | 'exit') {
    closeBehavior.value = behavior
    invoke(IPC.SET_APP_CONFIG, { key: 'close_behavior', value: behavior }).catch(console.error)
  }

  // ── Settings ───────────────────────────────────────────────────────────
  const isSettingsOpen = ref(false)

  // ── Pinned Settings ──────────────────────────────────────────────────────
  const pinnedSettings = ref<string[]>([])
  
  function togglePinnedSetting(key: string) {
    const idx = pinnedSettings.value.indexOf(key)
    if (idx >= 0) {
      pinnedSettings.value.splice(idx, 1)
    } else {
      pinnedSettings.value.push(key)
    }
    invoke(IPC.SET_APP_CONFIG, { key: 'pinned_settings', value: JSON.stringify(pinnedSettings.value) }).catch(console.error)
  }

  invoke<string | null>(IPC.GET_APP_CONFIG, { key: 'pinned_settings' })
    .then(saved => {
      if (saved) {
        try { pinnedSettings.value = JSON.parse(saved) } catch {}
      }
    }).catch(console.error)

  // ── Thumbnail Info Overlays ──────────────────────────────────────────────
  const showThumbInfo = ref<boolean>(false)
  const thumbInfoElements = ref<string[]>([])

  function setShowThumbInfo(val: boolean) {
    showThumbInfo.value = val
    invoke(IPC.SET_APP_CONFIG, { key: 'show_thumb_info', value: String(val) }).catch(console.error)
  }

  function setThumbInfoElements(elements: string[]) {
    thumbInfoElements.value = elements
    invoke(IPC.SET_APP_CONFIG, { key: 'thumb_info_elements', value: JSON.stringify(elements) }).catch(console.error)
  }

  invoke<string | null>(IPC.GET_APP_CONFIG, { key: 'show_thumb_info' })
    .then(saved => {
      if (saved) showThumbInfo.value = saved === 'true'
    }).catch(console.error)

  invoke<string | null>(IPC.GET_APP_CONFIG, { key: 'thumb_info_elements' })
    .then(saved => {
      if (saved) {
        try { thumbInfoElements.value = JSON.parse(saved) } catch {}
      }
    }).catch(console.error)

  return {
    // theme & language
    // 主题与语言
    theme, isDark, setTheme, cycleTheme, applyTheme,
    language, applyLanguage, setLanguage,
    // thumbnail strategy
    // 缩略图生成策略
    thumbStrategy, setThumbStrategy,
    gpuEngine, setGpuEngine,
    // sidebar
    // 侧边栏
    sidebarWidth, sidebarCollapsed, setSidebarWidth, persistSidebarWidth,
    // view
    // 视图
    activeSmartAlbum, activeDirectoryId, setSmartAlbum, setActiveDirectory,
    // grid display
    // 网格显示
    gridRowHeight, setGridRowHeight,
    // grouping & sorting
    // 分组和排序
    groupBy, setGroupBy, sortWithinGroup, setSortWithinGroup,
    // sort
    // 排序
    sortOrder,
    // toasts
    // 提示框
    toasts, addToast, removeToast,
    // search
    // 搜索
    searchQuery, searchScope, isSearching,
    // loading
    // 加载
    isLayoutLoading,
    // scroll target
    pendingScrollLabel, scrolledDirectoryId,
    // fullscreen
    // 全屏
    isFullscreen, initFullscreen, toggleFullscreen,
    // close behavior
    closeBehavior, showCloseConfirmDialog, setCloseBehavior,
    // settings
    isSettingsOpen,
    // pinned settings
    pinnedSettings, togglePinnedSetting,
    // thumb info
    showThumbInfo, setShowThumbInfo,
    thumbInfoElements, setThumbInfoElements,
  }
})
