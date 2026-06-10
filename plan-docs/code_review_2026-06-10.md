# picasa-next 代码审查报告

> **审查日期**: 2026-06-10  
> **审查范围**: Rust 后端 (`src-tauri/src/`) + TypeScript 前端 (`src/`)  
> **审查方式**: 静态代码审查（读取核心源文件，未运行测试或编译）

---

## 一、后端 (Rust) — 高优先级

### 1. SessionPool 在耗尽时 panic 而非返回错误
- **文件**: `src-tauri/src/ai/engine.rs:97`
- **问题**: `self.rx.recv().expect("Session pool channel disconnected")` — 当所有 Session 都在使用中且 channel 关闭时，会触发 panic 导致进程崩溃。
- **建议**: 返回 `Option` 或 `Result`，或使用 `recv_timeout` + 错误处理。

### 2. SessionGuard 的 Deref/DerefMut 使用裸 unwrap
- **文件**: `src-tauri/src/ai/engine.rs:122, 128`
- **问题**: `self.session.as_ref().unwrap()` — 如果 Drop 已执行后仍访问 guard（虽然不太可能），会 panic。更安全的做法是使用 `expect` 给出有意义的错误信息。

### 3. load_session_pool 加载失败时丢弃已成功加载的 Session
- **文件**: `src-tauri/src/ai/engine.rs:306-308`
- **问题**: 当某个 session 加载失败时，函数直接 `return None`，丢弃之前已成功加载的 session。这意味着即使只差一个 session，整个 pool 都不可用。
- **建议**: 返回已成功加载的 sessions，并通过日志警告降级。将 `return None` 改为 `break` 或 `continue`。

### 4. GPU 回退逻辑缺陷
- **文件**: `src-tauri/src/ai/engine.rs:216-226`
- **问题**: 文本编码器回退时，条件 `provider_info.provider != AiProvider::Cpu` 在图像编码器已先回退的场景下可能仍为 true（因为在第二个 if 块里又设了一遍），但实际上第二次回退会把 `clip_image_session` 重新覆盖掉。两者回退逻辑各自独立，但第二次可能错误地覆盖第一次的结果。
- **建议**: 合并两个回退分支，统一处理回退逻辑。

### 5. 数据库查询中 SQL 拼接非参数化
- **文件**: `src-tauri/src/ipc/thumbnail_commands.rs:48-51`
- **问题**: 
  ```rust
  let in_clause = item_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
  let sql = format!("SELECT ... WHERE id IN ({})", in_clause);
  ```
  虽然 `item_ids` 是 `Vec<i64>` 风险极低，但不符合项目自身宣称的"绝不使用字符串拼接"原则。当批量 ID 数量很大时，这也无法利用 SQLite 的预编译语句缓存。
- **建议**: 使用 `rusqlite` 的动态参数绑定（如 `rusqlite::params_from_iter` 或构建动态 `IN` 子句的参数化方案）。

### 6. 错误类型丢失原始错误链
- **文件**: `src-tauri/src/error.rs`
- **问题**: 所有 `From` 实现都使用 `e.to_string()` 转换，丢弃了原始错误的链式信息和回溯。例如 `impl From<rusqlite::Error> for AppError` 只保存了错误消息字符串，无法通过 `source()` 追溯根因。
- **建议**: 考虑在 `AppError` 变体中保留原始错误类型（如 `Db(rusqlite::Error)`）或使用 `#[source]` 属性。

### 7. AI Pipeline 完成时卸载引擎存在竞态风险
- **文件**: `src-tauri/src/ai/pipeline.rs:103-106`
- **问题**: 分析完成后的回调中直接 `*engine = None` 卸载引擎。如果用户恰好在此刻触发语义搜索（在另一个线程调用 `ensure_engine_initialised`），会看到引擎突然消失。虽然有 `RwLock` 保护，但逻辑上可能导致用户操作失败。
- **建议**: 使用引用计数或延迟卸载策略（如空闲超时后卸载）。

