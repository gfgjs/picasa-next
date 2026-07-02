// src/composables/selection/classicMode.spec.ts
// Part5 T4a 回归基线:锁死 classic 策略在 explicit / all 两态下对全部意图的行为。
// 这是进入 T4b/T5（改动多选核心、回归风险最高）前的安全网。

import { describe, it, expect } from 'vitest'
import { classicMode } from './classicMode'
import type { SelectionContext, SelectionState } from './types'

// 构造测试上下文:rangeBetween 在给定布局序数组上取闭区间,镜像 useViewIds.rangeBetween 语义。
function makeCtx(viewIds: number[]): SelectionContext {
  return {
    viewIds,
    totalCount: viewIds.length,
    rangeBetween: (a, b) => {
      const ai = viewIds.indexOf(a)
      const bi = viewIds.indexOf(b)
      if (ai === -1 || bi === -1) return []
      const lo = Math.min(ai, bi)
      const hi = Math.max(ai, bi)
      return viewIds.slice(lo, hi + 1)
    },
  }
}

const explicit = (...ids: number[]): SelectionState => ({ kind: 'explicit', ids: new Set(ids) })
const all = (...excluded: number[]): SelectionState => ({ kind: 'all', excluded: new Set(excluded) })

// 断言 explicit 态的 id 集合（顺序无关）
function expectExplicit(state: SelectionState, ids: number[]) {
  expect(state.kind).toBe('explicit')
  if (state.kind === 'explicit') {
    expect([...state.ids].sort((a, b) => a - b)).toEqual([...ids].sort((a, b) => a - b))
  }
}
function expectAll(state: SelectionState, excluded: number[]) {
  expect(state.kind).toBe('all')
  if (state.kind === 'all') {
    expect([...state.excluded].sort((a, b) => a - b)).toEqual([...excluded].sort((a, b) => a - b))
  }
}

const ctx = makeCtx([1, 2, 3, 4, 5])

describe('classicMode.apply · replace', () => {
  it('从 explicit 替换为单元素', () => {
    expectExplicit(classicMode.apply(explicit(1, 2, 3), { type: 'replace', id: 4 }, ctx), [4])
  })
  it('从 all 也落到 explicit 单元素（替换语义清空全选）', () => {
    expectExplicit(classicMode.apply(all(2), { type: 'replace', id: 4 }, ctx), [4])
  })
})

describe('classicMode.apply · toggle', () => {
  it('explicit:未选则加入', () => {
    expectExplicit(classicMode.apply(explicit(1, 2), { type: 'toggle', id: 3 }, ctx), [1, 2, 3])
  })
  it('explicit:已选则移除', () => {
    expectExplicit(classicMode.apply(explicit(1, 2, 3), { type: 'toggle', id: 2 }, ctx), [1, 3])
  })
  it('all:翻转语义相反——翻转已选项 = 加入排除集', () => {
    // all 态下 id=3 当前「已选」(不在 excluded),toggle 应把它挖掉 → excluded 增加 3
    expectAll(classicMode.apply(all(), { type: 'toggle', id: 3 }, ctx), [3])
  })
  it('all:翻转已排除项 = 移出排除集（恢复选中）', () => {
    expectAll(classicMode.apply(all(3), { type: 'toggle', id: 3 }, ctx), [])
  })
})

describe('classicMode.apply · range', () => {
  it('explicit:并入布局序闭区间', () => {
    expectExplicit(
      classicMode.apply(explicit(1), { type: 'range', anchorId: 2, toId: 4 }, ctx),
      [1, 2, 3, 4],
    )
  })
  it('explicit:区间方向无关（anchor>to 同结果）', () => {
    expectExplicit(
      classicMode.apply(explicit(), { type: 'range', anchorId: 4, toId: 2 }, ctx),
      [2, 3, 4],
    )
  })
  it('all:区间表示「选中这些」→ 从排除集移除', () => {
    // all 排除 [2,3,4],对 [2..4] 做 range → 这些恢复选中 → excluded 清空
    expectAll(classicMode.apply(all(2, 3, 4), { type: 'range', anchorId: 2, toId: 4 }, ctx), [])
  })
  it('端点不在视图（rangeBetween 返空）→ explicit 选区不变', () => {
    expectExplicit(
      classicMode.apply(explicit(1), { type: 'range', anchorId: 99, toId: 4 }, ctx),
      [1],
    )
  })
})

describe('classicMode.apply · selectAll', () => {
  it('归一为 all 态、排除集为空（不物化 id）', () => {
    expectAll(classicMode.apply(explicit(1, 2), { type: 'selectAll' }, ctx), [])
  })
})

describe('classicMode.apply · clear', () => {
  it('explicit 清空', () => {
    expectExplicit(classicMode.apply(explicit(1, 2, 3), { type: 'clear' }, ctx), [])
  })
  it('all 清空 → 归一为 explicit 空态', () => {
    expectExplicit(classicMode.apply(all(2), { type: 'clear' }, ctx), [])
  })
})

describe('classicMode.apply · invert', () => {
  it('explicit → 全集补集', () => {
    expectExplicit(classicMode.apply(explicit(1, 3, 5), { type: 'invert' }, ctx), [2, 4])
  })
  it('all{excluded} → explicit{excluded}（补集恰为排除集,廉价路径）', () => {
    expectExplicit(classicMode.apply(all(2, 4), { type: 'invert' }, ctx), [2, 4])
  })
})

describe('classicMode.apply · 纯函数不变性', () => {
  it('不修改入参 state 的 Set', () => {
    const state = explicit(1, 2, 3)
    const snapshot = state.kind === 'explicit' ? [...state.ids] : []
    classicMode.apply(state, { type: 'toggle', id: 9 }, ctx)
    classicMode.apply(state, { type: 'range', anchorId: 1, toId: 5 }, ctx)
    classicMode.apply(state, { type: 'invert' }, ctx)
    expect(state.kind === 'explicit' && [...state.ids]).toEqual(snapshot)
  })
  it('返回的是新对象引用', () => {
    const state = explicit(1)
    expect(classicMode.apply(state, { type: 'toggle', id: 2 }, ctx)).not.toBe(state)
  })
})
