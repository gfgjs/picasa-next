// src/composables/useMediaDetail.ts
// Component-level composable for the media detail overlay (§12.3)
// 媒体详情覆盖层的组件级组合式函数 (§12.3)
//
// IMPORTANT: Call this ONCE at component setup time, NOT inside a computed().
// 重要：在组件 setup 时调用此函数一次，不要在 computed() 内部调用。
// Calling inside computed() would recreate event listeners on every reactive
// 在 computed() 内部调用会在每次响应式依赖变化时重新创建事件监听器，
// dependency change, leaking mousemove/mouseup listeners permanently.
// 导致 mousemove/mouseup 监听器永久泄漏。

import { ref, computed } from 'vue'

export function useMediaDetail() {
  // Image viewer state
  // 图像查看器状态
  const scale       = ref(1.0)
  const translateX  = ref(0)
  const translateY  = ref(0)
  const isDragging  = ref(false)
  const showInfo    = ref(false)

  type ZoomMode = 'auto' | 'original' | 'fit-width' | 'fit-height' | 'custom'
  const zoomMode    = ref<ZoomMode>('auto')

  // Live photo state
  // Live 照片状态
  const isPlayingLive = ref(false)
  const liveVideoSrc  = ref<string | null>(null)

  const transform = computed(() =>
    `translate(${translateX.value}px, ${translateY.value}px) scale(${scale.value})`
  )

  function zoomIn()  { zoomMode.value = 'custom'; scale.value = Math.min(scale.value * 1.25, 10) }
  function zoomOut() { zoomMode.value = 'custom'; scale.value = Math.max(scale.value * 0.8, 0.1) }
  function resetZoom() {
    zoomMode.value   = 'auto'
    scale.value      = 1.0
    translateX.value = 0
    translateY.value = 0
  }

  function cycleZoomMode(cw: number, ch: number, iw: number, ih: number) {
    const modes: ('auto' | 'original' | 'fit-width' | 'fit-height')[] = ['auto', 'original', 'fit-width', 'fit-height']
    let currentIdx = modes.indexOf(zoomMode.value as any)
    if (currentIdx === -1) currentIdx = 3 // if custom, cycle to auto next (3+1=4=0)

    const nextMode = modes[(currentIdx + 1) % modes.length]
    setZoomMode(nextMode, cw, ch, iw, ih)
  }

  function setZoomMode(mode: ZoomMode, cw: number, ch: number, iw: number, ih: number) {
    zoomMode.value = mode
    translateX.value = 0
    translateY.value = 0
    
    if (mode === 'auto' || mode === 'custom') {
      scale.value = 1.0
      return
    }

    const safeIw = Math.max(iw, 1)
    const safeIh = Math.max(ih, 1)
    const safeCw = Math.max(cw, 1)
    const safeCh = Math.max(ch, 1)

    const base_w = Math.min(safeIw, safeCw, safeCh * (safeIw / safeIh))
    const base_h = Math.min(safeIh, safeCh, safeCw * (safeIh / safeIw))
    
    if (mode === 'original') {
      scale.value = safeIw / base_w
    } else if (mode === 'fit-width') {
      scale.value = safeCw / base_w
    } else if (mode === 'fit-height') {
      scale.value = safeCh / base_h
    }
  }

  /**
   * Fit the image to the container, scaling down only (never up).
   * 将图像适应容器，仅缩小（从不放大）。
   * Call with the actual image dimensions after they are known.
   * 在已知实际图像尺寸后调用。
   */
  function fitToWindow(containerW: number, containerH: number, imgW: number, imgH: number) {
    const s = Math.min(containerW / Math.max(imgW, 1), containerH / Math.max(imgH, 1), 1)
    scale.value      = s
    translateX.value = 0
    translateY.value = 0
  }

  // Drag to pan
  // 拖动平移
  let dragStartX = 0; let dragStartY = 0
  let dragInitX  = 0; let dragInitY  = 0

  function startDrag(e: MouseEvent) {
    if (scale.value <= 1) return
    isDragging.value = true
    dragStartX = e.clientX
    dragStartY = e.clientY
    dragInitX  = translateX.value
    dragInitY  = translateY.value
    document.addEventListener('mousemove', onDrag)
    document.addEventListener('mouseup',   stopDrag)
  }

  function onDrag(e: MouseEvent) {
    if (!isDragging.value) return
    translateX.value = dragInitX + (e.clientX - dragStartX)
    translateY.value = dragInitY + (e.clientY - dragStartY)
  }

  function stopDrag() {
    isDragging.value = false
    document.removeEventListener('mousemove', onDrag)
    document.removeEventListener('mouseup',   stopDrag)
  }

  /** Must be called from onBeforeUnmount to avoid listener leaks. */
  /** 必须从 onBeforeUnmount 调用以避免监听器泄漏。 */
  function cleanup() {
    document.removeEventListener('mousemove', onDrag)
    document.removeEventListener('mouseup',   stopDrag)
  }

  function onWheel(e: WheelEvent): boolean {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault()
      zoomMode.value = 'custom'
      const factor = e.deltaY < 0 ? 1.1 : 0.9
      scale.value = Math.max(0.1, Math.min(10, scale.value * factor))
      return true
    }
    return false
  }

  function toggleInfo() { showInfo.value = !showInfo.value }

  return {
    scale, translateX, translateY, isDragging, showInfo, transform, zoomMode,
    isPlayingLive, liveVideoSrc,
    zoomIn, zoomOut, resetZoom, cycleZoomMode, setZoomMode, fitToWindow,
    startDrag, onWheel, toggleInfo, cleanup,
  }
}
