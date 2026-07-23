# 实施计划

## 1. Regression Harness First

- [x] 在 isolated temp home/port 写红灯 Rust 测试：正常 listener 持续可用、显式占用端口当前会 panic、detached server task 结果当前不可观察、serve run 当前在 bind 后立即 success。
- [x] 写 shell/Node 红灯测试：入口 module 不执行时静态 sync-center 占位永不结束；API failure 后 `renderBootstrapError` 不替换该占位；initial dashboard failure 会 fan-out 13 legacy endpoints。
- [x] 固化脱敏 browser harness，只记录 request path/status/count、DOM 状态布尔值、first primary mutation、settle time 和 error kind；不生成 HAR/API body/真实截图。
- [x] 保留 `research/server-lifecycle.md` 与 `research/performance-baseline.md` 的 before evidence，不把故障注入当成自然复现。

## 2. Supervised Web Server

- [x] 在 `src/web/mod.rs` 提取 `bind_server`/owned handle，持有 `SocketAddr`、graceful-shutdown token 与 Axum `JoinHandle<Result<()>>`。
- [x] 用结构化 bind error 替换端口探测末尾 `unreachable!`；保留默认端口顺序和 port 0 行为。
- [x] 为 owned handle 实现 `wait`、有界 graceful shutdown 和显式 detach-with-error-logging；兼容 `web::serve`/`serve_on -> SocketAddr`，但 CLI 不再走 detached path。
- [x] 在 `src/commands/serve.rs` 把 `run_tracked("serve", ...)` 扩到 bind、URL 输出、browser policy、Ctrl+C/server-task select 和 shutdown 完成。
- [x] serve 开始前恢复 stale serve `running`；clean Ctrl+C 才记 success，bind/server/shutdown error 记 failed。硬终止记录由下次启动恢复为 aborted。
- [x] 注入 test shutdown/server future，覆盖 task `Ok` 提前结束、returned error、JoinError/panic、shutdown timeout；不得依赖真实 Ctrl+C 或休眠型 flaky test。
- [x] 保留 browser launcher failure 非致命、SSH/`--no-open`/public warning 与 loopback browser URL 契约。

## 3. Module-Independent Bootstrap Watchdog

- [x] 在 `src/web/shell.rs` 的 module script 之前安装最小 inline controller；实现 `claim -> ready` 两阶段握手，3 秒未 claim、claim 后 1 秒未 ready 或 ready 前 early error/rejection 时进入 watchdog error path。
- [x] live watchdog 用 1.5 秒 deadline 探测轻量 `/`：成功为 module/resource bootstrap failure，network/timeout 为 local service unavailable。
- [x] snapshot watchdog 不发 service probe，使用离线资源失败文案；ZH/EN 从 shell locale 选择，不依赖 `copy.js`。
- [x] `app.js` 首个可执行语句 claim controller；core-pending DOM 与 app error handling 建立后、第一次 await 前调用 ready。任一 imported module 失败时 app body 不执行，claim 后/ready 前异常由 shell 的 `error`/`unhandledrejection` handler 接管。
- [x] watchdog 替换 sync-center static loading、status host 并提供 reload 按钮；动作只刷新页面，不启动 sync。
- [x] Rust shell tests + deterministic JS DOM tests覆盖 claim/ready、late handoff、startup exception、unhandled rejection、timeout、probe success/failure/timeout、snapshot、listener/timer cleanup。

## 4. Core Load State And Failure Recovery

- [x] 新增纯 load-state reducer/helper：core pending、slow、secondary progress、complete、timeout/error、retry、generation rejection。
- [x] 第一次 `await` 前渲染 core-pending；初始 core 使用 AbortController、2 秒 slow 和 6 秒 deadline，并在所有分支清理 timer/listener。
- [x] live initial 请求改为 `scope=interactive` 且关闭 legacy fallback；snapshot 继续完整加载，full/legacy APIs 不改。
- [x] core network/HTTP/parse/timeout error 必须更新 sync-center、hero/status 和主要占位；Retry 新建 generation/controller，旧响应不可覆盖。
- [x] 断言 initial core failure 只发一个 dashboard API 请求，不调用 overview/trends/models/... fan-out。

## 5. Progressive Secondary Rendering

- [x] core 到达后立即 merge/render primary，并建立 activity/tools/optimize/explorer/compare 的显式 loading payload。
- [x] 复用 existing secondary loaders 与 concurrency 2；每个 fulfilled/degraded settle 只更新自身 section 和 progress host。
- [x] fulfilled/degraded 都推进 `0..5`；全部 settle 后停止全局 loading。旧 generation/aborted result 静默丢弃。
- [x] 保持 request cache、in-flight invalidation、filter matching、panel fingerprints、immutable replacement、auto-refresh 和 sync-complete refresh 契约。

## 6. Loading Instrument UI

