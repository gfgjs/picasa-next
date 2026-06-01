// src/types/ipc.ts
// IPC payload types for Tauri events and channel messages

import type { MediaFilter } from './media'

// ── Scan channel payloads ──────────────────────────────────────────────────

export interface ScanProgressPayload {
  rootId:     number
  scanned:    number
  total:      number
  currentDir: string
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

export interface MediaUpdatedPayload {
  action:  'update' | 'delete' | 'restore'
  itemIds: number[]
}

// ── Command params ─────────────────────────────────────────────────────────

export interface ComputeLayoutParams {
  directoryId?:    number | null
  filters?:        MediaFilter | null
  containerWidth:  number
  rowHeight:       number
  gap:             number
}
