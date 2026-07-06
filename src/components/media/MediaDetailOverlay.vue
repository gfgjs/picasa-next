<template>
  <Teleport to="body">
    <div v-if="media.isDetailOpen && media.detailItem" class="detail-overlay overlay-enter">
      <!-- Backdrop -->
      <!-- 背景幕 -->
      <div class="overlay-backdrop" @click="close" />

      <!-- Panel -->
      <!-- 面板 -->
      <div class="detail-panel">
        <!-- ── Viewer ───────────────────────────────────────────────────── -->
        <!-- ── 视图器 ───────────────────────────────────────────────────── -->
        <div
          class="detail-viewer"
          ref="viewerRef"
          @wheel.prevent="onWheelHandler"
          @mousedown="state.startDrag"
          @click="onImageClick"
          @contextmenu.prevent="onContextMenu"
        >
          <!-- 不可用占位（缺失检测 Part2 §3.2）：卷离线 / 文件缺失 / 加载失败时给明确提示，
               而非任由 <img>/<video> 显示浏览器的 broken 图标（体验差、不知所以然）。 -->
          <div v-if="isUnavailable" class="detail-viewer__unavailable">
            <ImageOff :size="56" />
            <p class="detail-viewer__unavailable-title">{{ unavailableInfo.title }}</p>
            <p class="detail-viewer__unavailable-hint">{{ unavailableInfo.hint }}</p>
          </div>
          <!-- Exotic 授权 gate（Part5 T12）：未授权的 exotic 格式（如未购买 PSD）显示购买/激活引导，
               而非任由 <img> 尝试解码注定失败的原图。已授权 / 普通格式不进此分支（showExoticGate=false）。 -->
          <div v-else-if="showExoticGate" class="detail-viewer__gate" @click.stop>
            <PluginGate
              :entitlement="exoticGate.entitlement.value"
              :feature-name="exoticFeatureName"
              @activate="activateOpen = true"
            />
          </div>
          <template v-else>
            <img
              v-if="detail.mediaType === 'image'"
              ref="imgRef"
              :src="absPath"
              class="detail-viewer__img"
              :class="{ 'is-dragging': state.isDragging.value }"
              :style="{ transform: state.transform.value }"
              draggable="false"
              @load="updateZoomRatio"
              @error="onMediaError"
            />
            <video
              v-else-if="detail.mediaType === 'video'"
              ref="videoRef"
              :src="absPath"
              class="detail-viewer__video"
              :class="{ 'is-dragging': state.isDragging.value }"
              :style="{ transform: state.transform.value }"
              draggable="false"
              controls
              autoplay
              @loadedmetadata="updateZoomRatio"
              @error="onMediaError"
            />
            <audio
              v-else-if="detail.mediaType === 'audio'"
              :src="absPath"
              class="detail-viewer__audio"
              controls
              autoplay
              @error="onMediaError"
            />
            <div v-else class="detail-viewer__document">
              <FileText :size="48" />
              <p>{{ detail.fileName }}</p>
            </div>
          </template>

          <!-- Live photo video overlay -->
          <!-- Live photo 视频覆盖层 -->
          <video
            v-if="state.isPlayingLive.value && state.liveVideoSrc.value"
            :src="state.liveVideoSrc.value"
            class="detail-viewer__live-video"
            autoplay
            loop
          />

          <!-- Face boxes (F6): projected onto the image content rect; pointer-events:none so they
               never intercept zoom/drag clicks. -->
          <!-- 人脸框（F6）：投影到图像内容矩形；pointer-events:none，绝不拦截缩放/拖拽点击。 -->
          <div
            v-if="detail.mediaType === 'image' && faces.length && showFaces"
            class="face-overlay"
          >
            <div v-for="f in faces" :key="f.id" class="face-box" :style="faceBoxStyle(f)">
              <span v-if="f.personName" class="face-box__label">{{ f.personName }}</span>
            </div>
          </div>
        </div>

        <!-- ── Controls ────────────────────────────────────────────────── -->
        <!-- ── 控制器 ────────────────────────────────────────────────── -->
        <div class="detail-controls">
          <!-- Left -->
          <!-- 左侧 -->
          <div class="detail-controls__left">
            <button
              class="btn-icon"
              @click="close"
              :title="$t('detail.close')"
              :aria-label="$t('detail.close')"
            >
              <X :size="18" />
            </button>
            <button
              class="btn-icon"
              @click="state.zoomOut()"
              :title="$t('detail.zoomOut')"
              :aria-label="$t('detail.zoomOut')"
            >
              <ZoomOut :size="18" />
            </button>
            <span class="zoom-percentage" :class="{ 'zoom-highlight': isZoomChanged }">
              {{ Math.round(state.scale.value * zoomRatio * 100) }}%
            </span>
            <button
              class="btn-icon"
              @click="state.zoomIn()"
              :title="$t('detail.zoomIn')"
              :aria-label="$t('detail.zoomIn')"
            >
              <ZoomIn :size="18" />
            </button>
            <span
              v-if="media.navContext"
              class="zoom-percentage"
              style="opacity: 0.8; margin-left: 8px"
            >
              {{ media.navContext.currentIndex + 1 }} / {{ media.navContext.itemIds.length }}
            </span>
            <button
              class="btn-icon"
              @click="handleToggleZoom"
              :title="zoomModeTitle"
              :aria-label="zoomModeTitle"
            >
              <Maximize v-if="state.zoomMode.value === 'auto'" :size="18" />
              <span
                v-else-if="state.zoomMode.value === 'original'"
                style="font-size: 12px; font-weight: 700"
                >1:1</span
              >
              <MoveHorizontal v-else-if="state.zoomMode.value === 'fit-width'" :size="18" />
              <MoveVertical v-else :size="18" />
            </button>
            <button
              v-if="detail.isLivePhoto"
              class="btn-icon"
              :class="{ active: state.isPlayingLive.value }"
              @click="toggleLive"
              :title="t('detail.livePhoto')"
            >
              LIVE
            </button>
          </div>

          <!-- Center: file name -->
          <!-- 中间: 文件名 -->
          <div class="detail-controls__center">
            <span class="detail-controls__name" :title="detail.fileName">{{
              detail.fileName
            }}</span>
          </div>

          <!-- Right -->
          <!-- 右侧 -->
          <div class="detail-controls__right">
            <!-- 人脸蓝框显隐开关（问题5）：仅图像且检测到脸时出现，默认显示，偏好记 localStorage。 -->
            <button
              v-if="detail.mediaType === 'image' && faces.length"
              class="btn-icon"
              :class="{ active: showFaces }"
              @click="toggleFaces"
              :title="showFaces ? t('detail.hideFaceBoxes') : t('detail.showFaceBoxes')"
              :aria-label="showFaces ? t('detail.hideFaceBoxes') : t('detail.showFaceBoxes')"
            >
              <ScanFace :size="18" />
            </button>
            <button
              class="btn-icon"
              :class="{ active: detail.isFavorited }"
              @click="toggleFav"
              :title="t('selection.favorite')"
              :aria-label="t('selection.favorite')"
            >
              <Heart
                :size="18"
                :fill="detail.isFavorited ? 'currentColor' : 'none'"
                :stroke-width="detail.isFavorited ? 0 : 2"
              />
            </button>
            <button
              class="btn-icon"
              @click="showInExplorer"
              :title="t('contextMenu.showInExplorer')"
              :aria-label="t('contextMenu.showInExplorer')"
            >
              <FolderOpen :size="18" />
            </button>
            <button
              class="btn-icon"
              @click="state.toggleInfo()"
              :title="$t('detail.info')"
              :aria-label="$t('detail.info')"
            >
              <Info :size="18" />
            </button>
          </div>
        </div>

        <!-- ── Info sidebar (slide in when showInfo) ────────────────────── -->
        <!-- ── 信息侧边栏 (showInfo 开启时滑入) ────────────────────── -->
        <Transition name="slide">
          <div v-if="state.showInfo.value" class="detail-info">
            <div class="detail-info__header">
              <span>{{ t('detail.fileInfo') }}</span>
              <button
                class="btn-icon"
                @click="state.toggleInfo()"
                :title="$t('common.close')"
                :aria-label="$t('common.close')"
              >
                <X :size="16" />
              </button>
            </div>

            <div class="info-section">
              <div class="info-row">
                <span class="info-label">{{ $t('detail.fileName') }}</span>
                <span class="info-value" :title="detail.fileName">{{ detail.fileName }}</span>
              </div>
              <div class="info-row">
                <span class="info-label">{{ $t('detail.fileSize') }}</span>
                <span class="info-value">{{ formatFileSize(detail.fileSize) }}</span>
              </div>
              <div class="info-row">
                <span class="info-label">{{ $t('detail.dimensions') }}</span>
                <span class="info-value" v-if="detail.width"
                  >{{ detail.width }} × {{ detail.height }}</span
                >
              </div>
              <div class="info-row">
                <span class="info-label">{{ $t('detail.format') }}</span>
                <span class="info-value">{{ detail.fileFormat.toUpperCase() }}</span>
              </div>
              <div
                class="info-row"
                style="flex-direction: column; align-items: flex-start; gap: 4px"
              >
                <span class="info-label">{{ t('detail.fullPath') }}</span>
                <span
                  class="info-value clickable-path"
                  :title="detail.absPath"
                  @click.stop.prevent="showInExplorer"
                  >{{ detail.absPath }}</span
                >
              </div>
            </div>

            <!-- EXIF -->
            <!-- EXIF -->
            <div v-if="detail.imageMeta" class="info-section">
              <div class="info-section__title">{{ $t('detail.exif') }}</div>
              <div v-if="detail.imageMeta.exifDatetime" class="info-row">
                <span class="info-label">{{ $t('detail.datetime') }}</span>
                <span class="info-value">{{ formatDateTime(detail.imageMeta.exifDatetime) }}</span>
              </div>
              <div v-if="detail.imageMeta.exifMake" class="info-row">
                <span class="info-label">{{ $t('detail.camera') }}</span>
                <span class="info-value"
                  >{{ detail.imageMeta.exifMake }} {{ detail.imageMeta.exifModel }}</span
                >
              </div>
              <div v-if="detail.imageMeta.exifFocalLength" class="info-row">
                <span class="info-label">{{ $t('detail.focalLength') }}</span>
                <span class="info-value">{{
                  formatFocalLength(detail.imageMeta.exifFocalLength)
                }}</span>
              </div>
              <div v-if="detail.imageMeta.exifAperture" class="info-row">
                <span class="info-label">{{ $t('detail.aperture') }}</span>
                <span class="info-value">{{ formatAperture(detail.imageMeta.exifAperture) }}</span>
              </div>
              <div v-if="detail.imageMeta.exifShutter" class="info-row">
                <span class="info-label">{{ $t('detail.exposure') }}</span>
                <span class="info-value">{{ detail.imageMeta.exifShutter }}s</span>
              </div>
              <div v-if="detail.imageMeta.exifIso" class="info-row">
                <span class="info-label">{{ $t('detail.iso') }}</span>
                <span class="info-value">{{ detail.imageMeta.exifIso }}</span>
              </div>
              <div v-if="detail.imageMeta.exifGpsLat" class="info-row">
                <span class="info-label">{{ $t('detail.location') }}</span>
                <span class="info-value">{{
                  formatGps(detail.imageMeta.exifGpsLat, detail.imageMeta.exifGpsLng!)
                }}</span>
              </div>
            </div>

            <!-- Rating -->
            <!-- 评分 -->
            <div class="info-section">
              <div class="info-section__title">{{ t('detail.rating') }}</div>
              <div class="rating-stars">
                <button
                  v-for="n in 5"
                  :key="n"
                  class="star"
                  :class="{ filled: n <= (detail.rating ?? 0) }"
                  :aria-label="$t('common.nStars', { n })"
                  @click="setRating(n)"
                >
                  <Star
                    :size="20"
                    :fill="n <= (detail.rating ?? 0) ? 'currentColor' : 'none'"
                    :stroke-width="1.5"
                  />
                </button>
              </div>
            </div>

            <!-- Color label -->
            <!-- 颜色标签：复用 ColorLabelPicker（点设/点当前清零），与工具栏批量设色、筛选同一控件。 -->
            <div class="info-section">
              <div class="info-section__title">{{ t('detail.colorLabel') }}</div>
              <ColorLabelPicker
                :model-value="detail.colorLabel ?? 0"
                :size="20"
                @change="setColorLabel"
              />
            </div>
          </div>
        </Transition>
      </div>
    </div>
  </Teleport>

  <ContextMenu
    :visible="ctxMenu.visible"
    :x="ctxMenu.x"
    :y="ctxMenu.y"
    :items="ctxMenu.items"
    @update:visible="ctxMenu.visible = $event"
  />

  <FolderTreeSelectorDialog
    v-if="moveCopyDialog.isOpen"
    :title="moveCopyDialog.mode === 'move' ? t('common.moveToFolder') : t('common.copyToFolder')"
    @close="moveCopyDialog.isOpen = false"
    @confirm="onMoveCopyConfirm"
  />

  <!-- Exotic 激活对话框（Part5 T12）：gate 的「已购买？激活」入口 → 输入 token → 后端验签。 -->
  <ExoticActivateDialog
    :open="activateOpen"
    :plugin-id="exoticGate.entitlement.value?.pluginId ?? ''"
    :feature-name="exoticFeatureName"
    @close="activateOpen = false"
    @activated="onExoticActivated"
  />
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, markRaw, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { invokeIpc, type IpcError } from '../../utils/ipc'
import { convertFileSrc } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import ContextMenu from '../common/ContextMenu.vue'
import type { ContextMenuItem } from '../common/ContextMenu.vue'
import ColorLabelPicker from '../common/ColorLabelPicker.vue'
import FolderTreeSelectorDialog from '../common/FolderTreeSelectorDialog.vue'
import PluginGate from '../exotic/PluginGate.vue'
import ExoticActivateDialog from '../exotic/ExoticActivateDialog.vue'
import { useExoticGate } from '../../composables/useExoticGate'
import { gateModeFor } from '../../composables/usePluginEntitlement'
import type { DirNode } from '../../types/media'
import { usePersonStore } from '../../stores/personStore'
import type { FaceBox } from '../../types/person'
import { useMediaStore } from '../../stores/mediaStore'
import { useUiStore } from '../../stores/uiStore'
import { useHistoryStore } from '../../stores/historyStore'
import { useMediaDetail } from '../../composables/useMediaDetail'
import {
  formatFileSize,
  formatDateTime,
  formatFocalLength,
  formatAperture,
  formatGps,
} from '../../utils/format'
import {
  X,
  ZoomIn,
  ZoomOut,
  Maximize,
  MoveHorizontal,
  MoveVertical,
  Heart,
  FolderOpen,
  Info,
  Star,
  FileText,
  Copy,
  Monitor,
  FolderInput,
  ScanFace,
  ImageOff,
} from '@lucide/vue'
import { IPC, EVENTS } from '../../constants/ipc'

