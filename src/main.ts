// src/main.ts
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import './assets/styles/index.css'

const app = createApp(App)

app.use(createPinia())
app.use(router)

// Set initial theme before mount to prevent FOUC
document.documentElement.setAttribute('data-theme', 'dark')

app.mount('#app')