- [x] 在 `copy.js` 添加 ZH/EN core、secondary、slow、timeout、service unavailable、bootstrap failure、retry 和 degraded-count 文案；inline shell 只保留最小 fallback 对应文案。
- [x] 在 sync command center 加稳定尺寸 status/rail：core indeterminate，secondary 五段 determinate，complete/error terminal。
- [x] CSS 只使用现有 Catppuccin semantic tokens、opacity/transform；增加 reduced-motion 静态分支、移动端换行与稳定尺寸约束。
- [x] 状态 host 使用 `role=status`/`aria-live=polite`；warning/error 使用 icon/text + semantic color，segment 有可访问 label，Retry 保持 44px mobile target。
- [x] Retry 与 sync action 在 selector、copy、handler 上完全分离；任何 dashboard recovery 不得 POST `/api/jobs`。
- [x] UI 完成后只运行一次 Impeccable detector：`node C:\Users\lyh\.skillsmanage\skills\impeccable\scripts\detect.mjs --json <changed-ui-files>`；结果仅有尺寸字面值 advisory。

## 7. Focused Validation

- [x] Rust lifecycle/run-log tests：occupied port、normal wait、task error/panic、bounded shutdown、stale recovery、browser failure、public/SSH/no-open。
- [x] Rust web contract tests：full/core/interactive/snapshot shapes、asset/root stability、query error不结束 listener、SQLite cancellation。
- [x] `node --check` 检查所有改动 JS；Windows 下显式列出 Node test 文件，不使用 `node --test scripts/tests/` 目录形式。
- [x] Node tests：watchdog、load reducer/fake timers、AbortSignal、one-request failure、retry/generation、secondary 0..5/degraded、section-local DOM。
- [x] 浏览器故障矩阵：normal、app.js abort、dependency abort、early startup exception、API abort、initial fetch 中 server stop；不保留敏感 artifact。
- [x] 浏览器视觉/无障碍：ZH/EN、light/dark、reduced motion、320x720、720x900、1440x900、1920x1080；验证无溢出/重叠/布局跳动和键盘 Retry。
- [x] 代表库 warm-up + 5 次 interactive/secondary，验证 PRD A6/A7，记录 p95、bytes 与 support levels。

## 8. Full Gate And Review

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo test --all-features -- --test-threads=1`
- [x] dashboard explicit Node syntax/tests
- [x] `npm --prefix docs run docs:build`
- [x] `git diff --check`
- [x] `just ci`
- [x] 更新 web-server/dashboard-performance specs，记录 supervised serve、bootstrap watchdog、interactive-first/no-fan-out 和 progressive/deadline 契约；除非引入新的长期架构决策，否则不新增 ADR。
- [x] 最终 diff 保持用户既有 `Cargo.toml` 版本修改独立，未包含真实 usage/HAR/screenshot 或临时诊断文件；验证服务按要求继续运行。

## 9. Client Filter Regression

- [x] 在用户原 Chrome 扩展环境确认顶层依赖 `/assets/data/fingerprint.js` 被 `net::ERR_BLOCKED_BY_CLIENT` 拦截，而服务端路由和 dashboard API 本身可用。
- [x] 将仅用于 render cache key 的资产 URL 改为中性 `data/render-key.js`；保持 `stableSerialize`、`panelFingerprint` 和 dirty-check 行为不变，不引入 alias 或新依赖。
- [x] 更新 embedded asset registry、live/snapshot module graph 和 Node import；Rust 回归断言新路由 200、旧路由 404，Node 回归断言 module graph 不含旧 URL。
- [x] 在同一用户 Chrome 环境复验：新资产 200、含 `fingerprint` 请求 0、`Network.loadingFailed` 0、runtime exception 0、bootstrap error false、初始 dashboard 请求 1，页面完成渲染。

## Risky Files And Rollback Points

- `src/web/mod.rs`：server handle 与旧 `SocketAddr` wrapper 容易造成 task/token 提前 drop；先通过 lifecycle tests 固化，再接 CLI。
- `src/commands/serve.rs`：run tracking、Ctrl+C 与 server task select 必须保证每条路径只 finish 一次；以 temp home 的 run-log 状态断言为准。
- `src/web/shell.rs`：inline watchdog 必须在 live/snapshot 共用 shell 中最小化，且不能依赖失败的 module graph。
- `src/web/assets/app.js`：bootstrap/reload/generation race；先落纯 reducer/tests，再接 renderer。
- `src/web/assets/data/fetch.js`：关闭 initial fallback 时不得删除 API 或破坏 normalized cache/in-flight coalescing。
- `src/web/assets/render/sync-command-center.js`：job overlay 与 dashboard loading/error fingerprint 必须分离，Retry 不能触发 sync。
- `src/web/assets/components.css`/`layout.css`：避免布局属性动画、one-note status color 和移动端溢出。
- 回滚顺序：先回滚 progressive UI/orchestration，保留 server supervision；再独立回滚 inline watchdog。不得回到 `unreachable!` 或静默丢弃 server task result。