### 8. encode_image_batch 切片操作无边界检查
- **文件**: `src-tauri/src/ai/clip.rs:254-258`
- **问题**: 
  ```rust
  let start = i * EMBED_DIM;
  let end = start + EMBED_DIM;
  let embedding: Vec<f32> = raw_slice[start..end].to_vec();
  ```
  如果输出张量尺寸与预期不符（模型异常），会导致索引越界 panic。这在生产环境中不应发生。
- **建议**: 添加 `debug_assert!(end <= raw_slice.len())` 或使用 `.get(start..end)` + 错误处理。

### 9. SQLite `strftime('%s','now')` 跨平台兼容性
- **文件**: `src-tauri/src/db/schema.rs`（多处）
- **问题**: SQLite 的 `strftime('%s', 'now')` 在 Windows 上的某些构建版本不支持。而项目明确以 Windows 为目标平台。
- **建议**: 在 Rust 端获取 Unix 时间戳并通过参数传入 SQL。

### 10. 后台 tokio::spawn 任务无生命周期管理
- **文件**: `src-tauri/src/lib.rs:272,290` 和 `ai_commands.rs` 等处
- **问题**: 多处使用 `tokio::spawn` 启动后台任务（PRAGMA optimize、缓存清理、AI pipeline 等），但没有对这些任务进行追踪。应用退出时，这些任务可能尚未完成，tokio runtime 的 shutdown 会丢弃它们。
- **建议**: 使用 `JoinHandle` 集合追踪并在 `on_exit` 中等待关键任务完成。

---

## 二、后端 (Rust) — 中优先级

### 11. preprocess_decoded 存在越界访问风险
- **文件**: `src-tauri/src/ai/clip.rs:345`
- **问题**: `let idx = (src_y * w + src_x) * 4;` — 如果 `cx + x` 或 `cy + y` 超出范围（虽然通常不会，因为调用了 `saturating_sub`），将导致越界访问。
- **建议**: 使用 `decoded.pixels.get(idx..idx+3)` 或预先验证边界。

### 12. `binary_search_by` 使用 `partial_cmp(...).unwrap()` 
- **文件**: `src-tauri/src/layout/cache.rs:199`
- **问题**: `data.rows.binary_search_by(|r| r.y().partial_cmp(&top_y).unwrap())` — 如果 `top_y` 是 NaN（浮点异常），`partial_cmp` 返回 `None`，会 panic。
- **建议**: 用 `unwrap_or(Ordering::Equal)` 或先验证 `top_y.is_finite()`。

### 13. AI pipeline 生产者持有写锁时间过长
- **文件**: `src-tauri/src/ai/pipeline.rs:219-224`
- **问题**: 在生产循环中每次获取写锁来更新 `ai_status`。虽然时间短，但如果 write lock 被 writer 线程阻塞，会影响其他操作的读写性能。
- **建议**: 将状态更新合并到 writer 线程；producer 只负责查询。

### 14. 缩略图通道结果可能丢失
- **文件**: `src-tauri/src/ipc/thumbnail_commands.rs:74`
- **问题**: `let _ = on_result.send(r.clone());` — send 失败被静默丢弃。如果前端已经取消订阅（如视图切换），结果被丢弃是正确的，但应至少记录一条 trace 日志。
- **建议**: 添加 `debug!` 级别的日志。

---

## 三、前端 (TypeScript/Vue) — 高优先级

### 15. 虚拟滚动坐标平移模式存在已知 bug
- **文件**: `src/composables/useVirtualScroll.ts:44-46`
- **问题**: 代码注释明确写着 "translated mode has a known scroll-misalignment bug (shelved)"。在大于约 25 万项的库中（逻辑高度超过 `SAFE_MAX = 2,000,000`），会进入平移模式，滚动行为已知有错误。
- **建议**: 修复该 bug 或降低 `SAFE_MAX` 进入平移模式的阈值，使更多库保持在正常模式。

