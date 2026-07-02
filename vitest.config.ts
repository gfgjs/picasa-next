import { defineConfig } from 'vitest/config'

// Part5 T4a 回归基线:选区策略层为纯函数,无需 DOM → 默认 node 环境即可。
// 将来若要测组件(useViewIds 的 IPC mock / SFC),再按需引入 jsdom + @vue/test-utils。
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.spec.ts'],
  },
})
