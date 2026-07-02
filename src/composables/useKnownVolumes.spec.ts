// src/composables/useKnownVolumes.spec.ts
// Part5 T13：锁死已知卷面板数据层的 IPC 契约（列表容错 / 重命名·忘记参数 + 后随重载）。
// 普通可变 handler（非 vi.fn）按 cmd 分发——规避 vi.fn/tinyspy 误报 caught 抛出（见 usePluginEntitlement.spec）。

import { describe, it, expect, beforeEach, vi } from 'vitest'

type InvokeHandler = (cmd: string, args: unknown) => unknown
const { state } = vi.hoisted(() => ({ state: { handler: (() => undefined) as InvokeHandler } }))
const calls: Array<{ cmd: string; args: unknown }> = []
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (cmd: string, args: unknown) => {
    calls.push({ cmd, args })
    return state.handler(cmd, args)
  },
}))

import { useKnownVolumes, type VolumeInfo } from './useKnownVolumes'

function vol(overrides: Partial<VolumeInfo> = {}): VolumeInfo {
  return {
    id: 1,
    stableId: 'vol-A',
    label: 'U盘',
    kind: 'removable',
    lastMountPath: 'E:\\',
    isOnline: true,
    lastSeen: 1000,
    itemCount: 42,
    ...overrides,
  }
}

function route(map: Record<string, unknown>) {
  state.handler = (cmd) => {
    if (cmd in map) {
      const v = map[cmd]
      if (v instanceof Error) throw v
      return v
    }
    return undefined
  }
}

beforeEach(() => {
  calls.length = 0
  state.handler = () => undefined
})

describe('useKnownVolumes.load', () => {
  it('填充 volumes、清 error', async () => {
    route({ list_volumes: [vol(), vol({ id: 2, isOnline: false, itemCount: 0 })] })
    const v = useKnownVolumes()
    await v.load()
    expect(v.volumes.value).toHaveLength(2)
    expect(v.volumes.value[1].isOnline).toBe(false)
    expect(v.error.value).toBeNull()
    expect(v.loading.value).toBe(false)
  })

  it('失败 → 置 error、不抛（面板容错）', async () => {
    route({ list_volumes: new Error('db down') })
    const v = useKnownVolumes()
    await v.load()
    expect(v.error.value).toBeTruthy()
    expect(v.loading.value).toBe(false)
  })
})

describe('useKnownVolumes 操作', () => {
  it('rename：camelCase volumeId+label，后随重载', async () => {
    route({ rename_volume: undefined, list_volumes: [vol({ label: '新名' })] })
    const v = useKnownVolumes()
    await v.rename(1, '新名')
    expect(calls[0]).toEqual({ cmd: 'rename_volume', args: { volumeId: 1, label: '新名' } })
    expect(calls[1].cmd).toBe('list_volumes')
    expect(v.volumes.value[0].label).toBe('新名')
  })

  it('forget：camelCase volumeId，后随重载', async () => {
    route({ forget_volume: undefined, list_volumes: [] })
    const v = useKnownVolumes()
    await v.forget(3)
    expect(calls[0]).toEqual({ cmd: 'forget_volume', args: { volumeId: 3 } })
    expect(calls[1].cmd).toBe('list_volumes')
    expect(v.volumes.value).toHaveLength(0)
  })

  it('rename 失败 → 错误上抛供 toast', async () => {
    route({ rename_volume: new Error('empty') })
    const v = useKnownVolumes()
    await expect(v.rename(1, '')).rejects.toBeTruthy()
  })
})