### 16. useRequestQueue 请求去重逻辑可能导致 Promise 永远不 resolve
- **文件**: `src/composables/useRequestQueue.ts:91-96`
- **问题**: 
  ```typescript
  if (inFlight.has(id)) {
    return // Already being processed
  }
  ```
  如果 `id` 在 `inFlight` 中（后端正在处理），但对应 batch 的 `onResult` 已返回且 resolver 已被消费，新的 `resolver` 被添加到 Map 中但永远不会被调用，导致 Promise 永久挂起。
- **建议**: 当 `inFlight.has(id)` 但 `resolvers` 中没有对应 entry 时，需要重新入队。

### 17. `ensureMeta` 防抖使用 setTimeout 存在竞争条件
- **文件**: `src/stores/mediaStore.ts:51-61`
- **问题**: 快速切换视图时，`pendingMetaIds` 可能被清空但之前的 `setTimeout` 仍然触发，导致无谓的网络请求或空请求。
- **建议**: 在 `flushMeta` 中先检查 `pendingMetaIds.size === 0` 已做了防护，但在 `clear` 时应同时 `clearTimeout(metaTimer)`。

### 18. 前端错误处理不统一
- **文件**: 多个 store 和 composable 文件
- **问题**: 大量 `catch (e) { console.error(...) }` 模式，没有统一的错误处理、用户反馈或重试机制。用户可能看不到错误信息。
- **建议**: 建立统一的错误处理层，通过 `useUiStore().addToast('error', ...)` 反馈给用户。

### 19. vue-tsc 构建依赖可能不可靠
- **文件**: `package.json:8`
- **问题**: `"build": "vue-tsc --noEmit && vite build"` — `vue-tsc` 在某些 TypeScript 版本组合下可能出现误报、性能问题或与 Vite 的兼容性问题。单独的类型检查不应阻塞构建。
- **建议**: 将 `vue-tsc --noEmit` 移到独立的 `typecheck` 脚本，`build` 只运行 `vite build`。

---

## 四、前端 (TypeScript/Vue) — 中优先级

### 20. `addToast` 定时器在组件卸载后可能继续运行
- **文件**: `src/stores/uiStore.ts:165`
- **问题**: `setTimeout(() => removeToast(id), duration + 300)` — 如果 toast 已被外部移除或 store 被销毁，定时器仍会触发。虽然是无害操作，但在长时间运行的 SPA 中可能累积。
- **建议**: 使用 `onBeforeUnmount` 或 AbortController 模式管理定时器生命周期。

### 21. `scrollToY` 在平移模式下可能不精确
- **文件**: `src/components/media/MediaGrid.vue:121`（时间轴点击跳转）
- **问题**: 时间轴点击使用 `scrollToY(sep.y)`，但在坐标平移模式下（`isTranslated`），`sep.y` 是逻辑坐标，需要先通过 `logicalToPhysical` 转换。
- **建议**: 检查 `isTranslated` 状态并应用坐标转换。

---

## 五、架构与工程实践

### 22. 测试覆盖严重不足
- **发现**: 
  - `package.json` 中**完全没有**前端测试脚本（无 vitest、无 playwright）。
  - `Cargo.toml` 中**没有配置** `[dev-dependencies]` 测试框架。
  - 整个项目中只有 `src-tauri/src/utils/format.rs` 有单元测试，以及 `layout::cache` 的测试。
  - README 中只提到 `cargo test --lib layout::cache::tests` 一条测试命令。
- **建议**: 至少为核心模块（布局算法、数据库查询、AI 推理预处理）添加单元测试。

### 23. 前端缺少组件级别的错误边界
- **问题**: Vue 3 中没有使用 `<Suspense>` 或 `onErrorCaptured` 等错误边界机制。单个组件中的 JavaScript 错误可能导致整个应用白屏。
- **建议**: 在 `AppShell.vue` 中添加全局错误处理钩子。

### 24. 硬编码值分散各处
- **发现**:
  - `SAFE_MAX = 2_000_000`（useVirtualScroll.ts）
  - `CHANNEL_CAPACITY = 1024`（pipeline.rs）
  - `BATCH_SIZE = 512`（pipeline.rs, fast_scan.rs）
  - `SESSION_LOAD_TIMEOUT = 600s`（engine.rs）
  - `IMG_SIZE = 224`（clip.rs）
