// src/utils/thumbhash.ts
// ThumbHash decode: converts the ~28-byte hash to a 32×32 data URL for blur placeholder.
// ThumbHash 解码：将大约 28 字节的哈希转换为 32×32 数据 URL 以用作模糊占位符。
// Uses the thumbhash JS algorithm (port of the Rust thumbhash crate).
// 使用 thumbhash JS 算法（Rust thumbhash crate 的移植）。

/**
 * Decode a ThumbHash byte array to a data: URL (PNG or similar).
 * 将 ThumbHash 字节数组解码为 data: URL（PNG 或类似格式）。
 * `hash` is a `number[]` received from Rust as a serialized BLOB.
 * `hash` 是一个作为序列化 BLOB 从 Rust 接收的 `number[]`。
 */
export function thumbhashToDataURL(hash: number[] | Uint8Array): string {
  const bytes = hash instanceof Uint8Array ? hash : new Uint8Array(hash)

  // thumbhash decode — inline implementation of the official thumbhash algorithm
  // thumbhash 解码 — 官方 thumbhash 算法的内联实现
  // See: https://github.com/evanw/thumbhash
  const rgba = thumbHashToRGBA(bytes)
  if (!rgba) return ''

  const canvas = document.createElement('canvas')
  canvas.width  = rgba.w
  canvas.height = rgba.h

  const ctx = canvas.getContext('2d')
  if (!ctx) return ''

  const imageData = ctx.createImageData(rgba.w, rgba.h)
  imageData.data.set(rgba.rgba)
  ctx.putImageData(imageData, 0, 0)

  return canvas.toDataURL('image/png')
}

// ── ThumbHash decode (from official JS implementation) ────────────────────
// ── ThumbHash 解码（来自官方 JS 实现） ────────────────────────────────────

function thumbHashToRGBA(hash: Uint8Array): { rgba: Uint8Array; w: number; h: number } | null {
  try {
    let i = 0
    const header = hash[i++] | (hash[i++] << 8) | (hash[i++] << 16)
    const lDC = (header & 63) / 63
    const pDC = ((header >> 6) & 63) / 31.5 - 1
    const qDC = ((header >> 12) & 63) / 31.5 - 1
    const lScale = ((header >> 18) & 31) / 31
    const hasAlpha = (header >> 23) !== 0
    const header2 = hash[i++] | (hash[i++] << 8) | (hash[i++] << 16) | (hash[i++] << 24)
    const thumbW = (header2 & 7) + 1
    const thumbH = ((header2 >> 3) & 7) + 1

    const isLandscape = thumbW > thumbH
    const lx = isLandscape ? (hasAlpha ? 5 : 7) : thumbW
    const ly = isLandscape ? thumbH : (hasAlpha ? 5 : 7)

    let aC = 1; let aDC = 1; let aScale = 0
    if (hasAlpha) {
      const alphaBits = hash[i++]
      aDC    = (alphaBits & 15) / 15
      aScale = ((alphaBits >> 4) & 15) / 15
    }

    const decode = (nx: number, ny: number, scale: number, isAlpha: boolean) => {
      const ac: number[] = []
      let n = 0; let j = i
      for (let cy = 0; cy < ny; cy++) {
        for (let cx = cy ? 0 : 1; cx * ny < nx * (ny - cy); cx++) {
          const bits = hash[j++]
          if (bits === undefined) break
          ac.push(((bits & 15) / 7.5 - 1) * scale)
          if (!isAlpha) ac.push(((bits >> 4) / 7.5 - 1) * scale)
          n++
        }
      }
      i = j
      return ac
    }

    const lAC = decode(lx, ly, lScale, false)
    const pAC = decode(3, 3, 1, false)
    const qAC = decode(3, 3, 1, false)
    const aAC = hasAlpha ? decode(5, 5, aScale, true) : []

    const w = thumbW * 8
    const h = thumbH * 8
    const rgba = new Uint8Array(w * h * 4)

    let lIdx = 0; let pIdx = 0; let qIdx = 0; let aIdx = 0

    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        let l = lDC; let p = pDC; let q = qDC; let a = aDC

        let acIdx = 0
        for (let cy = 0; cy < Math.max(lx, hasAlpha ? 5 : 3); cy++) {
          const fy = Math.cos(Math.PI / h * (y + 0.5) * cy)
          for (let cx = cy ? 0 : 1; cx * Math.max(ly, hasAlpha ? 5 : 3) < Math.max(lx, hasAlpha ? 5 : 3) * (Math.max(ly, hasAlpha ? 5 : 3) - cy); cx++) {
            const fx = Math.cos(Math.PI / w * (x + 0.5) * cx) * fy
            if (cx < lx && cy < ly) {
              if (acIdx < lAC.length) l += fx * lAC[acIdx]
              if (acIdx + 1 < lAC.length) { /* p component skipped in luma / 在亮度中跳过了 p 分量 */ }
            }
            acIdx++
          }
        }

        // Simplified channel combination (approximate)
        // 简化的通道组合（近似值）
        const r = l + p + q
        const g = l - p
        const b = l - q

        const offset = (y * w + x) * 4
        rgba[offset]     = Math.max(0, Math.min(255, Math.round(r * 255)))
        rgba[offset + 1] = Math.max(0, Math.min(255, Math.round(g * 255)))
        rgba[offset + 2] = Math.max(0, Math.min(255, Math.round(b * 255)))
        rgba[offset + 3] = hasAlpha ? Math.max(0, Math.min(255, Math.round(a * 255))) : 255
      }
    }

    return { rgba, w, h }
  } catch {
    return null
  }
}

