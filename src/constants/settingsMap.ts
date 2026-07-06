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
  Rows3,
} from '@lucide/vue'
import type { Component } from 'vue'

/** 设置卡分区 id(与 SettingsView 的 CollapsibleCard id 一致)。 */
export type SettingsSection = 'general' | 'thumbnails' | 'video' | 'aiModels' | 'debug'

/** select 类控件的选项:labelKey 走 i18n;语言名等「自名不随界面语言变」的场景用 label 原文。 */
export interface SettingOptionSpec {
  value: string
  labelKey?: string
  label?: string
}

/**
 * 单个设置项的声明式注册(设计 §8):新增常规设置只需在此登记一行 + i18n 文案,
 * SettingsView 的行体与 DynamicSettingControl 的控件形态均由注册表驱动。
 */
export interface SettingSpec {
  icon: Component
  /** 标签 i18n key(历史命名不规则——如 showThumbInfo→thumbInfoHover,须显式声明,不可由 key 推导)。 */
  label: string
  /** 行描述 i18n key;customRow 行的描述结构由 SettingsView 特例模板自带时缺省。 */
  descKey?: string
  section: SettingsSection
  /** 控件形态:toggle/select/number 走 DynamicSettingControl 通用分派;
   *  button=动作按钮;segmented/custom=特例控件。 */
  control: 'select' | 'number' | 'toggle' | 'button' | 'segmented' | 'custom'
  /** select 类的选项表(顺序即下拉顺序)。 */
  options?: SettingOptionSpec[]
  /** number 类的输入边界。 */
  min?: number
  max?: number
  /** true=特例行:行体(描述/控件)由 SettingsView 内嵌模板提供,pin/标签仍走注册表。 */
  customRow?: boolean
}

/**
 * 注册表本体。
 * ⚠ 插入顺序即设置页各分区内的行序(Object.keys 对字符串键保序),调序=改这里。
 */
