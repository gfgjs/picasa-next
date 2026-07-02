<template>
  <AppShell>
    <template #sidebar>
      <AppSidebar />
    </template>

    <template #toolbar>
      <AppToolbar @search="onSearch" @sort-change="onSortChange" />
    </template>

    <!-- Default slot: media content OR semantic search panel -->
    <!-- 默认插槽：媒体内容或语义搜索面板 -->
    <SemanticSearchPanel v-show="route.path === '/'" />
    <RouterView />

    <template #statusbar>
      <AppStatusBar />
    </template>
  </AppShell>

  <!-- Detail overlay (global) -->
  <!-- 详情覆盖层（全局） -->
  <MediaDetailOverlay />

  <!-- Settings overlay (global) -->
  <SettingsView v-if="ui.isSettingsOpen" />

  <!-- Document thumbnail offscreen renderer (P4, §3.4) — hidden, throttled -->
  <!-- 文档缩略图离屏渲染器（P4, §3.4）—— 隐藏、节流 -->
  <DocThumbRenderer />

  <!-- Toast notifications -->
  <!-- 吐司通知 -->
  <ToastContainer />

  <!-- Close Confirmation Dialog -->
  <!-- 关闭确认弹窗 -->
  <CloseConfirmDialog />

  <!-- First-launch onboarding wizard (T17, §3.8) — first_launch 缺省时显示 -->
  <!-- 首启向导：3 步引导（目录/主题/语言），完成或跳过后写 first_launch=false 不再弹 -->
  <OnboardingWizard v-if="showOnboarding" @done="showOnboarding = false" />
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { invokeIpc } from './utils/ipc'
import { IPC } from './constants/ipc'
import { useTheme } from './composables/useTheme'
import { useDerivationAutoStart } from './composables/useDerivationAutoStart'
import { useUiStore } from './stores/uiStore'

import AppShell from './components/layout/AppShell.vue'
import AppSidebar from './components/sidebar/AppSidebar.vue'
import AppToolbar from './components/layout/AppToolbar.vue'
import AppStatusBar from './components/layout/AppStatusBar.vue'
import MediaDetailOverlay from './components/media/MediaDetailOverlay.vue'
import SettingsView from './views/SettingsView.vue'
import SemanticSearchPanel from './components/media/SemanticSearchPanel.vue'
import ToastContainer from './components/common/ToastContainer.vue'
import CloseConfirmDialog from './components/common/CloseConfirmDialog.vue'
import OnboardingWizard from './components/common/OnboardingWizard.vue'
import DocThumbRenderer from './components/media/DocThumbRenderer.vue'
import { useAiStore } from './stores/aiStore'
import { useFaceStore } from './stores/faceStore'
import { useRoute } from 'vue-router'
import { listen } from '@tauri-apps/api/event'

const ui = useUiStore()
const ai = useAiStore()
const face = useFaceStore()
const route = useRoute()

// 首启向导显隐（T17）：onMounted 检测 first_launch 配置缺省时置真。
const showOnboarding = ref(false)

// Init theme
// 初始化主题
useTheme()

// 自动启动派生流水线（视频封面/关键帧、音频封面、epub 封面）。
// 此前无任何触发入口 → 流水线从未运行 → 视频封面不出现，本调用即根因修复。
useDerivationAutoStart()

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

  // R2-4:复用 uiStore 在 setup 期发出的唯一一次 get_startup_config(14 键批量)。
  // uiStore 自己的 9 键由 store 内 .then 应用;此处只消费全局项与 first_launch,
  // 整个启动阶段的配置 IPC 由 11 次归 1 次。
  try {
    const cfg = await ui.startupConfigPromise

    if (cfg.language) {
      ui.applyLanguage(cfg.language)
    } else {
      ui.applyLanguage(ui.language)
    }

    if (cfg.timelineScrollWidth) {
      document.documentElement.style.setProperty(
        '--scrollbar-width',
        `${cfg.timelineScrollWidth}px`,
      )
    }

    if (cfg.uiFontSize) {
      const size = parseInt(cfg.uiFontSize, 10)
      const diff = size - 15
      document.documentElement.style.setProperty('--font-size-xs', `${12 + diff}px`)
      document.documentElement.style.setProperty('--font-size-sm', `${13 + diff}px`)
      document.documentElement.style.setProperty('--font-size-base', `${15 + diff}px`)
      document.documentElement.style.setProperty('--font-size-md', `${16 + diff}px`)
      document.documentElement.style.setProperty('--font-size-lg', `${19 + diff}px`)
      document.documentElement.style.setProperty('--font-size-xl', `${23 + diff}px`)
      document.documentElement.style.setProperty('--font-size-2xl', `${28 + diff}px`)
    }

    if (cfg.enableThumbHoverScale === 'false') {
      document.documentElement.classList.add('disable-hover-scale')
    }

    // 首启检测(T17, §3.8;R2-4 并入启动批):仅当 first_launch 被显式写为 'false'(用户完成/
    // 跳过过引导)才抑制;其余一切情形(缺省 null / 空串 / 意外值)都视为「未走过引导」→ 显示向导。
    // 完成/跳过时向导自身写 'false'(见 OnboardingWizard)。
    if (cfg.firstLaunch !== 'false') showOnboarding.value = true
  } catch (e) {
    console.error('Failed to load startup config:', e)
  }

  // Close splashscreen and reveal the main window now that the app is fully ready.
  // App.vue 已挂载完成，通知 Rust 全次关闭 splashscreen 并显示主窗口。
  invokeIpc(IPC.CLOSE_SPLASHSCREEN).catch(() => {})

  // Fetch AI status AFTER the window is shown — non-blocking background refresh — and
  // auto-resume an analysis that was interrupted (crash / forced exit / pause) with work
  // left, so 断点续传 survives a program restart (问题7).
  // AI 状态在窗口显示后再获取（不影响启动速度），并自动续传被中断（崩溃/强退/暂停）且仍有
  // 剩余的分析，使断点续传能跨程序重启（问题7）。
  ai.maybeAutoResume().catch(() => {})
  // Face auto-resume runs alongside AI's — if both are "active with work left", the backend
  // GPU-analysis gate (single-owner) serializes them: whichever claims the slot first runs, the
  // other's start is rejected (logged, retried next launch). Mutual exclusion is enforced
  // backend-side, not by ordering here.
  // 人脸自动续传与 AI 并列触发——若两者都「期望运行且有剩余」，后端 GPU 分析门闩（单一持有者）
  // 将其串行化：先占到槽的运行，另一个的启动被拒（记日志，下次启动重试）。互斥由后端保证，
  // 而非此处的顺序。
  face.maybeAutoResume().catch(() => {})

  // Listen to custom window close request from backend
  // 监听来自后端的自定义窗口关闭请求
  listen('window-close-requested', async () => {
    if (ui.closeBehavior === 'minimize_to_tray') {
      await invokeIpc(IPC.HIDE_WINDOW)
    } else if (ui.closeBehavior === 'exit') {
      await invokeIpc(IPC.EXIT_APP)
    } else {
      ui.showCloseConfirmDialog = true
    }
  })
})
</script>
