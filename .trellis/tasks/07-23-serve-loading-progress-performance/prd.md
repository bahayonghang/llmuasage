# serve 生命周期、首屏加载与真实进度反馈

## Goal

让 `llmusage serve` 在大型本地数据库和服务异常两种情况下都给出可信、有限的结果：服务正常时优先呈现可用核心看板并渐进加载分析区；服务退出、入口模块失败或初始 API 失联时，在数秒内结束静态 loading 占位并明确显示重启/重试路径。任何场景都不能让用户等待 30 分钟仍只看到“正在读取同步状态…”。

## Background

- 用户补充页面等待 30 分钟仍无数据。后续 live 检查确认 `127.0.0.1:37421` 已无 listener/`llmusage` 进程，`/`、full dashboard 和 interactive dashboard 均 connection refused。原进程退出触发源因没有 stdout/stderr、exit code 或生命周期日志而无法追溯，不能把推测写成已确认根因。
- 相同 1.0.1 二进制在隔离端口正常持续运行；全新浏览器的 full dashboard fetch 为 1,150 ms，页面随后完成渲染。代表库约 1.16 GB、2,807 cursors；full 约 639-694 KiB/1.09-1.19 秒，interactive 约 14 KiB/6-106 ms。当前数据查询本身没有复现分钟级等待。
- `web::serve_on` 在 `src/web/mod.rs:243-244` detach Axum task 并丢弃结果；`commands::serve` 在 `src/commands/serve.rs:55` 只把端口绑定包进 run tracking，然后在 `src/commands/serve.rs:109` 独立等待 Ctrl+C。server task 结束与 CLI 生命周期没有监督关系。
- 显式端口被占用时，端口循环会到达 `src/web/mod.rs:251` 的 `unreachable!` 并 panic；受控复现还留下 stale `run_log.status=running`。正常 bind 的 serve 记录则在同一秒被标为 success，不能说明服务后来何时或为何结束。
- 静态 sync-center 占位位于 `src/web/shell.rs:238`，入口 module 位于 `src/web/shell.rs:550`。阻断 `app.js` 后，document complete 但 `main()` 从未运行，KPI/status 为空且占位持续存在。
- 阻断全部 API 后，`renderBootstrapError()` 虽生成 4 个错误块，却不替换 sync-center 占位。`src/web/assets/data/fetch.js:201-205` 还把一次 dashboard 失败并发扩散为 13 个 legacy endpoint，总计 14 个失败请求。
- 正常性能问题仍成立：live 首屏在 `src/web/assets/app.js:115` 等待无 scope full snapshot；full payload 包含完整 health cursor 列表，并在 behavior 组合中放大 SQLite 争用。现有 interactive、secondary 并发 2、generation/abort、10 秒请求缓存、局部 dirty-check 与 section-level degradation 应继续复用。

## Requirements

- R1：保留可重复、脱敏的诊断基线。记录二进制一致性、DB 规模、根 HTML、module 资源、full/interactive/secondary 时序、进程/listener 生命周期、错误注入结果和 support level；不得保存真实 API body、HAR、截图、项目路径、prompt、session 或原始事件。
- R2：CLI 必须监督实际 Web server task。正常启动后持续运行到 Ctrl+C；server task 提前返回、错误或 panic 必须使 serve 以结构化错误结束，而不是让 CLI 静默等待。显式端口占用必须返回含地址/原因的错误，不能 panic。
- R3：`run_log` 覆盖完整 serve 会话而非瞬时 bind。clean Ctrl+C 记 success 和真实持续时间；bind/server failure 记 failed；新 serve 启动时恢复上次被外部终止而遗留的 stale serve `running` 记录。浏览器 launcher 失败仍只告警且服务继续运行。
- R4：live HTML 必须有不依赖 ES module graph 的 bootstrap watchdog 与 `claim -> ready` 两阶段握手。若 `app.js`/任一依赖未在 3 秒内 claim、claim 后 1 秒未 ready，或 app 在 ready 前同步抛错/产生未处理 rejection，shell controller 对轻量根路由做有界探测并区分“本地服务不可达”与“页面应用资源未启动”；所有分支都替换静态占位并给出重启/刷新动作。snapshot 模式不得错误宣称本地服务已停止。
- R5：module 接管后使用显式加载状态机：`core_pending -> secondary_loading -> complete`，以及 `slow`、`error/timeout` 分支。状态在第一次 `await` 前写入 DOM；core 2 秒进入 slow，6 秒 abort 并显示可重试错误。服务在 initial JSON 期间退出时，sync-center 也必须进入终态。
- R6：live 初始 interactive 请求失败时直接进入错误/重试，不并发回退 13 个 legacy endpoint。旧 full API、旧分段 API 本身继续存在；仅取消当前同版本 live bootstrap 的失败 fan-out。
- R7：live 首屏先请求 `scope=interactive`，收到后立即渲染 hero、同步命令中心、当前趋势、模型、来源、项目、成本和 health summary；不得等待 activity/tools/optimize/explorer/compare。snapshot/export 和无 `scope` full API 保持兼容。
- R8：核心渲染后复用现有 secondary loaders，以并发 2 独立加载五个分析区。每个 section 到达后只更新自身；失败/超时计为 settled-degraded，不能阻塞其他 section、覆盖新 generation 或让全局进度停住。
- R9：同步命令中心展示真实加载语义。core 未知工作量时用 indeterminate rail；secondary 显示 settled `n/5` 分段进度；degraded 有文本/图标并计入完成。不得显示虚构百分比或根据耗时推算剩余时间。
- R10：动效只表达状态变化，使用 opacity/transform，不动画 width/height/top/left，不引入依赖、定时假进度、发光或装饰性循环。`prefers-reduced-motion: reduce` 停止循环动画并近乎立即完成过渡；状态仍由静态文本、非颜色标记、`role=status`/`aria-live=polite` 和键盘可用按钮表达。
- R11：ZH/EN、light/dark、320px mobile 到宽桌面均不溢出或明显跳动。文案复用 `copy.js`；bootstrap watchdog 只包含模块不可用时仍能显示的最小双语 fallback。Retry 只重载 dashboard 数据/页面，绝不启动 sync。
- R12：full/core/interactive payload、static snapshot/export、sync job start/poll/cancel、request cache、auto-refresh、filters/generation、SQLite cancellation、listener exposure 与 SSH/`--no-open` 契约保持不变。进一步 SQL 优化必须由新证据驱动，不能用隐藏 degraded/error 替代。
- R13：live/snapshot 的内嵌 module URL 必须避开容易被隐私/广告过滤器按路径关键字误判的 telemetry/identity 命名。render cache key helper 使用中性的 `data/render-key.js`；不得保留会触发客户端拦截的 `data/fingerprint.js` 别名，也不得改变其序列化、缓存键或渲染 dirty-check 语义。

