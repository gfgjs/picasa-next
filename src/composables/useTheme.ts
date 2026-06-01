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
    const t: Theme = (saved as Theme) ?? 'dark'
    ui.theme = t
    ui.applyTheme(t)

    // Watch system preference changes
    if (t === 'system') {
      window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
        if (ui.theme === 'system') ui.applyTheme('system')
      })
    }
  }

  onMounted(init)

  return {
    theme:       ui.theme,
    cycleTheme:  ui.cycleTheme,
    setTheme:    ui.setTheme,
  }
}
