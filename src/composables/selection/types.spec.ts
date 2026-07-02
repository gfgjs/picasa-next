// src/composables/selection/types.spec.ts
// Part5 T4a 回归基线:锁死抽象访问器在两态下的语义（消费方唯一入口,不能错）。

import { describe, it, expect } from 'vitest'
import {
  isSelected,
  selectionSize,
  isEmptySelection,
  toDescriptor,
  EMPTY_SELECTION,
  type SelectionState,
} from './types'

const explicit = (...ids: number[]): SelectionState => ({ kind: 'explicit', ids: new Set(ids) })
const all = (...excluded: number[]): SelectionState => ({ kind: 'all', excluded: new Set(excluded) })

describe('isSelected', () => {
  it('explicit:在集合内为 true', () => {
    expect(isSelected(explicit(1, 2), 2)).toBe(true)
    expect(isSelected(explicit(1, 2), 3)).toBe(false)
  })
  it('all:不在排除集即选中（语义反转）', () => {
    expect(isSelected(all(2), 5)).toBe(true) // 全选中,5 未被排除 → 选中
    expect(isSelected(all(2), 2)).toBe(false) // 2 被排除 → 未选中
  })
})

describe('selectionSize', () => {
  it('explicit:等于集合大小（忽略 totalCount）', () => {
    expect(selectionSize(explicit(1, 2, 3), 1000)).toBe(3)
  })
  it('all:全集减排除集', () => {
    expect(selectionSize(all(2, 4), 10)).toBe(8)
  })
  it('all:排除集越界也不为负（下界裁 0）', () => {
    expect(selectionSize(all(1, 2, 3), 2)).toBe(0)
  })
})

describe('isEmptySelection', () => {
  it('explicit 空 → true,非空 → false', () => {
    expect(isEmptySelection(EMPTY_SELECTION)).toBe(true)
    expect(isEmptySelection(explicit(1))).toBe(false)
  })
  it('all 恒非空（清空会归一为 explicit 空态,故 all 永远非空）', () => {
    expect(isEmptySelection(all())).toBe(false)
    expect(isEmptySelection(all(1, 2, 3))).toBe(false)
  })
})

describe('toDescriptor', () => {
  // kind 小驼峰对齐后端 serde tag(R1-2 定形,后端锁测试 selection_descriptor_wire_format_locks_camel_case)。
  it('explicit → explicit{ids}', () => {
    const d = toDescriptor(explicit(3, 1, 2), { foo: 'bar' })
    expect(d.kind).toBe('explicit')
    if (d.kind === 'explicit') expect([...d.ids].sort()).toEqual([1, 2, 3])
  })
  it('all → selectAll{view, excludedIds},view 原样透传', () => {
    const view = { scope: 'album', id: 7 }
    const d = toDescriptor(all(4, 5), view)
    expect(d.kind).toBe('selectAll')
    if (d.kind === 'selectAll') {
      expect(d.view).toBe(view) // 泛型 view 原样带出,不被改写
      expect([...d.excludedIds].sort()).toEqual([4, 5])
    }
  })
})
