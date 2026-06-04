// src/router/index.ts
import { createRouter, createWebHashHistory } from 'vue-router'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: '/',
      component: () => import('../components/media/MediaGrid.vue'),
      meta: { title: '全部媒体' },
    },
    {
      path: '/folder/:id',
      component: () => import('../components/media/MediaGrid.vue'),
      meta: { title: '文件夹' },
    },
    {
      path: '/favorites',
      component: () => import('../components/media/MediaGrid.vue'),
      meta: { title: '收藏' },
    },
    {
      path: '/trash',
      component: () => import('../components/media/MediaGrid.vue'),
      meta: { title: '回收站' },
    },
  ],
})

router.afterEach((to) => {
  document.title = `${to.meta.title as string} — Picasa Next`
})

export default router
