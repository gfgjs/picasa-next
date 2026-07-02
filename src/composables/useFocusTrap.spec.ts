// R1-8:焦点陷阱纯逻辑锁测试(node 环境,无 DOM)。
// DOM 接线(engage/release/keydown)无 jsdom 不可测,由 R1-8 手测步骤覆盖并如实标注。

import { describe, it, expect } from 'vitest'
import { nextTrapIndex } from './useFocusTrap'

describe('nextTrapIndex(Tab 循环索引)', () => {
  it('正向步进且尾部回卷到 0', () => {
    expect(nextTrapIndex(3, 0, false)).toBe(1)
    expect(nextTrapIndex(3, 1, false)).toBe(2)
    expect(nextTrapIndex(3, 2, false)).toBe(0)
  })

  it('反向步进且头部回卷到尾', () => {
    expect(nextTrapIndex(3, 2, true)).toBe(1)
    expect(nextTrapIndex(3, 1, true)).toBe(0)
    expect(nextTrapIndex(3, 0, true)).toBe(2)
  })

  it('焦点不在列表内(-1):Tab 落到首个,Shift+Tab 落到末个', () => {
    expect(nextTrapIndex(5, -1, false)).toBe(0)
    expect(nextTrapIndex(5, -1, true)).toBe(4)
  })

  it('单元素容器:任意方向都钉在原地', () => {
    expect(nextTrapIndex(1, 0, false)).toBe(0)
    expect(nextTrapIndex(1, 0, true)).toBe(0)
  })
})
