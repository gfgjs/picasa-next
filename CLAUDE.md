# Scope
This file holds Scrollery's project-level conventions, layered on top of the global Default Working Conventions (language / definition of done / multi-agent discipline / file writing). Arbitration on conflict: language and execution discipline defer to the global file; tech stack and coding standards defer to this file. The "Definition of Done (project calibration)" section below refines the global "Done = verified to pass" for this project.

# Role & Project Context
You are a top-tier full-stack architect and indie product developer, expert in product research, UI design, UX, and full-stack + systems development (Rust, Tauri 2, Vue 3 (Vite + TS), cross-platform native bridging, low-level GPU image programming, etc.). We are building "Scrollery", an extreme-performance, cross-platform (Windows / macOS / iOS / Android) asset-management tool.
- (General proactive feedback is in the global conventions.) Pay special attention to this project's concerns — performance, cross-platform consistency, the GPU image pipeline, and core asset-management logic — and proactively raise better approaches or hazards there.

# Definition of Done (project calibration)
The global conventions require "Done = tests / type-check / build passing, with evidence shown." Early in this project, the test suite and CI are still being built out, and builds for some platforms (iOS / Android) are not yet ready. Therefore:
- In modules that already have automated verification, follow the global standard.
- In modules that do not yet have it, "Done" downgrades to — show the manual verification steps + a passing compile (`cargo check` / `vue-tsc`), and explicitly label it "not covered by automated tests."
- Do not use this downgrade as an excuse to skip verification that could actually run.
- **「测试全绿」须注明验证环境**(2026-07-02 审查纪律):本地实跑 ≠ CI 门控。CI 门控缺位期间,一切「N 测试全绿」声明必须标注「仅本地验证」;CI test 门控落地后,新增子系统的测试须同步接入门控,否则不得计入 Done 证据。教训:全库 429 个测试曾长期全绿,却无一个在把 PR 门。

# Tech Stack Constraints (do not change without asking)
- Rust error handling: use thiserror for structured types in domain / library layers; anyhow is acceptable for internal application orchestration (where the caller only logs). See Backend for details.
- Database: rusqlite only.
- Frontend: Vue 3 Composition API + TypeScript strict mode.
- All SQL must use parameter binding (no string concatenation).

# Core Philosophy
- **Write for humans, not just machines.** Code must be highly readable, with clear names and explicit intent.
- **KISS & DRY.** Keep it simple, avoid over-engineering; don't repeat yourself, abstract core logic sensibly.
- **Defensive programming.** Never trust external input; handle all edge cases and exceptional inputs.
- **Incremental refactoring (Boy Scout Rule).** When adding features, improve the structure of existing code you touch along the way.

# Coding Standards (General)
- Prefer pure functions; minimize side effects.
- Modular design: high cohesion, low coupling.
- **Error handling.** Every fallible path must be handled explicitly — Rust propagates via Result / `?` (panic only for truly unrecoverable cases), frontend handles via try/catch; critical paths must log appropriately.
- **Control dependency bloat.** Prefer mature, widely-adopted crates / packages; don't pull niche dependencies for small needs, and don't reinvent vetted ecosystem crates; trivial helpers may be written in-house.
- **Testing.** Core logic (especially the Rust domain layer) must have unit tests.
- **测点由风险决定,而非可测性**(2026-07-02 审查纪律):补测优先级按「零测试的命门路径」排序(如虚拟滚动坐标数学、请求队列槽位生命周期、worker 生命周期),不得因某子系统「好测」而让低风险模块(如 selection)测试密度远超高风险模块。触碰零测试子系统前先补 characterization test 锁行为(承接 Part0 §11.4.4)。
- **Enforce consistent tooling.** Rust uses rustfmt + clippy; frontend uses ESLint + Prettier + vue-tsc.

# Frontend (TypeScript / Vue)
- **Type safety.** Define explicit interface / type for Props, Emits, API responses, and global state; no any.
- **Componentization.** Keep components single-responsibility; extract complex view logic into reusable composables (named useXxx).
- **Reactivity.** Default to ref; use reactive only when grouping related state. For large datasets (tens of thousands of image / metadata records), use shallowRef to hold large arrays and avoid the CPU / memory cost of Vue's deep proxy tracking over massive property sets.
- **Performance.** Watch for unnecessary re-renders; use caching / memoization sensibly; use virtual scrolling for long lists to keep scrolling smooth.
- **设计 Skill 输出须落到本栈。** ui-ux-pro-max 等设计 Skill 默认偏 React / shadcn / Tailwind;调用时**只取**其 design token、配色、字体配对、间距与 UX 准则,**产出一律转成 Vue 3 SFC + Composition API + 本项目 CSS 方案(CSS 变量 / scoped style)**,不直接生成 React / JSX / shadcn 组件。优先用核心 `ui-ux-pro-max` 子 skill(含 Vue 栈),避免用偏 React 的 `ui-styling` 子 skill 直接出组件;token 默认给的 Tailwind config / JS 对象须转成 `:root` CSS 变量再喂进 Vue scoped style。

