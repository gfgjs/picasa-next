// scripts/check-theme-contrast.mjs
// 主题对比度验证(设计 plan-docs/2026-07-06-前端UI优化与多主题系统.md §7):
// 对 themes/ 下每套主题计算关键「文本 × 底色」组合的 WCAG 2.x 对比度。
//
// 硬门槛(任一不达标 exit 1,输出留作 commit 证据):
//   text-primary        × bg-primary/secondary/surface/elevated  ≥ 4.5 (正文 AA)
//   text-secondary      × bg-primary/surface                     ≥ 3.0 (次级,设计 §5.1)
//   sidebar-active-text × bg-primary/secondary                   ≥ 4.5 (导航文本)
//   accent              × bg-primary                             ≥ 3.0 (UI 字形/图形)
// 其余组合(tertiary/accent-hover 等)仅报告不拦截。
//
// 用法: node scripts/check-theme-contrast.mjs
import { readFileSync, readdirSync } from 'node:fs'
import { join, dirname, basename } from 'node:path'
import { fileURLToPath } from 'node:url'

const themesDir = join(
  dirname(fileURLToPath(import.meta.url)),
  '../src/assets/styles/themes',
)

/** 解析一份主题 CSS 的自定义属性表(--x: value)。 */
function parseProps(css) {
  const props = {}
  for (const m of css.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    props[m[1]] = m[2].trim()
  }
  return props
}

/** hex/rgb/rgba → [r,g,b](0-255);带 alpha 时按给定底色合成。其他写法返回 null。 */
function resolveColor(value, bgRgb) {
  let m = value.match(/^#([0-9a-fA-F]{6})$/)
  if (m) {
    const n = parseInt(m[1], 16)
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
  }
  m = value.match(/^#([0-9a-fA-F]{3})$/)
  if (m) {
    return [...m[1]].map((c) => parseInt(c + c, 16))
  }
  m = value.match(/^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/)
  if (m) {
    const [r, g, b] = [Number(m[1]), Number(m[2]), Number(m[3])]
    const a = m[4] === undefined ? 1 : Number(m[4])
    if (a >= 1 || !bgRgb) return [r, g, b]
    // alpha 合成到底色(前景文本常见半透明写法)
    return [0, 1, 2].map((i) => Math.round([r, g, b][i] * a + bgRgb[i] * (1 - a)))
  }
  return null
}

/** WCAG 相对亮度。 */
function luminance([r, g, b]) {
  const lin = (c) => {
    const s = c / 255
    return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4)
  }
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

function contrast(fgRgb, bgRgb) {
  const l1 = luminance(fgRgb)
  const l2 = luminance(bgRgb)
  return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05)
}

// [前景 token, 底色 token, 硬门槛(null=仅报告)]
const PAIRS = [
  ['--color-text-primary', '--color-bg-primary', 4.5],
  ['--color-text-primary', '--color-bg-secondary', 4.5],
  ['--color-text-primary', '--color-bg-surface', 4.5],
  ['--color-text-primary', '--color-bg-elevated', 4.5],
  ['--color-text-secondary', '--color-bg-primary', 3.0],
  ['--color-text-secondary', '--color-bg-surface', 3.0],
  ['--color-sidebar-active-text', '--color-bg-primary', 4.5],
  ['--color-sidebar-active-text', '--color-bg-secondary', 4.5],
  ['--color-accent', '--color-bg-primary', 3.0],
  // ── 仅报告 ──
  ['--color-text-tertiary', '--color-bg-primary', null],
  ['--color-accent-hover', '--color-bg-primary', null],
  ['--color-text-placeholder', '--color-bg-surface', null],
]

let failures = 0
const files = readdirSync(themesDir).filter((f) => f.endsWith('.css')).sort()

for (const file of files) {
  const id = basename(file, '.css')
  const props = parseProps(readFileSync(join(themesDir, file), 'utf-8'))
  console.log(`\n═══ ${id} ═══`)
  for (const [fgKey, bgKey, threshold] of PAIRS) {
    const bgRgb = resolveColor(props[bgKey] ?? '', null)
    if (!bgRgb) {
      console.log(`  ?  ${fgKey} × ${bgKey}: 底色非纯色(${props[bgKey]}),跳过`)
      continue
    }
    const fgRgb = resolveColor(props[fgKey] ?? '', bgRgb)
    if (!fgRgb) {
      console.log(`  ?  ${fgKey} × ${bgKey}: 前景不可解析(${props[fgKey]}),跳过`)
      continue
    }
    const r = contrast(fgRgb, bgRgb)
    const tag =
      threshold === null ? '·' : r >= threshold ? '✓' : (failures++, '✗')
    const req = threshold === null ? '(报告)' : `(≥${threshold})`
    console.log(`  ${tag}  ${fgKey} × ${bgKey}: ${r.toFixed(2)} ${req}`)
  }
}

if (failures > 0) {
  console.error(`\n✗ ${failures} 个硬门槛组合不达标`)
  process.exit(1)
}
console.log('\n✓ 全部硬门槛通过')
