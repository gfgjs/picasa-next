// createWheelAnimator(H-Lab 滚轮动画器)确定性锁测。
// 这条路径六轮手测四次翻车(方向乱跳/连滚吞量/缓入迟滞/极快启动顿滞),按
// 「测点由风险决定」纪律锁定终态实现(帧积分式定时长线性重定标)的全部安全性质:
//   ① 帧 dt 钳 0——rAF 时间戳早于输入时钟(三轮方向乱跳根因)不得反向外插;
//   ② 方向单调由构造保证——每帧写值恒在 [当前位置, 目标] 区间内;
//   ③ 目标累积不丢滚动量(四轮连滚吞量教训);
//   ④ 高频输入速度不塌缩——输入不重置帧积分锚(六轮极快启动顿滞根因);
//   ⑤ 反向输入/外源位移立即重基;⑥ 边界钳制;⑦ 定时长必然终止。
// 范围:仅动画器数学。onWheel 的 deltaMode 归一/触摸板放行是事件层薄壳,不在此测;
// 观感指标(跟手/眩晕)自动化不可覆盖,见 plan §5 手测清单。
import { describe, it, expect } from 'vitest'
import { createWheelAnimator, type WheelScrollEl } from './useHVirtualScroll'

// ── 测试夹具:手动驱动的 raf/时钟 ────────────────────────────────────────────
// 动画器同一时刻至多挂一个 rAF(rafId 守卫),单槽 pending 忠实建模。
interface Rig {
  el: WheelScrollEl
  anim: ReturnType<typeof createWheelAnimator>
  /** 推进输入侧时钟(performance.now 注入),供 push 读取。 */
  setClock: (ms: number) => void
  /** 触发挂起的 rAF 回调并传入帧时间戳;返回是否确有回调被触发。 */
  fire: (frameTs: number) => boolean
  /** 当前是否有挂起的 rAF(= 动画仍在跑)。 */
  hasPending: () => boolean
}

function makeRig(elInit?: Partial<WheelScrollEl>, durMs?: number): Rig {
  const el: WheelScrollEl = { scrollLeft: 0, scrollWidth: 10_000, clientWidth: 1_000, ...elInit }
  let clock = 0
  let pending: FrameRequestCallback | null = null
  const anim = createWheelAnimator({
    el: () => el,
    durMs,
    raf: (cb) => {
      pending = cb
      return 1
    },
    caf: () => {
      pending = null
    },
    now: () => clock,
  })
  return {
    el,
    anim,
    setClock: (ms) => {
      clock = ms
    },
    fire: (frameTs) => {
      const cb = pending
      pending = null
      if (!cb) return false
      cb(frameTs)
      return true
    },
    hasPending: () => pending !== null,
  }
}

/** 一直触发帧直到动画自然终止(帧时间戳足够晚,t 必达 1)。 */
function finish(rig: Rig, lateTs = 1_000_000) {
  let guard = 0
  while (rig.fire(lateTs)) {
    if (++guard > 10) throw new Error('动画未在预期帧数内终止')
  }
}

