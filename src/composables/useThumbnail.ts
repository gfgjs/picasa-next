// src/composables/useThumbnail.ts
// Per-item thumbnail loading with ThumbHash placeholder and Image.decode() pre-rasterization.
// 带有 ThumbHash 占位符和 Image.decode() 预光栅化的按项缩略图加载。

import { ref, computed } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import type { ThumbResult } from '../types/media'
import { thumbhashToDataURL } from '../utils/thumbhash'

interface UseThumbnailOptions {
  thumbStatus: number
  thumbPath:   string | null
  thumbhash:   number[] | null
  cacheDir:    string
  thumbSize:   number
}

export function useThumbnail(opts: UseThumbnailOptions) {
  const isLoaded    = ref(false)
  const displaySrc  = ref('')
  const placeholderSrc = computed(() => {
    if (!opts.thumbhash) return ''
    return thumbhashToDataURL(opts.thumbhash)
  })

  async function load() {
    if (opts.thumbStatus === 0 || opts.thumbStatus === 2) return

    let src = ''
    if (opts.thumbStatus === 3) {
      // Direct source file display
      // 直接显示源文件
      return
    }

    if (opts.thumbPath) {
      const abs = `${opts.cacheDir}/thumbnails/${opts.thumbPath}`.replace(/\\/g, '/')
      src = convertFileSrc(abs)
    }

    if (!src) return

    try {
      const img = new Image()
      img.src = src
      await img.decode()
      displaySrc.value = src
      isLoaded.value   = true
    } catch {
      // decode failed — leave placeholder
      // 解码失败 — 保留占位符
    }
  }

  return { isLoaded, displaySrc, placeholderSrc, load }
}