export const SETTINGS_MAP: Record<string, SettingSpec> = {
  /* ── 外观 general ─────────────────────────────────────────── */
  theme: {
    icon: Sun,
    label: 'settings.theme',
    descKey: 'settings.themeDesc',
    section: 'general',
    control: 'select',
    // 特例行:设置页控件为下方 ThemePicker;select 声明仅供侧栏钉住区 compact 控件使用。
    customRow: true,
    options: [
      { value: 'system', labelKey: 'settings.themeSystem' },
      { value: 'light', labelKey: 'settings.themeLight' },
      { value: 'dark', labelKey: 'settings.themeDark' },
    ],
  },
  language: {
    icon: Globe,
    label: 'settings.language',
    descKey: 'settings.languageDesc',
    section: 'general',
    control: 'select',
    options: [
      { value: 'zh-CN', label: '简体中文' },
      { value: 'en-US', label: 'English' },
    ],
  },
  uiFontSize: {
    icon: Type,
    label: 'settings.uiFontSize',
    descKey: 'settings.uiFontSizeDesc',
    section: 'general',
    control: 'number',
    min: 12,
    max: 24,
  },
  hoverScale: {
    icon: Maximize,
    label: 'settings.hoverScale',
    descKey: 'settings.hoverScaleDesc',
    section: 'general',
    control: 'toggle',
  },
  hoverAutoplay: {
    icon: Play,
    label: 'settings.hoverAutoplay',
    descKey: 'settings.hoverAutoplayDesc',
    section: 'general',
    control: 'toggle',
  },
  bucketScroll: {
    icon: Rows3,
    label: 'settings.bucketScroll',
    descKey: 'settings.bucketScrollDesc',
    section: 'general',
    control: 'toggle',
  },
  closeBehavior: {
    icon: XSquare,
    label: 'settings.closeBehavior',
    descKey: 'settings.closeBehaviorDesc',
    section: 'general',
    control: 'select',
    options: [
      { value: 'ask', labelKey: 'settings.closeBehaviorAsk' },
      { value: 'minimize_to_tray', labelKey: 'settings.closeBehaviorMinimize' },
      { value: 'exit', labelKey: 'settings.closeBehaviorExit' },
    ],
  },

  /* ── 缩略图 thumbnails ────────────────────────────────────── */
  showThumbInfo: {
    icon: MessageSquare,
    label: 'settings.thumbInfoHover',
    descKey: 'settings.thumbInfoHoverDesc',
    section: 'thumbnails',
    control: 'toggle',
    // 特例行:开关下挂信息元素多选面板。
    customRow: true,
  },
  thumbDecodeStrategy: {
    icon: Cpu,
    label: 'settings.thumbDecodeStrategy',
    descKey: 'settings.thumbDecodeDesc',
    section: 'thumbnails',
    control: 'select',
    options: [
      { value: 'cpu', labelKey: 'settings.thumbStrategyCpu' },
      { value: 'gpu', labelKey: 'settings.thumbStrategyGpu' },
      { value: 'direct', labelKey: 'settings.thumbStrategyDirect' },
    ],
  },
  gpuEngine: {
    icon: Monitor,
    label: 'settings.gpuEngine',
    descKey: 'settings.gpuEngineDesc',
    section: 'thumbnails',
    control: 'select',
    options: [{ value: 'wic', labelKey: 'settings.gpuEngineWic' }],
  },
  thumbCacheDir: {
    icon: HardDrive,
    label: 'settings.thumbCacheDir',
    section: 'thumbnails',
    control: 'button',
    // 特例行:描述=可点击路径,控件=换目录按钮。
    customRow: true,
  },
  thumbSize: {
    icon: Image,
    label: 'settings.thumbSize',
    descKey: 'settings.thumbSizeHint',
    section: 'thumbnails',
    control: 'segmented',
  },
  thumbSkipMaxKb: {
    icon: Shield,
    label: 'settings.thumbSkipMaxKb',
    descKey: 'settings.thumbSkipDesc',
    section: 'thumbnails',
    control: 'number',
    min: 0,
    max: 1000000,
  },
  thumbCacheMaxMb: {
    icon: HardDrive,
    label: 'settings.thumbCacheMaxMb',
    descKey: 'settings.thumbCacheDesc',
    section: 'thumbnails',
    control: 'number',
    min: 100,
    max: 100000,
  },
  timelineScrollWidth: {
    icon: Map,
    label: 'settings.timelineScrollWidth',
    descKey: 'settings.timelineScrollDesc',
    section: 'thumbnails',
    control: 'number',
    min: 2,
    max: 40,
  },
  fullThumbGen: {
    icon: Image,
    label: 'settings.fullThumbGen',
    descKey: 'settings.fullThumbGenDesc',
    section: 'thumbnails',
    control: 'custom',
    // 特例行:生成进度条 + 启停按钮。
    customRow: true,
  },

  /* ── 视频 video ───────────────────────────────────────────── */
  enableVideoCover: {
    icon: Video,
    label: 'settings.enableVideoCover',
    descKey: 'settings.enableVideoCoverDesc',
    section: 'video',
    control: 'toggle',
  },
  enableVideoKeyframes: {
    icon: Film,
    label: 'settings.enableVideoKeyframes',
    descKey: 'settings.enableVideoKeyframesDesc',
    section: 'video',
    control: 'toggle',
  },

  /* ── AI 模型 aiModels ─────────────────────────────────────── */
  aiEngineStatus: {
    icon: Cpu,
    label: 'settings.aiEngineStatus',
    section: 'aiModels',
    control: 'button',
    // 特例行:描述=引擎/显存/模型加载状态,控件=测试加载按钮。
    customRow: true,
  },
  aiHqCache: {
    icon: Image,
    label: 'settings.aiHqCache',
    descKey: 'settings.aiHqCacheDesc',
    section: 'aiModels',
    control: 'toggle',
  },
  aiBatchSize: {
    icon: Database,
    label: 'settings.aiBatchSize',
    descKey: 'settings.aiBatchSizeDesc',
    section: 'aiModels',
    // custom:数字输入外挂固定 batch 钳制与风险提示(DynamicSettingControl 按键特判)。
    control: 'custom',
    min: 0,
    max: 512,
  },
  aiHardwareStrategy: {
    icon: Cpu,
    label: 'settings.aiHardwareStrategy',
    descKey: 'settings.aiHardwareDesc',
    section: 'aiModels',
    control: 'select',
    options: [
      { value: 'auto', labelKey: 'settings.aiAutoHardware' },
      { value: 'cpu', labelKey: 'settings.aiForceCpu' },
    ],
  },

  /* ── 开发者工具 debug ─────────────────────────────────────── */
  clearDb: {
    icon: Database,
    label: 'settings.clearDb',
    descKey: 'settings.clearDbDesc',
    section: 'debug',
    control: 'button',
  },
  clearSettings: {
    icon: Settings,
    label: 'settings.clearSettings',
    descKey: 'settings.clearSettingsDesc',
    section: 'debug',
    control: 'button',
  },
  logLevel: {
    icon: Terminal,
    label: 'settings.logLevel',
    descKey: 'settings.logLevelDesc',
    section: 'debug',
    control: 'select',
    options: [
      { value: 'trace', labelKey: 'settings.logLevelTrace' },
      { value: 'debug', labelKey: 'settings.logLevelDebug' },
      { value: 'info', labelKey: 'settings.logLevelInfo' },
      { value: 'warn', labelKey: 'settings.logLevelWarn' },
      { value: 'error', labelKey: 'settings.logLevelError' },
    ],
  },
  logDir: {
    icon: HardDrive,
    label: 'settings.logDir',
    section: 'debug',
    control: 'button',
    // 特例行:描述=可点击路径,控件=换目录按钮。
    customRow: true,
  },
  clearAllThumbnails: {
    icon: Trash2,
    label: 'settings.clearAllThumbnails',
    descKey: 'settings.clearAllThumbnailsDesc',
    section: 'debug',
    control: 'button',
  },
  clearBrowserCache: {
    icon: Trash2,
    label: 'settings.clearBrowserCache',
    descKey: 'settings.clearBrowserCacheDesc',
    section: 'debug',
    control: 'button',
  },
  clearLogs: {
    icon: Trash2,
    label: 'settings.clearLogs',
    descKey: 'settings.clearLogsDesc',
    section: 'debug',
    control: 'button',
  },
}

/** 分区内的行键序列(=注册表插入顺序)。SettingsView 各卡片由此驱动逐行渲染。 */
export function sectionSettingKeys(section: SettingsSection): string[] {
  return Object.entries(SETTINGS_MAP)
    .filter(([, spec]) => spec.section === section)
    .map(([key]) => key)
}
