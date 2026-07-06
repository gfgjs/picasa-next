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

// 兜底:防 FOUC 的首帧着色已由 index.html 内联脚本负责(读 localStorage 快照);
// 此处仅在内联脚本未生效的异常情况下按系统偏好补一份,避免无主题裸奔。
if (!document.documentElement.hasAttribute('data-theme')) {
  const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
  document.documentElement.setAttribute('data-theme', prefersDark ? 'dark' : 'light')
  document.documentElement.setAttribute('data-color-scheme', prefersDark ? 'dark' : 'light')
}

app.mount('#app')
