<template>
  <div class="audio-player">
    <!-- 工具栏：返回 / 标题 / 外部打开 -->
    <div class="audio-player__toolbar">
      <button class="ap-btn" @click="goBack" :title="t('common.back')">
        <ChevronLeft :size="18" /> <span>{{ t('common.back') }}</span>
      </button>
      <span class="audio-player__title" :title="title">{{ title }}</span>
      <div class="audio-player__spacer"></div>
      <button
        v-if="detail"
        class="ap-btn"
        @click="openExternal"
        :title="t('common.openExternal')"
        :aria-label="t('common.openExternal')"
      >
        <ExternalLink :size="16" />
      </button>
    </div>

    <div v-if="detail" class="audio-player__body">
      <!-- 左：封面 + 元数据 + 播放控件 -->
      <div class="audio-player__main">
        <div class="audio-player__cover">
          <img v-if="coverUrl" :src="coverUrl" :alt="title" />
          <div v-else class="audio-player__cover-fallback">
            <Music :size="96" />
          </div>
        </div>

        <div class="audio-player__info">
          <h1 class="audio-player__track">{{ meta.trackTitle || stripExt(detail.fileName) }}</h1>
          <p v-if="meta.artist" class="audio-player__artist">{{ meta.artist }}</p>
          <p v-if="meta.albumTitle" class="audio-player__album">
            {{ meta.albumTitle }}<span v-if="meta.year"> · {{ meta.year }}</span>
          </p>
        </div>

        <!-- 控件：进度条 + 播放/暂停 + 时间 + 音量 -->
        <div class="audio-player__controls">
          <div class="audio-player__seek">
            <span class="audio-player__time">{{ fmt(currentTime) }}</span>
            <input
              class="audio-player__range"
              type="range"
              min="0"
              :max="duration || 0"
              step="0.1"
              :value="currentTime"
              @input="onSeek"
            />
            <span class="audio-player__time">{{ fmt(duration) }}</span>
          </div>
          <div class="audio-player__buttons">
            <button
              class="ap-icon"
              @click="seekBy(-10)"
              :title="t('audio.rewind10')"
              :aria-label="t('audio.rewind10')"
            >
              <SkipBack :size="20" />
            </button>
            <button
              class="ap-icon ap-icon--play"
              @click="togglePlay"
              :title="playing ? t('common.pause') : t('audio.play')"
              :aria-label="playing ? t('common.pause') : t('audio.play')"
            >
              <component :is="playing ? Pause : Play" :size="26" :fill="'currentColor'" />
            </button>
            <button
              class="ap-icon"
              @click="seekBy(10)"
              :title="t('audio.forward10')"
              :aria-label="t('audio.forward10')"
            >
              <SkipForward :size="20" />
            </button>
            <div class="audio-player__volume">
              <Volume2 :size="16" />
              <input
                class="audio-player__range audio-player__range--vol"
                type="range"
                min="0"
                max="1"
                step="0.01"
                :value="volume"
                @input="onVolume"
              />
            </div>
          </div>
        </div>

        <!-- 元数据细节 -->
        <dl class="audio-player__meta">
          <template v-if="meta.genre"
            ><dt>{{ t('audio.genre') }}</dt>
            <dd>{{ meta.genre }}</dd></template
          >
          <template v-if="meta.trackNo"
            ><dt>{{ t('audio.trackNo') }}</dt>
            <dd>{{ meta.trackNo }}</dd></template
          >
          <template v-if="meta.audioCodec"
            ><dt>{{ t('audio.codec') }}</dt>
            <dd>{{ meta.audioCodec }}</dd></template
          >
          <template v-if="detail.fileFormat"
            ><dt>{{ t('detail.format') }}</dt>
            <dd>{{ detail.fileFormat.toUpperCase() }}</dd></template
          >
        </dl>
      </div>

      <!-- 右：歌词（同步高亮 / 纯文本） -->
      <div class="audio-player__lyrics" ref="lyricsBox">
        <template v-if="lyricsSynced && syncedLines.length">
          <p
            v-for="(line, i) in syncedLines"
            :key="i"
            :ref="(el) => setLineRef(el as HTMLElement | null, i)"
            class="audio-player__lyric-line"
            :class="{ 'is-active': i === activeLine }"
            @click="seekTo(line.time)"
          >
            {{ line.text || '♪' }}
          </p>
        </template>
        <pre v-else-if="detail.lyrics" class="audio-player__lyric-plain">{{ detail.lyrics }}</pre>
        <div v-else class="audio-player__no-lyrics">{{ t('audio.noLyrics') }}</div>
      </div>
    </div>

    <div v-else-if="error" class="audio-player__error">{{ error }}</div>

    <!-- 共享音频元素（隐藏，控件自绘） -->
    <audio
      ref="audioEl"
      :src="url"
      preload="metadata"
      @timeupdate="onTimeUpdate"
      @loadedmetadata="onLoadedMeta"
      @play="playing = true"
      @pause="playing = false"
      @ended="playing = false"
    ></audio>
  </div>
