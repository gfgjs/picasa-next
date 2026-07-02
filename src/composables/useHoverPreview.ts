// src/composables/useHoverPreview.ts
// Hover auto-play preview for video / live-photo cells (需求1, §3.1).
// 视频 / 动态照片格子的悬停自动播放预览（需求1, §3.1）。
//
// 设计要点（§3.1）：
//  - **共享池（容量 1）**：模块级 `activeId` 保证同一时刻最多一个格子在预览，移到新格子时
//    旧格子立即卸载 —— 严禁每格一个 video（百万级库内存/解码爆炸）。
//  - **悬停防抖 200ms**：快速划过不触发解码。
//  - **compact / 选择模式 / 设置关闭** 下禁用。
//  - 源解析：普通视频 → 原文件；Apple Live / Google Motion Photo → get_companion_video_url
//    （后端按 companion MOV 或内嵌 MP4 偏移自动抽出，前端无需区分）。
//  - **超大视频降级（§3.3）**：体积超过阈值的视频不解码播放，改用预生成的关键帧 sprite
//    做悬停 scrub（鼠标横向位置 → background-position 切帧）；无 sprite 时回退为直接播放。

import { ref, computed, onBeforeUnmount } from 'vue'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { IPC } from '../constants/ipc'
import { useUiStore } from '../stores/uiStore'

// 同一时刻仅允许一个格子预览（池容量 = 1）。
const activeId = ref<number | null>(null)
// 已解析的预览源缓存（id → convertFileSrc URL），避免重复 hover 重复 IPC。
const srcCache = new Map<number, string>()
// 已解析的 sprite 源缓存（id → URL | null）；null 表示「查过但无 sprite」，避免重复 IPC。
const spriteCache = new Map<number, string | null>()

const HOVER_DELAY_MS = 200
// 超大视频阈值：超过则优先走 sprite scrub 降级（§3.3）。
const LARGE_VIDEO_BYTES = 200 * 1024 * 1024
// 关键帧 sprite 的帧数（与后端 derive/video.rs::KEYFRAME_COUNT 一致）。
const KEYFRAME_COUNT = 10

export interface HoverPreviewOptions {
  id: () => number
  mediaType: () => string
  isLivePhoto: () => boolean
  fileSize: () => number
  compact: () => boolean
  isSelectionMode: () => boolean
}

export function useHoverPreview(opts: HoverPreviewOptions) {
  const ui = useUiStore()
  const previewSrc = ref<string>('') // <video> 源（直接播放模式）
  const spriteSrc = ref<string>('') // sprite 图源（scrub 模式）
  const scrubFrame = ref<number>(0) // 当前 scrub 帧索引 0..KEYFRAME_COUNT-1
  let timer: ReturnType<typeof setTimeout> | null = null
  // 防止「快速划走」后异步源解析仍然生效。
  let resolveToken = 0

  const isPreviewing = computed(() => activeId.value === opts.id() && !!previewSrc.value)
  const isScrubbing = computed(() => activeId.value === opts.id() && !!spriteSrc.value)

  // scrub 背景样式：sprite 为水平条带，按帧索引平移 background-position。
  const scrubStyle = computed(() => {
    const cols = KEYFRAME_COUNT
    const posX = cols > 1 ? (scrubFrame.value / (cols - 1)) * 100 : 0
    return {
      backgroundImage: `url("${spriteSrc.value}")`,
      backgroundSize: `${cols * 100}% 100%`,
      backgroundPosition: `${posX}% 0%`,
      backgroundRepeat: 'no-repeat',
    }
  })

  function eligible(): boolean {
    if (!ui.hoverAutoplay) return false
    if (opts.compact()) return false
    if (opts.isSelectionMode()) return false
    return opts.mediaType() === 'video' || opts.isLivePhoto()
  }

  async function resolveVideoSrc(): Promise<string> {
    const id = opts.id()
    const cached = srcCache.get(id)
    if (cached) return cached
    let url: string
    if (opts.mediaType() === 'video') {
      const detail = await invoke<{ absPath: string }>(IPC.GET_MEDIA_DETAIL, { id })
      url = convertFileSrc(detail.absPath)
    } else {
      // Apple Live / Google Motion Photo：后端 get_companion_video_url 自动处理两种来源。
      const path = await invoke<string>(IPC.GET_COMPANION_VIDEO_URL, { itemId: id })
      url = convertFileSrc(path)
    }
    srcCache.set(id, url)
    return url
  }

  async function resolveSprite(): Promise<string | null> {
    const id = opts.id()
    if (spriteCache.has(id)) return spriteCache.get(id) ?? null
    const path = await invoke<string | null>(IPC.GET_KEYFRAME_SPRITE, { itemId: id })
    const url = path ? convertFileSrc(path) : null
    spriteCache.set(id, url)
    return url
  }

  function onEnter() {
    if (!eligible()) return
    if (timer) clearTimeout(timer)
    const myToken = ++resolveToken
    timer = setTimeout(async () => {
      try {
        // 超大视频：优先 sprite scrub 降级；无 sprite 才回退直接播放。
        const preferSprite = opts.mediaType() === 'video' && opts.fileSize() > LARGE_VIDEO_BYTES
        if (preferSprite) {
          const sprite = await resolveSprite()
          if (myToken !== resolveToken) return // 已划走
          if (sprite) {
            spriteSrc.value = sprite
            scrubFrame.value = 0
            previewSrc.value = ''
            activeId.value = opts.id()
            return
          }
        }
        const url = await resolveVideoSrc()
        if (myToken !== resolveToken) return // 已划走，放弃
        previewSrc.value = url
        spriteSrc.value = ''
        activeId.value = opts.id()
      } catch {
        // 无 companion / 解析失败 → 不预览
      }
    }, HOVER_DELAY_MS)
  }

  // scrub 模式下：鼠标横向位置（0..1）→ 帧索引。
  function onMove(fraction: number) {
    if (!isScrubbing.value) return
    const f = Math.min(Math.max(fraction, 0), 1)
    scrubFrame.value = Math.min(KEYFRAME_COUNT - 1, Math.floor(f * KEYFRAME_COUNT))
  }

  function onLeave() {
    resolveToken++ // 作废在途解析
    if (timer) {
      clearTimeout(timer)
      timer = null
    }
    if (activeId.value === opts.id()) activeId.value = null
    previewSrc.value = ''
    spriteSrc.value = ''
    scrubFrame.value = 0
  }

  onBeforeUnmount(onLeave)

  return { isPreviewing, previewSrc, isScrubbing, spriteSrc, scrubStyle, onEnter, onLeave, onMove }
}
