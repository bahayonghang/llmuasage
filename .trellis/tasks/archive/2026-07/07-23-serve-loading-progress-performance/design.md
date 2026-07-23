# 设计：受监督的 serve 与渐进加载仪表

## 1. Evidence Boundary

本设计解决三个已证明的故障层：Web server 生命周期未受监督、HTML/module bootstrap 没有终态、live 首屏仍等待 full snapshot。用户报告的 30 分钟现象发生后，原 `37421` listener/process 已不存在；相同二进制的正常 full 请求仍约 1.15 秒。因此性能优化保留，但实施顺序必须先让服务退出可观察、页面失联可终止。

原进程为何退出保持 unknown。设计不以自动 daemon 重启掩盖证据缺口，也不把端口冲突、Ctrl+C 或 Axum error 中任一项冒充已经确认的历史原因。

## 2. Server Lifecycle

### 2.1 Owned server handle

把“bind + spawn + 丢弃”拆成一个内部 owned handle：

```text
BoundWebServer
  addr: SocketAddr
  shutdown: CancellationToken
  task: JoinHandle<Result<()>>

bind_server(store, preferred_port, bind_ip) -> Result<BoundWebServer>
BoundWebServer::wait() -> Result<()>
BoundWebServer::shutdown(deadline) -> Result<()>
BoundWebServer::detach_with_error_logging() -> SocketAddr
```

Axum 使用 `with_graceful_shutdown(shutdown.cancelled_owned())`。CLI 持有 handle；兼容的 `web::serve`/`serve_on` 仍返回 `SocketAddr`，但必须通过显式 detach helper 转交 task/token，并在后台记录 terminal error，不能再用 `let _ = ...` 静默吞掉。

`bind_server` 保留 `37421/37422/37423/0` 顺序。指定端口或全部候选绑定失败时返回带 attempted address/source 的 `Err`，删除 `unreachable!`。端口 `0` 仍由 OS 分配。

### 2.2 CLI supervision

```text
recover stale serve run
  -> run_tracked("serve", full serve_session)
      -> bind BoundWebServer
      -> print URLs / exposure warning
      -> browser policy (failure = warn only)
      -> select {
           Ctrl+C       => graceful shutdown, bounded await, success
           server task  => propagate Ok-unexpected/error/panic as failure
         }
```

`run_tracked` 包裹完整 `serve_session`，而不是只包 bind。clean Ctrl+C 返回监听地址供 success summary 使用；server task 无 Ctrl+C 提前结束即使返回 `Ok` 也视为 unexpected termination。JoinError/panic 转为带上下文的 `anyhow` error。

graceful shutdown 使用有限 deadline；超时后 abort task 并返回可诊断错误，避免退出本身无限等待。浏览器 launcher 继续保持非致命；SSH/`--no-open`/public URL 规则不变。

### 2.3 Run-log semantics

- 每次 serve 前恢复旧的 `status=running AND command='serve'` 为 `aborted`，与 sync/hook-run 的 stale recovery 语义一致。
- bind 开始时 run 为 running；只有 Ctrl+C 后 server task 被回收才记 success 和真实 duration。
- bind/server/shutdown failure 记 failed 与 error chain。
- 硬终止无法执行 finally，允许留下 running；下一次启动负责恢复。历史瞬时 success 记录不回写。

## 3. Three-Layer Bootstrap

### 3.1 Layer A: inline shell watchdog

HTML 在 module script 之前安装最小 `window.__LLMUSAGE_BOOTSTRAP__` controller，并启动 3 秒 watchdog。它不依赖 `app.js`、`copy.js` 或任何 imported module，并在 app ready 前监听 `error`/`unhandledrejection`。

```text
shell parsed
  -> module watchdog armed + early error listeners installed
  -> module graph loads
      -> app.js first executed statement calls claim()
      -> main renders core_pending before first await, then calls ready()
      -> controller removes early listeners; app load state owns later errors

module graph never executes, or app fails before ready
  -> watchdog fires
  -> live: GET / with cache=no-store + 1.5s AbortController
       success => page resources/bootstrap failed
       network/timeout => local service unavailable
  -> snapshot: offline asset/bootstrap failed (no service claim)
  -> replace sync-center static loading + status surface + reload action
```

