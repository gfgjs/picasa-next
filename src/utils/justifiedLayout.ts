// src/utils/justifiedLayout.ts
// A lightweight justified layout (waterfall) algorithm for frontend.
// 用于前端的轻量级等高自适应（瀑布流）布局算法。

export interface JustifiedLayoutItem {
  id: number
  width: number
  height: number
}

export interface PositionedItem {
  id: number
  x: number
  y: number
  w: number
  h: number
}

export interface JustifiedLayoutResult {
  items: PositionedItem[]
  totalHeight: number
}

/**
 * Computes a greedy justified layout.
 * 计算贪婪式自适应布局。
 */
export function computeJustifiedLayout<T extends JustifiedLayoutItem>(
  items: T[],
  containerWidth: number,
  targetRowHeight: number,
  gap: number
): JustifiedLayoutResult {
  if (!items || items.length === 0 || containerWidth <= 0) {
    return { items: [], totalHeight: 0 }
  }

  const resultItems: PositionedItem[] = []
  let currentX = 0
  let currentY = 0
  let rowBuffer: { item: T; scaledWidth: number }[] = []

  for (let i = 0; i < items.length; i++) {
    const item = items[i]
    // Avoid division by zero | 避免被零除
    const originalW = item.width || targetRowHeight
    const originalH = item.height || targetRowHeight

    // Scale to target row height | 缩放到目标行高
    const aspect = originalW / originalH
    const scaledWidth = targetRowHeight * aspect

    // Predict new row width | 预测新行宽
    const rowWidthWithItem = currentX + scaledWidth + (rowBuffer.length > 0 ? gap : 0)

    if (rowWidthWithItem > containerWidth && rowBuffer.length > 0) {
      // Row is full, calculate ratio | 行已满，计算比例
      const rowWidthWithoutGap = currentX - (rowBuffer.length - 1) * gap
      const availableWidth = containerWidth - (rowBuffer.length - 1) * gap
      const ratio = availableWidth / rowWidthWithoutGap

      const finalRowHeight = targetRowHeight * ratio
      let xOffset = 0

      for (const buf of rowBuffer) {
        const finalW = buf.scaledWidth * ratio
        resultItems.push({
          id: buf.item.id,
          x: xOffset,
          y: currentY,
          w: finalW,
          h: finalRowHeight
        })
        xOffset += finalW + gap
      }

      currentY += finalRowHeight + gap
      
      // Start new row | 开始新行
      rowBuffer = [{ item, scaledWidth }]
      currentX = scaledWidth
    } else {
      // Add to current row | 添加到当前行
      if (rowBuffer.length > 0) {
        currentX += gap
      }
      rowBuffer.push({ item, scaledWidth })
      currentX += scaledWidth
    }
  }

  // Handle last row (don't stretch it) | 处理最后一行（不拉伸）
  if (rowBuffer.length > 0) {
    let xOffset = 0
    for (const buf of rowBuffer) {
      resultItems.push({
        id: buf.item.id,
        x: xOffset,
        y: currentY,
        w: buf.scaledWidth,
        h: targetRowHeight
      })
      xOffset += buf.scaledWidth + gap
    }
    currentY += targetRowHeight
  }

  return {
    items: resultItems,
    totalHeight: currentY
  }
}