## Acceptance Criteria

- [x] A1/R1：`research/performance-baseline.md` 与 `research/server-lifecycle.md` 记录代表库、正常路径、无 listener 事实、受控故障注入和证据缺口；原进程退出触发源明确标为 unknown。
- [x] A2/R2：Rust 生命周期测试证明 listener 在正常等待阶段可用；模拟 server task error/panic 时 CLI supervisor 返回错误；Ctrl+C/测试 shutdown 后 server task 被有界回收。显式占用端口返回 `Err` 且没有 panic。
- [x] A3/R3：测试证明 serve run 在监听期间保持 `running`，clean shutdown 后才变 `success` 并有非零 duration，bind/server failure 为 `failed`，下次启动可恢复 stale serve 记录；browser launch failure 不停止 listener。
- [x] A4/R4：确定性 shell/browser 测试阻断 `app.js`、一个依赖 module，并注入 claim 后/ready 前 startup exception、unhandled rejection 与 ready timeout；未 claim <= 3 秒、已 claim <= 1 秒后静态 loading 被替换。根探测 success 显示资源启动失败，connection refused/timeout 显示服务不可达；late claim/ready 可安全接管旧 watchdog error，snapshot 使用离线资源错误文案。
- [x] A5/R5,R6：确定性 Node/browser 测试覆盖 core pending、2 秒 slow、API HTTP/network/parse failure、6 秒 timeout、Retry、stale generation 丢弃和服务中途退出。每个失败分支都替换 sync-center 占位，initial core 失败最多发出 1 个 dashboard API 请求，不触发 13 endpoint fan-out。
- [x] A6/R7：全新 live 页面初始请求使用 `scope=interactive`，不请求无 scope full；代表性只读副本 warm interactive p95 <= 400 ms、payload <= 128 KiB，首次核心面板渲染 p95 <= 1,000 ms。
- [x] A7/R8,R9：五个 secondary 以并发 2 独立 settle，单区失败不阻塞已就绪区，全部 fulfilled/degraded p95 <= 4,000 ms；progress 从 0/5 到 5/5 后终止且真实记录 degraded 数。
- [x] A8/R9,R10：确定性测试覆盖 0..5 settle、degraded settle、complete/error 动画停止；机械检查证明新动画只使用 opacity/transform，reduced-motion 下没有持续动画。
- [x] A9/R10,R11：ZH/EN、light/dark、reduced motion、320x720、720x900、1440x900、1920x1080 浏览器检查无溢出、重叠或明显布局跳动，ARIA 与 Retry 键盘行为有效。
- [x] A10/R12：full `/api/dashboard`、core/interactive、旧分段 API、snapshot/export、sync job、auto-refresh、filter/generation、public/SSH/`--no-open` 和 query cancellation 契约测试保持通过；代表库 secondary support level 如实记录。
- [x] A11：focused Node/Rust tests、`cargo fmt --check`、严格 Clippy、串行 Rust tests、dashboard JS checks、docs build、`git diff --check` 和最终 `just ci` 通过；用户既有 `Cargo.toml` 版本改动保持独立，未纳入真实 usage artifact。
- [x] A12/R13：用户原 Chrome 扩展环境中 `/assets/data/render-key.js` 返回 200，module graph 不再请求含 `fingerprint` 的 URL，`Network.loadingFailed=0`、runtime exception=0；页面只发出 1 个初始 dashboard 请求并完成渲染。Rust/Node 回归同时断言新路由 200、旧路由 404 和 live module graph 无旧路径。

## Out Of Scope

- 将 `llmusage serve` 安装成 Windows Service/daemon、在进程被操作系统或终端杀死后自动重启，或保证浏览器在后端不存在时仍能查询数据。
- 猜测或宣称已经还原原 `37421` 进程的精确退出触发源；没有历史 exit evidence 时只修复可观测性和可恢复 UX。
- 删除/改形 full `/api/dashboard`、旧分段 endpoint 或 snapshot/export payload。
- 用时间推算 sync job 百分比；未来 overall sync percentage 需要单独保留 denominator 的后端契约。
- 无证据地重写 `home_overview`、SQLite schema、bucket projection 或全部 behavior SQL。
- 持久化前端遥测、外部监控、前端框架、新生产依赖、自动 sync、数据 rebuild，或修改/提交用户现有 `Cargo.toml` 版本变更。