ES module dependencies are resolved before `app.js` body execution；因此任一 import 404/abort 都不会错误调用 `claim()`。`claim()` 只证明 module graph 已执行：它取消 3 秒 module timer，并启动 1 秒 ready timer；只有 core-pending DOM 与 app error handlers 就绪后才能调用 `ready()`。claim 后/ready 前的同步错误、未处理 rejection 或 ready timeout 由 shell controller 收敛到 bootstrap error。若 module 在 watchdog 已显示错误后迟到，claim/ready 与第一次 core render 必须可幂等接管并替换旧错误，不能让 stale watchdog 再写 DOM。

fallback 文案按 `<html data-locale>` 选择最小 ZH/EN 字典。Retry 使用 `location.reload()`，不启动 sync。live 根探测只请求轻量 `/`，不调用大 `health` payload；探测本身必须有 deadline。

### 3.2 Layer B: module/core load state

module 接管后创建显式不可变状态：

```text
phase: core_pending | secondary_loading | complete | error
started_at_ms
slow: boolean
generation
secondary_total: 5
secondary_settled
secondary_degraded
error_kind: timeout | network | http | parse | cancelled | null
```

第一次 `await` 前同步渲染 `core_pending`。2 秒 timer 只设置 slow；6 秒 timer abort core controller 并进入 timeout。Retry 增加 generation、abort 旧 controller、清理 timers/cache entry，然后重新开始。旧响应只有 generation/filter/signal 全匹配才能更新 UI。

所有 core error renderer 都拥有 `sync-command-center`，不能只更新 KPI/status。error 终态保留导航、filters、local-only identity 和 Retry。

### 3.3 Layer C: progressive secondary state

core 成功后立即进入 `secondary_loading`，创建五个显式 loading section，并通过现有 concurrency-2 loader 队列请求 activity/tools/optimize/explorer/compare。每个 loader fulfilled 或 degraded 都执行一次 settle；settle 只替换本 section 的 immutable payload、局部 renderer 和进度 host。

五个 section 均 settle 后进入 complete，停止所有 loading motion。新 generation 会 abort/丢弃旧 secondary 结果；现有 cache、dirty-check、section support 和 stale notice 继续使用。

## 4. Request Contract

### 4.1 Initial live request

```text
GET /api/dashboard?scope=interactive&window=<...>&range=<...>&filters...
```

初始 live 同版本资产与 API 由同一个二进制提供，因此 interactive 请求失败时直接显示错误，不调用 legacy 13-endpoint fallback。为兼容其他显式调用方，`loadDashboardSnapshot` 的旧 fallback 可保留为 opt-in/default legacy 行为；new bootstrap 调用必须传 `legacyFallback: false`，并有请求计数测试。

snapshot 模式继续读完整 `snapshot.json`。无 scope full API、core、interactive 与旧 endpoints 均不删除、不改字段。

### 4.2 Performance path

interactive core 包含 selected trend、overview、models、sources、projects、costs、sync command center、diagnostics 与 health summary，不带 full cursor array。core 达到后先 paint，再调 secondary；不得把五个请求重新 `Promise.all` 到首次渲染之前。

## 5. Progress And Motion

- Core：indeterminate rail，无百分比；2 秒后改为 slow tone/copy，可显示“耗时较长”，不显示预计剩余时间。
- Secondary：同一 rail 变成五个固定 segment，显示 settled `n/5`；degraded segment 用 icon/text + warning tone，仍计入 settled。
- Complete/error：停止循环动画；complete 短暂 opacity transition 后让位给真实 sync command center，error 保留 Retry。
- 动画仅使用 transform/opacity。禁止 width/height/top/left、bounce、glow、blur、shadow pulse 或时间驱动假进度。
- reduced motion 下 scan 静止、transition 近乎即时；文字、segment label、ARIA 不减少。
- rail/status 预留稳定尺寸；长 ZH/EN 文案换行，不改变外层 instrument card 宽度。

## 6. Failure Matrix

