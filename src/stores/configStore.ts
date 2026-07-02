import { defineStore } from 'pinia'
import { invokeIpc } from '../utils/ipc'
import { IPC } from '../constants/ipc'

export const useConfigStore = defineStore('config', {
  state: () => ({
    thumbSkipMaxKb: 200,
    thumbCacheMaxMb: 1024,
    thumbSize: 480,
    timelineScrollWidth: 6,
    uiFontSize: 16,
    enableHoverScale: true,
    logLevel: 'info',
    thumbStrategy: 'cpu',
    gpuEngine: 'wic',
    aiProviderOverride: 'auto',
    aiBatchSize: 0,
    // AI 模型下载首选源：'official'=官方 HuggingFace 优先，'mirror'=国内镜像 hf-mirror.com 优先。
    aiDownloadSource: 'official',
    // 视频派生开关：是否提取视频封面 / 关键帧雪碧图（默认开启）。
    enableVideoCover: true,
    enableVideoKeyframes: true,
    // AI 高清缓存开关（opt-in，默认关）：开启后后台静默为每张图生成短边≥336 的 WebP 缓存，
    // 使 CLIP 分析解码该小缓存而非全分辨率原图，大幅降低分析时的 CPU 占用。
    aiHqCache: false,
    isLoaded: false,
  }),

  actions: {
    async loadConfig() {
      if (this.isLoaded) return
      try {
        const fetchInt = async (key: string, defaultVal: number) => {
          const val = await invokeIpc<string | null>(IPC.GET_APP_CONFIG, { key })
          return val ? parseInt(val, 10) : defaultVal
        }
        const fetchStr = async (key: string, defaultVal: string) => {
          const val = await invokeIpc<string | null>(IPC.GET_APP_CONFIG, { key })
          return val ? val : defaultVal
        }
        const fetchBool = async (key: string, defaultVal: boolean) => {
          const val = await invokeIpc<string | null>(IPC.GET_APP_CONFIG, { key })
          return val ? val === 'true' : defaultVal
        }

        this.thumbSkipMaxKb = await fetchInt('thumb_skip_max_kb', 200)
        this.thumbCacheMaxMb = await fetchInt('thumb_cache_max_mb', 1024)
        this.thumbSize = await fetchInt('thumb_size', 240)
        this.timelineScrollWidth = await fetchInt('timeline_scroll_width', 6)
        this.uiFontSize = await fetchInt('ui_font_size', 16)
        this.enableHoverScale = await fetchBool('enable_thumb_hover_scale', true)
        this.logLevel = await fetchStr('log_level', 'info')
        this.thumbStrategy = await fetchStr('thumb_strategy', 'cpu')
        this.gpuEngine = await fetchStr('gpu_engine', 'wic')
        this.aiProviderOverride = await fetchStr('ai_provider_override', 'auto')
        this.aiBatchSize = await fetchInt('ai_batch_size', 0)
        this.aiDownloadSource = await fetchStr('ai_download_source', 'official')
        this.enableVideoCover = await fetchBool('enable_video_cover', true)
        this.enableVideoKeyframes = await fetchBool('enable_video_keyframes', true)
        this.aiHqCache = await fetchBool('ai_hq_cache_enabled', false)

        // 应用一些 CSS 变量（thumbStrategy/gpuEngine 单一来源即本 store，无需再镜像至 uiStore）。
        document.documentElement.style.setProperty(
          '--scrollbar-width',
          `${this.timelineScrollWidth}px`,
        )

        const diff = this.uiFontSize - 16
        document.documentElement.style.setProperty('--font-size-xs', `${12 + diff}px`)
        document.documentElement.style.setProperty('--font-size-sm', `${13 + diff}px`)
        document.documentElement.style.setProperty('--font-size-base', `${16 + diff}px`)
        document.documentElement.style.setProperty('--font-size-md', `${17 + diff}px`)
        document.documentElement.style.setProperty('--font-size-lg', `${20 + diff}px`)
        document.documentElement.style.setProperty('--font-size-xl', `${24 + diff}px`)
        document.documentElement.style.setProperty('--font-size-2xl', `${30 + diff}px`)

        if (this.enableHoverScale) {
          document.documentElement.classList.remove('disable-hover-scale')
        } else {
          document.documentElement.classList.add('disable-hover-scale')
        }

        this.isLoaded = true
      } catch (e) {
        console.error('Failed to load config', e)
      }
    },

    async saveConfig(key: string, value: string) {
      await invokeIpc(IPC.SET_APP_CONFIG, { key, value })
    },

    async setThumbSkipMaxKb(val: number) {
      this.thumbSkipMaxKb = val
      await this.saveConfig('thumb_skip_max_kb', val.toString())
    },
    async setThumbCacheMaxMb(val: number) {
      this.thumbCacheMaxMb = val
      await this.saveConfig('thumb_cache_max_mb', val.toString())
    },
    async setThumbSize(val: number) {
      console.log(val)

      this.thumbSize = val
      await this.saveConfig('thumb_size', val.toString())
    },
    async setTimelineScrollWidth(val: number) {
      this.timelineScrollWidth = val
      await this.saveConfig('timeline_scroll_width', val.toString())
      document.documentElement.style.setProperty('--scrollbar-width', `${val}px`)
    },
    async setUiFontSize(val: number) {
      this.uiFontSize = val
      await this.saveConfig('ui_font_size', val.toString())
      const diff = val - 16
      document.documentElement.style.setProperty('--font-size-xs', `${12 + diff}px`)
      document.documentElement.style.setProperty('--font-size-sm', `${13 + diff}px`)
      document.documentElement.style.setProperty('--font-size-base', `${16 + diff}px`)
      document.documentElement.style.setProperty('--font-size-md', `${17 + diff}px`)
      document.documentElement.style.setProperty('--font-size-lg', `${20 + diff}px`)
      document.documentElement.style.setProperty('--font-size-xl', `${24 + diff}px`)
      document.documentElement.style.setProperty('--font-size-2xl', `${30 + diff}px`)
    },
    async setEnableHoverScale(val: boolean) {
      this.enableHoverScale = val
      await this.saveConfig('enable_thumb_hover_scale', val.toString())
      if (val) {
        document.documentElement.classList.remove('disable-hover-scale')
      } else {
        document.documentElement.classList.add('disable-hover-scale')
      }
    },
    async setLogLevel(val: string) {
      this.logLevel = val
      await this.saveConfig('log_level', val)
    },
    async setThumbStrategy(val: string) {
      this.thumbStrategy = val
      await this.saveConfig('thumb_strategy', val)
    },
    async setGpuEngine(val: string) {
      this.gpuEngine = val
      await this.saveConfig('gpu_engine', val)
    },
    async setAiProviderOverride(val: string) {
      this.aiProviderOverride = val
      await this.saveConfig('ai_provider_override', val)
    },
    async setAiBatchSize(val: number) {
      this.aiBatchSize = val
      await this.saveConfig('ai_batch_size', val.toString())
    },
    async setAiDownloadSource(val: string) {
      this.aiDownloadSource = val
      await this.saveConfig('ai_download_source', val)
    },
    async setEnableVideoCover(val: boolean) {
      this.enableVideoCover = val
      await this.saveConfig('enable_video_cover', val.toString())
      await this.restartDerivation()
    },
    async setEnableVideoKeyframes(val: boolean) {
      this.enableVideoKeyframes = val
      await this.saveConfig('enable_video_keyframes', val.toString())
      await this.restartDerivation()
    },
    // AI 高清缓存开关：开启 → 重启派生流水线，后台静默 backfill 并生成 ai_thumb 缓存；
    // 关闭 → 重启后生产者排除 ai_thumb（已生成的缓存仍保留并被分析复用）。
    async setAiHqCache(val: boolean) {
      this.aiHqCache = val
      await this.saveConfig('ai_hq_cache_enabled', val.toString())
      await this.restartDerivation()
    },
    // 重启派生流水线，使新开关立即生效：开启 → 接续该 kind 的待处理项；
    // 关闭 → 重启后生产者会排除该 kind（在途任务恢复为待处理并暂停）。失败静默（仅尽力而为）。
    async restartDerivation() {
      try {
        await invokeIpc(IPC.START_DERIVATION)
      } catch (e) {
        console.warn('Failed to restart derivation pipeline after toggle', e)
      }
    },
  },
})