const media = useMediaStore()
const person = usePersonStore()
const ui = useUiStore()
const history = useHistoryStore()
const { t } = useI18n()

const ctxMenu = ref({
  visible: false,
  x: 0,
  y: 0,
  items: [] as ContextMenuItem[],
})

const moveCopyDialog = ref({
  isOpen: false,
  mode: 'move' as 'move' | 'copy',
  targetId: null as number | null,
})

function onContextMenu(e: MouseEvent) {
  if (!detail.value) return
  const item = detail.value
  const id = item.id
  const items: ContextMenuItem[] = [
    {
      id: 'copy',
      label: t('contextMenu.copyImage'),
      icon: markRaw(Copy),
      action: () => invokeIpc(IPC.COPY_IMAGE_TO_CLIPBOARD, { itemId: id }),
    },
    {
      id: 'open_explorer',
      label: t('contextMenu.showInExplorer'),
      icon: markRaw(FolderOpen),
      action: () => invoke(IPC.SHOW_IN_EXPLORER, { itemId: id }),
    },
    {
      id: 'move_to',
      label: t('common.moveTo'),
      icon: markRaw(FolderInput),
      action: () => {
        moveCopyDialog.value.mode = 'move'
        moveCopyDialog.value.targetId = id
        moveCopyDialog.value.isOpen = true
      },
    },
    {
      id: 'copy_to',
      label: t('common.copyTo'),
      icon: markRaw(Copy),
      action: () => {
        moveCopyDialog.value.mode = 'copy'
        moveCopyDialog.value.targetId = id
        moveCopyDialog.value.isOpen = true
      },
    },
  ]

  if (item.mediaType === 'image') {
    items.push({
      id: 'set_wallpaper',
      label: t('contextMenu.setWallpaper'),
      icon: markRaw(Monitor),
      action: async () => {
        try {
          await invokeIpc(IPC.SET_AS_WALLPAPER, { itemId: id })
          ui.addToast('success', t('contextMenu.wallpaperSet'))
        } catch (err) {
          console.error(err)
        }
      },
    })
  }

  ctxMenu.value = {
    visible: true,
    x: e.clientX,
    y: e.clientY,
    items,
  }
}

