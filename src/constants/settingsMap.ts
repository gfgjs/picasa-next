import {
  Sun,
  Globe,
  Type,
  Maximize,
  XSquare,
  MessageSquare,
  Monitor,
  Image,
  Cpu,
  HardDrive,
  Database,
  Settings,
  Terminal,
  Map,
  Trash2,
  Shield,
  Play,
  Video,
  Film,
} from '@lucide/vue'
import type { Component } from 'vue'

export const SETTINGS_MAP: Record<
  string,
  {
    icon: Component
    label: string // fallback or i18n key
    type: 'select' | 'number' | 'toggle' | 'button' | 'segmented' | 'custom'
  }
> = {
  theme: { icon: Sun, label: 'settings.theme', type: 'select' },
  language: { icon: Globe, label: 'settings.language', type: 'select' },
  uiFontSize: { icon: Type, label: 'settings.uiFontSize', type: 'number' },
  hoverScale: { icon: Maximize, label: 'settings.hoverScale', type: 'toggle' },
  hoverAutoplay: { icon: Play, label: 'settings.hoverAutoplay', type: 'toggle' },
  closeBehavior: { icon: XSquare, label: 'settings.closeBehavior', type: 'select' },

  showThumbInfo: { icon: MessageSquare, label: 'settings.thumbInfoHover', type: 'toggle' },
  thumbDecodeStrategy: { icon: Cpu, label: 'settings.thumbDecodeStrategy', type: 'select' },
  gpuEngine: { icon: Monitor, label: 'settings.gpuEngine', type: 'select' },
  thumbCacheDir: { icon: HardDrive, label: 'settings.thumbCacheDir', type: 'button' },
  thumbSize: { icon: Image, label: 'settings.thumbSize', type: 'segmented' },
  thumbSkipMaxKb: { icon: Shield, label: 'settings.thumbSkipMaxKb', type: 'number' },
  thumbCacheMaxMb: { icon: HardDrive, label: 'settings.thumbCacheMaxMb', type: 'number' },
  timelineScrollWidth: { icon: Map, label: 'settings.timelineScrollWidth', type: 'number' },
  fullThumbGen: { icon: Image, label: 'settings.fullThumbGen', type: 'custom' },

  enableVideoCover: { icon: Video, label: 'settings.enableVideoCover', type: 'toggle' },
  enableVideoKeyframes: { icon: Film, label: 'settings.enableVideoKeyframes', type: 'toggle' },

  aiEngineStatus: { icon: Cpu, label: 'settings.aiEngineStatus', type: 'button' },
  aiHqCache: { icon: Image, label: 'settings.aiHqCache', type: 'toggle' },
  aiBatchSize: { icon: Database, label: 'settings.aiBatchSize', type: 'number' },
  aiHardwareStrategy: { icon: Cpu, label: 'settings.aiHardwareStrategy', type: 'select' },

  clearDb: { icon: Database, label: 'settings.clearDb', type: 'button' },
  clearSettings: { icon: Settings, label: 'settings.clearSettings', type: 'button' },
  logLevel: { icon: Terminal, label: 'settings.logLevel', type: 'select' },
  logDir: { icon: HardDrive, label: 'settings.logDir', type: 'button' },
  clearAllThumbnails: { icon: Trash2, label: 'settings.clearAllThumbnails', type: 'button' },
  clearBrowserCache: { icon: Trash2, label: 'settings.clearBrowserCache', type: 'button' },
  clearLogs: { icon: Trash2, label: 'settings.clearLogs', type: 'button' },
}
