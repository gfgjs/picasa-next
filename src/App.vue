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

    <!-- Default slot: media content OR semantic search panel -->
    <!-- 默认插槽：媒体内容或语义搜索面板 -->
    <SemanticSearchPanel v-show="route.path === '/'" @item-click="onSemanticItemClick" />
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
import { useMediaStore } from './stores/mediaStore'

import AppShell           from './components/layout/AppShell.vue'
import AppSidebar         from './components/sidebar/AppSidebar.vue'
import AppToolbar         from './components/layout/AppToolbar.vue'
import AppStatusBar       from './components/layout/AppStatusBar.vue'
import MediaDetailOverlay from './components/media/MediaDetailOverlay.vue'
import SemanticSearchPanel from './components/media/SemanticSearchPanel.vue'
import ToastContainer     from './components/common/ToastContainer.vue'
import { useAiStore }     from './stores/aiStore'
import { useRoute }       from 'vue-router'
import type { SemanticSearchResult } from './types/ai'

const ui = useUiStore()
const ai = useAiStore()
const media = useMediaStore()
const route = useRoute()

function onSemanticItemClick(item: SemanticSearchResult) {
  // Open the detail overlay for the clicked semantic search result.
  // 为点击的语义搜索结果打开详情视图。
  media.openDetail(item.id)
}

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
  // 仅初始化主题 — 数据加载在 AppSidebar.vue 的 onMounted 中处理

  // Load global UI config in a SINGLE IPC round-trip via get_startup_config.
  // Previously 4× get_app_config calls (even in parallel) each incurred:
  //   serialisation + Tokio scheduling + r2d2 pool acquire + SQLite read + deserialisation
  // Now reduced to 1× that overhead + 4 cheap SQLite row reads on the same connection.
  //
  // 通过 get_startup_config 在单次 IPC 往返内批量获取所有启动配置。
  // 之前 4 次并行 get_app_config 各自承担：序列化 + Tokio 调度 + 连接池获取 + SQLite 读 + 反序列化
  // 现在降低为 1 次相同开销 + 同一连接上 4 次轻量 SQLite 行读取。
  try {
    const cfg = await invoke<{
      language: string | null
      timelineScrollWidth: string | null
      uiFontSize: string | null
      enableThumbHoverScale: string | null
    }>('get_startup_config')

    if (cfg.language) {
      ui.applyLanguage(cfg.language)
    } else {
      ui.applyLanguage(ui.language)
    }

    if (cfg.timelineScrollWidth) {
      document.documentElement.style.setProperty('--scrollbar-width', `${cfg.timelineScrollWidth}px`)
    }

    if (cfg.uiFontSize) {
      const size = parseInt(cfg.uiFontSize, 10)
      const diff = size - 15
      document.documentElement.style.setProperty('--font-size-xs',   `${12 + diff}px`)
      document.documentElement.style.setProperty('--font-size-sm',   `${13 + diff}px`)
      document.documentElement.style.setProperty('--font-size-base', `${15 + diff}px`)
      document.documentElement.style.setProperty('--font-size-md',   `${16 + diff}px`)
      document.documentElement.style.setProperty('--font-size-lg',   `${19 + diff}px`)
      document.documentElement.style.setProperty('--font-size-xl',   `${23 + diff}px`)
      document.documentElement.style.setProperty('--font-size-2xl',  `${28 + diff}px`)
    }

    if (cfg.enableThumbHoverScale === 'false') {
      document.documentElement.classList.add('disable-hover-scale')
    }
  } catch (e) {
    console.error('Failed to load startup config:', e)
  }

  // Close splashscreen and reveal the main window now that the app is fully ready.
  // App.vue 已挂载完成，通知 Rust 全次关闭 splashscreen 并显示主窗口。
  invoke('close_splashscreen').catch(() => {})

  // Fetch AI status AFTER the window is shown — non-blocking background refresh.
  // Deferring this avoids competing with the 4 config IPC calls on the critical path.
  // AI 状态在窗口显示后再获取，不影响启动速度。
  ai.fetchStatus().catch(() => {})
})
</script>