const detail = computed(() => media.detailItem!)
const absPath = computed(() => (detail.value ? convertFileSrc(detail.value.absPath) : ''))

// ── 不可用态（缺失检测 Part2 §3.2）──────────────────────────────────────────
// 卷离线 / 文件缺失 → 直接出明确提示，不尝试加载注定失败的源；
// availability='online' 但加载仍失败（文件被外部移动/删除而未重扫）→ @error 兜底。
const loadError = ref(false)
// 切换查看项时复位加载失败标志（否则上一张的失败会污染下一张）。
watch(
  () => media.detailItem?.id,
  () => {
    loadError.value = false
  },
)

const isUnavailable = computed(
  () => (detail.value?.availability && detail.value.availability !== 'online') || loadError.value,
)

const unavailableInfo = computed<{ title: string; hint: string }>(() => {
  const a = detail.value?.availability
  if (a === 'offline') {
    return { title: t('detail.unavailableOfflineTitle'), hint: t('detail.unavailableOfflineHint') }
  }
  if (a === 'missing') {
    return { title: t('detail.unavailableMissingTitle'), hint: t('detail.unavailableMissingHint') }
  }
  // online 但加载失败：文件可能被外部移动/删除而尚未重扫。
  return { title: t('detail.unavailableErrorTitle'), hint: t('detail.unavailableErrorHint') }
})

