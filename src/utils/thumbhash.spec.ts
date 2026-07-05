// src/utils/thumbhash.spec.ts
// 前端 ThumbHash 解码器 ↔ Rust thumbhash crate 跨语言对拍。
// 金标(thumbhash.golden.ts)由 Rust 侧 #[ignore] 测试 print_thumbhash_golden_fixtures
// 生成:同一哈希经 crate 自带解码器产出的参考像素/平均色。TS 解码器输出与之逐通道
// 比对;容差 ±2 覆盖 f32(Rust)与 f64(JS)余弦误差经 ×255 截断后的最大偏移。
import { describe, it, expect } from 'vitest'
import { thumbHashToRGBA, thumbhashToAverageColor } from './thumbhash'
import { THUMBHASH_GOLDEN } from './thumbhash.golden'

/** base64 → 字节(node 环境无 Buffer 类型依赖,用全局 atob) */
function decodeB64(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

describe('thumbHashToRGBA — 与 Rust 解码器金标逐通道对拍', () => {
  for (const fx of THUMBHASH_GOLDEN) {
    it(`${fx.name}: 尺寸一致且各通道偏差 ≤2`, () => {
      const img = thumbHashToRGBA(Uint8Array.from(fx.hash))
      expect(img).not.toBeNull()
      const { w, h, rgba } = img!
      expect(w).toBe(fx.w)
      expect(h).toBe(fx.h)

      const golden = decodeB64(fx.rgbaBase64)
      expect(rgba.length).toBe(golden.length)

      let maxDiff = 0
      let sumDiff = 0
      for (let i = 0; i < rgba.length; i++) {
        const d = Math.abs(rgba[i] - golden[i])
        if (d > maxDiff) maxDiff = d
        sumDiff += d
      }
      expect(maxDiff).toBeLessThanOrEqual(2)
      // 均值差远小于逐点容差:排除「整体系统性偏色但逐点恰在容差内」的假绿
      expect(sumDiff / rgba.length).toBeLessThanOrEqual(0.5)
    })
  }

  it('square_alpha_radial: alpha 通道被真实重建(存在半透明像素)', () => {
    const fx = THUMBHASH_GOLDEN.find((f) => f.name === 'square_alpha_radial')!
    const img = thumbHashToRGBA(Uint8Array.from(fx.hash))!
    let translucent = 0
    for (let i = 3; i < img.rgba.length; i += 4) {
      if (img.rgba[i] < 200) translucent++
    }
    expect(translucent).toBeGreaterThan(0)
  })
})

describe('thumbhashToAverageColor — 与 Rust thumb_hash_to_average_rgba 对拍', () => {
  for (const fx of THUMBHASH_GOLDEN) {
    it(`${fx.name}: 平均色各通道偏差 ≤2`, () => {
      const hex = thumbhashToAverageColor(fx.hash)
      expect(hex).toMatch(/^#[0-9a-f]{6}$/)
      const r = parseInt(hex.slice(1, 3), 16)
      const g = parseInt(hex.slice(3, 5), 16)
      const b = parseInt(hex.slice(5, 7), 16)
      expect(Math.abs(r - Math.round(fx.avg.r * 255))).toBeLessThanOrEqual(2)
      expect(Math.abs(g - Math.round(fx.avg.g * 255))).toBeLessThanOrEqual(2)
      expect(Math.abs(b - Math.round(fx.avg.b * 255))).toBeLessThanOrEqual(2)
    })
  }
})

describe('畸形输入防御(与 Rust Err 行为对齐)', () => {
  it('空 / 不足 5 字节 → null;平均色回退占位色', () => {
    expect(thumbHashToRGBA(new Uint8Array())).toBeNull()
    expect(thumbHashToRGBA(new Uint8Array([1, 2, 3, 4]))).toBeNull()
    expect(thumbhashToAverageColor([])).toBe('#333333')
    expect(thumbhashToAverageColor([1, 2, 3, 4])).toBe('#333333')
  })

  it('AC nibble 流被截断 → null(不产出半截像素)', () => {
    const full = THUMBHASH_GOLDEN[0].hash
    expect(thumbHashToRGBA(Uint8Array.from(full.slice(0, 6)))).toBeNull()
  })

  it('带 alpha 但缺 alpha 头字节 → null', () => {
    const alphaFx = THUMBHASH_GOLDEN.find((f) => f.name === 'square_alpha_radial')!
    expect(thumbHashToRGBA(Uint8Array.from(alphaFx.hash.slice(0, 5)))).toBeNull()
  })
})
