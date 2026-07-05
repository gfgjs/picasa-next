// src/composables/useBucketVirtualScroll.ts
// T16 方案 B(B1.5):bucket 分段虚拟滚动——等高算术分段 + 单飞取数管线。
//
// 机制:废弃「单 spacer + 行级坐标压缩」——容器总高 = 真实逻辑总高,每段(bucket)一个
// 绝对定位 div(top=seg.start、height=段真实高),段内行以 (row.y - seg.start) 定位。
// 滚动/惯性/滚动条全走浏览器原生,**零坐标平移、零每帧补偿**(方案 A 平移模式不顺滑的
// 根因即「原生滚动 + JS 反向补偿」的双步差帧,本引擎从结构上消除它,见 T16 评估文档 §2)。
//
// B1.5 重构(2026-07-04,真机四根因修复;原 B1 语义边界 + IntersectionObserver 方案已废):
// - **等高算术分段**:段边界 = 0, S, 2S, …(与日期/目录语义无关)。get_bucket_rows 的
//   半开区间归属保证任意边界下每行恰属一段 → 三种分组(date/folder/none)统一覆盖,
//   段大小恒定 → 「超大单桶」(Immich #28861)从构造上消失。
// - **可见段 = 纯算术**(scrollTop/S 两次除法)→ 不再需要 IntersectionObserver,也不再
//   渲染全量占位 div(原方案数百段 div × 内联函数 ref × 每滚动帧重渲染 = 每帧数千次
//   observe churn,真机根因 C);只渲染愿望窗口内的 2-3 个段。
// - **愿望清单 + 单飞取数**:任意时刻至多 1 个 IPC 在途;出队时按「距视口中心最近」
//   重新挑选,应答落地前复核「该段仍被需要且仍是同一对象」——滚动条横扫时飞掠段自然
//   被跳过、终点段最先取(真机根因 A:取数风暴无优先级、终点段排队尾 → 白屏 1-2s);
//   离屏应答一律丢弃(真机根因 B:幽灵挂载致 DOM 无界驻留 → 选择模式全量重 patch 卡顿)。
//
// B3 段级坐标映射(2026-07-04):总高 > 物理 spacer 上限(16M,WebView2 2^24 钳制留余量)
// 时进入「映射态」——spacer 封顶,段以 (seg.start − anchorDelta) 物理定位。滚动语义按
// **输入源分类**(B3.1,2026-07-04 真机回报修复,见 onScroll/onWheel):
//  - **滚轮/触摸/滚动键**(有 1:1 印记):局部 1:1——1 物理 px = 1 逻辑 px,零补偿零重锚;
//    物理钉边后原生不再产生 scroll 事件,由 onWheel 推锚差续滚到**真正的逻辑边缘**;
//  - **滚动条拖动/轨道点击/Home/End**(无印记的滚动链,或单事件巨跳):**逐事件**全局
//    线性重锚——拇指位置 ≈ 库内比例(滚动条的用户心智模型),拖到边 = 逻辑边,构造上
//    无钉住(B3 初版只认单事件巨跳,慢拖被误判 1:1 → 拖到底逻辑远未到底);
//  - **压缩债**(局部 1:1 令拇指渐失真):仅在滚动**停稳**时原子偿还——同帧改
//    delta+scrollTop,内容零位移、仅拇指悄然归位。手势进行中绝不写 scrollTop(B3 初版
//    「钉边立即偿债」与拖拽/惯性互搏,真机表现为到边后一跳一跳还能继续滚,已废)。
// 与方案 A 行级每帧补偿的本质区别:1:1 路径零干预,重锚只发生在拖动/远跳/停稳这些
// 低频或本就非连续的事件上。
//
// 与方案 A 的关系:双引擎并存(ui.bucketSegmentedScroll,B0-B3.2 真机验收后**默认开**),
// 各自 enabled() 互斥激活,运行时即切即生效(T16 评估文档 §5 迁移策略);方案 A 保留为
// 回退引擎(设置关闭即回退)。