function onMediaError() {
  loadError.value = true
}

// ── Exotic 授权 gate（Part5 T12 增量3）───────────────────────────────────────
// 打开某项时解析其 exotic 授权态；未授权（purchase/blocked）→ 视图区显 PluginGate 引导，
// 而非渲染注定失败的原图。普通格式（非 exotic catalog）resolveForItem 直接放行、不发 IPC。
const exoticGate = useExoticGate()
const activateOpen = ref(false)

// gate 只在「有产品未授权」或「纯不可用」时接管视图；已授权 / 放行走原渲染。
const showExoticGate = computed(() => {
  const m = gateModeFor(exoticGate.entitlement.value)
  return m === 'purchase' || m === 'blocked'
})
// 传给 gate / 激活对话框的功能名：用格式名（如 PSD）给出上下文；gate 无名时会退回通用标题。
const exoticFeatureName = computed(() =>
  detail.value ? detail.value.fileFormat.toUpperCase() : '',
)

// 查看项变化即重解析（immediate 覆盖「详情已开时组件才挂载」的情形）。
// 关闭 / 无项时清态，避免上一项的 gate 残留污染下一项。
watch(
  () => media.detailItem?.id,
  (id) => {
    if (!id || !media.detailItem) {
      exoticGate.reset()
      return
    }
    // 失败/普通格式内部已置 entitlement=null（放行），无需在此 try/catch。
    void exoticGate.resolveForItem(id, media.detailItem.fileFormat)
  },
  { immediate: true },
)

// 激活成功 → 重解析授权态（转 Authorized 后 showExoticGate 归 false，gate 自动撤下）。
async function onExoticActivated() {
  const item = media.detailItem
  if (item) await exoticGate.resolveForItem(item.id, item.fileFormat)
}