describe('createWheelAnimator · 定时长线性重定标', () => {
  it('线性插值:中点时刻恰在中点(核心数学)', () => {
    const rig = makeRig({}, 200)
    rig.setClock(0)
    rig.anim.push(200)
    rig.fire(100) // t = 0.5
    expect(rig.el.scrollLeft).toBe(100)
    rig.fire(150) // t = 0.75
    expect(rig.el.scrollLeft).toBe(150)
    rig.fire(200) // t = 1 → 终点并终止
    expect(rig.el.scrollLeft).toBe(200)
    expect(rig.hasPending()).toBe(false)
  })

  it('负帧差钳 0:rAF 帧时间戳早于输入时钟时原地不动,绝不反向(三轮方向乱跳根因)', () => {
    const rig = makeRig({ scrollLeft: 500 })
    rig.setClock(1000)
    rig.anim.push(100) // target=600, 帧锚播种于 1000
    rig.fire(990) // 帧时间戳早于输入时钟 10ms → dt<0 钳 0
    expect(rig.el.scrollLeft).toBe(500) // 原地 no-op,而非 500-ε 的反向外插
    expect(rig.hasPending()).toBe(true) // 动画未终止,继续跑
    finish(rig)
    expect(rig.el.scrollLeft).toBe(600)
  })

  it('方向单调:连滚 + 乱序帧时间戳下位置不回退、不越过目标', () => {
    const rig = makeRig()
    const writes: number[] = []
    const record = () => writes.push(rig.el.scrollLeft)

    rig.setClock(0)
    rig.anim.push(100)
    rig.fire(-5) // 乱序:早于 start
    record()
    rig.fire(40)
    record()
    rig.setClock(50)
    rig.anim.push(100) // 重定标:from=当前位,target 累积
    rig.fire(45) // 乱序:早于新 start → 写新起点
    record()
    rig.fire(120)
    record()
    finish(rig)
    record()

    for (let i = 1; i < writes.length; i++) {
      expect(writes[i]).toBeGreaterThanOrEqual(writes[i - 1])
    }
    expect(writes[writes.length - 1]).toBe(200) // 两格全额到账
  })

  it('高频输入速度不塌缩:输入快于帧率时仍按帧间隔积分(六轮极快启动顿滞根因)', () => {
    const rig = makeRig()
    // 极快起手:每 4ms 一格(高于 16ms 帧率),帧到来前已积累 4 格
    rig.setClock(0)
    rig.anim.push(100) // 帧锚播种于 0,deadline=160
    rig.setClock(4)
    rig.anim.push(100)
    rig.setClock(8)
    rig.anim.push(100)
    rig.setClock(12)
    rig.anim.push(100) // target=400, deadline=172
    rig.fire(16)
    // 帧积分:dt=16(完整帧间隔), timeLeft=172-0=172 → 位移 = 400×16/172 ≈ 37.2px。
    // 旧锚点插值实现:每次输入重置时间锚 → t=(16-12)/160=2.5% → 仅 10px,
    // 速度塌缩近 4 倍,起手形同冻结(「顿一下」的根因)。
    const d1 = rig.el.scrollLeft
    expect(d1).toBeCloseTo((400 * 16) / 172, 6)
    // 突发继续:再 3 格,帧 2 位移必须大于帧 1(随累积加速,而非停滞)
    rig.setClock(20)
    rig.anim.push(100)
    rig.setClock(24)
    rig.anim.push(100)
    rig.setClock(28)
    rig.anim.push(100) // target=700, deadline=188
    rig.fire(32) // dt=16, timeLeft=188-16=172
    const d2 = rig.el.scrollLeft - d1
    expect(d2).toBeCloseTo(((700 - d1) * 16) / 172, 6)
    expect(d2).toBeGreaterThan(d1)
    finish(rig)
    expect(rig.el.scrollLeft).toBe(700) // 突发全额到账
  })

  it('目标累积:快速三连滚不丢滚动量(四轮连滚吞量教训)', () => {
    const rig = makeRig()
    rig.setClock(0)
    rig.anim.push(100)
    rig.fire(16)
    rig.setClock(30)
    rig.anim.push(100)
    rig.fire(46)
    rig.setClock(60)
    rig.anim.push(100)
    finish(rig)
    expect(rig.el.scrollLeft).toBe(300)
  })

  it('完成即终止且目标作废:结束后不再有挂起帧,后续输入从当前位置重新起算', () => {
    const rig = makeRig()
    rig.setClock(0)
    rig.anim.push(100)
    finish(rig)
    expect(rig.el.scrollLeft).toBe(100)
    expect(rig.hasPending()).toBe(false)
    // 外源大位移(如拖滚动条)后再滚:必须基于新位置,不得复用已完成的旧目标
    rig.el.scrollLeft = 1000
    rig.setClock(500)
    rig.anim.push(100)
    finish(rig)
    expect(rig.el.scrollLeft).toBe(1100)
  })

  it('反向输入立即重基:从当前位置反向起算,不与旧目标抵消', () => {
    const rig = makeRig()
    rig.setClock(0)
    rig.anim.push(300) // target=300
    rig.fire(80) // t=0.5 → 150
    expect(rig.el.scrollLeft).toBe(150)
    rig.setClock(80)
    rig.anim.push(-100) // 反向:base=当前位 150(非旧目标 300)→ target=50
    finish(rig)
    expect(rig.el.scrollLeft).toBe(50)
  })

  it('外源位移越过目标后同向输入重基:不回跳向旧目标', () => {
    const rig = makeRig()
    rig.setClock(0)
    rig.anim.push(100) // target=100
    rig.fire(80) // 动画中,位置 50
    rig.el.scrollLeft = 400 // 外源(拖滚动条)把位置拖过目标
    rig.setClock(100)
    rig.anim.push(100) // (100-400)*dy < 0 → 从 400 重算,target=500
    finish(rig)
    expect(rig.el.scrollLeft).toBe(500) // 而非回跳到 100/200
  })

  it('边界钳制:目标不越过 [0, scrollWidth - clientWidth]', () => {
    const right = makeRig({ scrollLeft: 8_950 }) // maxLeft = 9000
    right.setClock(0)
    right.anim.push(1_000_000)
    finish(right)
    expect(right.el.scrollLeft).toBe(9_000)

    const left = makeRig({ scrollLeft: 30 })
    left.setClock(0)
    left.anim.push(-500)
    finish(left)
    expect(left.el.scrollLeft).toBe(0)
  })

  it('stop():取消挂起帧、作废目标,位置停在当前值', () => {
    const rig = makeRig()
    rig.setClock(0)
    rig.anim.push(200)
    rig.fire(80) // t=0.5 → 100
    rig.anim.stop()
    expect(rig.hasPending()).toBe(false)
    expect(rig.el.scrollLeft).toBe(100) // 停在中途,不跳到目标
    // stop 后输入从当前位置重新起算
    rig.setClock(200)
    rig.anim.push(50)
    finish(rig)
    expect(rig.el.scrollLeft).toBe(150)
  })

  it('el 为 null 时全程安全 no-op(push 与飞行中的帧)', () => {
    let el: WheelScrollEl | null = { scrollLeft: 0, scrollWidth: 5_000, clientWidth: 1_000 }
    let pending: FrameRequestCallback | null = null
    let clock = 0
    const anim = createWheelAnimator({
      el: () => el,
      raf: (cb) => {
        pending = cb
        return 1
      },
      caf: () => {
        pending = null
      },
      now: () => clock,
    })
    anim.push(100)
    el = null // 容器卸载
    const cb = pending!
    pending = null
    expect(() => cb(50)).not.toThrow()
    expect(pending).toBeNull() // 不再续排帧
    clock = 100
    expect(() => anim.push(100)).not.toThrow()
    expect(pending).toBeNull()
  })
})
