// TimelineScrubber 的纯映射逻辑（与组件分离以便单测，Part5 §3.3）。
// 无 DOM / 无响应式依赖 —— 时间均布的 index↔逻辑 y 映射、密度归一化、年份边界判定，是 scrubber
// 最易藏 off-by-one 的部分（frac=1 越界、末月 +∞ 上界、空桶）。组件里依赖 getBoundingClientRect /
// 指针事件的部分留在 .vue（DOM 相关，不在此测）。
//
// 入参用最小结构化类型（只取所需字段）而非整个 MonthBucket，便于测试构造轻量 fixture。

/** 最热月项数（密度归一化分母）；至少 1 防除零（与组件 reduce(...,1) 一致）。 */
export function maxBucketCount(buckets: readonly { count: number }[]): number {
  return buckets.reduce((m, b) => Math.max(m, b.count), 1)
}

/**
 * 密度热力条宽度（占轨道宽百分比）：保底 `floorPct` 让「有但少」的月也可见，其余按比例铺到 100%。
 * @param count 该月项数
 * @param maxCount 最热月项数（归一化分母）
 * @param floorPct 保底百分比（默认 12）
 */
export function densityBarWidth(count: number, maxCount: number, floorPct = 12): number {
  const safeMax = maxCount > 0 ? maxCount : 1
  return floorPct + (count / safeMax) * (100 - floorPct)
}

/**
 * 当前逻辑 y 落在哪个月：buckets 按显示序排列，月 i 覆盖 `[b[i].y, b[i+1].y)`（末月上界 +∞）。
 * 空桶返回 -1；y 在首月之前等未命中区间时兜底返回 0（最新月）。
 */
export function findActiveMonthIndex(
  buckets: readonly { y: number }[],
  currentY: number,
): number {
  if (buckets.length === 0) return -1
  for (let i = 0; i < buckets.length; i++) {
    const nextY = i + 1 < buckets.length ? buckets[i + 1].y : Infinity
    if (currentY >= buckets[i].y && currentY < nextY) return i
  }
  return 0
}

/** 是否某年首月：i=0 恒真；否则与上一桶年份不同处为真（最新→最旧排列下即每年最上一格）。 */
export function isYearBoundary(buckets: readonly { year: number }[], i: number): boolean {
  if (i === 0) return true
  return buckets[i].year !== buckets[i - 1].year
}

/**
 * 轨道纵向比例 → 月索引：`floor(frac*n)` 并 clamp 到 `[0, n-1]`（frac=1 时不越界到 n）。
 * @param frac 已 clamp 到 [0,1] 的纵向比例
 * @param monthCount 月数；<=0 返回 0（无月可指）
 */
export function fractionToMonthIndex(frac: number, monthCount: number): number {
  if (monthCount <= 0) return 0
  return Math.min(monthCount - 1, Math.floor(frac * monthCount))
}