// ── Viewer state — created ONCE, not inside computed() ─────────────────────
// ── 视图器状态 — 仅创建一次，不要放在 computed() 内部 ─────────────────────
// Calling useMediaDetail() inside computed() would recreate internal refs and
// re-register document event listeners every time a reactive dependency changes,
// permanently leaking mousemove/mouseup handlers.
// 如果在 computed() 内部调用 useMediaDetail()，每次响应式依赖变化时都会重新创建内部的 refs
// 并重新注册 document 事件监听器，从而导致 mousemove/mouseup 处理程序永久泄漏。
const state = useMediaDetail()

const viewerRef = ref<HTMLElement | null>(null)
const imgRef = ref<HTMLImageElement | null>(null)
const videoRef = ref<HTMLVideoElement | null>(null)

const zoomModeTitle = computed(() => {
  switch (state.zoomMode.value) {
    case 'auto':
      return t('detail.zoomModeAuto')
    case 'original':
      return t('detail.zoomModeOriginal')
    case 'fit-width':
      return t('detail.zoomModeFitWidth')
    case 'fit-height':
      return t('detail.zoomModeFitHeight')
    default:
      return t('detail.resetZoom')
  }
})

// 获取媒体真实宽高
// Get media true dimensions
function getMediaDimensions() {
  if (imgRef.value) {
    return { w: imgRef.value.naturalWidth, h: imgRef.value.naturalHeight }
  }
  if (videoRef.value) {
    return { w: videoRef.value.videoWidth, h: videoRef.value.videoHeight }
  }
  return { w: 1, h: 1 }
}

function handleToggleZoom() {
  if (!viewerRef.value || (!imgRef.value && !videoRef.value)) {
    state.resetZoom()
    return
  }
  const { w: iw, h: ih } = getMediaDimensions()
  const cw = viewerRef.value.clientWidth
  const ch = viewerRef.value.clientHeight
  if (!iw || !ih || !cw || !ch) {
    state.resetZoom()
    return
  }
  state.cycleZoomMode(cw, ch, iw, ih)
}

// Reset zoom whenever the viewed item changes
// 当查看的项目更改时重置缩放
watch(
  () => media.detailItem,
  (item) => {
    state.resetZoom()
    state.isPlayingLive.value = false
    state.liveVideoSrc.value = null
    zoomRatio.value = 1.0
    loadFacesFor(item ? { id: item.id, mediaType: item.mediaType } : null)
  },
)

const zoomRatio = ref(1.0)
function updateZoomRatio() {
  if (!viewerRef.value || (!imgRef.value && !videoRef.value)) return
  const { w, h } = getMediaDimensions()
  const iw = w || 1
  const ih = h || 1
  const cw = viewerRef.value.clientWidth || 1
  const ch = viewerRef.value.clientHeight || 1
  const base_w = Math.min(iw, cw, ch * (iw / ih))
  zoomRatio.value = base_w / iw
  recomputeFaceLayout()
}

const isZoomChanged = ref(false)
let zoomHighlightTimer: ReturnType<typeof setTimeout> | null = null

watch(
  () => state.scale.value,
  () => {
    isZoomChanged.value = true
    if (zoomHighlightTimer) clearTimeout(zoomHighlightTimer)
    zoomHighlightTimer = setTimeout(() => {
      isZoomChanged.value = false
    }, 2000)
    updateZoomRatio()
  },
)

// ── Face boxes (F6) ──────────────────────────────────────────────────────────
// Overlay the detected faces on the image. bbox is normalized [0,1] against the image's own
// pixels, so we project it onto the image's CONTENT rect (object-fit:contain leaves letterbox
// margins) measured live via getBoundingClientRect — which already includes the zoom/pan
// transform, so boxes track scaling and dragging for free.
// ── 人脸框（F6）─── 把检测到的人脸叠加到图上。bbox 是相对图像自身像素的归一化 [0,1]，故投影到
// 图像的**内容矩形**（object-fit:contain 会留黑边），用 getBoundingClientRect 实时测量——它已含
// 缩放/平移 transform，所以框会自动跟随缩放与拖拽。
const faces = ref<FaceBox[]>([])
// 人脸蓝框显隐开关（问题5）。默认显示；纯前端查看偏好，持久化到 localStorage（无需 IPC）。
const showFaces = ref(localStorage.getItem('detail_show_faces') !== 'false')
function toggleFaces() {
  showFaces.value = !showFaces.value
  localStorage.setItem('detail_show_faces', String(showFaces.value))
}
// Image content rect in viewer-local coords {x,y,w,h}; null when not measurable.
// 图像内容矩形（viewer 局部坐标 {x,y,w,h}）；不可测时为 null。
const faceContentRect = ref<{ x: number; y: number; w: number; h: number } | null>(null)
let faceRaf: number | null = null

async function loadFacesFor(item: { id: number; mediaType: string } | null) {
  if (!item || item.mediaType !== 'image') {
    faces.value = []
    return
  }
  faces.value = await person.getFacesForItem(item.id)
  await nextTick()
  recomputeFaceLayout()
}

