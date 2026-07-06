# Scrollery

High-performance, cross-platform media asset manager aimed at **smooth gallery browsing of 100k–1M photos**.
面向**十万~百万张照片流畅画廊式浏览**的高性能跨平台媒体资源管理器。

> Status: active development. Image browsing + AI semantic search are implemented.
> 状态：活跃开发中。已实现图片浏览与 AI 语义搜索。

## Tech stack | 技术栈

- **Backend | 后端**: Rust + [Tauri v2](https://tauri.app) — SQLite (`rusqlite` + WAL, write `Mutex` + read `r2d2` pool), `rayon` (CPU parallelism), `fast_image_resize`, WIC GPU decode (Windows), `kamadak-exif`. Chinese-CLIP semantic search runs its ONNX Runtime (`ort`, DirectML) inference in a separate `ai-worker` subprocess, keeping the host binary free of `ort`.
- **Frontend | 前端**: Vue 3 (`<script setup>`) + Pinia + Vue Router + Vite + TypeScript, vanilla CSS variables.

## Performance architecture | 性能架构

- **Two-phase scan | 两阶段扫描**: fast scan (header-only dimensions) shows the grid in seconds; background enrichment (EXIF/XMP/Live Photo) runs silently.
- **Backend Justified Layout | 后端两端对齐布局**: the layout is computed in Rust and cached in memory; the frontend pulls only the visible rows (row-level virtualization).
- **Resident layout cache** holds only render-essential fields per item; heavy metadata (EXIF/GPS/path/filename) is fetched on demand for the visible viewport (`get_meta_for_viewport`).
- **O(1) layout index**: thumbnail write-back and adjacent-item navigation use an `id → (row, col)` index (no full-table scans).
- **Bucket virtualization** (large libraries): the grid renders only a few fixed-size segments driven by a wishlist single-flight fetch pump; a custom logical scrollbar maps logical↔physical scroll and caps the physical spacer under the browser's ~16.7M px element-height limit.
- **AI semantic search**: CLIP embeddings kept resident in a half-precision (f16) cache; cosine similarity computed with `rayon`.

See [`plan-docs/`](plan-docs/) for the full design — the `refactor_2026/` series (Part0–Part8) is the current architecture of record, and `todo.md` tracks live status.

## Develop | 开发

Prerequisites: Node.js + Rust toolchain + [Tauri v2 prerequisites](https://tauri.app/start/prerequisites/) (on Windows: WebView2 runtime, MSVC).

```bash
npm install
npm run tauri dev      # run the desktop app in dev mode | 开发模式运行桌面应用
```

```bash
npm run build          # type-check (vue-tsc) + build the frontend
npm run tauri build    # produce a release bundle | 生成发布包
```

Backend-only checks | 仅后端检查:

```bash
cargo check  --manifest-path src-tauri/Cargo.toml --tests
cargo test   --manifest-path src-tauri/Cargo.toml --lib layout::cache::tests
```

## Source layout | 源码结构

```
src-tauri/src/
  db/          connection pool, schema, migrations, queries, models
  scanner/     two-phase scan: fast_scan, enricher, walker, metadata, live_photo
  layout/      justified layout algorithm + in-memory cache (+ O(1) index)
  thumbnail/   generation pipeline, sized cache, thumbhash, EXIF fast path
  engine/      image decode engines (image-rs + WIC GPU)
  ai/          CLIP engine pool, embedding pipeline, resident search cache
  ipc/         Tauri command handlers (scan/layout/media/thumbnail/search/ai/...)
src/
  components/  layout shell, media grid/thumb/detail, sidebar, settings
  composables/ virtual scroll, justified layout consumer, request queue, ...
  stores/      Pinia stores (media/scan/ui/filter/config/ai)
```

## License | 许可

Copyright 2026 The Scrollery Authors.

The open-source core (this repository) is licensed under the [Apache License 2.0](LICENSE); third-party attributions are listed in [NOTICE.md](NOTICE.md).
开源核心(本仓库)以 [Apache-2.0](LICENSE) 授权;第三方组件署名见 [NOTICE.md](NOTICE.md)。

The "Scrollery" name and logo are trademarks and are **not** licensed under Apache-2.0 — see [TRADEMARK.md](TRADEMARK.md). Contributions require the [CLA](CLA.md); see [CONTRIBUTING.md](CONTRIBUTING.md).
「Scrollery」名称与标识属商标,**不在** Apache-2.0 授权范围内(见 [TRADEMARK.md](TRADEMARK.md));贡献代码须签署 [CLA](CLA.md),流程见 [CONTRIBUTING.md](CONTRIBUTING.md)。
