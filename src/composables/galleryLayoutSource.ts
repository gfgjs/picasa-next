// src/composables/galleryLayoutSource.ts
// 画廊布局策略接缝（T18-布局切片 + T20，详见 plan-docs/refactor_2026/
// T20_T18-layout_布局策略接缝_合并设计.md）。
//
// 关键架构事实：useVirtualScroll 本身布局无关——它只依赖 { totalHeight, totalRows,
// fetchRowsByY } + 每行 y/height 这个抽象契约，坐标平移 / SAFE_MAX / 滚轮补偿全与布局
// 模式解耦。因此"加 Grid 模式"= 给同一个虚拟滚动换一个行供给源，而非改虚拟滚动。
//
// 又因 T20 选了后端方案 a（uniform-packing）：justified 与 grid 由后端产出**兼容的
// LayoutRow**（grid 行的 w=h=方格边长），故前端的几何源 / 行供给 / 单元渲染对两模式
// **完全相同**——卡片按后端 w/h 定尺寸、MediaThumb 的 object-fit:cover 自然把非方图裁成方格。
// 模式切换仅靠 useJustifiedLayout 把 ui.layoutMode 透传给后端（compute_layout 换排版算法）
// + relayout watch 监听 layoutMode。本接缝因此是模式无关的纯透传，无需逐模式分支。

import type { LayoutRow } from '../types/layout'
import { useMediaStore } from '../stores/mediaStore'
import { useJustifiedLayout } from './useJustifiedLayout'

/**
 * 画廊布局源：把"几何来源 + 行供给 + 重算触发"收敛成一个契约，供 MediaGrid 喂给
 * useVirtualScroll，将布局接线与虚拟滚动解耦（A1）。后端方案 a 下两种布局共用本源。
 */
export interface GalleryLayoutSource {
  /** 逻辑总高（px）——驱动虚拟滚动几何。 */
  totalHeight: () => number
  /** 总行数。 */
  totalRows: () => number
  /** 按逻辑 Y 区间取可见行（虚拟滚动的行供给）。 */
  fetchRowsByY: (topY: number, bottomY: number) => Promise<LayoutRow[]>
  /** 触发布局（重）计算。width 缺省时由策略自取容器宽。 */
  recompute: (width?: number) => Promise<void>
  /** 容器尺寸变化时的（防抖）重算入口。 */
  onResize: (newWidth: number) => void
}

/**
 * 画廊布局源实现。封装现 useJustifiedLayout（含 ui.layoutMode 透传与 relayout watch）+
 * mediaStore 几何 / 行供给。justified 与 grid 共用——模式差异在后端排版，前端零分支。
 *
 * @param containerWidthRef 容器内容区宽度的惰性 getter（透传给 useJustifiedLayout）。
 *
 * 注意：内部调用 useJustifiedLayout（含 watch / onBeforeUnmount），故必须在组件 setup 上下文中调用。
 */
export function useGalleryLayoutSource(containerWidthRef: () => number): GalleryLayoutSource {
  const media = useMediaStore()
  const { compute, onResize } = useJustifiedLayout(containerWidthRef)

  return {
    totalHeight: () => media.totalHeight,
    totalRows: () => media.totalRows,
    fetchRowsByY: (topY, bottomY) => media.fetchRowsByY(topY, bottomY),
    recompute: compute,
    onResize,
  }
}