function recomputeFaceLayout() {
  const img = imgRef.value
  const viewer = viewerRef.value
  if (!img || !viewer || !img.naturalWidth || !img.naturalHeight) {
    faceContentRect.value = null
    return
  }
  const ir = img.getBoundingClientRect()
  const vr = viewer.getBoundingClientRect()
  const natRatio = img.naturalWidth / img.naturalHeight
  const boxRatio = ir.width / Math.max(ir.height, 1)
  let cw: number, ch: number
  if (natRatio > boxRatio) {
    cw = ir.width
    ch = ir.width / natRatio
  } else {
    ch = ir.height
    cw = ir.height * natRatio
  }
  faceContentRect.value = {
    x: ir.left - vr.left + (ir.width - cw) / 2,
    y: ir.top - vr.top + (ir.height - ch) / 2,
    w: cw,
    h: ch,
  }
}

// rAF-throttled recompute (transform changes fire rapidly while dragging).
// rAF 节流重算（拖拽时 transform 高频变化）。
function scheduleFaceRecompute() {
  if (faceRaf != null) return
  faceRaf = requestAnimationFrame(() => {
    faceRaf = null
    recomputeFaceLayout()
  })
}

watch(() => state.transform.value, scheduleFaceRecompute)
// Toggling the info panel resizes the viewer → reproject boxes.
// 切换信息面板会改变 viewer 宽度 → 重新投影框。
watch(
  () => state.showInfo.value,
  () => nextTick(recomputeFaceLayout),
)

function faceBoxStyle(f: FaceBox): Record<string, string> {
  const r = faceContentRect.value
  if (!r) return { display: 'none' }
  return {
    left: `${r.x + f.bbox[0] * r.w}px`,
    top: `${r.y + f.bbox[1] * r.h}px`,
    width: `${f.bbox[2] * r.w}px`,
    height: `${f.bbox[3] * r.h}px`,
  }
}

// ── Keyboard shortcuts ─────────────────────────────────────────────────────
// ── 快捷键 ─────────────────────────────────────────────────────
// Registered via onMounted / onBeforeUnmount to avoid accumulating listeners
// on each open/close cycle (Teleport keeps the component alive).
// 通过 onMounted / onBeforeUnmount 注册以避免在每次打开/关闭周期内积累监听器
// (Teleport 使组件保持活动状态)。
function onKeydown(e: KeyboardEvent) {
  if (!media.isDetailOpen) return
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopImmediatePropagation()
    close()
    return
  }
  if (e.key === '+' || e.key === '=') {
    state.zoomIn()
    return
  }
  if (e.key === '-') {
    state.zoomOut()
    return
  }
  if (e.key === 'i' || e.key === 'I') {
    state.toggleInfo()
    return
  }
  if (e.key === 'ArrowLeft') {
    media.navigateDetail(-1)
    return
  }
  if (e.key === 'ArrowRight') {
    media.navigateDetail(1)
    return
  }
}

let accumulatedDelta = 0
let deltaTimer: ReturnType<typeof setTimeout> | null = null

function onWheelHandler(e: WheelEvent) {
  const handledZoom = state.onWheel(e)

  // If useMediaDetail wasn't HMR'd properly, handledZoom might be undefined.
  // Explicitly check for true.
  // 如果 useMediaDetail 没有正确触发 HMR 更新，handledZoom 可能为 undefined。
  // 请显式检查是否为 true。
  if (handledZoom !== true) {
    accumulatedDelta += e.deltaY

    // Clear accumulator after scrolling stops (e.g. 50ms)
    // 停止滚动后清除累加器 (例如 50 毫秒)
    if (deltaTimer) clearTimeout(deltaTimer)
    deltaTimer = setTimeout(() => {
      accumulatedDelta = 0
    }, 50)

    if (accumulatedDelta >= 50) {
      media.navigateDetail(1)
      accumulatedDelta = 0
    } else if (accumulatedDelta <= -50) {
      media.navigateDetail(-1)
      accumulatedDelta = 0
    }
  }
}

function onImageClick(_e: MouseEvent) {
  // If user is dragging (scale > 1), we shouldn't close the info.
  // We can just close it if it's open.
  // 如果用户正在拖拽 (scale > 1)，我们不应该关闭信息栏。
  // 如果它打开着，我们可以直接关闭它。
  if (state.showInfo.value && state.scale.value <= 1) {
    state.showInfo.value = false
  }
}

// 卷插拔监听（T13 §3.7 离线 UX 验收点「重连自动恢复」）：覆盖层打开时若卷重连，
// 刷新当前项可用态 → isUnavailable 归 false → 原图自动加载，无需用户手动关开。
let unlistenVolumes: UnlistenFn | null = null

onMounted(() => {
  document.addEventListener('keydown', onKeydown)
  window.addEventListener('resize', updateZoomRatio)
  // listen 异步返回 unlisten；不阻塞挂载，就绪后存句柄供卸载时解绑。
  listen(EVENTS.VOLUMES_CHANGED, () => {
    if (media.isDetailOpen) void media.refreshDetailAvailability()
  }).then((un) => {
    unlistenVolumes = un
  })
})
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown)
  window.removeEventListener('resize', updateZoomRatio)
  unlistenVolumes?.()
  state.cleanup()
})

