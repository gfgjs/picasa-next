// src/composables/useFullscreenExitGuard.ts
// Browser-style "hold Esc to exit fullscreen" guard.
// 浏览器风格的「按住 Esc 退出全屏」守卫。
//
// In native (Tauri) fullscreen a single Esc tap exits immediately (WebView2's default
// fullscreen behaviour) — surprising for a desktop app. We intercept Esc on the window
// CAPTURE phase so our handler runs before any other Esc consumer: if a modal / overlay
// / selection is active we defer (Esc keeps its normal job); otherwise we suppress the
// native exit and require the user to HOLD Esc for HOLD_MS, showing a top-center hint —
// exactly like Chrome / Edge.
// 原生（Tauri）全屏下单击 Esc 会立即退出（WebView2 的默认全屏行为），对桌面应用而言很意外。
// 我们在 window 捕获阶段拦截 Esc，使其先于其它 Esc 消费者运行：若有弹层/选择模式，则让行
// （Esc 执行常规行为）；否则抑制原生退出，要求用户按住 Esc 达 HOLD_MS，并在顶部居中显示
// 提示——与 Chrome / Edge 一致。

import { ref, onMounted, onBeforeUnmount, watch } from 'vue'
import { useUiStore } from '../stores/uiStore'
import { useMediaStore } from '../stores/mediaStore'
import { useSelection } from './useSelection'

/// How long Esc must be held before fullscreen actually exits.
/// Esc 需按住多久才真正退出全屏。
const HOLD_MS = 1000

export function useFullscreenExitGuard() {
  const ui = useUiStore()
  const media = useMediaStore()
  const selection = useSelection()

  const hintVisible = ref(false)
  const holding = ref(false)
  const holdMs = HOLD_MS

  let holdTimer: ReturnType<typeof setTimeout> | null = null
  let hintHideTimer: ReturnType<typeof setTimeout> | null = null

  // Other Esc consumers take precedence — detail overlay, settings, active selection,
  // and any open dialog. In those cases Esc should do its normal job, not the
  // fullscreen hold-to-exit.
  // 其它 Esc 消费者优先——详情浮层、设置、激活的选择、以及任何打开的对话框。
  // 这些情况下 Esc 执行常规行为，而非全屏的「按住退出」。
  function shouldDeferEsc(): boolean {
    if (media.isDetailOpen) return true
    if (ui.isSettingsOpen) return true
    if (selection.isSelectionMode.value) return true
    if (document.querySelector('.dialog-overlay')) return true
    return false
  }

  function showHint() {
    hintVisible.value = true
    if (hintHideTimer !== null) {
      clearTimeout(hintHideTimer)
      hintHideTimer = null
    }
  }

  function scheduleHintHide(delay = 1200) {
    if (hintHideTimer !== null) clearTimeout(hintHideTimer)
    hintHideTimer = setTimeout(() => {
      hintVisible.value = false
      hintHideTimer = null
    }, delay)
  }

  function cancelHold() {
    if (holdTimer !== null) {
      clearTimeout(holdTimer)
      holdTimer = null
    }
    holding.value = false
  }

  function onKeydownCapture(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !ui.isFullscreen) return
    if (shouldDeferEsc()) return
    // Suppress the native single-tap exit; require a hold instead.
    // 抑制原生的单击退出；改为要求按住。
    e.preventDefault()
    e.stopPropagation()
    if (holdTimer !== null) return // key auto-repeat while already holding | 按住期间的自动重复
    holding.value = true
    showHint()
    holdTimer = setTimeout(() => {
      holdTimer = null
      holding.value = false
      hintVisible.value = false
      ui.toggleFullscreen() // confirmed hold → exit | 确认按住 → 退出
    }, HOLD_MS)
  }

  function onKeyupCapture(e: KeyboardEvent) {
    if (e.key !== 'Escape') return
    if (holdTimer === null && !holding.value) return
    // Released before the threshold — cancel exit, linger the hint briefly so it's read.
    // 在阈值前松开——取消退出，让提示短暂停留以便看清。
    cancelHold()
    scheduleHintHide()
  }

  // If fullscreen ends by any other path (F11, toolbar button, OS), drop the hint/hold.
  // 若全屏经其它途径结束（F11、工具栏按钮、系统），清掉提示/按住状态。
  watch(
    () => ui.isFullscreen,
    (fs) => {
      if (!fs) {
        cancelHold()
        hintVisible.value = false
        if (hintHideTimer !== null) {
          clearTimeout(hintHideTimer)
          hintHideTimer = null
        }
      }
    },
  )

  onMounted(() => {
    window.addEventListener('keydown', onKeydownCapture, true)
    window.addEventListener('keyup', onKeyupCapture, true)
  })

  onBeforeUnmount(() => {
    window.removeEventListener('keydown', onKeydownCapture, true)
    window.removeEventListener('keyup', onKeyupCapture, true)
    cancelHold()
    if (hintHideTimer !== null) clearTimeout(hintHideTimer)
  })

  return { hintVisible, holding, holdMs }
}
