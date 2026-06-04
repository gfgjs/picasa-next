// src/utils/justifiedLayout.ts

export interface JustifiedItem {
  id: number
  w: number
  h: number
  // Allow other data to pass through
  [key: string]: any
}

export type LayoutItemResult<T extends JustifiedItem> = T & {
  scaledWidth: number
  scaledHeight: number
  offsetX: number
  offsetY: number
}

export interface LayoutRowResult<T extends JustifiedItem> {
  height: number
  y: number
  items: LayoutItemResult<T>[]
}

/**
 * Lightweight frontend implementation of the justified layout algorithm.
 * Used for search results and other small collections that don't need backend pagination.
 * 轻量级前端实现两端对齐布局算法。用于搜索结果和其他不需要后端分页的小集合。
 */
export function computeJustifiedLayout<T extends JustifiedItem>(
  items: T[],
  containerWidth: number,
  targetRowHeight: number,
  gap: number
): LayoutRowResult<T>[] {
  const rows: LayoutRowResult<T>[] = []
  let currentY = 0

  let pending: T[] = []
  let pendingArSum = 0

  const commitRow = (isLast: boolean) => {
    if (pending.length === 0) return

    const totalGaps = gap * (pending.length - 1)
    const availableW = containerWidth - totalGaps
    
    // 如果最后一行不足宽度的 60%，则不强制两端对齐
    const isIncomplete = isLast && (pendingArSum * targetRowHeight < availableW * 0.6)
    const idealH = availableW / pendingArSum
    const rowH = isIncomplete ? targetRowHeight : Math.min(idealH, targetRowHeight * 2.0)

    const shouldSnap = !isIncomplete && idealH <= targetRowHeight * 2.0

    const unroundedWidths = pending.map(item => (item.w / item.h) * rowH)
    
    if (shouldSnap && pending.length > 1) {
      const totalUnrounded = unroundedWidths.reduce((a, b) => a + b, 0)
      if (totalUnrounded > 0) {
        const scale = availableW / totalUnrounded
        for (let i = 0; i < unroundedWidths.length; i++) {
          unroundedWidths[i] *= scale
        }
      }
    }

    const finalWidths = unroundedWidths.map(w => Math.round(w))
    if (shouldSnap && pending.length > 1) {
      const currentTotal = finalWidths.reduce((a, b) => a + b, 0)
      let diff = Math.round(availableW) - currentTotal
      let i = 0
      while (diff !== 0) {
        if (diff > 0) { finalWidths[i % pending.length] += 1; diff -= 1 }
        else { finalWidths[i % pending.length] -= 1; diff += 1 }
        i++
      }
    }

    let x = 0
    const rowItems: LayoutItemResult<T>[] = []
    pending.forEach((item, i) => {
      rowItems.push({
        ...item,
        scaledWidth: Math.max(1, finalWidths[i]),
        scaledHeight: Math.round(rowH),
        offsetX: Math.round(x),
        offsetY: Math.round(currentY),
      })
      x += finalWidths[i] + gap
    })

    rows.push({
      height: Math.round(rowH),
      y: Math.round(currentY),
      items: rowItems
    })

    currentY += Math.round(rowH) + gap
    pending = []
    pendingArSum = 0
  }

  for (const item of items) {
    const h = Math.max(1, item.h)
    const w = Math.max(1, item.w)
    const ar = Math.max(0.2, Math.min(5.0, w / h))
    pending.push(item)
    pendingArSum += ar

    const availableW = containerWidth - gap * (pending.length - 1)
    if (pendingArSum * targetRowHeight >= availableW) {
      commitRow(false)
    }
  }

  commitRow(true)

  return rows
}