function close() {
  media.closeDetail()
}

async function toggleFav() {
  if (!detail.value) return
  const newVal = await media.toggleFavorite(detail.value.id)
  detail.value.isFavorited = newVal
}

async function setRating(n: number) {
  if (!detail.value) return
  // 点当前星 = 清零（toggle-off）。next 算一次供 DB 写入与本地回写共用——
  // 此前本地恒写 n，点当前星清零后 UI 仍显 n（与 DB 不一致），一并修正。
  const next = n === detail.value.rating ? 0 : n
  await media.setRating(detail.value.id, next)
  if (detail.value) detail.value.rating = next
}

async function setColorLabel(value: number) {
  if (!detail.value) return
  // ColorLabelPicker 的 @change 已吐出 toggle 后的值（点当前色→0），直接持久化即可，
  // 与工具栏批量设色 / 按色筛选同一语义。乐观回写 store 响应对象即时刷新色块。
  await media.setColorLabel(detail.value.id, value)
  if (detail.value) detail.value.colorLabel = value
}

async function showInExplorer() {
  if (!detail.value) return
  await invoke(IPC.SHOW_IN_EXPLORER, { itemId: detail.value.id })
}

async function toggleLive() {
  if (!detail.value) return
  if (state.isPlayingLive.value) {
    state.isPlayingLive.value = false
    state.liveVideoSrc.value = null
  } else {
    try {
      // 走 invokeIpc（而非裸 invoke）以拿到结构化 IpcError.code——据此分流卷离线。
      const path = await invokeIpc<string>(IPC.GET_COMPANION_VIDEO_URL, { itemId: detail.value.id })
      state.liveVideoSrc.value = convertFileSrc(path)
      state.isPlayingLive.value = true
    } catch (e) {
      // 卷离线（T13）：后端返 VolumeOffline{message=卷标签} → 提示「请插入设备 <label>」而非泛化错误。
      const err = e as IpcError
      if (err?.code === 'VolumeOffline') {
        ui.addToast('error', t('detail.volumeOffline', { label: err.message }))
      } else {
        ui.addToast('error', t('detail.livePhotoError'))
      }
    }
  }
}

async function onMoveCopyConfirm(targetNode: DirNode) {
  const id = moveCopyDialog.value.targetId
  // targetNode 为选中的 DirNode,以 id（目录 id）为落点。
  if (!id || targetNode?.id == null) return
  moveCopyDialog.value.isOpen = false
  const mode = moveCopyDialog.value.mode

  // T6：经 historyStore 走 relocate_media_items / copy_media_items_db（DB 级、可撤销），
  // 与画廊 performMediaDrop / MediaGrid 对话框同一路径。history 内部 refresh() 已重载树（实时计数）
  // + 重算网格,故不再手动调整计数 / loadStats / startScan。
  try {
    const n =
      mode === 'copy'
        ? await history.copyMedia([id], targetNode.id, `复制 1 项`)
        : await history.moveMedia([id], targetNode.id, `移动 1 项`)
    if (n === 0) return // 无实际移动/复制（已在目标 / 冲突）

    ui.addToast(
      'success',
      mode === 'move' ? t('contextMenu.moveSuccess') : t('contextMenu.copySuccess'),
    )

    // 移动后当前项已离开视图 → 跳到下一张;若跳转后仍是同一项,说明它是最后一张,关闭详情。
    // 这是详情覆盖层特有的导航 UX,与撤销无关,须保留。
    if (mode === 'move') {
      media.navigateDetail(1)
      if (media.detailItem?.id === id) {
        media.closeDetail()
      }
    }
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    ui.addToast(
      'error',
      mode === 'copy'
        ? t('common.copyFailed', { error: msg })
        : t('common.moveFailed', { error: msg }),
    )
  }
}
</script>

<style scoped>
/* 硬编码色豁免说明(S5,设计 §6.2):看图台 #000、人脸框青色、照片上的白字/黑纱
   渐变、星级金色均为「媒体上的恒定浮层」语义——看图台永远黑(专业看图惯例)、
   人脸框需在任意照片上可见、星级金全主题统一,刻意不随主题。 */
.detail-overlay {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: flex;
}

.detail-panel {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  z-index: 201;
}

/* ── Viewer ───────────────────────────────────────────────────────────── */
/* ── 视图器 ───────────────────────────────────────────────────────────── */
.detail-viewer {
  flex: 1;
  position: relative;
  overflow: hidden;
  background: #000;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: grab;
}
.detail-viewer:active {
  cursor: grabbing;
}

.detail-viewer__img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  transform-origin: center;
  user-select: none;
  pointer-events: none;
  transition: transform 0.2s cubic-bezier(0.25, 0.46, 0.45, 0.94);
}
.detail-viewer__img.is-dragging {
  transition: none;
}
.detail-viewer__video,
.detail-viewer__audio {
  max-width: 90%;
  max-height: 90%;
}
.detail-viewer__document {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--spacing-md);
  color: var(--color-text-secondary);
  font-size: 48px;
}

