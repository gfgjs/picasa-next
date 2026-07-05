// useBucketVirtualScroll(T16 方案B B1.5:等高算术分段 + 单飞取数管线)单测:
//   1) desiredSegmentRange 纯函数——愿望窗口的段区间数学(边距/边界/钳制);
//   2) 取数管线——单飞、视口中心优先、飞掠丢弃(反幽灵挂载)、换代作废、开关清空。
// node 环境,无 DOM:容器用普通对象伪造;composable 在组件外调用时 onMounted/
// onBeforeUnmount 为 no-op(仅 [Vue warn],ResizeObserver 永不构造),与方案 A spec 同法。
// 真机四根因(2026-07-04 诊断)在此的对应锁定:根因 A→「远跳后终点段最先取」;
// 根因 B→「离窗段在途应答丢弃、无幽灵挂载」;根因 C/D 由架构消除(无观察器/无全量占位)。
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { ref, nextTick } from 'vue'
import {
  useBucketVirtualScroll,
  desiredSegmentRange,
  SEGMENT_PX,
  PRELOAD_MARGIN_PX,
  BUCKET_NATIVE_MAX,
} from './useBucketVirtualScroll'
import type { LayoutRow } from '../types/layout'

beforeEach(() => {
  vi.spyOn(console, 'warn').mockImplementation(() => {})
  vi.spyOn(console, 'error').mockImplementation(() => {})
})

afterEach(() => {
  vi.restoreAllMocks()
})

// ── desiredSegmentRange:愿望窗口数学(默认 SEGMENT_PX=4000, MARGIN=1000) ──────

describe('desiredSegmentRange', () => {
  it('无内容/无视口 → null(含 NaN 防御)', () => {
    expect(desiredSegmentRange(0, 1000, 0)).toBeNull()
    expect(desiredSegmentRange(0, 0, 15_000)).toBeNull()
    expect(desiredSegmentRange(0, 1000, NaN)).toBeNull()
  })

  it('顶部视口:窗口 [0, viewH+margin) 只触及段 0', () => {
    // bottom = 0+1000+1000 = 2000 < 4000 → [0,0]
    expect(desiredSegmentRange(0, 1000, 15_000)).toEqual([0, 0])
  })

  it('跨界:窗口伸入下一段即纳入', () => {
    // top=2200, bottom=5200 → 段 0 与段 1
    expect(desiredSegmentRange(3200, 1000, 15_000)).toEqual([0, 1])
  })

  it('bottom 恰落段边界时该段不纳入(半开语义)', () => {
    // scrollTop=2000: bottom=4000 → last=floor(3999/4000)=0
    expect(desiredSegmentRange(2000, 1000, 15_000)).toEqual([0, 0])
    // 再多 1px 即纳入段 1
    expect(desiredSegmentRange(2001, 1000, 15_000)).toEqual([0, 1])
  })

  it('末段钳制:超底 scrollTop 收敛到最后一段', () => {
    // total=15000 → lastIndex = ceil(15000/4000)-1 = 3
    expect(desiredSegmentRange(999_999, 1000, 15_000)).toEqual([3, 3])
  })

  it('总高不足一段 → 恒 [0,0]', () => {
    expect(desiredSegmentRange(0, 1000, 2500)).toEqual([0, 0])
    expect(desiredSegmentRange(500, 1000, 2500)).toEqual([0, 0])
  })

  it('自定义段高/边距生效', () => {
    // seg=1000, margin=0, viewH=1000, scrollTop=1500 → top=1500, bottom=2500 → [1,2]
    expect(desiredSegmentRange(1500, 1000, 10_000, 1000, 0)).toEqual([1, 2])
  })

  it('常量防呆:适用域上限低于 WebView2 元素钳制;边距覆盖最大行高', () => {
    expect(BUCKET_NATIVE_MAX).toBeLessThan(16_777_216)
    expect(PRELOAD_MARGIN_PX).toBeGreaterThanOrEqual(1000) // 行高上限 ~450px,余量 2×+
    expect(SEGMENT_PX).toBeGreaterThan(PRELOAD_MARGIN_PX)
  })
})

