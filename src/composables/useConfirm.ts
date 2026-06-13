// src/composables/useConfirm.ts
// Promise-based confirm-dialog singleton (replaces per-component inline modals).
// 基于 Promise 的确认对话框单例（取代各组件内联弹窗）。
//
// Any component calls `useConfirm().confirm(opts)` and awaits the result; a
// single <ConfirmDialog> mounted once (in AppSidebar) renders the shared state.
// This lets sibling sections (Folders / Management) share one dialog without
// prop-drilling or duplicating modal markup.
// 任意组件调用 `useConfirm().confirm(opts)` 并 await 结果；仅挂载一次的
// <ConfirmDialog>（在 AppSidebar 中）渲染共享状态。这使同级区块（文件夹 / 管理）
// 无需层层传 prop 或重复弹窗标记即可共用一个对话框。

import { reactive } from 'vue'

export interface ConfirmOptions {
  title: string
  message: string
  confirmText?: string
  cancelText?: string
  /** Show an extra checkbox (e.g. "also clear thumbnails"). | 显示额外复选框（如「同时清除缩略图」）。 */
  showCheckbox?: boolean
  checkboxLabel?: string
  /** Initial checkbox value. | 复选框初始值。 */
  checkboxValue?: boolean
}

export interface ConfirmResult {
  /** true if the user confirmed, false if cancelled. | 用户确认为 true，取消为 false。 */
  confirmed: boolean
  /** Final checkbox value (only meaningful when showCheckbox). | 复选框最终值（仅在 showCheckbox 时有意义）。 */
  checkboxValue: boolean
}

interface ConfirmState extends Required<ConfirmOptions> {
  isOpen: boolean
  resolve: ((r: ConfirmResult) => void) | null
}

// Module-level singleton shared by every useConfirm() caller and the dialog.
// 模块级单例，被每个 useConfirm() 调用方与对话框共享。
const state = reactive<ConfirmState>({
  isOpen: false,
  title: '',
  message: '',
  confirmText: '确认',
  cancelText: '取消',
  showCheckbox: false,
  checkboxLabel: '',
  checkboxValue: true,
  resolve: null,
})

export function useConfirm() {
  /** Open the dialog and resolve once the user confirms or cancels. | 打开对话框，用户确认或取消后解析。 */
  function confirm(opts: ConfirmOptions): Promise<ConfirmResult> {
    // Reject any in-flight dialog before opening a new one. | 打开新对话框前先结算未完成的旧对话框。
    state.resolve?.({ confirmed: false, checkboxValue: state.checkboxValue })
    return new Promise<ConfirmResult>(resolve => {
      state.isOpen = true
      state.title = opts.title
      state.message = opts.message
      state.confirmText = opts.confirmText ?? '确认'
      state.cancelText = opts.cancelText ?? '取消'
      state.showCheckbox = opts.showCheckbox ?? false
      state.checkboxLabel = opts.checkboxLabel ?? ''
      state.checkboxValue = opts.checkboxValue ?? true
      state.resolve = resolve
    })
  }
  return { confirm }
}

/** Internal accessor for <ConfirmDialog> only. | 仅供 <ConfirmDialog> 使用的内部访问器。 */
export function useConfirmDialogState() {
  function close(confirmed: boolean) {
    state.resolve?.({ confirmed, checkboxValue: state.checkboxValue })
    state.resolve = null
    state.isOpen = false
  }
  return { state, close }
}
