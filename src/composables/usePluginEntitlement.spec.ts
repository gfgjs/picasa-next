// src/composables/usePluginEntitlement.spec.ts
// Part5 T12：锁死「后端 availability → 前端 gate 展示判定」的映射（gate 逻辑的回归网）。
// 授权真相在后端；本 spec 只验前端映射（哪些态显 gate / 认作已授权 / 认作无产品）不跑偏。

import { describe, it, expect, beforeEach, vi } from 'vitest'

// mock 最外层 Tauri `invoke`（而非 utils/ipc 的 invokeIpc）：让**真实** invokeIpc/parseAppError 走完整
// 错误路径（更保真）。用**普通可变 handler**（非 vi.fn）——vi.fn/tinyspy 会把 mock 抛出/拒绝的值记录并
// 经其内部机制上报为 unhandled，即便下游已 catch（vitest v3.2.6 实测误判）；普通函数无此副作用。
type InvokeHandler = (cmd: string, args: unknown) => unknown
const { state } = vi.hoisted(() => ({ state: { handler: (() => undefined) as InvokeHandler } }))
const calls: Array<{ cmd: string; args: unknown }> = []
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args: unknown) => {
    calls.push({ cmd, args })
    return state.handler(cmd, args)
  },
}))

import { usePluginEntitlement, gateModeFor, type GateMode } from './usePluginEntitlement'
import type { Availability, PluginEntitlement } from '../types/exotic'

function ent(availability: Availability, extra: Partial<PluginEntitlement> = {}): PluginEntitlement {
  return { pluginId: 'p', availability, sourceTag: 'free', sku: null, storeUrl: null, ...extra }
}
/** 设 invoke 成功返回值。 */
function resolveWith(v: unknown) {
  state.handler = () => v
}
/** 设 invoke 同步抛（模拟后端 AppError 经 IPC 拒绝；invokeIpc 的 try 会捕获并 parseAppError）。 */
function rejectWith(v: unknown) {
  state.handler = () => {
    throw v
  }
}

beforeEach(() => {
  calls.length = 0
  state.handler = () => undefined
})

describe('usePluginEntitlement', () => {
  it('authorized → 已授权、不 gate', async () => {
    resolveWith(ent('authorized'))
    const e = usePluginEntitlement()
    const r = await e.fetchEntitlement('exotic-image-psd')
    expect(r?.availability).toBe('authorized')
    expect(e.isAuthorized.value).toBe(true)
    expect(e.isGated.value).toBe(false)
    // 参数契约：传 camelCase pluginId（Tauri 转 snake_case）。
    expect(calls).toEqual([{ cmd: 'get_plugin_entitlement', args: { pluginId: 'exotic-image-psd' } }])
  })

  it.each<Availability>(['availableUninstalled', 'installedUnlicensed', 'licenseExpired'])(
    '%s → 需要 gate（有产品可领）',
    async (a) => {
      resolveWith(ent(a))
      const e = usePluginEntitlement()
      await e.fetchEntitlement('p')
      expect(e.isGated.value).toBe(true)
      expect(e.isAuthorized.value).toBe(false)
    },
  )

  it.each<Availability>([
    'unsupportedPlatform',
    'incompatibleHost',
    'invalidInstallation',
    'disabled',
    'noOffering',
  ])('%s → 纯不可用，不引导购买（不 gate）', async (a) => {
    resolveWith(ent(a))
    const e = usePluginEntitlement()
    await e.fetchEntitlement('p')
    expect(e.isGated.value).toBe(false)
  })

  it('no_offering 错误 → isNoOffering，不 gate、不 authorized，返回 null', async () => {
    // 后端 AppError::Exotic{code:"no_offering"} 经 IPC 序列化为 { code, message }。
    rejectWith({ code: 'no_offering', message: 'Catalog 无此插件' })
    const e = usePluginEntitlement()
    const r = await e.fetchEntitlement('p')
    expect(r).toBeNull()
    expect(e.isNoOffering.value).toBe(true)
    expect(e.isGated.value).toBe(false)
    expect(e.isAuthorized.value).toBe(false)
    expect(e.error.value?.code).toBe('no_offering')
  })

  it('storeUrl / sku 透出供购买引导', async () => {
    resolveWith(
      ent('availableUninstalled', { sku: 'psd-engine-2026', storeUrl: 'https://store.example/psd' }),
    )
    const e = usePluginEntitlement()
    await e.fetchEntitlement('p')
    expect(e.storeUrl.value).toBe('https://store.example/psd')
    expect(e.entitlement.value?.sku).toBe('psd-engine-2026')
  })

  it('loading 在调用前后翻转', async () => {
    resolveWith(ent('authorized'))
    const e = usePluginEntitlement()
    const p = e.fetchEntitlement('p')
    expect(e.loading.value).toBe(true)
    await p
    expect(e.loading.value).toBe(false)
  })
})

// gateModeFor 是 PluginGate.vue 的渲染分支决策器（gate 逻辑单一事实源）：
// 穷尽 9 个 availability + null，锁死「哪些放行 / 购买 / 拦截提示」。
describe('gateModeFor', () => {
  it('null → passthrough（不确定不藏功能）', () => {
    const m: GateMode = 'passthrough'
    expect(gateModeFor(null)).toBe(m)
  })

  it.each<[Availability, GateMode]>([
    ['authorized', 'authorized'],
    ['availableUninstalled', 'purchase'],
    ['installedUnlicensed', 'purchase'],
    ['licenseExpired', 'purchase'],
    ['noOffering', 'passthrough'], // 无产品可售 → 放行，不误显购买引导
    ['unsupportedPlatform', 'blocked'],
    ['incompatibleHost', 'blocked'],
    ['invalidInstallation', 'blocked'],
    ['disabled', 'blocked'],
  ])('%s → %s', (availability, expected) => {
    expect(gateModeFor(ent(availability))).toBe(expected)
  })
})