</template>

<script setup lang="ts">
// 音频播放器（需求6, §3.6）：路由 /audio/:id。封面 + 控件 + 同步歌词 + 元数据面板。
// 标签/歌词由后端 get_audio_detail 懒加载（既有库无需重扫）；封面为后端按需抽取的全分辨率内嵌图。
import { ref, computed, watch, onBeforeUnmount, nextTick } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { open as shellOpen } from '@tauri-apps/plugin-shell'
import {
  ChevronLeft,
  ExternalLink,
  Music,
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Volume2,
} from '@lucide/vue'
import { IPC } from '../constants/ipc'
import { formatDuration } from '../utils/format'
import { parseLrc, activeLineIndex, type LrcLine } from '../utils/lrc'

interface AudioMeta {
  audioCodec?: string | null
  artist?: string | null
  albumTitle?: string | null
  trackTitle?: string | null
  trackNo?: number | null
  year?: number | null
  genre?: string | null
}
interface AudioDetail {
  fileName: string
  fileFormat: string
  absPath: string
  meta: AudioMeta
  coverPath?: string | null
  lyrics?: string | null
  lyricsSynced: boolean
}

const route = useRoute()
const router = useRouter()
const { t } = useI18n()
const id = computed(() => Number(route.params.id))

const detail = ref<AudioDetail | null>(null)
const error = ref('')

const audioEl = ref<HTMLAudioElement | null>(null)
const playing = ref(false)
const currentTime = ref(0)
const duration = ref(0)
const volume = ref(1)

const url = computed(() => (detail.value ? convertFileSrc(detail.value.absPath) : ''))
const coverUrl = computed(() =>
  detail.value?.coverPath ? convertFileSrc(detail.value.coverPath) : '',
)
const title = computed(() => detail.value?.fileName ?? t('routes.audio'))
const meta = computed<AudioMeta>(() => detail.value?.meta ?? {})
const lyricsSynced = computed(() => detail.value?.lyricsSynced ?? false)

// 同步歌词：解析为带时间轴的行。
const syncedLines = ref<LrcLine[]>([])
const activeLine = ref(-1)
const lineEls: (HTMLElement | null)[] = []
const lyricsBox = ref<HTMLElement | null>(null)

function setLineRef(el: HTMLElement | null, i: number) {
  lineEls[i] = el
}

function fmt(sec: number): string {
  if (!Number.isFinite(sec) || sec <= 0) return '0:00'
  return formatDuration(sec * 1000)
}
function stripExt(name: string): string {
  const i = name.lastIndexOf('.')
  return i > 0 ? name.slice(0, i) : name
}

// ── 播放控制 ──────────────────────────────────────────────────────────────────
function togglePlay() {
  const el = audioEl.value
  if (!el) return
  if (el.paused) el.play().catch(() => {})
  else el.pause()
}
function onSeek(e: Event) {
  const el = audioEl.value
  if (!el) return
  el.currentTime = Number((e.target as HTMLInputElement).value)
}
function seekTo(sec: number) {
  const el = audioEl.value
  if (!el) return
  el.currentTime = sec
  if (el.paused) el.play().catch(() => {})
}
function seekBy(delta: number) {
  const el = audioEl.value
  if (!el) return
  el.currentTime = Math.max(0, Math.min(duration.value, el.currentTime + delta))
}
function onVolume(e: Event) {
  const el = audioEl.value
  if (!el) return
  const v = Number((e.target as HTMLInputElement).value)
  el.volume = v
  volume.value = v
}

function onLoadedMeta() {
  const el = audioEl.value
  if (el) duration.value = el.duration || 0
}
function onTimeUpdate() {
  const el = audioEl.value
  if (!el) return
  currentTime.value = el.currentTime
  if (lyricsSynced.value && syncedLines.value.length) {
    const idx = activeLineIndex(syncedLines.value, el.currentTime)
    if (idx !== activeLine.value) {
      activeLine.value = idx
      scrollActiveIntoView()
    }
  }
}

// 高亮行居中滚动（仅在切换时触发，避免抖动）。
function scrollActiveIntoView() {
  const el = lineEls[activeLine.value]
  if (el) el.scrollIntoView({ block: 'center', behavior: 'smooth' })
}

