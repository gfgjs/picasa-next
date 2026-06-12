import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { useUiStore } from './uiStore'

export const useConfigStore = defineStore('config', {
  state: () => ({
    thumbSkipMaxKb: 200,
    thumbCacheMaxMb: 1024,
    thumbSize: 240,
    timelineScrollWidth: 6,
    uiFontSize: 16,
    enableHoverScale: true,
    logLevel: 'info',
    thumbStrategy: 'cpu',
    gpuEngine: 'wic',
    aiImageModel: 'cn-clip-vit-b16-image.onnx',
    aiTextModel: 'cn-clip-vit-b16-text.onnx',
    aiProviderOverride: 'auto',
    aiBatchSize: 0,
    isLoaded: false
  }),

  actions: {
    async loadConfig() {
      if (this.isLoaded) return;
      try {
        const fetchInt = async (key: string, defaultVal: number) => {
          const val = await invoke<string | null>('get_app_config', { key })
          return val ? parseInt(val, 10) : defaultVal
        }
        const fetchStr = async (key: string, defaultVal: string) => {
          const val = await invoke<string | null>('get_app_config', { key })
          return val ? val : defaultVal
        }
        const fetchBool = async (key: string, defaultVal: boolean) => {
          const val = await invoke<string | null>('get_app_config', { key })
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
        this.aiImageModel = await fetchStr('ai_image_model', 'cn-clip-vit-b16-image.onnx')
        this.aiTextModel = await fetchStr('ai_text_model', 'cn-clip-vit-b16-text.onnx')
        this.aiProviderOverride = await fetchStr('ai_provider_override', 'auto')
        this.aiBatchSize = await fetchInt('ai_batch_size', 0)

        // 应用一些 CSS 变量和 UI 状态
        const ui = useUiStore()
        if (this.thumbStrategy) ui.setThumbStrategy(this.thumbStrategy)
        if (this.gpuEngine) ui.setGpuEngine(this.gpuEngine)

        document.documentElement.style.setProperty('--scrollbar-width', `${this.timelineScrollWidth}px`)

        const diff = this.uiFontSize - 16;
        document.documentElement.style.setProperty('--font-size-xs', `${12 + diff}px`);
        document.documentElement.style.setProperty('--font-size-sm', `${13 + diff}px`);
        document.documentElement.style.setProperty('--font-size-base', `${16 + diff}px`);
        document.documentElement.style.setProperty('--font-size-md', `${17 + diff}px`);
        document.documentElement.style.setProperty('--font-size-lg', `${20 + diff}px`);
        document.documentElement.style.setProperty('--font-size-xl', `${24 + diff}px`);
        document.documentElement.style.setProperty('--font-size-2xl', `${30 + diff}px`);

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
      await invoke('set_app_config', { key, value })
    },

    async setThumbSkipMaxKb(val: number) {
      this.thumbSkipMaxKb = val;
      await this.saveConfig('thumb_skip_max_kb', val.toString())
    },
    async setThumbCacheMaxMb(val: number) {
      this.thumbCacheMaxMb = val;
      await this.saveConfig('thumb_cache_max_mb', val.toString())
    },
    async setThumbSize(val: number) {
      console.log(val);

      this.thumbSize = val;
      await this.saveConfig('thumb_size', val.toString())
    },
    async setTimelineScrollWidth(val: number) {
      this.timelineScrollWidth = val;
      await this.saveConfig('timeline_scroll_width', val.toString())
      document.documentElement.style.setProperty('--scrollbar-width', `${val}px`)
    },
    async setUiFontSize(val: number) {
      this.uiFontSize = val;
      await this.saveConfig('ui_font_size', val.toString())
      const diff = val - 16;
      document.documentElement.style.setProperty('--font-size-xs', `${12 + diff}px`);
      document.documentElement.style.setProperty('--font-size-sm', `${13 + diff}px`);
      document.documentElement.style.setProperty('--font-size-base', `${16 + diff}px`);
      document.documentElement.style.setProperty('--font-size-md', `${17 + diff}px`);
      document.documentElement.style.setProperty('--font-size-lg', `${20 + diff}px`);
      document.documentElement.style.setProperty('--font-size-xl', `${24 + diff}px`);
      document.documentElement.style.setProperty('--font-size-2xl', `${30 + diff}px`);
    },
    async setEnableHoverScale(val: boolean) {
      this.enableHoverScale = val;
      await this.saveConfig('enable_thumb_hover_scale', val.toString())
      if (val) {
        document.documentElement.classList.remove('disable-hover-scale')
      } else {
        document.documentElement.classList.add('disable-hover-scale')
      }
    },
    async setLogLevel(val: string) {
      this.logLevel = val;
      await this.saveConfig('log_level', val)
    },
    async setThumbStrategy(val: string) {
      this.thumbStrategy = val;
      await this.saveConfig('thumb_strategy', val)
      const ui = useUiStore()
      ui.setThumbStrategy(val)
    },
    async setGpuEngine(val: string) {
      this.gpuEngine = val;
      await this.saveConfig('gpu_engine', val)
      const ui = useUiStore()
      ui.setGpuEngine(val)
    },
    async setAiImageModel(val: string) {
      this.aiImageModel = val;
      await this.saveConfig('ai_image_model', val)
    },
    async setAiTextModel(val: string) {
      this.aiTextModel = val;
      await this.saveConfig('ai_text_model', val)
    },
    async setAiProviderOverride(val: string) {
      this.aiProviderOverride = val;
      await this.saveConfig('ai_provider_override', val)
    },
    async setAiBatchSize(val: number) {
      this.aiBatchSize = val;
      await this.saveConfig('ai_batch_size', val.toString())
    }
  }
})
