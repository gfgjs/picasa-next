// src/types/ipc.ts
// IPC payload types for Tauri events and channel messages
// Tauri 事件和通道消息的 IPC 负载类型

import type { MediaFilter } from './media'

// ── Scan channel payloads ──────────────────────────────────────────────────
// ── 扫描通道负载 ──────────────────────────────────────────────────────────

export interface ScanProgressPayload {
  rootId:     number
  scanned:    number
  total:      number
  currentDir: string
  status:     'discovering' | 'scanning'
}

export interface ScanCompletedPayload {
  rootId:     number
  totalItems: number
  elapsedMs:  number
}

export interface ScanErrorPayload {
  rootId: number
  error:  string
}

export type ScanChannelPayload =
  | { type: 'progress';  progress:  ScanProgressPayload }
  | { type: 'completed'; completed: ScanCompletedPayload }
  | { type: 'error';     error:     ScanErrorPayload }

// ── Enrichment events ──────────────────────────────────────────────────────
// ── 丰富化事件 ──────────────────────────────────────────────────────────────

export interface MediaEnrichedPayload {
  rootId:        number
  enrichedCount: number
  total:         number
}

export interface EnrichmentCompletedPayload {
  rootId:    number
  elapsedMs: number
}

// ── DB update event ────────────────────────────────────────────────────────
// ── 数据库更新事件 ──────────────────────────────────────────────────────────

export interface MediaUpdatedPayload {
  action:  'update' | 'delete' | 'restore'
  itemIds: number[]
}

// ── Command params ─────────────────────────────────────────────────────────
// ── 命令参数 ───────────────────────────────────────────────────────────────

export interface ComputeLayoutParams {
  directoryId?:    number | null
  filters?:        MediaFilter | null
  containerWidth:  number
  rowHeight:       number
  gap:             number
}

export interface FullThumbProgressPayload {
  generated: number
  total:     number
  status:    'running' | 'completed' | 'cancelled'
  currentItem?: string
  phase?: string
}