async function load() {
  detail.value = null
  error.value = ''
  syncedLines.value = []
  activeLine.value = -1
  lineEls.length = 0
  currentTime.value = 0
  duration.value = 0
  try {
    const d = await invoke<AudioDetail>(IPC.GET_AUDIO_DETAIL, { id: id.value })
    detail.value = d
    if (d.lyricsSynced && d.lyrics) {
      syncedLines.value = parseLrc(d.lyrics)
    }
    // 等 <audio> 重新绑定 src 后自动尝试播放（静音策略不影响音频）。
    await nextTick()
    audioEl.value?.play().catch(() => {})
  } catch (e) {
    error.value = t('audio.openFailed', { error: (e as Error)?.message ?? e })
  }
}

function goBack() {
  if (window.history.length > 1) router.back()
  else router.push('/')
}
async function openExternal() {
  if (detail.value) await shellOpen(detail.value.absPath).catch(() => {})
}

watch(id, load, { immediate: true })

onBeforeUnmount(() => {
  audioEl.value?.pause()
})
</script>

<style scoped>
.audio-player {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  background: var(--color-bg-base);
  z-index: 5;
}
.audio-player__toolbar {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--color-border);
  background: var(--color-bg-surface);
}
.ap-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: transparent;
  border: 1px solid var(--color-border);
  color: var(--color-text-primary);
  padding: 5px 10px;
  border-radius: var(--radius-md);
  cursor: pointer;
  font-size: var(--font-size-sm);
}
.ap-btn:hover {
  background: var(--color-bg-elevated);
}
.audio-player__title {
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 50vw;
}
.audio-player__spacer {
  flex: 1;
}

.audio-player__body {
  flex: 1;
  min-height: 0;
  display: flex;
}
.audio-player__main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 18px;
  padding: 32px;
  overflow-y: auto;
}
.audio-player__cover {
  width: min(42vh, 360px);
  height: min(42vh, 360px);
  border-radius: var(--radius-md);
  overflow: hidden;
  background: var(--color-bg-elevated);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.35);
  flex: 0 0 auto;
}
.audio-player__cover img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.audio-player__cover-fallback {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-secondary);
}
.audio-player__info {
  text-align: center;
}
.audio-player__track {
  font-size: 1.4rem;
  font-weight: 700;
  margin: 0;
  color: var(--color-text-primary);
}
.audio-player__artist {
  margin: 6px 0 0;
  color: var(--color-text-primary);
}
.audio-player__album {
  margin: 2px 0 0;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
}

.audio-player__controls {
  width: min(100%, 460px);
}
.audio-player__seek {
  display: flex;
  align-items: center;
  gap: 10px;
}
.audio-player__time {
  font-family: var(--font-mono);
  font-size: var(--font-size-xs);
  color: var(--color-text-secondary);
  min-width: 42px;
  text-align: center;
}
.audio-player__range {
  flex: 1;
  accent-color: var(--color-accent);
  cursor: pointer;
}
.audio-player__range--vol {
  flex: 0 0 90px;
}
.audio-player__buttons {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 18px;
  margin-top: 14px;
}
.ap-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: var(--color-text-primary);
  cursor: pointer;
  padding: 6px;
  border-radius: 50%;
}
.ap-icon:hover {
  background: var(--color-bg-elevated);
}
.ap-icon--play {
  background: var(--color-accent);
  color: #fff;
  width: 52px;
  height: 52px;
}
.ap-icon--play:hover {
  background: var(--color-accent);
  filter: brightness(1.08);
}
.audio-player__volume {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--color-text-secondary);
  margin-left: 8px;
}

.audio-player__meta {
  display: grid;
  grid-template-columns: auto auto;
  gap: 4px 14px;
  margin: 0;
  font-size: var(--font-size-sm);
}
.audio-player__meta dt {
  color: var(--color-text-secondary);
  text-align: right;
}
.audio-player__meta dd {
  margin: 0;
  color: var(--color-text-primary);
}

.audio-player__lyrics {
  flex: 0 0 38%;
  max-width: 460px;
  min-width: 280px;
  border-left: 1px solid var(--color-border);
  overflow-y: auto;
  padding: 40px 28px;
  background: var(--color-bg-surface);
}
.audio-player__lyric-line {
  margin: 0;
  padding: 8px 0;
  text-align: center;
  color: var(--color-text-secondary);
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition:
    color 0.2s,
    transform 0.2s;
}
.audio-player__lyric-line.is-active {
  color: var(--color-accent);
  font-weight: 700;
  transform: scale(1.05);
}
.audio-player__lyric-plain {
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--color-text-primary);
  font-size: var(--font-size-sm);
  line-height: 1.8;
  margin: 0;
  font-family: inherit;
}
.audio-player__no-lyrics {
  color: var(--color-text-secondary);
  text-align: center;
  margin-top: 40px;
  font-size: var(--font-size-sm);
}
.audio-player__error {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-secondary);
}
</style>
