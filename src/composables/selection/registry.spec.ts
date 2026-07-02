// src/composables/selection/registry.spec.ts
// Part5 T4a 验收 §9.7:用一个 stub 模式证明 SelectionMode.apply 可替换、可与 classic 并存。
// 这是「可插拔 seam 就位」的可执行证据——加模式无需改协议/消费方/后端。

import { describe, it, expect } from 'vitest'
import {
  getSelectionMode,
  registerSelectionMode,
  listSelectionModeIds,
  DEFAULT_MODE_ID,
} from './registry'
import type { SelectionMode, SelectionState } from './types'

// 一个行为与 classic 截然不同的最小 stub:无论何意图,永远清空为 explicit 空态。
// 行为本身不重要,重点是「能注册、能取出、apply 被真正调用、且不影响 classic」。
const nukeStub: SelectionMode = {
  id: 'nuke-stub',
  label: '测试桩(清空一切)',
  apply(): SelectionState {
    return { kind: 'explicit', ids: new Set() }
  },
}

describe('registry · 默认与回退', () => {
  it('默认模式 id 为 classic', () => {
    expect(DEFAULT_MODE_ID).toBe('classic')
    expect(getSelectionMode().id).toBe('classic')
  })
  it('未知 id 回退到 classic（调度层永不拿到 undefined）', () => {
    expect(getSelectionMode('does-not-exist').id).toBe('classic')
  })
})

describe('registry · 可插拔(§9.7)', () => {
  it('注册 stub → 列表含 classic 与 stub（并存）', () => {
    expect(registerSelectionMode(nukeStub)).toBe(true) // 首次注册为新增
    const ids = listSelectionModeIds()
    expect(ids).toContain('classic')
    expect(ids).toContain('nuke-stub')
  })

  it('取出 stub 即为注入实例,apply 走 stub 逻辑而非 classic', () => {
    registerSelectionMode(nukeStub)
    const mode = getSelectionMode('nuke-stub')
    expect(mode).toBe(nukeStub)
    // classic 对 toggle 会加入元素;stub 则清空 → 以此区分确实换了实现
    const out = mode.apply(
      { kind: 'explicit', ids: new Set([1, 2, 3]) },
      { type: 'toggle', id: 9 },
      { viewIds: [1, 2, 3], totalCount: 3, rangeBetween: () => [] },
    )
    expect(out).toEqual({ kind: 'explicit', ids: new Set() })
  })

  it('注册 stub 后 classic 仍可独立取出、行为不被污染（并存非覆盖）', () => {
    registerSelectionMode(nukeStub)
    const classic = getSelectionMode('classic')
    expect(classic.id).toBe('classic')
    const out = classic.apply(
      { kind: 'explicit', ids: new Set([1]) },
      { type: 'toggle', id: 2 },
      { viewIds: [1, 2], totalCount: 2, rangeBetween: () => [] },
    )
    // classic 的 toggle 应加入 2,证明它没被 stub 取代
    expect(out.kind === 'explicit' && [...out.ids].sort()).toEqual([1, 2])
  })

  it('同 id 重复注册为覆盖（返回 false 表示非新增）', () => {
    registerSelectionMode(nukeStub)
    expect(registerSelectionMode(nukeStub)).toBe(false)
  })
})
