<template>
  <AppShell>
    <template #sidebar>
      <AppSidebar />
    </template>

    <template #toolbar>
      <AppToolbar
        @search="onSearch"
        @sort-change="onSortChange"
      />
    </template>

    <RouterView />

    <template #statusbar>
      <AppStatusBar />
    </template>
  </AppShell>

  <!-- Detail overlay (global) -->
  <!-- 详情覆盖层（全局） -->
  <MediaDetailOverlay />

  <!-- Toast notifications -->
  <!-- 吐司通知 -->
  <ToastContainer />
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { invoke }    from '@tauri-apps/api/core'
import { useTheme }    from './composables/useTheme'
import { useUiStore }  from './stores/uiStore'

import AppShell          from './components/layout/AppShell.vue'
import AppSidebar        from './components/sidebar/AppSidebar.vue'
import AppToolbar        from './components/layout/AppToolbar.vue'
import AppStatusBar      from './components/layout/AppStatusBar.vue'
import MediaDetailOverlay from './components/media/MediaDetailOverlay.vue'
import ToastContainer    from './components/common/ToastContainer.vue'

const ui    = useUiStore()

// Init theme
// 初始化主题
useTheme()

function onSearch(query: string) {
  ui.searchQuery = query
}

function onSortChange() {
  // Trigger layout re-computation (handled via watcher in useJustifiedLayout)
  // 触发布局重新计算（通过 useJustifiedLayout 中的观察者处理）
}

onMounted(async () => {
  // Theme init only — data loading is handled in AppSidebar.vue onMounted
  // to keep initialization sequential and avoid double-compute races.
  // 仅初始化主题 — 数据加载在 AppSidebar.vue 的 onMounted 中处理，
  // 以保持初始化顺序并避免重复计算导致的竞争。
  
  // Load global UI configurations
  // 加载全局 UI 配置
  try {
    const lang = await invoke<string | null>('get_app_config', { key: 'language' })
    if (lang) {
      ui.applyLanguage(lang)
    } else {
      // Default initialized language in uiStore / i18n
      ui.applyLanguage(ui.language)
    }

    const val = await invoke<string | null>('get_app_config', { key: 'timeline_scroll_width' })
    if (val) {
      document.documentElement.style.setProperty('--scrollbar-width', `${val}px`)
    }

    const valFontSize = await invoke<string | null>('get_app_config', { key: 'ui_font_size' })
    if (valFontSize) {
      const size = parseInt(valFontSize, 10)
      const diff = size - 15
      document.documentElement.style.setProperty('--font-size-xs', `${12 + diff}px`);
      document.documentElement.style.setProperty('--font-size-sm', `${13 + diff}px`);
      document.documentElement.style.setProperty('--font-size-base', `${15 + diff}px`);
      document.documentElement.style.setProperty('--font-size-md', `${16 + diff}px`);
      document.documentElement.style.setProperty('--font-size-lg', `${19 + diff}px`);
      document.documentElement.style.setProperty('--font-size-xl', `${23 + diff}px`);
      document.documentElement.style.setProperty('--font-size-2xl', `${28 + diff}px`);
    }

    const valHoverScale = await invoke<string | null>('get_app_config', { key: 'enable_thumb_hover_scale' })
    if (valHoverScale === 'false') {
      document.documentElement.classList.add('disable-hover-scale')
    }
  } catch (e) {
    console.error('Failed to load global config:', e)
  }
})
</script>