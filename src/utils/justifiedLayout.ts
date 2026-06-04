// src/utils/justifiedLayout.ts
// Lightweight frontend justified layout utility (≤100 lines).
// 轻量级前端两端对齐布局工具（≤100行）。
//
// Input: items with id, width, height
// 输入：带有 id、width、height 的项目
// Output: items with id, x, y, w, h positions
// 输出：带有 id、x、y、w、h 位置的项目

export interface JustifiedInput {
  id: number
  width: number
  height: number
}

export interface JustifiedOutput {
  id: number
  x: number
  y: number
  w: number
  h: number
}

interface JustifiedOptions {
  /** Container width in pixels | 容器宽度（像素）*/
  containerWidth: number
  /** Target row height in pixels | 目标行高（像素）*/
  targetRowHeight: number
  /** Gap between items in pixels | 项目间距（像素）*/
  gap?: number
  /** Maximum row height ratio vs target | 最大行高比例（相对目标行高）*/
  maxRowHeightFactor?: number
}

/**
 * Compute a justified layout for a list of items.
 * 计算项目列表的两端对齐布局。
 *
 * Algorithm: same Flickr-style pack-and-scale as backend Rust implementation.
 * 算法：与后端 Rust 实现相同的 Flickr 风格打包缩放算法。
 */
export function computeJustifiedLayout(
  items: JustifiedInput[],
  opts: JustifiedOptions,
): JustifiedOutput[] {
  const {
    containerWidth,
    targetRowHeight,
    gap = 4,
    maxRowHeightFactor = 2.0,
  } = opts

  const result: JustifiedOutput[] = []
  let currentY = 0
  let rowItems: JustifiedInput[] = []
  let rowArSum = 0 // sum of aspect ratios in the current row
                   // 当前行的宽高比总和

  // Clamp aspect ratio to prevent extreme values
  // 将宽高比限制在合理范围内，防止极端值
  const ar = (item: JustifiedInput) =>
    Math.max(0.2, Math.min(5.0, (item.width || 1) / (item.height || 1)))

  function commitRow(isLast: boolean) {
    if (rowItems.length === 0) return

    const totalGaps = gap * (rowItems.length - 1)
    const availableW = containerWidth - totalGaps
    const idealH = availableW / rowArSum

    // For the final incomplete row, don't stretch — use target height
    // 最后一行若不满，不要拉伸，使用目标行高
    const isIncomplete = isLast && rowArSum * targetRowHeight < availableW * 0.6
    const rowH = isIncomplete
      ? targetRowHeight
      : Math.min(idealH, targetRowHeight * maxRowHeightFactor)

    let x = 0
    for (const item of rowItems) {
      const w = ar(item) * rowH
      result.push({
        id: item.id,
        x: Math.round(x),
        y: Math.round(currentY),
        w: Math.round(w),
        h: Math.round(rowH),
      })
      x += w + gap
    }

    currentY += rowH + gap
    rowItems = []
    rowArSum = 0
  }

  for (const item of items) {
    rowItems.push(item)
    rowArSum += ar(item)

    const totalGaps = gap * (rowItems.length - 1)
    const availableW = containerWidth - totalGaps
    const projectedW = rowArSum * targetRowHeight

    if (projectedW >= availableW) {
      commitRow(false)
    }
  }

  // Commit remaining items as the last row
  // 提交剩余项目作为最后一行
  commitRow(true)

  return result
}
