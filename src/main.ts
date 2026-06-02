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

// Set initial theme before mount to prevent FOUC
// 在挂载前设置初始主题以防止 FOUC（无样式内容闪烁）
document.documentElement.setAttribute('data-theme', 'dark')

app.mount('#app')
