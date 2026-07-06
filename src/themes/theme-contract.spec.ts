// src/themes/theme-contract.spec.ts
// 主题 token 契约测试(设计 §4.6):themes/ 下所有主题的 CSS 变量键集必须完全一致,
// 新主题不齐键直接红。本测试即未来「主题商店」manifest 校验器的雏形——外置主题
// 上架前跑同一套键集校验。
import { describe, it, expect } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join, basename } from 'node:path'
import { BUILTIN_THEMES, DEFAULT_LIGHT_THEME, DEFAULT_DARK_THEME, getTheme } from './registry'

// 直接 fs 读 CSS 源码(vitest 为 node 环境)。不用 import.meta.glob('?raw'):
// vitest 默认对 .css 做空桩(test.css=false),?raw 查询同样被截空——实测返回 ''。
const themesDir = join(dirname(fileURLToPath(import.meta.url)), '../assets/styles/themes')
const cssById = new Map<string, string>(
  readdirSync(themesDir)
    .filter((f) => f.endsWith('.css'))
    .map((f) => [basename(f, '.css'), readFileSync(join(themesDir, f), 'utf-8')]),
)

/** 提取声明位置的自定义属性名(`--x:` 形式;var(--x) 用法因后随 `)` 不会被误捕)。 */
function customPropsOf(css: string): Set<string> {
  return new Set(css.match(/--[\w-]+(?=\s*:)/g) ?? [])
}

describe('主题 token 契约', () => {
  it('CSS 文件与注册表条目一一对应', () => {
    const registryIds = BUILTIN_THEMES.map((t) => t.id).sort()
    expect([...cssById.keys()].sort()).toEqual(registryIds)
    // id 唯一
    expect(new Set(registryIds).size).toBe(registryIds.length)
  })

  it('每份主题文件的选择器与文件名一致', () => {
    for (const [id, css] of cssById) {
      expect(css, `${id}.css 缺 [data-theme='${id}'] 选择器`).toContain(`[data-theme='${id}']`)
    }
  })

  it('全部主题键集完全一致且非空', () => {
    const entries = [...cssById.entries()]
    const [refId, refCss] = entries[0]
    const refKeys = customPropsOf(refCss)
    // 语义色层至少覆盖 bg/text/accent/border/shadow/状态色等大类,空壳主题直接红
    expect(refKeys.size).toBeGreaterThanOrEqual(30)
    for (const [id, css] of entries.slice(1)) {
      const keys = customPropsOf(css)
      const missing = [...refKeys].filter((k) => !keys.has(k))
      const extra = [...keys].filter((k) => !refKeys.has(k))
      expect(missing, `${id}.css 相对 ${refId}.css 缺键`).toEqual([])
      expect(extra, `${id}.css 相对 ${refId}.css 多键`).toEqual([])
    }
  })

  it('每份主题声明 color-scheme 且与注册表 kind 一致', () => {
    for (const [id, css] of cssById) {
      const kind = getTheme(id)?.kind
      expect(kind, `注册表缺 ${id}`).toBeDefined()
      const m = css.match(/color-scheme:\s*(light|dark)/)
      expect(m, `${id}.css 缺 color-scheme 声明`).not.toBeNull()
      expect(m?.[1], `${id}.css 的 color-scheme 与注册表 kind 不符`).toBe(kind)
    }
  })

  it('注册表 preview 为合法 hex 色', () => {
    const hex = /^#[0-9a-fA-F]{6}$/
    for (const t of BUILTIN_THEMES) {
      for (const [slot, v] of Object.entries(t.preview)) {
        expect(v, `${t.id}.preview.${slot} 非法`).toMatch(hex)
      }
    }
  })

  it('亮暗槽位默认主题存在且 kind 正确', () => {
    expect(getTheme(DEFAULT_LIGHT_THEME)?.kind).toBe('light')
    expect(getTheme(DEFAULT_DARK_THEME)?.kind).toBe('dark')
  })
})
