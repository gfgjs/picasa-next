// src/main.ts
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import i18n from './i18n'
import './assets/styles/index.css'

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.use(i18n)

// Detect system theme before mount to prevent FOUC on fresh install.
// Returning users: saved preference is applied in useTheme.ts onMounted (overrides this).
// 全新安装时跟随系统主题防止闪烁；老用户会在 useTheme.ts 的 onMounted 中覆盖为已保存的偏好。
const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
document.documentElement.setAttribute('data-theme', prefersDark ? 'dark' : 'light')

app.mount('#app')