import { shallowRef, reactive, ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from 'vue'
import type { LayoutRow } from '../types/layout'

const LOG = '[BucketScroll]'

/// 物理 spacer 上限(px):低于 WebView2 单元素高度钳制(2^24 = 16,777,216)的保守值。
/// 总高 ≤ 此值 → 纯原生(零映射,B1.5 形态);总高 > 此值 → B3 映射态(spacer 封顶,段级重锚)。
export const BUCKET_NATIVE_MAX = 16_000_000

/// dev-only:localStorage['picasa.debug.bucketSpacer'] 覆盖 spacer 上限(镜像方案 A 的
/// debug.safeMax 先例)——调小(如 2_000_000)即可用中小库在真机触发 B3 映射态验收,
/// 免造百万项库。模块加载时读一次(零每帧开销),改值刷新生效、清除恢复默认。
export function resolveBucketSpacerCap(): number {
  try {
    if (import.meta.env.DEV) {
      const raw = localStorage.getItem('picasa.debug.bucketSpacer')
      const o = raw == null ? NaN : Number(raw)
      if (Number.isFinite(o) && o > 0) {
        console.info(LOG, `spacer cap overridden → ${o} (debug, B3)`)
        return o
      }
    }
  } catch {
    /* localStorage 不可用(隐私模式等)→ 默认 */
  }
  return BUCKET_NATIVE_MAX
}

const SPACER_CAP = resolveBucketSpacerCap()

/// 滚动停稳判定(ms):停稳后偿还映射态压缩债(repayDebt)。
const SCROLL_SETTLE_MS = 200

/// 手势链间隔(ms):相邻 scroll 事件间隔小于此值视为**同一手势**,沿用手势起点的输入源
/// 分类——触摸板惯性/平滑滚动动画的后续事件不会因印记过期被误判为滚动条拖动。
export const SCROLL_CHAIN_MS = 100

/// 1:1 输入印记时效(ms):wheel/touchmove/滚动键之后,此窗口内**新起**的滚动手势按局部
/// 1:1 分类;窗口外新起的滚动链只能来自滚动条拖动/轨道点击 → 按全局比例逐事件重锚。
export const ONE_TO_ONE_STICKY_MS = 250

/// 1:1 印记适格的滚动键。Home/End **有意不含**——其语义是文档边界,走巨跳兜底按全局
/// 比例重锚,恰好精确落到逻辑边界。
const ONE_TO_ONE_KEYS = new Set(['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', ' '])

/// 段高(px,约 2-4 屏):既是懒加载取数单元(一次 IPC ≈ 数十行),也是挂载粒度——
/// 愿望窗口通常只含 1-2 段,挂载 DOM 面与方案 A 同量级。注意这与「元素高度上限」是两个
/// 量级的问题:>16.7M 总高所需的粗粒度(~10M)段级映射是 B3 在本层之上的正交一层。
export const SEGMENT_PX = 4_000

/// 预取边距(px):愿望窗口 = 视口 ± 此值。须大于最大行高(跨界行归「行首 y 所在段」,
/// 由边距保证其所属段在该行可见前已被挂载),其余部分是纯预取余量。
export const PRELOAD_MARGIN_PX = 1_000

/// 渲染段 = 几何 + 懒加载状态。仅愿望窗口内的段存在(离窗即整体丢弃,无占位)。
export interface RenderSegment {
  /// 段序号(start = index × SEGMENT_PX)。模板 key。
  index: number
  start: number
  /// 段结束逻辑 y(不含)= min((index+1)×SEGMENT_PX, totalHeight)。
  end: number
  rows: LayoutRow[] | null
  state: 'idle' | 'loading' | 'ready' | 'error'
}

/**
 * 愿望窗口的段序号闭区间(纯函数,单测锁定):视口 ± margin 所触及的段。
 * 返回 null 表示无内容或视口不可用。
 */
export function desiredSegmentRange(
  scrollTop: number,
  viewportHeight: number,
  totalHeight: number,
  segmentPx: number = SEGMENT_PX,
  marginPx: number = PRELOAD_MARGIN_PX,
): [number, number] | null {
  if (!(totalHeight > 0) || !(viewportHeight > 0)) return null
  const lastIndex = Math.max(0, Math.ceil(totalHeight / segmentPx) - 1)
  const top = Math.max(0, scrollTop - marginPx)
  const bottom = Math.min(totalHeight, scrollTop + viewportHeight + marginPx)
  const first = Math.min(lastIndex, Math.floor(top / segmentPx))
  // bottom 恰落段边界时该段不需要(半开区间)→ 取 bottom-1 所在段。
  const last = Math.min(lastIndex, Math.floor(Math.max(0, bottom - 1) / segmentPx))
  return [first, Math.max(first, last)]
}

interface UseBucketVirtualScrollOptions {
  /// 双引擎互斥开关:false 时本引擎休眠(段表清空、取数泵停转)。
  enabled: () => boolean
  totalHeight: () => number
  /// 布局版本:变化即换代重建段表,在途应答按代/按对象丢弃。
  layoutVersion: () => number
  fetchBucketRows: (startY: number, endY: number) => Promise<LayoutRow[]>
  containerRef: () => HTMLElement | null
}

export function useBucketVirtualScroll(opts: UseBucketVirtualScrollOptions) {
  /// 当前愿望窗口内的段(按 index 升序)。数组本身 shallowRef(仅成员变化时整体替换);
  /// 段对象为 reactive——rows/state 变更及宿主对行 item 的就地 patch(缩略图/收藏/评分
  /// 回写)直接触发渲染,面积有界(1-3 段,与方案 A 可见行同量级)。
  const segments = shallowRef<RenderSegment[]>([])
  /// 当前逻辑滚动位。bucket 模式零映射 → 恒等于容器 scrollTop;供 scrubber 高亮与
  /// 画廊→侧栏分隔符联动(与方案 A 的 logicalScrollTop 对偶)。
  const logicalScrollTop = ref(0)

  /// 愿望集(index → 段对象,与 segments 数组共享同一批 reactive 对象)。
  const desired = new Map<number, RenderSegment>()
  /// 换代计数:布局版本变化/开关翻转即 +1;在途应答须同时满足「同代 + 段对象仍在愿望集
  /// 且是同一对象」才落地——飞掠丢弃与幽灵挂载的双保险。
  let generation = 0
  /// 单飞泵標志:任意时刻至多一个取数循环、至多一个 IPC 在途。
  let pumping = false
  /// 愿望窗口快速路径 key(代数+区间):滚动帧内区间未变则整个 sync 为 no-op。
  let lastRangeKey = ''
  let resizeObserver: ResizeObserver | null = null

  // ── B3 段级坐标映射状态 ────────────────────────────────────────────────────
  /// 「逻辑 − 物理」的当前窗口锚差(≥0)。非映射态恒 0;映射态下段物理位 =
  /// seg.start − anchorDelta(模板绑定,重锚这一低频事件才触发段 div 重定位)。
  const anchorDelta = ref(0)
  /// 物理 spacer 高(模板绑定):min(totalHeight, SPACER_CAP)。
  const spacerHeight = computed(() => Math.min(opts.totalHeight(), SPACER_CAP))
  /// 上次 scrollTop(单事件位移 = 跳变检测输入)。
  let lastP = 0
  /// 内部 scrollTop 写(偿债/远跳落点)标志:下一个 scroll 事件跳过处理(状态已就绪)。
  let internalScroll = false
  let settleTimer: ReturnType<typeof setTimeout> | null = null
  // ── B3.1 输入源分类状态(仅映射态消费)──────────────────────────────────────
  /// 上一 scroll 事件时刻(手势链判定);-Infinity = 链已断(程序化落点/引擎重开)。
  let lastScrollTs = -Infinity
  /// 1:1 印记过期时刻(wheel/touchmove/滚动键/程序化局部滚动盖印)。
  let oneToOneUntil = -Infinity
  /// 当前手势是否局部 1:1(手势起点定类,链内沿用)。
  let gestureOneToOne = false

  function geometry() {
    const el = opts.containerRef()
    const viewH = el?.clientHeight ?? 0
    const total = opts.totalHeight()
    return {
      viewH,
      total,
      physMax: Math.max(0, Math.min(total, SPACER_CAP) - viewH),
      logMax: Math.max(0, total - viewH),
      mapped: total > SPACER_CAP,
    }
  }

  /// 全局线性:物理 p → 逻辑 L(滚动条拇指比例语义;仅大位移重锚/偿债时使用)。
  function globalLogical(p: number): number {
    const { physMax, logMax } = geometry()
    if (physMax <= 0) return 0
    return (Math.min(Math.max(p, 0), physMax) / physMax) * logMax
  }

  /// 全局线性:逻辑 L → 物理 p。
  function globalPhysical(l: number): number {
    const { physMax, logMax } = geometry()
    if (logMax <= 0) return 0
    return (Math.min(Math.max(l, 0), logMax) / logMax) * physMax
  }

  function makeSegment(index: number): RenderSegment {
    const total = opts.totalHeight()
    return reactive({
      index,
      start: index * SEGMENT_PX,
      end: Math.min((index + 1) * SEGMENT_PX, total),
      rows: null,
      state: 'idle',
    }) as RenderSegment
  }

  function publish() {
    segments.value = Array.from(desired.values()).sort((a, b) => a.index - b.index)
  }

  /// 按当前逻辑位同步愿望集:窗外段整体丢弃(含 loading 中的——其在途应答将因
  /// 「对象已不在愿望集」被丢弃),窗内缺失段以 idle 补齐,然后踢一脚取数泵。
  /// B3:逻辑位 = scrollTop + anchorDelta;远跳时 scrollTop 尚未落位,经 logicalOverride
  /// 显式传入目标逻辑位。
  function syncDesired(force = false, logicalOverride?: number) {
    if (!opts.enabled()) return
    const el = opts.containerRef()
    if (!el) return
    const logicalTop = logicalOverride ?? el.scrollTop + anchorDelta.value
    const range = desiredSegmentRange(logicalTop, el.clientHeight, opts.totalHeight())
    if (!range) {
      if (desired.size > 0) {
        desired.clear()
        publish()
      }
      return
    }
    const [first, last] = range
    const key = `${generation}:${first}:${last}`
    if (!force && key === lastRangeKey) return
    lastRangeKey = key

    let changed = false
    for (const i of Array.from(desired.keys())) {
      if (i < first || i > last) {
        desired.delete(i)
        changed = true
      }
    }
    for (let i = first; i <= last; i++) {
      if (!desired.has(i)) {
        desired.set(i, makeSegment(i))
        changed = true
      }
    }
    if (changed) publish()
    void pumpFetch()
    flushSettled()
  }

  /// 挑下一个要取的段:idle 中距视口中心(逻辑坐标)最近者——远跳后终点段永远最先取。
  function pickNextIdle(): RenderSegment | null {
    const el = opts.containerRef()
    const center = el ? el.scrollTop + anchorDelta.value + el.clientHeight / 2 : 0
    let best: RenderSegment | null = null
    let bestDist = Infinity
    for (const seg of desired.values()) {
      if (seg.state !== 'idle') continue
      const d = Math.abs((seg.start + seg.end) / 2 - center)
      if (d < bestDist) {
        bestDist = d
        best = seg
      }
    }
    return best
  }

  /// 单飞取数泵:循环「挑最近的 idle → fetch → 复核落地」直到无事可做。每次 await 恢复后
  /// 都重新对当前愿望集挑选,滚动期间新增/丢弃的段自然被接上/跳过;跨代存活(换代后继续
  /// 为新段表服务)。唯一的挂起点在 await 上,break 到清标志之间无挂起 → 无 TOCTOU 悬空。
  async function pumpFetch() {
    if (pumping) return
    pumping = true
    try {
      while (opts.enabled()) {
        const seg = pickNextIdle()
        if (!seg) break
        const myGeneration = generation
        seg.state = 'loading'
        try {
          const rows = await opts.fetchBucketRows(seg.start, seg.end)
          // 落地三重复核:同代 + 该 index 的愿望对象仍是本对象(飞掠段已被 syncDesired
          // 丢弃 → get 返回 undefined 或新建对象 → 丢弃应答,杜绝幽灵挂载)。
          if (myGeneration === generation && desired.get(seg.index) === seg) {
            seg.rows = rows
            seg.state = 'ready'
          }
        } catch (err) {
          if (myGeneration === generation && desired.get(seg.index) === seg) {
            // error 粘滞至该段离窗重进或布局换代——LayoutNotReady 多为换代竞态,
            // 换代 watch 马上会整表重建。
            seg.state = 'error'
            console.error(LOG, `fetchBucketRows(${seg.start}, ${seg.end}) FAILED:`, err)
          }
        }
        flushSettled()
      }
    } finally {
      pumping = false
      flushSettled()
    }
  }

  // ── 段稳定屏障(B2):FLIP 重排动画的 Last 快照必须等段行落地 ─────────────────
  // bucket 模式下 compute 换版本 → 段表重建 → 行数据**异步**回填,mutate()+nextTick 时
  // DOM 尚空 → FLIP 读不到新位置。whenSettled() 在「愿望集内无 idle/loading 段」时兑现,
  // 供删除重排等一次性时序消费;error 段视为已稳定(不无限等待)。
  const settleWaiters: Array<() => void> = []

  function isSettled(): boolean {
    for (const seg of desired.values()) {
      if (seg.state === 'idle' || seg.state === 'loading') return false
    }
    return true
  }

  function whenSettled(): Promise<void> {
    if (isSettled()) return Promise.resolve()
    return new Promise((resolve) => settleWaiters.push(resolve))
  }

  function flushSettled() {
    if (!isSettled()) return
    while (settleWaiters.length) settleWaiters.shift()!()
  }

  function rebuild() {
    generation++
    desired.clear()
    lastRangeKey = ''
    // 布局换代后钳制锚差(总高可能缩水;非映射态自然归 0)。滚动位恢复由宿主的
    // layoutVersion watcher 经 scrollToLogicalY 完成,此处只保证几何不越界。
    anchorDelta.value = Math.min(anchorDelta.value, Math.max(0, opts.totalHeight() - SPACER_CAP))
    publish()
    syncDesired(true)
  }

  /// 宿主 @scroll 转发入口。非映射态:纯记录 + 算术同步(零映射零补偿——顺滑来源)。
  /// 映射态(B3.1 输入源分类):有 1:1 印记的手势 → 局部 1:1(滚轮/惯性/键盘原生手感,
  /// 零干预);无印记的滚动链 = 滚动条拖动/轨道点击 → **逐事件**全局线性重锚(拇指比例
  /// 语义,拖到边 = 逻辑边);单事件巨跳(Home/End/拇指跳转)无论分类一律比例兜底。
  /// 手势进行中绝不写 scrollTop——偿债只在停稳后(scheduleRepay)。
  function onScroll() {
    const el = opts.containerRef()
    if (!el) return
    const p = el.scrollTop
    if (internalScroll) {
      // 偿债/远跳落点的自触发事件:逻辑位与愿望窗口已就绪,仅更新跳变基准并断开手势链
      // (程序化落点不是用户手势,下一事件重新定类)。
      internalScroll = false
      lastP = p
      lastScrollTs = -Infinity
      return
    }
    const g = geometry()
    if (g.mapped) {
      const now = Date.now()
      if (now - lastScrollTs > SCROLL_CHAIN_MS) gestureOneToOne = now < oneToOneUntil
      lastScrollTs = now
      // 巨跳兜底:单个 scroll 事件位移超 3 屏,滚轮/触摸物理上给不出 → 必是拇指跳转。
      if (Math.abs(p - lastP) > Math.max(3 * g.viewH, 6000)) gestureOneToOne = false
      if (!gestureOneToOne) anchorDelta.value = globalLogical(p) - p
    }
    lastP = p
    logicalScrollTop.value = p + anchorDelta.value
    syncDesired()
    if (g.mapped) scheduleRepay()
  }

  /// 偿还压缩债(映射态):scrollTop 归位到当前逻辑位的全局线性位置。原子重锚——
  /// 先改 delta(段 top 绑定随 Vue 渲染更新),nextTick 后**同一事件循环任务内**写
  /// scrollTop:两写落同一渲染帧,内容零位移、仅滚动条拇指悄然归位。
  async function repayDebt() {
    const el = opts.containerRef()
    if (!el || !geometry().mapped) return
    const logical = el.scrollTop + anchorDelta.value
    const pStar = Math.round(globalPhysical(logical))
    if (Math.abs(pStar - el.scrollTop) < 2) return
    anchorDelta.value = logical - pStar
    await nextTick()
    internalScroll = true
    el.scrollTop = pStar
    lastP = pStar
  }

  function scheduleRepay() {
    if (settleTimer !== null) clearTimeout(settleTimer)
    const id = setTimeout(() => {
      if (settleTimer === id) settleTimer = null
      // 竞态守卫(B3.2):本回调可能在 clearTimeout 生效前已入任务队列——期间若有
      // 新滚动(lastScrollTs 更新),放弃本次偿债(新滚动已重新武装定时器),避免在
      // 手势恢复瞬间写 scrollTop 与其互搏。
      if (Date.now() - lastScrollTs < SCROLL_SETTLE_MS) return
      void repayDebt()
    }, SCROLL_SETTLE_MS)
    settleTimer = id
  }

  /// 宿主 @wheel.passive 转发入口(B3.1)。双职责:①盖 1:1 印记——新起手势据此与滚动条
  /// 拖动区分;②**边缘续滚**——物理钉边后原生不再产生 scroll 事件,改为直接推锚差,内容
  /// 以 1:1 继续滚到真正的逻辑边缘(修复真机「到边一跳一跳还能继续滚」:旧的钉边立即偿债
  /// 在手势中改 scrollTop,与拖拽/惯性互搏)。永不 preventDefault,对原生滚动零干预。
  function onWheel(e: WheelEvent) {
    if (!opts.enabled()) return
    oneToOneUntil = Date.now() + ONE_TO_ONE_STICKY_MS
    const g = geometry()
    if (!g.mapped) return
    const el = opts.containerRef()
    if (!el) return
    // deltaMode:0=像素(WebView2 常见)、1=行、2=页——归一到像素(镜像方案 A wheel 补偿)。
    let dy = e.deltaY
    if (e.deltaMode === 1) dy *= 16
    else if (e.deltaMode === 2) dy *= g.viewH || 800
    const p = el.scrollTop
    const logical = p + anchorDelta.value
    // 钉边判定留 1px 容差(缩放下 scrollTop 可为分数);推进后锚差可有 ±1px 瞬时越界,
    // 停稳偿债即归一。到达逻辑边缘后条件不再成立 → 硬停,不再「还能继续滚」。
    const pinnedBottom = dy > 0 && p >= g.physMax - 1 && logical < g.logMax
    const pinnedTop = dy < 0 && p <= 1 && logical > 0
    if (!pinnedBottom && !pinnedTop) return
    const next = Math.min(g.logMax, Math.max(0, logical + dy))
    anchorDelta.value = next - p
    logicalScrollTop.value = next
    // 边缘续滚在逻辑上就是一步滚动:记入 lastScrollTs,让停稳判定/偿债竞态守卫与
    // 手势链分类把它当作滚动事件对待。
    lastScrollTs = Date.now()
    syncDesired()
    scheduleRepay()
  }

  /// 宿主 @keydown 转发入口(B3.1):滚动键盖 1:1 印记(与滚轮同权)。
  function onKeydown(e: KeyboardEvent) {
    if (!opts.enabled()) return
    if (ONE_TO_ONE_KEYS.has(e.key)) oneToOneUntil = Date.now() + ONE_TO_ONE_STICKY_MS
  }

  /// 宿主 @touchmove.passive 转发入口(B3.1):触屏平移盖 1:1 印记(平移中持续刷新,
  /// 抬指后的惯性滚动由手势链续接分类)。
  function onTouchmove() {
    if (!opts.enabled()) return
    oneToOneUntil = Date.now() + ONE_TO_ONE_STICKY_MS
  }

  /// 程序化跳转到逻辑 y——scrubber/侧栏文件夹/锚点与缓存恢复/引擎切换的统一入口。
  /// 非映射态 = 直滚;映射态:近距(≤3 屏)且物理可达 → 局部滚动(可平滑,不重锚);
  /// 远跳 → 全局重锚 + 立即落点(跨千万 px 的平滑无意义,落点即出骨架/内容)。
  async function scrollToLogicalY(y: number, o?: { smooth?: boolean }) {
    const el = opts.containerRef()
    if (!el) return
    const g = geometry()
    const target = Math.min(Math.max(0, y), g.logMax)
    if (!g.mapped) {
      el.scrollTo({ top: target, behavior: o?.smooth ? 'smooth' : 'auto' })
      return
    }
    const pLocal = target - anchorDelta.value
    if (
      Math.abs(target - (el.scrollTop + anchorDelta.value)) <= 3 * g.viewH &&
      pLocal >= 0 &&
      pLocal <= g.physMax
    ) {
      // 程序化局部滚动(尤其 smooth 动画)产生的 scroll 事件序列必须按 1:1 分类,否则
      // 会被当作滚动条拖动逐事件重锚、破坏落点;600ms 覆盖动画启动,后续由手势链续接。
      oneToOneUntil = Date.now() + 600
      el.scrollTo({ top: pLocal, behavior: o?.smooth ? 'smooth' : 'auto' })
      return
    }
    const pStar = Math.round(globalPhysical(target))
    anchorDelta.value = target - pStar
    logicalScrollTop.value = target
    syncDesired(true, target)
    await nextTick()
    internalScroll = true
    el.scrollTop = pStar
    lastP = pStar
  }

  /// 当前已挂载各段的行(平铺)。供宿主对可视项就地 patch(缩略图/收藏/评分/色标回写、
  /// 上下文菜单查找)——与方案 A 的 visibleRows 消费面对齐。
  function mountedRows(): LayoutRow[] {
    const out: LayoutRow[] = []
    for (const seg of segments.value) {
      if (seg.rows) out.push(...seg.rows)
    }
    return out
  }

  // 开关翻转 / 布局换代 → 重建或清空段表。immediate:挂载时若开关已开(持久化配置)即建。
  watch(
    () => [opts.enabled(), opts.layoutVersion()] as const,
    ([on]) => {
      if (!on) {
        generation++
        desired.clear()
        lastRangeKey = ''
        anchorDelta.value = 0
        lastP = 0
        lastScrollTs = -Infinity
        oneToOneUntil = -Infinity
        gestureOneToOne = false
        publish()
        flushSettled() // 空愿望集 = 已稳定,释放等待者(如引擎切换瞬间的 FLIP)
        return
      }
      rebuild()
    },
    { immediate: true },
  )

  // 视口尺寸变化 → 愿望窗口变化(宽度变化走 relayout→版本重建,此处兜住纯高度变化)。
  onMounted(() => {
    const el = opts.containerRef()
    if (!el || typeof ResizeObserver === 'undefined') return
    resizeObserver = new ResizeObserver(() => syncDesired(true))
    resizeObserver.observe(el)
  })

  onBeforeUnmount(() => {
    resizeObserver?.disconnect()
    resizeObserver = null
    if (settleTimer !== null) {
      clearTimeout(settleTimer)
      settleTimer = null
    }
  })

  return {
    segments,
    logicalScrollTop,
    anchorDelta,
    spacerHeight,
    onScroll,
    onWheel,
    onKeydown,
    onTouchmove,
    scrollToLogicalY,
    mountedRows,
    whenSettled,
  }
}
