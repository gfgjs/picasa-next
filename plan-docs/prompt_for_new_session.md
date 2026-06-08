# 角色与背景
你是 Picasa Next 项目的架构师+全栈开发者。这是一个基于 Tauri 2 + Vue 3 (Composition API + TS strict) + Rust + SQLite 的桌面图片管理应用。
请你帮我实现两个与 UI 交互和数据显示相关的功能。

# 需求说明
**功能一：设置项常驻侧边栏工具菜单**
- 允许用户将特定的简单设置项（如主题切换、清理数据库按钮等简单控件）固定到主页左侧的“工具”菜单下。
- 预期表现：在 `SettingsView.vue` 的对应设置项旁边增加一个“图钉”图标或复选框。勾选后，该设置的控件会直接渲染在侧边栏的“工具”区域。
- 状态需要被持久化保存（可以通过 `uiStore` 和 IPC 读写配置）。

**功能二：画廊缩略图定制悬浮信息**
- 允许用户勾选在画廊缩略图上显示哪些信息：原图/缩略图状态、大小、分辨率、日期、文件名、路径、以及**高级元数据（地理信息经纬度、相机型号、镜头型号、光圈快门ISO等拍摄参数）**。
- 增加一个“一键开关”用于全局控制是否显示这些覆盖层信息。
- **性能优化与警告要求：**
  由于获取高级元数据需要在 Rust 后端构建布局时对 `image_meta` 表进行 `LEFT JOIN` 查询，这在数万张图片时会有性能开销。因此：
  1. **按需统一查询**：由于经纬度、相机型号、拍摄参数等均存在于 `image_meta` 表中，只要用户勾选了**任何一项**高级元数据，前端在触发 `compute_layout` 时就传入一个统一的 `includeMeta: true` 标识，后端才去 `LEFT JOIN image_meta` 并一次性查出所有所需的额外字段。否则直接返回 `NULL`。
  2. **开销警告**：用户在设置页首次勾选任何高级元数据选项（地理信息、相机型号、拍摄参数等）时，前端应弹出一个警告（原生 `confirm` 或自定义弹窗均可），提示：“开启高级元数据（如地理信息、相机参数等）将增加布局计算时的性能开销，导致相册加载变慢。确定要开启吗？”。

---

# 详尽实现计划（请严格按此计划修改文件）

## 1. 后端 (Rust) 改造

**文件：`src-tauri/src/db/models.rs`**
- 修改 `LayoutItem` 结构体，增加如下字段：
  ```rust
  pub original_width: i64,
  pub original_height: i64,
  pub file_name: String,
  pub dir_path: Option<String>,
  pub sort_datetime: i64,
  pub gps_lat: Option<f64>,
  pub gps_lng: Option<f64>,
  pub exif_make: Option<String>,
  pub exif_model: Option<String>,
  pub exif_lens: Option<String>,
  pub exif_focal_length: Option<f64>,
  pub exif_aperture: Option<f64>,
  pub exif_shutter: Option<String>,
  pub exif_iso: Option<i64>,
  ```
- 修改 `LayoutRowItem` 结构体，增加对应的前端字段，并**补齐** `pub is_favorited: bool`。

**文件：`src-tauri/src/db/queries.rs`**
- 在 `query_layout_items` 的参数中，增加一个标识 `include_meta: bool`（可通过 `MediaFilter` 或函数传参传入）。
- 动态构建 `SELECT` 字符串：
  ```rust
  let mut sql = String::from("SELECT m.id, m.width, m.height, m.file_size, m.sort_datetime, m.file_format, m.media_type, m.is_live_photo, m.duration_ms, m.thumb_status, m.thumb_path, m.thumbhash, m.is_favorited, d.rel_path as dir_path, d.name as dir_name, m.file_name, m.directory_id as dir_id, ");
  // AI 字段逻辑不变...
  
  // 动态高级元数据
  if include_meta {
      sql.push_str("im.exif_gps_lat, im.exif_gps_lng, im.exif_make, im.exif_model, im.exif_lens, im.exif_focal_length, im.exif_aperture, im.exif_shutter, im.exif_iso ");
  } else {
      sql.push_str("NULL as exif_gps_lat, NULL as exif_gps_lng, NULL as exif_make, NULL as exif_model, NULL as exif_lens, NULL as exif_focal_length, NULL as exif_aperture, NULL as exif_shutter, NULL as exif_iso ");
  }
  ```