- **问题**: 部分值是合理的常量，但部分（如 batch size、channel capacity）应该可配置或至少集中管理。
- **建议**: 将可调参数移到配置文件或 `app_config` 表中。

### 25. 缺少 CI/CD 配置
- **发现**: 仓库中有 `.github/workflows/dependency-review.yml` 但仅做依赖审查，没有 build/test/lint 的 CI 流水线。
- **建议**: 添加 CI 流水线：`cargo check`、`cargo clippy`、`cargo test`、`npm run build`。

### 26. 项目根目录存在临时文件
- **发现**: 根目录有 `temp_header.onnx`、`temp_text.onnx`、`codewhale-tui-windows-x64.exe`、`whale-windows-amd64.exe` 等二进制/临时文件。
- **建议**: 添加到 `.gitignore` 或清理。

### 27. .gitignore 缺少临时 ONNX 文件
- **文件**: `.gitignore`（项目根目录）
- **问题**: `temp_header.onnx`（1MB）和 `temp_text.onnx`（1MB）是模型转换的中间产物，未加入 `.gitignore`。虽然 `.exe` 文件已列入，但这些 `.onnx` 临时文件仍然可能被误提交。
- **建议**: 添加 `temp_*.onnx` 到 `.gitignore`，或将临时文件统一移到 `scratch/` 目录。

### 28. mediaStore 中 `openDetailFromLayout` 与 `openDetail` 功能重叠
- **文件**: `src/stores/mediaStore.ts:177-190`
- **问题**: `openDetailFromLayout(id)` 仅比 `openDetail(id)` 多一行 `navContext.value = null`，其余逻辑完全相同。存在不必要的代码重复，增加维护负担。
- **建议**: 将 `openDetail` 改为接受可选 `navContext` 参数，`openDetailFromLayout` 委托给统一实现。

### 29. useJustifiedLayout watch 使用 `deep: true` 可能过度触发
- **文件**: `src/composables/useJustifiedLayout.ts:89-105`
- **问题**: 14 个响应式源的 `watch` 使用了 `{ deep: true }`。对于 `filter.mediaTypes`（数组）和 `filters` 对象这类引用类型，任何嵌套属性的变化都会触发 `compute()`，甚至在批量更新场景下可能被多次调用。
- **建议**: 评估是否真的需要 deep watch；如果确实需要，考虑添加 `flush: 'post'` 以在同一 tick 中合并多次变更，或使用显式 getter 函数返回原始值而非 deep watch。

### 30. aiStore 中 `startStatusPolling` 无组件生命周期绑定
- **文件**: `src/stores/aiStore.ts:272-278`
- **问题**: `setInterval` 定时器在 store 级别创建，但仅在 `status.value.isAnalyzing` 变为 false 时自动停止。如果用户在分析期间导航离开，轮询继续运行直到分析完成。虽然功能正确，但存在不必要的网络请求。
- **建议**: 在 Pinia store 的 `$dispose` 中清理定时器；或使用 `$subscribe` 监听 `isAnalyzing` 变化时自动启停。

### 31. Rust 后台 tokio 任务无 JoinHandle 追踪
- **文件**: `src-tauri/src/lib.rs:272,290`
- **问题**: `tauri::async_runtime::spawn` 创建的 PRAGMA optimize 和缩略图缓存清理任务，其 `JoinHandle` 被丢弃。如果 tokio runtime 关闭时任务尚未完成（例如 24 小时循环中途退出），会被静默丢弃，无法获知任务是否完整执行。
- **建议**: 保存 `JoinHandle` 并在 `RunEvent::ExitRequested` 中使用 `tokio::time::timeout` 等待任务完成（最多 5s），超时则放弃。