| Failure | Required behavior |
| --- | --- |
| Explicit port occupied | Structured bind error, failed run record, no panic/browser open |
| Browser launcher fails | Warn and continue serving |
| Server task errors/panics | Supervisor exits failed and records error |
| Ctrl+C | Bounded graceful shutdown, success duration |
| Process hard-killed | Current page reaches watchdog/core error when applicable; next serve recovers stale run |
| `app.js` missing | Inline watchdog replaces static loading within 3s |
| Imported module missing | Same watchdog path; app body never claims bootstrap |
| App throws/rejects after claim but before ready | Shell early-error handler renders bootstrap failure |
| Core API network/HTTP/parse failure | One dashboard request, explicit sync-center error, Retry |
| Core API hangs | 2s slow, 6s abort/error |
| One secondary fails | That section degraded, progress settles, other sections continue |
| All secondary fail | Core remains usable, progress completes with degraded count |
| Snapshot module fails | Offline asset error, never “local service stopped” |

## 7. Rendering Boundaries

- 新增小型纯 load-state reducer/helper；不让每个 panel 理解全局生命周期。
- shell watchdog 只写预先约定的 bootstrap/status hosts；app claim 后不再写 DOM。
- `renderSyncCommandCenter` 接收 load state 作为 fingerprint input；job polling 与 dashboard loading 是不同状态源，Retry 不触发 job。
- 复用 `renderPrimaryDashboard`、`renderSecondarySection`、panel fingerprints 和 immutable `rawData` replacement。
- loading payload 必须与 no-data/degraded 区分，不能把“未到达”渲染成 0。

## 8. Compatibility And Rollback

- 不改 DB schema、parser、projection、sync event 或 JobSnapshot。
- 不改 public listener/SSH/`--no-open`/unauthenticated exposure 契约。
- public `web::serve`/internal `serve_on` 保持返回 `SocketAddr`；CLI 改用 owned handle path。
- live orchestration可回滚到旧 full loader；server supervision 与 bootstrap watchdog可独立保留，因为它们修复不同故障层。
- inline watchdog 必须同时在 live/snapshot shell contract tests 中验证，避免离线 export 误报。

## 9. Validation Design

- Rust：occupied port、normal listener、server task injected error/panic、graceful shutdown、browser failure、run-log running/success/failed/stale recovery。
- Shell/Node：watchdog claim/timeout/live probe/snapshot copy、load reducer timers、Retry/generation、no legacy fan-out、secondary settle。
- Browser：正常、阻断 app.js、阻断一个 dependency、阻断 API、server 在 initial fetch 中退出；仅保留布尔状态、请求数、时长和错误类别。
- Performance：同版本二进制与代表性只读副本，warm-up + 5 次 interactive/secondary；记录 p95、bytes、support level，不保存正文。
- Compatibility：full/core/interactive/legacy endpoints、snapshot/export、sync job、auto-refresh、filters、public/SSH/no-open、ETag/compression 与 cancellation。

## 10. Client-Filter-Safe Module URLs

用户原 Chrome 环境把 `/assets/data/fingerprint.js` 以 `net::ERR_BLOCKED_BY_CLIENT` 拦截。该文件只计算 panel render cache key，不采集、不传输数据；但它是 `app.js` 的顶层依赖，因此基于 URL 关键字的误拦截会让整个 ES module graph 在 `claim()` 前失败。干净浏览器和直接 HTTP 资产测试不会复现带扩展的客户端过滤规则。

修复限定在资产 URL 边界：文件和嵌入路由改为 `data/render-key.js`，`stableSerialize` / `panelFingerprint` 导出、实现和调用语义不变。旧 `/assets/data/fingerprint.js` 不提供兼容别名，避免继续暴露会被拦截的路径，也让 stale import 在测试中明确失败。live 与 snapshot 共用的新 module graph 必须只引用中性路径。

验证分三层：Rust 资产路由断言新 URL 200、旧 URL 404；Node 从 live module graph 断言存在新 URL且不含旧 URL；最后在用户原 Chrome 扩展环境检查无 `ERR_BLOCKED_BY_CLIENT`、无 runtime exception，且 dashboard 完成初始渲染。此约束只针对浏览器可见资产 URL，不要求重命名内部 cache-key 符号或产品文案。
