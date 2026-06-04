// src/composables/useTheme.ts
import { onMounted } from 'vue'
import { useUiStore } from '../stores/uiStore'
import { invoke } from '@tauri-apps/api/core'
import { IPC } from '../constants/ipc'
import type { Theme } from '../types/ui'

export function useTheme() {
  const ui = useUiStore()

  async function init() {
    const saved = await invoke<string | null>(IPC.GET_APP_CONFIG, { key: 'theme' }).catch(() => null)
    // No saved theme — default to 'system' so fresh installs follow OS preference
    // 未保存主题 — 默认为 'system'，全新安装时跟随系统偏好
    const t: Theme = (saved as Theme) ?? 'system'
    ui.theme = t
    ui.applyTheme(t)

    // Watch system preference changes is now handled globally inside uiStore.ts
  }

  onMounted(init)

  return {
    theme:       ui.theme,
    cycleTheme:  ui.cycleTheme,
    setTheme:    ui.setTheme,
  }
}
