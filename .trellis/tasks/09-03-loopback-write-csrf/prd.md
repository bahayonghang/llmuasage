# loopback 写路由 CSRF 防护

## Goal

默认 `serve`（127.0.0.1）上的 mutation 不能仅靠 TCP 环回判断。浏览器跨站或 DNS rebind 不能触发 sync rebuild / forget-file。

## Background

`WriteExposure::LocalOnly` 用 `ConnectInfo` 的 `is_loopback()`（`src/web/mod.rs:1368-1381`）。测试 `4753-4776` 标明只认 peer。`POST /api/jobs` 可带 `rebuild: true`（HTTP 已强制 `allow_lossy_rebuild: false`）。无 CSRF token、无 Origin/Host 白名单、无 CSP。`--public` 不挂载写路由，本任务不得改 public allowlist。

推荐：校验 `Origin`/`Host` 为 `http://127.0.0.1:<port>` 或 `http://localhost:<port>`。本地 token 作为备选，成本更高。

## Requirements

- R1. loopback 写路由（jobs 创建/取消、diagnostics forget，以及当前所有 POST mutation）在 peer 环回之外，还要校验 Origin 或 Host 与监听地址一致。
- R2. 缺 Origin 且 Host 不是 loopback URL 时拒绝（401/403），不得“缺 Origin 即放行”。
- R3. `--public` 仍不挂载 mutation；现有 public 安全测试继续绿。
- R4. 同源 dashboard fetch 继续成功（浏览器从 `http://127.0.0.1:<port>` 打开的页面）。
- R5. HTTP 创建的 job 不得比现在更容易做 lossy rebuild。若 UI 不需要 rebuild，从 HTTP 入参去掉 rebuild。
- R6. 用真实 TCP 测试：错误 Origin 的 POST 失败；正确 Origin 的 POST 成功。

## Acceptance Criteria

- [ ] AC1. 带 `Origin: https://evil.example` 的 loopback POST `/api/jobs` 非 2xx。
- [ ] AC2. 无 Origin、Host 为外部名的 POST 非 2xx。
- [ ] AC3. public 模式 mutation 仍 404/405。
- [ ] AC4. 现有 loopback dashboard 写路径集成测试更新后通过。

## Out of scope

- 给 `--public` 加认证/TLS。
- tracer HTTP（独立子任务）。
- 完整 CSP 可放到 `09-03-remaining-security-hygiene`；本任务若顺手加 `X-Frame-Options` 也可以，但不作为 AC。
