// src/composables/usePointerDrag.ts
// In-app pointer-based drag helper (NOT HTML5 DnD).
// 应用内基于指针的拖拽助手（非 HTML5 DnD）。
//
// Tauri v2 window `dragDropEnabled` defaults to true; its native OS drag-drop
// handler intercepts dragover/drop over the webview, so HTML5 DnD silently fails
// (shows the no-drop cursor, never fires `drop`). Pointer events never leave the
// window, so they never conflict with native OS drag-drop — keeping the door
// open for a future "drag files in from the system" feature.
// Tauri v2 窗口 `dragDropEnabled` 默认开启，其原生系统拖放会拦截 webview 上的
// dragover/drop，使 HTML5 DnD 静默失效（显示禁止光标，永不触发 `drop`）。指针
// 事件全程不离开窗口，因此与原生系统拖放永不冲突——为将来「从系统拖入文件」
// 功能留有余地。

/** px a press must move before it counts as a drag | 按下需移动多少像素才算拖拽 */
export const DRAG_THRESHOLD = 5

/**
 * Start a pointer-drag session. Registers window-level listeners and tears them
 * down automatically on pointerup / pointercancel.
 * 开始一次指针拖拽会话。注册 window 级监听，并在 pointerup / pointercancel 时自动清理。
 *
 * @param onMove   called on every pointermove while dragging | 拖拽期间每次 pointermove 调用
 * @param onFinish called once when the gesture ends; `cancelled` is true on pointercancel
 *                 手势结束时调用一次；pointercancel 时 `cancelled` 为 true
 */
export function beginPointerDrag(
  onMove: (e: PointerEvent) => void,
  onFinish: (e: PointerEvent, cancelled: boolean) => void,
): void {
  const move = (e: PointerEvent) => onMove(e)
  const up = (e: PointerEvent) => { teardown(); onFinish(e, false) }
  const cancel = (e: PointerEvent) => { teardown(); onFinish(e, true) }

  function teardown() {
    window.removeEventListener('pointermove', move)
    window.removeEventListener('pointerup', up)
    window.removeEventListener('pointercancel', cancel)
    // Restore any cursor/selection overrides the caller set while dragging.
    // 恢复调用方在拖拽期间设置的光标/选区覆盖。
    document.body.style.userSelect = ''
    document.body.style.cursor = ''
  }

  window.addEventListener('pointermove', move)
  window.addEventListener('pointerup', up)
  window.addEventListener('pointercancel', cancel)
}
