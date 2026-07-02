import { describe, it, expect } from 'vitest'
import {
  maxBucketCount,
  densityBarWidth,
  findActiveMonthIndex,
  isYearBoundary,
  fractionToMonthIndex,
} from './timelineScrubber.helpers'

// TimelineScrubber 纯映射逻辑回归（Part5 §3.3）。scrubber 视觉/手感无法在 node 环境验证，
// 但其映射数学（index↔y、密度归一化、边界）可测 —— 本 spec 锁定这些最易 off-by-one 的点。

describe('maxBucketCount', () => {
  it('空桶返回 1（防除零的种子值）', () => {
    expect(maxBucketCount([])).toBe(1)
  })
  it('全为 0 时仍返回 1（种子兜底）', () => {
    expect(maxBucketCount([{ count: 0 }, { count: 0 }])).toBe(1)
  })
  it('取最大项数', () => {
    expect(maxBucketCount([{ count: 3 }, { count: 17 }, { count: 5 }])).toBe(17)
  })
})

describe('densityBarWidth', () => {
  it('count=0 → 保底 12%', () => {
    expect(densityBarWidth(0, 100)).toBe(12)
  })
  it('count=maxCount → 满铺 100%', () => {
    expect(densityBarWidth(50, 50)).toBe(100)
  })
  it('半值 → 12 + 50%*88 = 56%', () => {
    expect(densityBarWidth(50, 100)).toBeCloseTo(56)
  })
  it('maxCount=0 异常输入按 1 处理不崩（除零防护）', () => {
    expect(densityBarWidth(0, 0)).toBe(12)
  })
})

describe('findActiveMonthIndex', () => {
  // 三个月：[y=0, y=100, y=300)，末月上界 +∞。
  const buckets = [{ y: 0 }, { y: 100 }, { y: 300 }]

  it('空桶返回 -1', () => {
    expect(findActiveMonthIndex([], 50)).toBe(-1)
  })
  it('y 在首月区间 → 0', () => {
    expect(findActiveMonthIndex(buckets, 50)).toBe(0)
  })
  it('y 落在区间下边界（含左闭）→ 命中该月', () => {
    expect(findActiveMonthIndex(buckets, 100)).toBe(1)
  })
  it('y 在中间月区间内 → 该月', () => {
    expect(findActiveMonthIndex(buckets, 250)).toBe(1)
  })
  it('y 落入末月（+∞ 上界，再大也命中）→ 末月', () => {
    expect(findActiveMonthIndex(buckets, 99999)).toBe(2)
    expect(findActiveMonthIndex(buckets, 300)).toBe(2)
  })
  it('y 在首月之前（未命中任何区间）兜底 → 0', () => {
    expect(findActiveMonthIndex([{ y: 100 }, { y: 200 }], 50)).toBe(0)
  })
})

describe('isYearBoundary', () => {
  // 最新→最旧：2025-03, 2025-02, 2024-12, 2024-11
  const buckets = [{ year: 2025 }, { year: 2025 }, { year: 2024 }, { year: 2024 }]

  it('i=0 恒为年首月', () => {
    expect(isYearBoundary(buckets, 0)).toBe(true)
  })
  it('同年内部不是年边界', () => {
    expect(isYearBoundary(buckets, 1)).toBe(false)
  })
  it('年份变化处是年边界', () => {
    expect(isYearBoundary(buckets, 2)).toBe(true)
  })
  it('下一年的内部月不是年边界', () => {
    expect(isYearBoundary(buckets, 3)).toBe(false)
  })
})

describe('fractionToMonthIndex', () => {
  it('frac=0 → 第 0 月', () => {
    expect(fractionToMonthIndex(0, 12)).toBe(0)
  })
  it('frac=1 → clamp 到 n-1（不越界到 n）', () => {
    expect(fractionToMonthIndex(1, 12)).toBe(11)
  })
  it('frac=0.5，12 月 → floor(6)=6', () => {
    expect(fractionToMonthIndex(0.5, 12)).toBe(6)
  })
  it('monthCount<=0（无月）→ 0', () => {
    expect(fractionToMonthIndex(0.5, 0)).toBe(0)
  })
})
