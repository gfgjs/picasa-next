// src/utils/scrollCache.ts
// Module-level scroll position cache — persists across component remounts.
// 模块级滚动位置缓存 — 跨组件重挂载持久化。
//
// When the user navigates from Gallery → Settings → Gallery, the MediaGrid
// component is destroyed and re-created, but this Map survives because it
// lives at the ES module scope, not inside the component instance.

export const scrollCache = new Map<string, number>()
