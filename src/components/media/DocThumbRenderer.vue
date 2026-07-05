<template>
  <!-- Hidden offscreen renderer (§3.4 Lite 路径). 无可见 DOM —— canvas 程序化创建后即弃。 -->
  <div class="doc-thumb-renderer" aria-hidden="true"></div>
</template>

<script setup lang="ts">
// DocThumbRenderer —— 文档缩略图的前端离屏渲染器（需求4, §3.4 Lite 路径）。
//
// 设计要点：
//  - **轻量取舍**：Lite 变体无 native 栅格化器 → pdf/svg 由前端 Webview 离屏渲染截图，
//    经 store_doc_thumbnail 回传后端落盘（epub 封面由后端 zip 处理，不经此组件）。
//  - **单线程节流**：同一时刻只渲染一个文档；每个之间让出主线程，避免卡主窗口。
//  - **仅窗口可见时跑**：visibilitychange 暂停/恢复（风险表：占用主窗口/单线程 → 严格节流）。
//  - **pdf.js 懒加载**：仅当真的遇到 PDF 时才动态 import，避免拖慢启动 / 膨胀首屏包。
//  - **失败即标记**：渲染失败回传空字节，后端标 status=3，避免无限重试坏文件。

import { onMounted, onBeforeUnmount } from 'vue'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { IPC, EVENTS } from '../../constants/ipc'
import { getPdfjs } from '../../utils/pdfjs'

interface PendingDocThumb {
  itemId: number
  absPath: string
  fileFormat: string
}

// 渲染目标长边（px）。后端会再按缩略图档位缩放，这里只需给一个清晰的中间分辨率。
const TARGET = 512
// 每轮领取的批量（与后端默认一致即可）。
const BATCH = 4

function canvasToPng(canvas: HTMLCanvasElement): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('toBlob returned null'))), 'image/png')
  })
}

// 渲染 PDF 首页 → PNG + 总页数（T10:文档已打开,顺带取 numPages 回填 document_meta,
// 免为页数二次解析文档）。PDF 透明 → 先铺白底，避免黑/透明缩略图。
async function renderPdf(url: string): Promise<{ blob: Blob; pages: number | null }> {
  const lib = await getPdfjs()
  const doc = await lib.getDocument({ url }).promise
  try {
    const pages = doc.numPages ?? null
    const page = await doc.getPage(1)
    const base = page.getViewport({ scale: 1 })
    const scale = TARGET / Math.max(base.width, base.height)
    const viewport = page.getViewport({ scale })
    const canvas = document.createElement('canvas')
    canvas.width = Math.max(1, Math.round(viewport.width))
    canvas.height = Math.max(1, Math.round(viewport.height))
    const ctx = canvas.getContext('2d')!
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    await page.render({ canvasContext: ctx, viewport }).promise
    return { blob: await canvasToPng(canvas), pages }
  } finally {
    doc.destroy().catch(() => {})
  }
}

// 渲染 SVG → PNG（Webview 原生解析 <img>，再绘到离屏 canvas）。SVG 无页概念 → pages: null。
function renderSvg(url: string): Promise<{ blob: Blob; pages: number | null }> {
  return new Promise((resolve, reject) => {
    const img = new Image()
    img.onload = () => {
      // 部分 SVG 无 intrinsic 尺寸 → 退化为方形目标尺寸。
      const nw = img.naturalWidth || TARGET
      const nh = img.naturalHeight || TARGET
      const scale = TARGET / Math.max(nw, nh)
      const w = Math.max(1, Math.round(nw * scale))
      const h = Math.max(1, Math.round(nh * scale))
      const canvas = document.createElement('canvas')
      canvas.width = w
      canvas.height = h
      const ctx = canvas.getContext('2d')!
      ctx.fillStyle = '#fff'
      ctx.fillRect(0, 0, w, h)
      try {
        ctx.drawImage(img, 0, 0, w, h)
        resolve(canvasToPng(canvas).then((blob) => ({ blob, pages: null })))
      } catch (e) {
        reject(e) // 跨源污染等 → toBlob 会抛，标记失败
      }
    }
    img.onerror = () => reject(new Error('svg image load failed'))
    img.src = url
  })
}

async function renderOne(doc: PendingDocThumb): Promise<{ blob: Blob; pages: number | null }> {
  const url = convertFileSrc(doc.absPath)
  return doc.fileFormat === 'pdf' ? renderPdf(url) : renderSvg(url)
}

// ── 主泵：可见时循环领取并处理，直到无待处理 ───────────────────────────────────
let running = false
async function pump() {
  if (running || document.visibilityState !== 'visible') return
  running = true
  try {
    // 自行播种队列：pdf/svg 为前端驱动，需在用户未启动后端派生流水线时也能工作（幂等、廉价）。
    await invoke(IPC.ENSURE_DOC_THUMB_QUEUE).catch(() => {})
    while (document.visibilityState === 'visible') {
      const pending = await invoke<PendingDocThumb[]>(IPC.LIST_PENDING_DOC_THUMBS, { limit: BATCH })
      if (!pending.length) break
      for (const doc of pending) {
        if (document.visibilityState !== 'visible') break
        try {
          const { blob, pages } = await renderOne(doc)
          const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()))
          await invoke(IPC.STORE_DOC_THUMBNAIL, {
            itemId: doc.itemId,
            pngBytes: bytes,
            pageCount: pages,
          })
        } catch {
          // 渲染失败 → 回传空字节，后端标错，停止无限重试。
          await invoke(IPC.STORE_DOC_THUMBNAIL, { itemId: doc.itemId, pngBytes: [] }).catch(
            () => {},
          )
        }
        // 让出主线程一帧，保持窗口响应。
        await new Promise((r) => setTimeout(r, 30))
      }
    }
  } finally {
    running = false
  }
}

// 事件去抖：扫描/封面落地会高频触发 db:media_enriched，避免每条都重入。
let debounceTimer: ReturnType<typeof setTimeout> | null = null
function pumpDebounced() {
  if (debounceTimer) clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => pump(), 800)
}

let unlisten: UnlistenFn | null = null
function onVisible() {
  if (document.visibilityState === 'visible') pump()
}

onMounted(async () => {
  unlisten = await listen(EVENTS.MEDIA_ENRICHED, pumpDebounced)
  document.addEventListener('visibilitychange', onVisible)
  pump()
})

onBeforeUnmount(() => {
  if (unlisten) unlisten()
  if (debounceTimer) clearTimeout(debounceTimer)
  document.removeEventListener('visibilitychange', onVisible)
})
</script>

<style scoped>
.doc-thumb-renderer {
  display: none;
}
</style>
