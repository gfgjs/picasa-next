// src/composables/useMediaDetail.ts
// Component-level composable for the media detail overlay (§12.3)

import { ref, computed } from 'vue'
import type { MediaDetail } from '../types/media'

export function useMediaDetail(detail: MediaDetail) {
  // Image viewer state
  const scale       = ref(1.0)
  const translateX  = ref(0)
  const translateY  = ref(0)
  const isDragging  = ref(false)
  const showInfo    = ref(false)

  // Live photo state
  const isPlayingLive = ref(false)
  const liveVideoSrc  = ref<string | null>(null)

  const transform = computed(() =>
    `translate(${translateX.value}px, ${translateY.value}px) scale(${scale.value})`
  )

  function zoomIn()  { scale.value = Math.min(scale.value * 1.25, 10) }
  function zoomOut() { scale.value = Math.max(scale.value * 0.8, 0.1) }
  function resetZoom() {
    scale.value      = 1.0
    translateX.value = 0
    translateY.value = 0
  }
  function fitToWindow(containerW: number, containerH: number) {
    const imgW = detail.width || 1
    const imgH = detail.height || 1
    const s = Math.min(containerW / imgW, containerH / imgH, 1)
    scale.value      = s
    translateX.value = 0
    translateY.value = 0
  }

  // Drag to pan
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

  function onWheel(e: WheelEvent) {
    e.preventDefault()
    const factor = e.deltaY < 0 ? 1.1 : 0.9
    scale.value = Math.max(0.1, Math.min(10, scale.value * factor))
  }

  function toggleInfo() { showInfo.value = !showInfo.value }

  return {
    scale, translateX, translateY, isDragging, showInfo, transform,
    isPlayingLive, liveVideoSrc,
    zoomIn, zoomOut, resetZoom, fitToWindow,
    startDrag, onWheel, toggleInfo,
  }
}
