// src/types/ipc.ts
// IPC payload types for Tauri events and channel messages
// Tauri 事件和通道消息的 IPC 负载类型

import type { MediaFilter } from './media'

// ── Scan channel payloads ──────────────────────────────────────────────────
// ── 扫描通道负载 ──────────────────────────────────────────────────────────

export interface ScanProgressPayload {
  rootId: number
  scanned: number
  total: number
  currentDir: string
  status: 'discovering' | 'scanning' | 'enriching'
}

export interface ScanCompletedPayload {
  rootId: number
  totalItems: number
  elapsedMs: number
  /** 本次缺失检测标记为「缺失」(availability='missing') 的项数（四道闸通过才 >0）。 */
  markedMissing: number
}

export interface ScanErrorPayload {
  rootId: number
  error: string
}

// 🔴 第 8 轮核验 P1-5：对齐 Rust wire format——后端 ScanChannelPayload 为
// `#[serde(tag = "type")]` 的 internally-tagged enum，newtype variant 把内层 struct 字段
// **扁平铺开**到 `type` 旁（非嵌在 progress/completed/error 键下）。故此处用「扁平联合」
// （`{ type } & Payload`），消除运行时 `as unknown as` 强制 cast。
export type ScanChannelPayload =
  | ({ type: 'progress' } & ScanProgressPayload)
  | ({ type: 'completed' } & ScanCompletedPayload)
  | ({ type: 'error' } & ScanErrorPayload)

// ── Enrichment events ──────────────────────────────────────────────────────
// ── 丰富化事件 ──────────────────────────────────────────────────────────────

export interface MediaEnrichedPayload {
  rootId: number
  enrichedCount: number
  total: number
}

export interface EnrichmentCompletedPayload {
  rootId: number
  elapsedMs: number
  /** 终态错误码：缺省/null = 正常完成；'enrich_failed' | 'enrich_panicked' = 后台补全异常终止
   *  （前端据此弹 warning，提示部分元数据可能缺失）。携带稳定码而非原始错误串。 */
  errorCode?: string | null
}

// ── DB update event ────────────────────────────────────────────────────────
// ── 数据库更新事件 ──────────────────────────────────────────────────────────

export interface MediaUpdatedPayload {
  action: 'update' | 'delete' | 'restore'
  itemIds: number[]
}

// ── Command params ─────────────────────────────────────────────────────────
// ── 命令参数 ───────────────────────────────────────────────────────────────

export interface ComputeLayoutParams {
  directoryId?: number | null
  filters?: MediaFilter | null
  containerWidth: number
  rowHeight: number
  gap: number
}

export interface FullThumbProgressPayload {
  generated: number
  total: number
  status: 'running' | 'completed' | 'cancelled'
  currentItem?: string
  phase?: string
}
