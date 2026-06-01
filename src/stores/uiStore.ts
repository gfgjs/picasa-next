// src/stores/uiStore.ts
// Global UI state — persisted to app_config where noted.

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { Theme, SmartAlbum, ToastMessage } from '../types/ui'
import { IPC } from '../constants/ipc'

export const useUiStore = defineStore('ui', () => {
  // ── Theme ──────────────────────────────────────────────────────────────
  const theme = ref<Theme>('dark')

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

  // ── Sidebar ────────────────────────────────────────────────────────────
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
  const sortBy    = ref<string>('sort_datetime')
  const sortOrder = ref<'asc' | 'desc'>('desc')

  // ── Toasts ─────────────────────────────────────────────────────────────
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
  const searchQuery = ref('')
  const isSearching = ref(false)

  // ── Loading states ─────────────────────────────────────────────────────
  const isLayoutLoading = ref(false)

  return {
    // theme
    theme, setTheme, cycleTheme, applyTheme,
    // sidebar
    sidebarWidth, sidebarCollapsed, setSidebarWidth, persistSidebarWidth,
    // view
    activeSmartAlbum, activeDirectoryId, setSmartAlbum, setActiveDirectory,
    // sort
    sortBy, sortOrder,
    // toasts
    toasts, addToast, removeToast,
    // search
    searchQuery, isSearching,
    // loading
    isLayoutLoading,
  }
})