### 32. AppError `Serialize` 派生可能泄露文件系统路径
- **文件**: `src-tauri/src/error.rs`
- **问题**: `#[derive(Serialize)]` 将所有错误变体（包括 `Io(String)`、`Db(String)`）完整序列化发送到前端。如果错误消息包含用户文件系统路径，可能泄露隐私信息到前端日志或界面。
- **建议**: 为 IPC 传输实现单独的 `UserFacingError` 类型，仅暴露用户友好的错误描述；完整错误信息保留在后端日志中。

### 33. thumbnail_commands 中 `Mutex::lock().unwrap()` 在毒化时连锁 panic
- **文件**: `src-tauri/src/ipc/thumbnail_commands.rs`（多处）
- **问题**: `state_arc.cancelled_thumb_ids.lock().unwrap().remove(&id)` — 如果持有该 Mutex 的线程因 panic 导致锁毒化（poisoned），后续所有缩略图请求都会连锁 panic。虽然在正常操作中很少发生，但缩略图生成涉及第三方图像编解码器（可能 panic 被 `catch_unwind` 捕获），有一定概率出现。
- **建议**: 将 `.lock().unwrap()` 替换为 `.lock().ok()`，毒化时优雅降级（记录警告日志并返回空结果，而非 crash）。

### 34. Store 间存在隐式循环依赖风险
- **文件**: 多个 store 文件（`scanStore.ts` / `mediaStore.ts` / `aiStore.ts` / `uiStore.ts`）
- **问题**: `useRequestQueue` 导入 `useScanStore`；`useMediaStore` 导入 `useUiStore`；`useAiStore` 导入 `useMediaStore` 和 `useUiStore`。虽然 Pinia 允许跨 store 引用，但如果某个 store 在 setup 阶段调用另一个尚未初始化的 store 的方法，可能导致未定义行为或死循环。
- **建议**: 文档化 store 依赖关系图；避免在 store 的 setup 函数体内调用其他 store 的 action；改为在 `$onAction` 或 watcher 中惰性调用。

---

## 六、补充验证与更正

以下是对原审查中部分项目的验证结果：

- **#21 (scrollToY 坐标转换)**: **已修复**。实际代码（`MediaGrid.vue:337`）已调用 `logicalToPhysical(y)` 进行坐标映射，该问题不存在。
- **#9 (strftime 跨平台)**: SQLite 的 `strftime('%s', 'now')` 在 Windows 上的官方构建自 3.38.0 起支持，`rusqlite` bundled feature 使用的 SQLite 版本 ≥ 3.43，**当前安全**。但若未来切换到系统 SQLite 需重新评估。

---

## 七、更新后的总结

| 类别 | 高优先级 | 中优先级 | 低优先级 |
|------|---------|---------|---------|
| 后端崩溃风险 | 4 项 (#1, #2, #3, #8) | 3 项 (#11, #12, #33) | — |
| 后端逻辑缺陷 | 3 项 (#4, #5, #7) | 1 项 (#13) | 1 项 (#14) |
| 后端工程问题 | 3 项 (#6, #9, #10) | 1 项 (#32) | 1 项 (#31) |
| 前端功能缺陷 | 2 项 (#15, #16) | 3 项 (#17, #20, #29) | 1 项 (#28) |
| 前端工程问题 | 2 项 (#18, #19) | 1 项 (#30) | 1 项 (#34) |
| 架构/测试 | 2 项 (#22, #23) | 2 项 (#24, #25) | 2 项 (#26, #27) |

**共 34 项**（原 26 项 + 新增 8 项 #27–#34，更正 #21 已修复、#9 当前安全）

**整体评价**: 项目的核心架构设计是扎实的 —— 两阶段扫描、后端布局计算、行级虚拟滚动、AI 嵌入缓存等设计体现了良好的性能意识。Rust 代码中的踩坑文档（clip.rs 和 engine.rs 的头部注释）质量很高，对后续维护者非常有价值。主要不足集中在**错误处理的健壮性**（多处 unwrap/panic）、**测试缺失**、以及坐标平移模式的已知 bug。建议优先修复标记为"高优先级"的崩溃风险和功能缺陷。
