import { createI18n } from 'vue-i18n'
import zhCN from './locales/zh-CN'
import enUS from './locales/en-US'

// 默认语言: 尝试获取系统语言，如果是以 zh 开头则使用 zh-CN，否则默认 en-US
const systemLang = navigator.language || 'en-US'
const defaultLocale = systemLang.startsWith('zh') ? 'zh-CN' : 'en-US'

const i18n = createI18n({
  legacy: false, // 必须为 false 以支持 Composition API
  locale: defaultLocale,
  fallbackLocale: 'en-US',
  messages: {
    'zh-CN': zhCN,
    'en-US': enUS,
  },
})

export default i18n