/* ── 不可用占位（卷离线 / 文件缺失 / 加载失败）──────────────────────────── */
.detail-viewer__unavailable {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--spacing-sm);
  color: var(--color-text-tertiary);
  text-align: center;
  padding: var(--spacing-xl);
  user-select: none;
}
.detail-viewer__unavailable-title {
  font-size: var(--font-size-md);
  font-weight: 600;
  color: var(--color-text-secondary);
}
.detail-viewer__unavailable-hint {
  font-size: var(--font-size-sm);
  color: var(--color-text-tertiary);
  max-width: 360px;
  line-height: 1.5;
}

/* ── Exotic 授权 gate（居中限宽，浮于全黑视图区）─────────────────────────── */
.detail-viewer__gate {
  width: 100%;
  max-width: 440px;
  padding: var(--spacing-xl);
  /* 视图区 cursor:grab 对 gate 无意义，恢复默认指针以免误导可拖拽。 */
  cursor: default;
}

.detail-viewer__live-video {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: contain;
}

/* ── Face boxes (F6) ──────────────────────────────────────────────────── */
.face-overlay {
  position: absolute;
  inset: 0;
  pointer-events: none; /* 纯展示，绝不拦截缩放/拖拽 */
  z-index: 2;
}
.face-box {
  position: absolute;
  border: 2px solid rgba(80, 200, 255, 0.9);
  border-radius: 4px;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.4);
}
.face-box__label {
  position: absolute;
  left: 0;
  bottom: 100%;
  margin-bottom: 2px;
  padding: 1px 6px;
  font-size: 11px;
  line-height: 1.4;
  white-space: nowrap;
  color: #fff;
  background: rgba(80, 200, 255, 0.92);
  border-radius: 3px;
}

/* ── Controls ─────────────────────────────────────────────────────────── */
/* ── 控制器 ─────────────────────────────────────────────────────────── */
.detail-controls {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 52px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 var(--spacing-md);
  background: linear-gradient(transparent, rgba(0, 0, 0, 0.75));
  color: #fff;
  z-index: 10;
}
.detail-controls__left,
.detail-controls__right {
  display: flex;
  align-items: center;
  gap: 4px;
}
.detail-controls .btn-icon {
  color: rgba(255, 255, 255, 0.8);
  font-size: 14px;
}
.detail-controls .btn-icon:hover {
  color: #fff;
  background: rgba(255, 255, 255, 0.12);
}
.detail-controls .btn-icon.active {
  color: var(--color-accent);
}
.zoom-percentage {
  font-variant-numeric: tabular-nums; /* 等宽数字避免抖动 */
  min-width: 48px;
  text-align: center;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.5);
  transition: color 0.3s ease;
  user-select: none;
}
.zoom-percentage.zoom-highlight {
  color: #fff;
}
.detail-controls__center {
  flex: 1;
  text-align: center;
  overflow: hidden;
}
.detail-controls__name {
  font-size: var(--font-size-sm);
  color: rgba(255, 255, 255, 0.85);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  display: block;
  user-select: text;
  -webkit-user-select: text;
  cursor: text;
}

/* ── Info sidebar ─────────────────────────────────────────────────────── */
/* ── 信息侧边栏 ─────────────────────────────────────────────────────── */
.detail-info {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  width: 300px;
  background: var(--color-bg-secondary);
  border-left: 1px solid var(--color-border);
  overflow-y: auto;
  z-index: 12;
  display: flex;
  flex-direction: column;
  user-select: text;
  -webkit-user-select: text;
}
.detail-info__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--spacing-md);
  border-bottom: 1px solid var(--color-border);
  font-weight: 600;
  font-size: var(--font-size-sm);
  flex-shrink: 0;
}

.info-section {
  padding: var(--spacing-md);
  border-bottom: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  gap: var(--spacing-sm);
}
.info-section__title {
  font-size: var(--font-size-xs);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--color-text-tertiary);
  margin-bottom: 4px;
}
.info-row {
  display: flex;
  justify-content: space-between;
  gap: var(--spacing-sm);
}
.info-label {
  font-size: var(--font-size-xs);
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}
.info-value {
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: text;
  -webkit-user-select: text;
  cursor: text;
}

.rating-stars {
  display: flex;
  gap: 4px;
}
.star {
  font-size: 22px;
  color: var(--color-border);
  transition: color var(--transition-fast);
  cursor: pointer;
}
.star.filled {
  color: #ffc107;
}
.star:hover {
  color: #ffd54f;
}

.clickable-path {
  text-align: left;
  white-space: normal;
  word-break: break-all;
  line-height: 1.4;
  cursor: pointer;
  transition: color var(--transition-fast);
  text-decoration: underline;
  text-decoration-color: transparent;
}
.clickable-path:hover {
  color: var(--color-accent);
  text-decoration-color: var(--color-accent);
}

/* ── Slide transition ─────────────────────────────────────────────────── */
/* ── 滑动过渡 ─────────────────────────────────────────────────── */
.slide-enter-from,
.slide-leave-to {
  transform: translateX(100%);
}
.slide-enter-active,
.slide-leave-active {
  transition: transform var(--transition-normal);
}
</style>