- 更新 `needs_meta_join` 的判断逻辑：如果 `include_meta` 为 `true`，则 `needs_meta_join = true`，触发 `LEFT JOIN image_meta im`。
- 更新 `map_layout_item` 函数，将增加的列全部读取并映射到 `LayoutItem`。

**文件：`src-tauri/src/layout/justified.rs`**
- 更新 `compute_justified_layout` 接口，在构建 `LayoutRowItem` 时，从传入的 `LayoutItem` 中提取上述新添加的所有字段并赋值。
- 请同时检查调用 `compute_justified_layout` 的入口处（如 `src-tauri/src/commands/layout.rs`），确保正确接收前端的 `includeMeta` 参数。

## 2. 前端 (Vue 3 + TS) 改造

**文件：`src/types/layout.ts`**
- 对应 Rust 修改 `LayoutRowItem` 接口，包含基本信息、高级元数据（可选类型）以及 `isFavorited: boolean`。

**文件：`src/stores/uiStore.ts`**
- 增加 `pinnedSettings: string[]`（存储如 `'theme'`, `'clearDb'`）。
- 增加 `showThumbInfo: boolean`（全局信息开关）。
- 增加 `thumbInfoElements: string[]`（可选项增加如 `'geo'`, `'camera'`, `'params'`）。
- 同步在 `loadConfig` / `saveConfig` 逻辑中持久化这些状态。

**文件：`src/stores/mediaStore.ts`**
- 在 `computeLayout` 函数中，判断 `uiStore().thumbInfoElements` 中是否包含了任何需要高级元数据的选项（如 `'geo'`, `'camera'`, `'params'`），如果是，则作为 `includeMeta: true` 传给 Tauri。

**文件：`src/views/SettingsView.vue`**
- **功能一**：在相应的设置卡片头部或旁边，添加“固定到侧边栏”复选框（绑定 `togglePinnedSetting(key)`）。
- **功能二**：新增“缩略图信息悬浮窗”设置项：
  - 一个主开关控制 `showThumbInfo`。
  - 一组复选框控制 `thumbInfoElements`。
  - **性能警告实现**：当用户试图勾选任何涉及高级元数据的选项时，进行 `window.confirm` 拦截。只有确认才添加到数组并触发 `mediaStore.invalidateLayout()`。

**文件：`src/components/sidebar/AppSidebar.vue`**
- 在左侧 Tools 列表中，编写 `<template v-if="uiStore.pinnedSettings.includes('xxx')">` 块，直接将设置项对应的控件（如主题选择器等）在此处复用渲染。

**文件：`src/components/media/MediaGrid.vue` & `MediaThumb.vue`**
- `MediaGrid`：将新返回的各项元数据作为 Prop 传给 `MediaThumb`，并移除旧版手动打补丁 `:is-favorited` 的逻辑。
- `MediaThumb`：在 `.media-thumb__overlays` 区域通过 `v-if="uiStore.showThumbInfo"` 控制，内部针对各项 `thumbInfoElements` 渲染对应的角标（如相机图标+型号，光圈快门文本等）。
- 注意使用优雅的 CSS 布局（flex-wrap/绝对定位），防止多信息重叠。对于日期，使用类似 `new Date(sortDatetime * 1000).toLocaleDateString()` 处理。

# 执行指示
请立刻开始执行上述计划。你可以先调用工具检查相关文件目前的结构，然后通过文件编辑工具逐一实施更改。所有代码注释请使用中文。
