// src/composables/useSidebarSections.ts
// VSCode-style accordion controller for the sidebar (provide/inject).
// 侧边栏 VSCode 风格手风琴控制器（provide/inject）。
//
// Responsibilities | 职责:
//  1. Per-section expand/collapse state, persisted to app_config so it survives
//     reloads. Collapsing only hides the body (callers use v-show), so nested
//     state such as folder-tree expansion is preserved — 「多级展开状态记忆」.
//     每个区块的展开/折叠状态，持久化到 app_config 以在重启后保留。折叠仅隐藏
//     主体（调用方用 v-show），因此文件夹树等嵌套展开状态得以保留——「多级展开状态记忆」。
//  2. Sticky-header stacking math: each visible section header registers its
//     `order`; from the sorted list of *currently visible* sections we derive
//     each header's index/total so it can pin to top AND bottom (stacked), and
//     conditional sections (e.g. Management) don't leave gaps in the offsets.
//     粘性标题堆叠计算：每个可见区块标题登记其 `order`；从当前可见区块的排序
//     列表推导出各标题的 index/total，使其能粘顶与粘底（堆叠），且条件区块
//     （如「管理」）不会在偏移中留下空档。

import {
  inject, provide, reactive, computed,
  type ComputedRef, type InjectionKey,
} from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { IPC } from '../constants/ipc'

/** app_config key holding the `{ [sectionId]: expanded }` map | 保存展开状态映射的 app_config 键 */
const CONFIG_KEY = 'sidebar_sections_expanded'

export interface SidebarSectionsApi {
  /** Whether section `id` is expanded (defaults to true when unknown). | 区块 `id` 是否展开（未知时默认展开）。 */
  isExpanded: (id: string) => boolean
  /** Toggle section `id` and persist. | 切换区块 `id` 并持久化。 */
  toggle: (id: string) => void
  /** Register a now-visible section (call on mount). | 登记一个已可见的区块（挂载时调用）。 */
  register: (id: string, order: number) => void
  /** Unregister a section (call on unmount). | 注销区块（卸载时调用）。 */
  unregister: (id: string) => void
  /** Ids of currently-visible sections, sorted by their `order`. | 当前可见区块的 id，按 `order` 排序。 */
  visibleIds: ComputedRef<string[]>
}

const KEY: InjectionKey<SidebarSectionsApi> = Symbol('sidebar-sections')

/**
 * Create the controller and provide it to descendant section components.
 * Call once, in the sidebar container's setup().
 * 创建控制器并向后代区块组件提供。仅在侧边栏容器的 setup() 中调用一次。
 */
export function provideSidebarSections(): SidebarSectionsApi {
  // sectionId -> expanded; missing key means "use default (true)".
  // sectionId -> 是否展开；缺失的键表示「使用默认值（true）」。
  const expanded = reactive<Record<string, boolean>>({})
  // sectionId -> order, for sections that are currently mounted/visible.
  // sectionId -> order，仅包含当前已挂载/可见的区块。
  const registered = reactive<Record<string, number>>({})

  // Load persisted expand state asynchronously; defaults (all expanded) stand
  // until it resolves, so there's at most a brief flash on first launch.
  // 异步加载已持久化的展开状态；解析前沿用默认值（全部展开），首启至多有一瞬闪烁。
  invoke<string | null>(IPC.GET_APP_CONFIG, { key: CONFIG_KEY })
    .then(saved => {
      if (!saved) return
      try {
        const obj = JSON.parse(saved) as Record<string, boolean>
        for (const k in obj) expanded[k] = obj[k]
      } catch { /* ignore malformed value | 忽略损坏的值 */ }
    })
    .catch(() => {})

  function persist() {
    invoke(IPC.SET_APP_CONFIG, { key: CONFIG_KEY, value: JSON.stringify(expanded) })
      .catch(() => {})
  }

  function isExpanded(id: string): boolean {
    return expanded[id] !== false
  }

  function toggle(id: string) {
    expanded[id] = !isExpanded(id)
    persist()
  }

  function register(id: string, order: number) {
    registered[id] = order
  }

  function unregister(id: string) {
    delete registered[id]
  }

  const visibleIds = computed(() =>
    Object.keys(registered).sort((a, b) => registered[a] - registered[b]),
  )

  const api: SidebarSectionsApi = { isExpanded, toggle, register, unregister, visibleIds }
  provide(KEY, api)
  return api
}

/** Inject the controller from a section component. | 在区块组件中注入控制器。 */
export function useSidebarSections(): SidebarSectionsApi {
  const api = inject(KEY)
  if (!api) {
    throw new Error('[useSidebarSections] must be used inside a sidebar that called provideSidebarSections()')
  }
  return api
}