// ── 取数管线 ─────────────────────────────────────────────────────────────────

type Deferred<T> = { promise: Promise<T>; resolve: (v: T) => void; reject: (e: unknown) => void }
function deferred<T>(): Deferred<T> {
  let resolve!: (v: T) => void
  let reject!: (e: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

function normalRow(y: number, height: number): LayoutRow {
  return { rowType: 'normal', y, height, items: [] }
}

/** 等待泵循环消化 resolve/reject(宏任务一跳,保证 await 链走完)。 */
function flush() {
  return new Promise((r) => setTimeout(r, 0))
}

function makeHarness(init?: {
  totalHeight?: number
  scrollTop?: number
  deferred?: boolean
  rows?: LayoutRow[]
}) {
  const enabled = ref(true)
  const version = ref(1)
  const totalHeight = ref(init?.totalHeight ?? 15_000)
  const container = {
    scrollTop: init?.scrollTop ?? 0,
    clientHeight: 1000,
    // scrollToLogicalY 的局部/非映射路径经 el.scrollTo 落位(behavior 在 node 无意义)。
    scrollTo(o: { top: number }) {
      container.scrollTop = o.top
    },
  }
  const fetchCalls: Array<[number, number]> = []
  const pendingFetches: Array<Deferred<LayoutRow[]>> = []
  // 注意:非 deferred 模式下构造即取数(immediate watch),初始应答须经 init.rows 注入。
  const state = { useDeferred: init?.deferred ?? false, rowsToReturn: init?.rows ?? [] }

  const bs = useBucketVirtualScroll({
    enabled: () => enabled.value,
    totalHeight: () => totalHeight.value,
    layoutVersion: () => version.value,
    fetchBucketRows: (s, e) => {
      fetchCalls.push([s, e])
      if (state.useDeferred) {
        const d = deferred<LayoutRow[]>()
        pendingFetches.push(d)
        return d.promise
      }
      return Promise.resolve(state.rowsToReturn)
    },
    containerRef: () => container as unknown as HTMLElement,
  })

  return { bs, enabled, version, totalHeight, container, fetchCalls, pendingFetches, state }
}

describe('useBucketVirtualScroll:单飞/优先/丢弃/换代', () => {
  it('构造即按初始视口建段并取数;应答落地 → ready', async () => {
    const h = makeHarness({ rows: [normalRow(0, 200), normalRow(200, 200)] })
    expect(h.fetchCalls).toEqual([[0, 4000]]) // 窗口 [0,2000) 只触段 0
    await flush()
    expect(h.bs.segments.value.length).toBe(1)
    expect(h.bs.segments.value[0].state).toBe('ready')
    expect(h.bs.mountedRows().length).toBe(2)
  })

  it('单飞:窗口含两段时串行取数,不并发', async () => {
    const h = makeHarness({ scrollTop: 3200, deferred: true })
    // 愿望 [0,1];泵先挑距中心(3700)近的段 0(中心 2000)
    expect(h.fetchCalls).toEqual([[0, 4000]])
    expect(h.pendingFetches.length).toBe(1) // 在途仅 1
    h.pendingFetches[0].resolve([normalRow(0, 100)])
    await flush()
    expect(h.fetchCalls).toEqual([
      [0, 4000],
      [4000, 8000],
    ])
  })

  it('远跳(根因 A/B 锁定):飞掠段被丢弃、其应答不落地(无幽灵),终点段最先取', async () => {
    const h = makeHarness({ deferred: true })
    expect(h.fetchCalls).toEqual([[0, 4000]]) // 段 0 在途

    // 滚动条拖到 12000:愿望变 [2,3],段 0 离窗被丢弃
    h.container.scrollTop = 12_000
    h.bs.onScroll()
    expect(h.bs.segments.value.map((s) => s.index)).toEqual([2, 3])

    // 陈旧应答落地 → 复核失败被丢弃:段表无 index 0、无行
    h.pendingFetches[0].resolve([normalRow(0, 100)])
    await flush()
    expect(h.bs.segments.value.map((s) => s.index)).toEqual([2, 3])
    expect(h.bs.mountedRows().length).toBe(0)

    // 泵继续:距视口中心(12500)更近的段 3(中心 13500)先于段 2(中心 10000)
    expect(h.fetchCalls[1]).toEqual([12_000, 15_000])
    h.pendingFetches[1].resolve([normalRow(12_000, 100)])
    await flush()
    expect(h.fetchCalls[2]).toEqual([8000, 12_000])
  })

  it('布局换代:段表重建,在途应答按代+对象双重丢弃,新代重取', async () => {
    const h = makeHarness({ deferred: true })
    h.version.value = 2
    await nextTick() // watch → rebuild(desired 换新对象)
    h.pendingFetches[0].resolve([normalRow(0, 100)]) // 旧代应答
    await flush()
    // 旧应答未落地;新代已为段 0 重新发起取数
    expect(h.fetchCalls.length).toBe(2)
    h.pendingFetches[1].resolve([normalRow(0, 100), normalRow(100, 100)])
    await flush()
    expect(h.bs.segments.value[0].state).toBe('ready')
    expect(h.bs.mountedRows().length).toBe(2)
  })

  it('fetch 失败 → error 粘滞(不重试风暴),换代恢复', async () => {
    const h = makeHarness({ deferred: true })
    h.pendingFetches[0].reject(new Error('LayoutNotReady'))
    await flush()
    expect(h.bs.segments.value[0].state).toBe('error')
    // 同窗滚动不触发重取(error 非 idle)
    h.bs.onScroll()
    await flush()
    expect(h.fetchCalls.length).toBe(1)
    // 换代 → 重建重取
    h.state.useDeferred = false
    h.state.rowsToReturn = [normalRow(0, 100)]
    h.version.value = 2
    await nextTick()
    await flush()
    expect(h.bs.segments.value[0].state).toBe('ready')
  })

  it('enabled=false → 段表清空、滚动不取数;重开 → 重建', async () => {
    const h = makeHarness()
    await flush()
    expect(h.bs.segments.value.length).toBe(1)

    h.enabled.value = false
    await nextTick()
    expect(h.bs.segments.value.length).toBe(0)
    const calls = h.fetchCalls.length
    h.bs.onScroll() // 休眠期滚动
    await flush()
    expect(h.fetchCalls.length).toBe(calls)

    h.enabled.value = true
    await nextTick()
    await flush()
    expect(h.bs.segments.value.length).toBe(1)
    expect(h.bs.segments.value[0].state).toBe('ready')
  })

  it('onScroll:logicalScrollTop 恒等于容器 scrollTop(零映射);同窗滚动为 no-op 快路径', async () => {
    const h = makeHarness()
    await flush()
    const calls = h.fetchCalls.length
    h.container.scrollTop = 300 // 窗口区间不变([0,0])
    h.bs.onScroll()
    expect(h.bs.logicalScrollTop.value).toBe(300)
    await flush()
    expect(h.fetchCalls.length).toBe(calls) // 区间未变 → 不重算/不取数
  })

  it('末段几何:end 钳制到 totalHeight', async () => {
    const h = makeHarness({ scrollTop: 999_999 }) // 钳到末段 [12000,15000)
    expect(h.fetchCalls).toEqual([[12_000, 15_000]])
    await flush()
    expect(h.bs.segments.value[0].end).toBe(15_000)
  })
})

// ── whenSettled:段稳定屏障(B2,FLIP 的 Last 快照前置) ───────────────────────

describe('whenSettled', () => {
  it('全段 ready 时立即兑现', async () => {
    const h = makeHarness({ rows: [normalRow(0, 100)] })
    await flush()
    let settled = false
    void h.bs.whenSettled().then(() => (settled = true))
    await flush()
    expect(settled).toBe(true)
  })

  it('有段在途时挂起,落地后兑现;error 段视为已稳定(不无限等待)', async () => {
    const h = makeHarness({ scrollTop: 3200, deferred: true }) // 愿望 [0,1],段 0 在途
    let settled = false
    void h.bs.whenSettled().then(() => (settled = true))
    await flush()
    expect(settled).toBe(false)

    h.pendingFetches[0].resolve([normalRow(0, 100)]) // 段 0 ready → 段 1 开始在途
    await flush()
    expect(settled).toBe(false)

    h.pendingFetches[1].reject(new Error('boom')) // 段 1 error → 全部非 idle/loading
    await flush()
    expect(settled).toBe(true)
  })

  it('引擎停用(愿望集清空)兑现等待者——引擎切换瞬间的 FLIP 不悬挂', async () => {
    const h = makeHarness({ deferred: true })
    let settled = false
    void h.bs.whenSettled().then(() => (settled = true))
    await flush()
    expect(settled).toBe(false)

    h.enabled.value = false
    await nextTick()
    await flush()
    expect(settled).toBe(true)
  })
})

// ── 映射态(B3/B3.1):总高 > 16M 的段级坐标映射 ──────────────────────────────
// B3.1 输入源分类锁定:有 1:1 印记(onWheel/onKeydown/onTouchmove/程序化局部滚动)的
// 手势局部 1:1;无印记滚动链 = 滚动条拖动 → **逐事件**比例重锚(拖到边 = 逻辑边);
// 物理钉边由 onWheel 推锚差续滚到逻辑边缘,到边即硬停;偿债仅停稳后,手势中零 scrollTop
// 写入(真机「到边一跳一跳还能继续滚」回归锁)。手势链沿用起点分类——fake timers 驱动
// Date.now,用 advanceTimersByTimeAsync(150) 断链(< SETTLE_MS,不误触偿债)。

describe('映射态(B3):段级坐标映射', () => {
  const PHYS_MAX = 16_000_000 - 1000 // spacer 封顶 − viewH
  const LOG_MAX = 40_000_000 - 1000
  const gLogical = (p: number) => (p / PHYS_MAX) * LOG_MAX
  const gPhysical = (l: number) => (l / LOG_MAX) * PHYS_MAX
  const wheel = (dy: number) => ({ deltaY: dy, deltaMode: 0 }) as unknown as WheelEvent

  it('spacer 封顶 16M;初始零锚差', () => {
    const h = makeHarness({ totalHeight: 40_000_000 })
    expect(h.bs.spacerHeight.value).toBe(16_000_000)
    expect(h.bs.anchorDelta.value).toBe(0)
    expect(h.fetchCalls[0]).toEqual([0, 4000])
  })

  it('单事件巨跳(拇指跳转/End)→ 全局线性重锚:拇指位置 ≈ 库内比例', async () => {
    const h = makeHarness({ totalHeight: 40_000_000 })
    await flush()
    h.container.scrollTop = 8_000_000
    h.bs.onScroll()
    const L = gLogical(8_000_000)
    expect(h.bs.logicalScrollTop.value).toBeCloseTo(L, 5)
    expect(h.bs.anchorDelta.value).toBeCloseTo(L - 8_000_000, 5)
    // 愿望窗口按逻辑位选段(段几何仍是逻辑坐标,物理定位靠模板减锚差)
    expect(h.bs.segments.value[0].start).toBe(Math.floor((L - 1000) / 4000) * 4000)
  })

  it('滚轮印记内的小位移手势:零重锚,1 物理 px = 1 逻辑 px', async () => {
    vi.useFakeTimers()
    try {
      const h = makeHarness({ totalHeight: 40_000_000 })
      h.container.scrollTop = 8_000_000
      h.bs.onScroll() // 巨跳 → 比例重锚(债 0)
      await vi.advanceTimersByTimeAsync(150) // 断开手势链
      h.bs.onWheel(wheel(120)) // 盖 1:1 印记(未钉边 → 仅印记)
      const delta = h.bs.anchorDelta.value
      const l0 = h.bs.logicalScrollTop.value
      h.container.scrollTop += 500
      h.bs.onScroll() // 新手势,印记内 → 局部 1:1
      h.container.scrollTop += 500
      h.bs.onScroll() // 链内沿用
      expect(h.bs.anchorDelta.value).toBe(delta)
      expect(h.bs.logicalScrollTop.value).toBeCloseTo(l0 + 1000, 8)
    } finally {
      vi.useRealTimers()
    }
  })

  it('无印记慢速滚动链 = 滚动条拖动:逐事件比例重锚,拖到底即逻辑底(真机回归)', async () => {
    vi.useFakeTimers()
    try {
      const h = makeHarness({ totalHeight: 40_000_000 })
      // 从静止慢拖:每事件 2000px ≪ 巨跳阈值——B3 初版误判 1:1,B3.1 链内沿用「比例」
      for (let p = 2000; p <= 10_000; p += 2000) {
        h.container.scrollTop = p
        h.bs.onScroll()
        expect(h.bs.logicalScrollTop.value).toBeCloseTo(gLogical(p), 5)
      }
      // 慢拖到物理底 → 逻辑恰为库底,不存在「还能继续滚」的钉住态
      h.container.scrollTop = PHYS_MAX
      h.bs.onScroll()
      expect(h.bs.logicalScrollTop.value).toBeCloseTo(LOG_MAX, 5)
      // 逆向慢拖离底:仍是链内比例,而非 1:1
      h.container.scrollTop = PHYS_MAX - 3000
      h.bs.onScroll()
      expect(h.bs.logicalScrollTop.value).toBeCloseTo(gLogical(PHYS_MAX - 3000), 5)
    } finally {
      vi.useRealTimers()
    }
  })

  it('停稳偿债:scrollTop 归位全局线性位,逻辑位(内容)零位移', async () => {
    vi.useFakeTimers()
    try {
      const h = makeHarness({ totalHeight: 40_000_000 })
      h.container.scrollTop = 8_000_000
      h.bs.onScroll() // 巨跳重锚 → 债 0
      await vi.advanceTimersByTimeAsync(150) // 断链(未到停稳)
      h.bs.onWheel(wheel(120))
      h.container.scrollTop += 3000
      h.bs.onScroll() // 印记内新手势 → 局部 1:1 → 积累压缩债
      const l = h.bs.logicalScrollTop.value
      expect(l).toBeCloseTo(gLogical(8_000_000) + 3000, 5)
      await vi.advanceTimersByTimeAsync(250) // 停稳 → repayDebt(内部 nextTick 已被 flush)
      await nextTick()
      expect(h.container.scrollTop).toBe(Math.round(gPhysical(l)))
      expect(h.container.scrollTop + h.bs.anchorDelta.value).toBeCloseTo(l, 5)
      expect(h.bs.logicalScrollTop.value).toBeCloseTo(l, 5)
    } finally {
      vi.useRealTimers()
    }
  })

  it('物理钉顶后滚轮续滚:onWheel 推锚差直达逻辑 0,手势中零 scrollTop 写入', async () => {
    vi.useFakeTimers()
    try {
      const h = makeHarness({ totalHeight: 40_000_000 })
      h.container.scrollTop = 100_000
      h.bs.onScroll() // 巨跳 → 比例重锚(delta > 0)
      expect(h.bs.anchorDelta.value).toBeGreaterThan(0)
      await vi.advanceTimersByTimeAsync(150)
      h.bs.onWheel(wheel(-120)) // 印记
      for (let p = 95_000; p >= 0; p -= 5000) {
        h.container.scrollTop = p // 链内 1:1
        h.bs.onScroll()
      }
      expect(h.bs.logicalScrollTop.value).toBeGreaterThan(1) // p=0 但逻辑未到顶:钉住态
      // 真机「一跳一跳」回归锁:手势进行中不得偿债改 scrollTop
      expect(h.container.scrollTop).toBe(0)
      // 继续滚轮上滚:原生已无事件,onWheel 直接推锚差,直到真正的逻辑顶
      let guard = 0
      while (h.bs.logicalScrollTop.value > 0 && ++guard < 10_000) {
        h.bs.onWheel(wheel(-40_000))
      }
      expect(h.bs.logicalScrollTop.value).toBe(0)
      expect(h.bs.anchorDelta.value).toBe(0) // L=0,p=0 → 锚差归零
      expect(h.container.scrollTop).toBe(0) // 全程未写 scrollTop
      expect(h.bs.segments.value[0].start).toBe(0) // 愿望窗口已达顶
    } finally {
      vi.useRealTimers()
    }
  })

  it('物理钉底后滚轮续滚:直达逻辑底后硬停,不再「还能继续滚」', async () => {
    vi.useFakeTimers()
    try {
      const h = makeHarness({ totalHeight: 40_000_000 })
      h.container.scrollTop = PHYS_MAX - 5000
      h.bs.onScroll() // 巨跳 → 比例重锚
      await vi.advanceTimersByTimeAsync(150)
      h.bs.onWheel(wheel(120)) // 印记
      h.container.scrollTop = PHYS_MAX // 1:1 下滚 5000 → 钉底
      h.bs.onScroll()
      expect(h.bs.logicalScrollTop.value).toBeLessThan(LOG_MAX - 1) // 钉底但逻辑未到底
      let guard = 0
      while (h.bs.logicalScrollTop.value < LOG_MAX && ++guard < 10_000) {
        h.bs.onWheel(wheel(40_000))
      }
      expect(h.bs.logicalScrollTop.value).toBe(LOG_MAX)
      // 到达逻辑底后再滚:钉边条件不再成立 → no-op 硬停
      const dAtEnd = h.bs.anchorDelta.value
      h.bs.onWheel(wheel(1000))
      expect(h.bs.anchorDelta.value).toBe(dAtEnd)
      expect(h.bs.logicalScrollTop.value).toBe(LOG_MAX)
      expect(h.container.scrollTop).toBe(PHYS_MAX) // 全程未写 scrollTop
    } finally {
      vi.useRealTimers()
    }
  })

  it('scrollToLogicalY 近距平滑的后续 scroll 事件按 1:1 分类,不被当拖动重锚', async () => {
    vi.useFakeTimers()
    try {
      const h = makeHarness({ totalHeight: 40_000_000 })
      h.container.scrollTop = 8_000_000
      h.bs.onScroll()
      await vi.advanceTimersByTimeAsync(150)
      await h.bs.scrollToLogicalY(h.bs.logicalScrollTop.value + 2500, { smooth: true })
      const delta = h.bs.anchorDelta.value
      // 模拟 smooth 动画产生的 scroll 事件(harness scrollTo 已同步落位,补发事件)
      h.bs.onScroll()
      expect(h.bs.anchorDelta.value).toBe(delta) // 程序化印记生效:1:1,零重锚
    } finally {
      vi.useRealTimers()
    }
  })

  it('scrollToLogicalY 远跳:全局重锚 + 立即落点,愿望窗口即达目标', async () => {
    const h = makeHarness({ totalHeight: 40_000_000 })
    await flush()
    await h.bs.scrollToLogicalY(30_000_000)
    expect(h.container.scrollTop).toBe(Math.round(gPhysical(30_000_000)))
    expect(h.bs.logicalScrollTop.value).toBe(30_000_000)
    expect(h.bs.segments.value[0].start).toBe(Math.floor((30_000_000 - 1000) / 4000) * 4000)
  })

  it('scrollToLogicalY 近距(≤3 屏):不重锚,局部落位', async () => {
    const h = makeHarness({ totalHeight: 40_000_000 })
    await flush()
    await h.bs.scrollToLogicalY(30_000_000)
    const delta = h.bs.anchorDelta.value
    await h.bs.scrollToLogicalY(30_002_000, { smooth: true })
    expect(h.bs.anchorDelta.value).toBe(delta)
    expect(h.container.scrollTop).toBeCloseTo(30_002_000 - delta, 5)
  })

  it('非映射态 scrollToLogicalY 直滚零锚差(回归)', async () => {
    const h = makeHarness() // 15000 ≤ 16M
    await flush()
    await h.bs.scrollToLogicalY(5000)
    expect(h.container.scrollTop).toBe(5000)
    expect(h.bs.anchorDelta.value).toBe(0)
  })
})
