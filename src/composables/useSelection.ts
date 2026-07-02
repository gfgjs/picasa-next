// src/composables/useSelection.ts
// 旗舰级选择 composable —— 手势采集层 + classicMode.apply 调度 + SelectionState 持有。
//
// Part5 T4b：选区脱离 DOM。内部状态由「裸 Set」改为判别联合 SelectionState（explicit{ids} |
// all{excluded}），离散意图经 classicMode（可插拔策略）解释，顺序/全集取自 useViewIds 的布局序
// flat_ids 而非可视 DOM。这根治 G1 三症状：Shift 跨视口失效 / Ctrl+A 只选一屏 / 框选漏滚出项。
// 🔴 开发期不冻结契约：当前固定 classic 模式 + 当前意图集，两层结构令后续演进影响面收敛。
//
// 设计依据：plan-docs/refactor_2026/2026-06-30-Part5-选区契约与可插拔多模式设计.md §3/§4/§6。

import { ref, computed, readonly, watch } from 'vue'
import {
  EMPTY_SELECTION,
  isSelected as isSelectedIn,
  isEmptySelection,
  selectionSize,
  toDescriptor,
  type SelectionDescriptor,
  type SelectionState,
  type SelectionIntent,
  type SelectionContext,
} from './selection/types'
import { getSelectionMode } from './selection/registry'
import { useViewIds } from './useViewIds'
import { buildCurrentViewDescriptor } from './useViewDescriptor'
import type { ViewDescriptorDto } from '../types/view'

/** 批量命令的选区入参实型（R1-2/S4：泛型协议层绑定后端 ViewDescriptor 镜像）。 */
export type BackendSelectionDescriptor = SelectionDescriptor<ViewDescriptorDto>

type DragMode = 'select' | 'deselect' | null

// 视图布局序全集（range / 全选物化 / 反选 的顺序与全集来源,脱离可视 DOM）。
const viewIds = useViewIds()
// 当前激活的选择模式（现固定 classic;未来从 config 读、设置页可切——不冻结）。
const mode = getSelectionMode()

// ── 单例状态 | Singleton state ──
// 所有消费者共享同一选区。判别联合:explicit 显式枚举 / all 全选语义(不物化百万 id)。
const state = ref<SelectionState>(EMPTY_SELECTION)
const isSelectionMode = ref(false)

// 框选（拖拽选择）瞬时状态
const isDragging = ref(false)
const dragMode = ref<DragMode>(null)
const dragStartPos = ref<{ x: number; y: number } | null>(null)
const dragStartId = ref<number | null>(null)
const lastHoveredId = ref<number | null>(null)
const hasDragMoved = ref(false)
const lastClickedId = ref<number | null>(null)
const dragStartContainer = ref<HTMLElement | null>(null)
// 框选基线:拖拽起手时的选区快照（物化为 Set,框选在其上叠加/扣除,支持反向框选与跨行区间）。
const dragBaseline = ref<Set<number>>(new Set())
// 上次有效区间:viewIds 未就绪时的 DOM 回退兜底（避免拖拽锚点滚出屏幕后区间断裂）。
const lastValidRangeIds = ref<number[]>([])

// 移动阈值:必须移动 > 5px 才算拖拽（不是单击）。
const DRAG_THRESHOLD = 5

// 选区基数:explicit→ids.size;all→全集数−排除数（需 viewIds 已就绪,随其刷新自动重算）。
const selectedCount = computed(() => selectionSize(state.value, viewIds.totalCount()))

// 选区为空时自动退出选择模式。用 isEmptySelection 而非 count===0:
// all 态在 viewIds 未加载时 count 会暂为 0,但它**不是空选区**,不能误退出（isEmptySelection 对 all 恒 false）。
watch(
  () => isEmptySelection(state.value),
  (empty) => {
    if (empty && isSelectionMode.value) isSelectionMode.value = false
  },
)

// ── 调度核心 | Dispatch core ──

