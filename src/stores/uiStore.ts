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
      ? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
      : t
    document.documentElement.setAttribute('data-theme', resolved)
  }

  function setTheme(t: Theme) {
    theme.value = t
    applyTheme(t)
    invoke(IPC.SET_APP_CONFIG, { key: 'theme', value: t }).catch(console.error)
  }

  function cycleTheme() {
    const next: Record<Theme, Theme> = { dark: 'light', light: 'system', system: 'dark' }
    setTheme(next[theme.value])
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
  const sortBy    = ref<string>('sort_datetime')
  const sortOrder = ref<'asc' | 'desc'>('desc')

  // ── Grid row height ─────────────────────────────────────────────────────
  // ── 网格行高 ─────────────────────────────────────────────────────────────
  const gridRowHeight = ref<number>(200)

  function setGridRowHeight(h: number) {
    gridRowHeight.value = Math.max(60, Math.min(960, h))
  }

  // ── Folder group sort ───────────────────────────────────────────────────
  // ── 文件夹分组内排序 ─────────────────────────────────────────────────────
  const folderSortBy = ref<'sort_datetime' | 'file_name'>('sort_datetime')
  const folderSortOrder = ref<'asc' | 'desc'>('desc')

  function setFolderSort(by: 'sort_datetime' | 'file_name', order?: 'asc' | 'desc') {
    folderSortBy.value = by
    if (order) folderSortOrder.value = order
  }

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
  const isSearching = ref(false)

  // ── Loading states ─────────────────────────────────────────────────────
  // ── 加载状态 ─────────────────────────────────────────────────────
  const isLayoutLoading = ref(false)

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

  return {
    // theme & language
    // 主题与语言
    theme, setTheme, cycleTheme, applyTheme,
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
    // grid
    // 网格行高
    gridRowHeight, setGridRowHeight,
    // sort
    // 排序
    sortBy, sortOrder,
    // folder sort
    // 文件夹分组内排序
    folderSortBy, folderSortOrder, setFolderSort,
    // toasts
    // 提示框
    toasts, addToast, removeToast,
    // search
    // 搜索
    searchQuery, isSearching,
    // loading
    // 加载
    isLayoutLoading,
    // fullscreen
    // 全屏
    isFullscreen, initFullscreen, toggleFullscreen,
  }
})