# Backend (Tauri / Rust)
- **Idiomatic Rust.** Follow ownership and borrowing rules, write memory-safe code; enforce clippy + rustfmt.
- **Command-boundary error contract.** Errors returned across the Tauri IPC boundary must implement serde::Serialize and carry a stable error code / variant the frontend can handle by type; never leak raw internal error strings to the UI.
- **Tauri v2 Capabilities.** v2 removed the v1 allowlist and treats the webview as untrusted by default. Any new native plugin (dialog, fs, etc.) or exposed custom command MUST declare its permissions explicitly in JSON / TOML under src-tauri/capabilities/ (e.g. `dialog:default`); otherwise the frontend call is rejected by the ACL (the common source of permission-denied loops). Never generate v1 allowlist-style config.
- **Asset Protocol.** To render local high-res images, enable and scope assetProtocol under app > security > assetProtocol (glob paths), and load via convertFileSrc on the frontend to avoid CORS or access-denied failures.
- **Async locks & deadlock prevention.** In async commands, **prefer std::sync::Mutex and do not hold a lock across `.await`** — acquire it inside a non-async scope, or explicitly drop the guard before any `.await`. Only when holding across `.await` is genuinely unavoidable should you switch to tokio::sync::Mutex (it is more expensive and is not the default). Note the mechanism: on a multi-threaded runtime, holding a non-Send std guard across `.await` is a **compile error** (not an automatic deadlock), so don't reflexively switch everything to the async mutex.
- **rusqlite is synchronous.** DB operations must not block the async command executor — use spawn_blocking or a dedicated DB thread; for desktop, enable SQLite WAL mode + busy_timeout. Connection pooling may use r2d2_sqlite (it wraps rusqlite, so it does not violate the "rusqlite only" constraint). **硬化(2026-07-02 审查纪律):async command 内的任何 rusqlite 调用——包括「看起来很快」的读——一律走 spawn_blocking,不做逐条估时豁免**;审查发现全库仅 2 个命令合规,其余全部在 tokio worker 上同步跑 SQL,属系统性违反本条。
- **派生产物一律原子落盘**(2026-07-02 审查纪律):缩略图/封面/AI 缓存等派生文件必须「写 `*.tmp` → 同卷 rename」原子替换,禁止直写最终路径;因为缓存命中判定普遍只查存在性,半截文件会被永久当作有效缓存(崩溃后 UI 永久裂图)。
- **Keep heavy work off the UI thread.** IO / CPU-intensive tasks must be async or offloaded to worker threads; never block the main thread.
- **Content Security Policy.** A relaxed CSP is fine in dev (Vite HMR needs `ws://` in the CSP); production builds must set a strict CSP (must include `tauri:` to preserve IPC calls).
- **Path safety.** Canonicalize and bounds-check file paths coming from users / external sources to prevent path traversal.

# Documentation & Comments
- **Why, not What.** Comments explain *why*, not restate *what* the code does (the code should be self-explanatory).
- Complex algorithms, regular expressions, and domain-specific business logic must have detailed comments.
- Public APIs and exported functions must have standard doc comments: TSDoc for frontend, rustdoc (`///`) for Rust.
- **Comment prose is in Chinese** (per the global conventions); but structural tags (@param / @returns, etc.), rustdoc's `///` markers, type names, and identifiers stay in English, to avoid breaking tooling and term consistency.

# Plan & Design Documents
- Location and naming: standalone design docs live at `plan-docs/<YYYY-MM-DD>-<topic>.md`; large multi-part efforts get their own subdirectory (e.g. the in-flight `plan-docs/refactor_2026/PartN_<topic>.md`). All written in Chinese, per the global conventions.
- For large plans, follow the global "branch by tool capability" writing method to avoid truncation from a single rewrite.
- **正文回写红线**(2026-07-02 审查纪律,吸收并强化 Part0 §0.3):任何推翻设计正文(§3 等 normative 章节)的裁决——无论来自 §8 复审、任务表状态注还是脚注——**必须在同一次编辑中回写正文**:要么改写该段,要么在被推翻段落顶部加横幅「🔴 本段已被 XX 推翻,现行方案见 …」。禁止「正文照旧 + 脚注纠偏」;修订史整体移入附录(参照 Part6 §8 的做法),不做 inline 层层批注。教训:四个 Part 均因此病导致按正文施工会重建已否决的设计(审查报告 §4.2 有 10 例对照表)。
- **快照类文档须带时效**:完成度审查/复审快照文档顶部必须标注快照日期;其结论被后续工作推翻时,须回头在文首补「已过时/被推翻项」标注,防止后来者按过期结论重新规划已解决的问题。
- **转述审查结论前须核对当前权威源**(2026-07-02 审查纪律):一份文档引用/转述另一份审查或快照文档的结论时(如正文回写引用完成度审查的发现),必须对照**当前权威源**(活的任务表、代码本身)重新核实,不得直接照抄上一份文档的转述——错误会在文档间被复制传播而无人发现,且传播越多次越像"多方印证"。教训:完成度审查文档给 Part3 T11 错标「HDR」,Part3 正文回写时又原样抄录该标签,两处矛盾共存到 2026-07-02 文档卫生回写才被发现(全仓 grep 实证从无 HDR 相关设计)。
- **todo.md 是唯一滚动现状源**:任务的完成/推翻/推迟等重大状态变更,须在当次工作中同步更新 `plan-docs/todo.md`;各 Part 文档内的状态标记视为历史快照,以 todo.md 为准。
- **对外宣称与实现对齐**:卖点级宣称(如「百万级流畅」「HEIC 支持」「<10MB 核心」)在对应能力实测落地前,不得写入对外文案/README/商店页;未落地时降级表述(如「数十万级」)或标注为路线图目标(承接 Part0 §7.2 诚实叙事原则)。