/** 组装策略上下文:布局序全集 + O(1) 区间 + 全集计数,均来自 useViewIds。 */
function makeCtx(): SelectionContext {
  return {
    viewIds: viewIds.allIds(),
    rangeBetween: viewIds.rangeBetween,
    totalCount: viewIds.totalCount(),
  }
}

/** 把一个离散意图交当前模式解释为新选区状态,并同步选择模式开关。 */
function dispatch(intent: SelectionIntent) {
  state.value = mode.apply(state.value, intent, makeCtx())
  if (!isEmptySelection(state.value)) isSelectionMode.value = true
  // 置 false 交给上方 watch（空选区 → 退出）,避免两处重复判定。
}

export function useSelection() {
  // ── 单项操作 | Single item operations ──

  function toggleSelect(id: number) {
    dispatch({ type: 'toggle', id })
    lastClickedId.value = id
  }

  /** 布局序中第一个已选项（锚点缺失时的回退,实现「从已选区间首项起区间」的直觉）。 */
  function firstSelectedAnchor(): number | null {
    for (const id of viewIds.allIds()) {
      if (isSelected(id)) return id
    }
    return null
  }

  /**
   * 区间选择（Shift+单击 / 框选）。
   * 锚点缺失（anchorId 为 null,如经框选/全选进入选择态、未经单击设锚）时,回退到布局序第一个已选项
   * → 仍能「从已选首项到点击项」成区间;若当前无任何已选项,才退化为单项翻转。
   * 区间 id 由布局序 flat_ids 计算（跨视口稳定）,不依赖可视 DOM。
   */
  function selectRange(anchorId: number | null, toId: number) {
    const anchor = anchorId ?? firstSelectedAnchor()
    if (anchor === null) {
      toggleSelect(toId)
      return
    }
    dispatch({ type: 'range', anchorId: anchor, toId })
    lastClickedId.value = toId
  }

  /** 全选:进入 all 态,不物化 id（百万级内存恒定）。 */
  function selectAll() {
    dispatch({ type: 'selectAll' })
  }

  function clearSelection() {
    dispatch({ type: 'clear' })
    lastClickedId.value = null
  }

  /** 反选:基于布局序全集（ctx.viewIds）翻转,不再依赖可视 DOM。 */
  function invertSelection() {
    dispatch({ type: 'invert' })
  }

  function isSelected(id: number): boolean {
    return isSelectedIn(state.value, id)
  }

  /**
   * 物化当前选区为 id 数组。
   * R1-2/S4 后批量命令（收藏/评分/色签/删除/恢复）已改走 [`toBackendDescriptor`] 零物化直传;
   * 本函数保留给**确需具体 id** 的消费者:移动/复制/加收藏夹（后端命令按 id 编排）、删除暂存
   * 置灰集与撤销闭包、框选基线快照、语义搜索视图的回退路径。all 态在这些场景仍会物化大数组
   * （其 SelectAll 深迁移属 T18 后续,按需再动）。
   */
  function materializeIds(): number[] {
    if (state.value.kind === 'explicit') return [...state.value.ids]
    const excluded = state.value.excluded
    return viewIds.allIds().filter((id) => !excluded.has(id))
  }

  /**
   * 选区 → 后端 SelectionDescriptor（R1-2/S4 批量命令的首选出口）。
   * explicit → explicit{ids};all → selectAll{view, excludedIds}——百万级全选 payload 恒定
   * （只含视图描述 + 排除集）,id 物化收敛到后端 SQL 层。
   * @returns null = 当前视图不可 SQL 描述（语义搜索）,调用方回退 Explicit(materializeIds())。
   */
  function toBackendDescriptor(): BackendSelectionDescriptor | null {
    if (state.value.kind === 'explicit') {
      // explicit 态不消费 view,传 null 占位（toDescriptor 泛型透传,不读取）。
      return toDescriptor(state.value, null as unknown as ViewDescriptorDto)
    }
    const view = buildCurrentViewDescriptor()
    if (!view) return null
    return toDescriptor(state.value, view)
  }

  // ── 框选（拖拽选择）| Drag selection ──

  function onPointerDown(id: number, event: PointerEvent) {
    // 仅处理主按钮（左键）
    if (event.button !== 0) return

    dragStartPos.value = { x: event.clientX, y: event.clientY }
    dragStartId.value = id
    // hasDragMoved 的复位统一由 beginInteraction() 在交互起手时做(覆盖框选与拖图两条路径),
    // 此处不再各自复位——避免双轨复位时机不一致(T5 单 flag 化)。
    lastHoveredId.value = id

    const target = event.target as HTMLElement | null
    dragStartContainer.value = target?.closest(
      '.media-grid, .semantic-panel__grid',
    ) as HTMLElement | null

    // 确定拖拽模式:从已选中项开始 → 反选;否则选中。
    dragMode.value = isSelectionMode.value && isSelected(id) ? 'deselect' : 'select'

    // 基线快照:框选在当前选区之上叠加/扣除。物化当前态（all 态亦可,虽框选起于全选属罕见路径）。
    dragBaseline.value = new Set(materializeIds())
    lastValidRangeIds.value = []

    // 注册文档级监听器（在 onPointerUp 中清理）
    document.addEventListener('pointermove', onPointerMoveGlobal)
    document.addEventListener('pointerup', onPointerUpGlobal)

    // 防止拖拽期间的文本选择
    event.preventDefault()
  }

  function onPointerMoveGlobal(event: PointerEvent) {
    if (!dragStartPos.value) return

    // 检查是否超过移动阈值
    if (!hasDragMoved.value) {
      const dx = event.clientX - dragStartPos.value.x
      const dy = event.clientY - dragStartPos.value.y
      if (Math.sqrt(dx * dx + dy * dy) < DRAG_THRESHOLD) return

      // 超过阈值 — 正式开始拖拽
      hasDragMoved.value = true
      isDragging.value = true

      if (dragStartId.value !== null) {
        applyDragRange(dragStartId.value, dragStartId.value)
      }
    }

    // 通过 elementFromPoint 找到指针下方的项
    const el = document.elementFromPoint(event.clientX, event.clientY)
    if (!el) return

    const card = (el as HTMLElement).closest('[data-item-id]') as HTMLElement | null
    if (!card) return

    const itemId = parseInt(card.dataset.itemId!, 10)
    if (isNaN(itemId)) return

    if (itemId !== lastHoveredId.value) {
      lastHoveredId.value = itemId
      applyDragRange(dragStartId.value!, itemId)
    }
  }

  /**
   * 把 [startId, endId] 区间按 dragMode 叠加/扣除到基线,产出新 explicit 选区。
   * 区间优先取自布局序 flat_ids（跨已滚动区间稳定,根治 G1③ 框选漏滚出项）;
   * viewIds 未就绪时回退到可视 DOM 扫描（绝不比旧实现差）。
   */
  function applyDragRange(startId: number, endId: number) {
    let rangeIds = viewIds.rangeBetween(startId, endId)
    if (rangeIds.length === 0) {
      rangeIds = domFallbackRange(startId, endId)
    } else {
      lastValidRangeIds.value = rangeIds
    }

    const newSet = new Set(dragBaseline.value)
    for (const id of rangeIds) {
      if (dragMode.value === 'select') newSet.add(id)
      else if (dragMode.value === 'deselect') newSet.delete(id)
    }

    state.value = { kind: 'explicit', ids: newSet }
    if (newSet.size > 0) isSelectionMode.value = true
    // 空选区 → 退出,交给 isEmptySelection watch。
  }

  /** viewIds 未就绪时的兜底:在容器内可视 DOM 上算区间（旧逻辑,仅作回退）。 */
  function domFallbackRange(startId: number, endId: number): number[] {
    const container = dragStartContainer.value || document
    const cards = container.querySelectorAll('[data-item-id]')
    const visibleIds: number[] = []
    for (let i = 0; i < cards.length; i++) {
      const id = parseInt((cards[i] as HTMLElement).dataset.itemId || '', 10)
      if (!isNaN(id)) visibleIds.push(id)
    }

    const s = visibleIds.indexOf(startId)
    const e = visibleIds.indexOf(endId)
    if (s !== -1 && e !== -1) {
      const lo = Math.min(s, e)
      const hi = Math.max(s, e)
      const r = visibleIds.slice(lo, hi + 1)
      lastValidRangeIds.value = r
      return r
    }
    // 锚点已滚出:沿用上次有效区间 + 当前命中点
    const r = [...lastValidRangeIds.value]
    if (!r.includes(endId)) r.push(endId)
    return r
  }

  function onPointerUpGlobal(_event: PointerEvent) {
    // 清理文档监听器
    document.removeEventListener('pointermove', onPointerMoveGlobal)
    document.removeEventListener('pointerup', onPointerUpGlobal)

    // 框选若真发生了位移 → 把锚点设到拖拽终点项,使其后的 Shift+单击能从此处起区间。
    // 修复:经框选进入选择状态后,首次 Shift+单击因 lastClickedId=null 落入 toggle 兜底、不成区间。
    if (hasDragMoved.value && lastHoveredId.value !== null) {
      lastClickedId.value = lastHoveredId.value
    }

    isDragging.value = false
    dragStartPos.value = null
    dragStartId.value = null
    lastHoveredId.value = null
    dragMode.value = null
    dragStartContainer.value = null
    // 注意:不在此复位 hasDragMoved——它必须撑过紧随的尾随 click 以抑制之(框选/拖图结束不应
    // 再触发单击)。复位统一在下次交互起手 beginInteraction() 做(T5)。在 pointerup 复位会让
    // 「小框选在同一卡片上松开」的尾随 click 误判为普通单击(回归)。
  }

  // ── 键盘处理 | Keyboard handling ──

  function onKeyDown(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null
    if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) {
      return
    }
    if (event.key === 'Escape' && isSelectionMode.value) {
      clearSelection()
      event.preventDefault()
    }
    if ((event.ctrlKey || event.metaKey) && event.key === 'a' && isSelectionMode.value) {
      selectAll()
      event.preventDefault()
    }
  }

  // ── 统一拖拽标志（T5：消除 mediaWasDrag/hasDragMoved 双轨）──
  // 单一 flag hasDragMoved 同时服务两种拖拽:框选(lasso)与拖图到文件夹(move/copy)。
  // 复位时机统一:每次卡片交互起手 beginInteraction() 复位 → 移动越阈则 markDragMoved() 置位
  // → 尾随 click 读 wasDrag() 抑制 → 下次起手再复位。flag 撑过尾随 click,故不在 pointerup 复位。

  /** 交互起手:复位拖拽标志。由 onCardPointerDown 在分流(框选/拖图)前统一调用。 */
  function beginInteraction() {
    hasDragMoved.value = false
  }

  /** 标记本次交互已发生拖拽位移(拖图路径越过阈值时调用;框选路径在 onPointerMove 内部自置)。 */
  function markDragMoved() {
    hasDragMoved.value = true
  }

  // 返回值 — 紧邻的这次 pointer 交互是拖拽还是单击？尾随 click 据此抑制。
  function wasDrag(): boolean {
    return hasDragMoved.value
  }

  return {
    // 状态（只读）
    isSelectionMode: readonly(isSelectionMode),
    isDragging: readonly(isDragging),
    selectedCount,
    lastClickedId: readonly(lastClickedId),

    // 查询
    isSelected,
    materializeIds,
    toBackendDescriptor,

    // 意图（离散手势）
    toggleSelect,
    selectRange,
    selectAll,
    clearSelection,
    invertSelection,

    // 手势采集
    onPointerDown,
    beginInteraction,
    markDragMoved,
    wasDrag,
    onKeyDown,
  }
}