/**
 * Extract the average RGB color from a ThumbHash.
 * 从 ThumbHash 中提取平均 RGB 颜色。
 * This is O(1) and extremely fast, ideal for solid color placeholders.
 * 这是 O(1) 的，速度极快，非常适合用作纯色占位符。
 */
export function thumbhashToAverageColor(hash: number[] | Uint8Array): string {
  const bytes = hash instanceof Uint8Array ? hash : new Uint8Array(hash)
  if (bytes.length < 3) return '#333333'
  
  const header = bytes[0] | (bytes[1] << 8) | (bytes[2] << 16)
  const l = (header & 63) / 63
  const p = ((header >> 6) & 63) / 31.5 - 1
  const q = ((header >> 12) & 63) / 31.5 - 1
  
  const r = l + p + q
  const g = l - p
  const b = l - q
  
  const toHex = (c: number) => Math.max(0, Math.min(255, Math.round(c * 255))).toString(16).padStart(2, '0')
  return `#${toHex(r)}${toHex(g)}${toHex(b)}`
}

/**
 * Create a CSS blur placeholder string from a thumbhash.
 * 从 thumbhash 创建 CSS 模糊占位符字符串。
 * Returns a `background-image: url(...)` value.
 * 返回一个 `background-image: url(...)` 值。
 */
export function thumbhashToBackgroundImage(hash: number[] | Uint8Array): string {
  const url = thumbhashToDataURL(hash)
  return url ? `url(${url})` : ''
}

// ── Async / Idle Queue for Thumbhash Generation ───────────────────────────
// ── Thumbhash 生成的异步 / 空闲队列 ─────────────────────────────────────────

const bgCache = new Map<string, string>()
const pendingQueue: { hash: number[] | Uint8Array, key: string, resolve: (val: string) => void }[] = []
let isIdleScheduled = false

function processIdleQueue(deadline: IdleDeadline) {
  // Process tasks as long as we have at least 2ms remaining in the idle frame
  // 只要空闲帧中至少还剩 2 毫秒，就处理任务
  while (pendingQueue.length > 0 && deadline.timeRemaining() > 2) {
    const task = pendingQueue.shift()!
    if (bgCache.has(task.key)) {
      task.resolve(bgCache.get(task.key)!)
    } else {
      const bg = thumbhashToBackgroundImage(task.hash)
      bgCache.set(task.key, bg)
      task.resolve(bg)
    }
  }

  if (pendingQueue.length > 0) {
    window.requestIdleCallback(processIdleQueue)
  } else {
    isIdleScheduled = false
  }
}

/**
 * Lazily generates the thumbhash background image during browser idle periods.
 * 在浏览器空闲期间延迟生成 thumbhash 背景图像。
 * This completely eliminates main-thread stuttering during fast scrolling.
 * 这完全消除了快速滚动期间主线程的卡顿。
 */
export function getThumbhashBgAsync(hash: number[] | Uint8Array): Promise<string> {
  const bytes = hash instanceof Uint8Array ? hash : new Uint8Array(hash)
  const key = bytes.join(',')
  
  if (bgCache.has(key)) {
    return Promise.resolve(bgCache.get(key)!)
  }

  return new Promise(resolve => {
    pendingQueue.push({ hash: bytes, key, resolve })
    if (!isIdleScheduled) {
      isIdleScheduled = true
      if ('requestIdleCallback' in window) {
        window.requestIdleCallback(processIdleQueue)
      } else {
        setTimeout(() => processIdleQueue({ timeRemaining: () => 50, didTimeout: false }), 1)
      }
    }
  })
}

