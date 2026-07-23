# serve 生命周期与永久加载诊断（2026-07-23）

## 结论

- 当前 `127.0.0.1:37421` 已无监听，且没有 `llmusage` 进程。对 `/`、`/api/dashboard` 和 `scope=interactive` 的请求均为 connection refused。用户看到的 30 分钟无数据不能再解释为 SQLite 查询持续 30 分钟；直接故障是浏览器保留了静态页面，而对应本地服务已经不可用。
- 原进程为何退出仍无历史证据。原 `37421` 运行在 `run_log` 中只记录了“绑定成功”，没有退出时间、退出码、Ctrl+C、server task 错误或父进程终止原因；持久化 ndjson 日志自 2026-07-16 后也没有新记录，Windows Application 日志未找到 `llmusage` 事件。
- 正常路径仍是约 1 秒级而不是分钟级。相同的已安装 1.0.1 二进制在隔离端口 `37424` 持续监听；全新浏览器 navigation 为 218 ms，`app.js` 62 ms，其余模块约 6-15 ms，full dashboard fetch 1,150 ms，随后真实面板渲染。
- 当前页面有两个确定的永久占位缺口：ES module 图失败时 `main()` 根本不会执行；API 失败时 `renderBootstrapError()` 不更新同步命令中心。两种情况下“正在读取同步状态…”都会继续存在。
- `/api/dashboard` 失败还会触发 13 个旧分段 API 的并发回退。故障注入观测到总计 14 个 API 请求，这会把一个核心请求失败放大为本地请求风暴。

## 当前代码证据

1. `src/web/mod.rs:170` 的 `serve_on` 在 `src/web/mod.rs:243-244` 后台 spawn Axum 并丢弃返回值，调用方拿不到 server task 的结束、错误或 panic。
2. `src/commands/serve.rs:55` 的 `run_tracked` 只包住绑定动作；`src/commands/mod.rs:238-245` 因而在监听刚创建时就把 serve 记为 success，而 CLI 之后才在 `src/commands/serve.rs:109` 等待 Ctrl+C。
3. 指定端口绑定失败会走到 `src/web/mod.rs:251` 的 `unreachable!`。在已占用的 `37424` 上可确定性复现 panic；相应 `run_log` 记录永久停在 `running`，因为 stale recovery 只覆盖 sync/hook-run。
4. 静态占位由 `src/web/shell.rs:238` 直接写入 HTML，唯一模块入口在 `src/web/shell.rs:550`。模块图任一资源失败时，`src/web/assets/app.js:71` 的 `main()` 不会运行，因此没有机会显示 JS 错误态。
5. 模块成功后，首次数据加载在 `src/web/assets/app.js:115` 等待。catch 虽在 `src/web/assets/app.js:130` 调用 `renderBootstrapError`，但该 renderer（`src/web/assets/app.js:448`）没有替换 `sync-command-center`。
6. `src/web/assets/data/fetch.js:201-205` 在 dashboard 请求失败后并发加载旧分段 API。连接/进程异常时，这既不能恢复数据，也增加失败请求数量。

## 受控复现矩阵

所有复现均使用已安装 1.0.1/与 `target\\release` 同 SHA-256 的二进制、隔离端口和隔离浏览器 session。未保存 API body、HAR、截图、项目名、session、prompt 或原始事件。

| 场景 | 观测结果 | 判定 |
| --- | --- | --- |
| 正常 `serve --no-open --port 37424` | 进程与 listener 持续存在，dashboard fetch 1,150 ms，静态占位被真实内容替换 | 当前数据规模不产生 30 分钟查询 |
| 阻断 `/assets/app.js` | 2 秒后 document complete，KPI/status 均未启动，静态占位仍在 | module bootstrap failure 可永久伪装成加载 |
| 阻断全部 `/api/**` | 触发 14 个 API 请求，页面出现 4 个 bootstrap error block，但同步命令中心仍显示静态占位 | JS 错误处理不完整，且存在回退风暴 |
| HTML 已加载后停止诊断服务 | 新 fetch 为 `Failed to fetch`，无 listener/进程，静态占位仍在 | 与“页面留着但服务已停”症状一致 |
| 指定已占用端口 | `src/web/mod.rs:251` panic；退出后 `run_log` 留下 stale `running` | 启动错误不是结构化错误，诊断记录不可信 |
| 浏览器 launcher 失败代码审查 | `src/commands/serve.rs:91-97` 只 warn 并继续 | 不是当前退出的代码路径 |

## 因果分层

### 已确认

1. 30 分钟无数据发生时，当前本地服务已经不存在。
2. 静态 HTML 无独立 bootstrap watchdog；module graph 失败不会触发任何应用错误 renderer。
3. API 错误 renderer 不拥有同步命令中心，导致错误态与永久 loading 文案同时出现。
4. dashboard 失败触发 13 请求并发回退。
5. CLI 不监督 server task，run log 也不覆盖服务会话生命周期。
6. 显式端口冲突 panic 且污染 run log。

### 尚未证明

- 原 `37421` 进程是收到 Ctrl+C、终端/父进程结束、signal 注册错误、操作系统终止，还是其他未记录原因。现有日志不足以从结果反推出触发源。
- Axum accept loop 在原会话中是否先于进程退出。代码允许它静默结束，但本轮没有自然复现该错误。
- 用户最初页面是否在 HTML、module graph 或 initial fetch 的哪一个精确阶段失联。三个阶段都需要独立终态。

## 修复边界建议

1. 让 CLI 持有并监督 Web server task，在 Ctrl+C 与 server task 结束之间 `select`；端口绑定失败返回结构化错误，不能 panic。
2. 把 `run_tracked` 边界扩到完整 serve 会话，并在新会话启动时恢复 stale serve 记录。clean Ctrl+C、bind failure、server error 应有不同的终态证据。
3. 在 HTML shell 内增加不依赖 module graph 的轻量 bootstrap watchdog。模块未接管时，探测本地根路由并在有限 deadline 后明确显示“服务已停止”或“页面资源启动失败”，而不是保留 loading。
4. 模块接管后由统一 load-state 负责 2 秒 slow、6 秒 timeout、Retry 和 generation/abort；所有错误分支必须替换同步命令中心占位。
5. live 初始 interactive 请求失败时直接进入可重试错误，不再并发回退 13 个旧接口。full API 与 snapshot/export 契约继续保留。
6. 在生命周期正确后实施 interactive-first + secondary `n/5` 渐进加载。性能优化仍有价值，但不再被当作 30 分钟故障的首要根因。

