// src/themes/registry.ts
// 内置主题注册表 — 「主题商店」解耦边界(设计: plan-docs/2026-07-06-前端UI优化与多主题系统.md §4.2)。
// 一切主题相关 UI(设置卡片/侧栏切换)只读本注册表渲染,禁止硬编码主题清单;未来商店/
// 本地导入只需注入 source:'external' 条目 + 运行时 <style> loader,状态模型与持久化零改动。

/** 设置页主题卡片的四色预览(与主题 CSS 文件手动同步维护;契约测试校验 hex 合法性)。 */
export interface ThemePreview {
  bg: string
  surface: string
  text: string
  accent: string
}

export interface ThemeDefinition {
  /** data-theme 值,亦是 theme_light/theme_dark 的持久化值 */
  id: string
  /** i18n key(themes.<id>),如 'themes.ink' → 「墨 · Ink」 */
  nameKey: string
  /** 亮/暗槽位归属,决定 data-color-scheme 与原生标题栏明暗 */
  kind: 'light' | 'dark'
  preview: ThemePreview
  /** 'builtin' = 随包静态打包;预留 'external'(主题商店/本地导入) */
  source: 'builtin'
}

// 传统色家族(2026-07-06 拍板):墨 Ink / 素 Porcelain 先行,宣 Xuan / 玄 Obsidian /
// 黛 Dai 随 S3 逐套追加。preview 四色取自对应 CSS 的 bg-primary/surface/text-primary/accent。
export const BUILTIN_THEMES: readonly ThemeDefinition[] = [
  {
    id: 'ink',
    nameKey: 'themes.ink',
    kind: 'dark',
    preview: { bg: '#121214', surface: '#1c1c1f', text: '#ececef', accent: '#818cf8' },
    source: 'builtin',
  },
  {
    id: 'porcelain',
    nameKey: 'themes.porcelain',
    kind: 'light',
    preview: { bg: '#f6f8fa', surface: '#ffffff', text: '#1a1a1e', accent: '#6366f1' },
    source: 'builtin',
  },
  {
    id: 'moonlight',
    nameKey: 'themes.moonlight',
    kind: 'light',
    preview: { bg: '#f4f8f9', surface: '#ffffff', text: '#15222e', accent: '#2177b8' },
    source: 'builtin',
  },
  {
    id: 'xuan',
    nameKey: 'themes.xuan',
    kind: 'light',
    preview: { bg: '#f7f4ed', surface: '#fffef8', text: '#292521', accent: '#d42517' },
    source: 'builtin',
  },
  {
    id: 'obsidian',
    nameKey: 'themes.obsidian',
    kind: 'dark',
    preview: { bg: '#000000', surface: '#101012', text: '#e8e8ea', accent: '#818cf8' },
    source: 'builtin',
  },
  {
    id: 'dai',
    nameKey: 'themes.dai',
    kind: 'dark',
    preview: { bg: '#171c26', surface: '#1d2431', text: '#e4e9f0', accent: '#8fb2c9' },
    source: 'builtin',
  },
]

/** 亮/暗槽位的出厂默认主题 id。 */
export const DEFAULT_LIGHT_THEME = 'porcelain'
export const DEFAULT_DARK_THEME = 'ink'

export function getTheme(id: string): ThemeDefinition | undefined {
  return BUILTIN_THEMES.find((t) => t.id === id)
}

export function themesByKind(kind: 'light' | 'dark'): ThemeDefinition[] {
  return BUILTIN_THEMES.filter((t) => t.kind === kind)
}

/**
 * 归一化槽位主题 id:legacy 值('light'/'dark',S1 及更早版本的持久化值)映射到新 id;
 * 未注册/kind 不符的 id(如将来已卸载的外置主题)落回该槽位默认——data-theme 指向
 * 不存在的主题会丢失全部颜色变量,必须在入口挡住。
 */
export function normalizeThemeId(raw: string | null, kind: 'light' | 'dark'): string {
  const fallback = kind === 'light' ? DEFAULT_LIGHT_THEME : DEFAULT_DARK_THEME
  if (!raw) return fallback
  const mapped = raw === 'light' ? DEFAULT_LIGHT_THEME : raw === 'dark' ? DEFAULT_DARK_THEME : raw
  return getTheme(mapped)?.kind === kind ? mapped : fallback
}
