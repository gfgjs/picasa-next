// src/utils/ipc.ts
// Part5 T8/T9 · 统一 IPC 封装 + 结构化错误解析。
//
// 目的:
// 1. **常量强制**——invokeIpc 只接受 `IpcCommand`(IPC 常量的值类型),类型层禁止裸字符串命令名,
//    消除「改后端命令名而前端漏改」的隐患(裸字符串改名零保障)。
// 2. **结构化错误**——后端 AppError 经 IPC 序列化为 `{ code, message }`(error.rs)。invokeIpc 捕获
//    invoke 的 reject,统一解析为 IpcError(带稳定 code),调用方据 code **按类型分流**(而非匹配文案)。
//    仍返回裸字符串的旧命令(如 ai_commands 尚未迁移)被宽容降级为 code='Unknown',不致解析崩溃。

import { invoke } from '@tauri-apps/api/core'
import { IPC } from '../constants/ipc'

/** 所有已登记 IPC 命令名的联合类型(IPC 常量的值)。invokeIpc 仅接受此类型 → 杜绝裸字符串。 */
export type IpcCommand = (typeof IPC)[keyof typeof IPC]

/**
 * 后端 AppError 的已知稳定 code(error.rs 的 Serialize 实现)。**非穷尽**——code 为开放字符串,
 * exotic 子系统会透出底层码(如 'rollback'/'http');这里仅列前端会按类型分流的常用码,便于 IDE 补全。
 */
export type AppErrorCode =
  | 'Io'
  | 'Db'
  | 'Pool'
  | 'UnsupportedFormat'
  | 'PathResolution'
  | 'LayoutNotReady'
  | 'ViewStale'
  | 'ScanRootNotFound'
  | 'MediaNotFound'
  | 'Cancelled'
  | 'Ai'
  | 'AiModelNotLoaded'
  | 'System'
  | 'Internal'
  | 'VolumeOffline' // 前向声明:T13 离线 UX 落地后由后端打开原图/视频命令返回(见 §3.7)
  | 'Unknown' // 前端兜底:无法解析为结构化 AppError 时(如旧命令裸字符串)
  | (string & {}) // 开放:保留任意后端/exotic 自定义 code,同时不丢上面字面量的补全

/** 结构化 IPC 错误。`code` 供按类型分流;`message` 仅作展示/日志,不承担分流职责。 */
export class IpcError extends Error {
  readonly code: AppErrorCode
  constructor(code: AppErrorCode, message: string) {
    super(message)
    this.name = 'IpcError'
    this.code = code
  }
}

/**
 * 把 invoke 的 reject 值解析为 IpcError。
 * - 结构化 `{ code, message }`(后端 AppError) → 原样取 code/message。
 * - 裸字符串(尚未迁移为 AppError 的旧命令) → code='Unknown',message 即该串。
 * - 其它(Error / 未知) → code='Unknown',尽力取 message。
 */
export function parseAppError(e: unknown): IpcError {
  if (e && typeof e === 'object' && 'code' in e && 'message' in e) {
    const o = e as { code: unknown; message: unknown }
    return new IpcError(String(o.code), String(o.message))
  }
  if (typeof e === 'string') return new IpcError('Unknown', e)
  if (e instanceof Error) return new IpcError('Unknown', e.message)
  return new IpcError('Unknown', String(e))
}

/**
 * 统一 IPC 调用入口:常量强制 + 错误结构化。
 * 调用方 `try { await invokeIpc(IPC.X, args) } catch (e) { if ((e as IpcError).code === 'Cancelled') … }`。
 * @param cmd IPC 命令(必须取自 IPC 常量;裸字符串被类型层拒绝)
 * @param args 命令参数(snake_case 由 Tauri 自动转;前端传 camelCase 键)
 */
export async function invokeIpc<T>(cmd: IpcCommand, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (e) {
    throw parseAppError(e)
  }
}

/** 从任意捕获值提取可展示的错误文案(优先 IpcError.message)。toast 等展示用。 */
export function ipcErrorMessage(e: unknown): string {
  if (e instanceof IpcError || e instanceof Error) return e.message
  return parseAppError(e).message
}
