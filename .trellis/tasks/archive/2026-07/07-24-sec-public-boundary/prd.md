# public 模式安全边界修复（SEC-001/003/004）

## Goal

消除 `serve --public` 下的未授权远程写路径与信息泄露，使 public 模式具有明确、可验证的安全边界。默认 loopback 模式行为保持不变。

## 覆盖发现（已核实，行号对应当前 dev HEAD）

- **SEC-001（条件性 P0）**：`src/web/mod.rs:862-903` `reject_non_local_write` 仅解析客户端可控的 `Host` header 判断 loopback，`Origin` 缺失时直接放行；`src/commands/serve.rs:52-56` public 模式绑定 `0.0.0.0`；mutation 路由（`POST /api/jobs`、`/api/jobs/{id}/cancel`、`/api/diagnostics/forget`，`src/web/mod.rs:313-316`）与读路由同一 Router。远程原始 HTTP 客户端发送 `Host: localhost:<port>` 且不带 Origin 即可调用写 API。
- **SEC-003（P2）**：`src/web/mod.rs:762-774,1565-1582` 500 响应把 `err.to_string()` 放入 `detail` 字段返回客户端，public 模式可泄露 SQLite 错误、文件路径、内部状态。
- **SEC-004（P2，设计风险）**：public 模式将本地诊断 API 整体暴露：read API 返回项目路径，logs API 可返回 raw JSON（`src/query/logs.rs:44-107,159-199`），无认证、无脱敏。

## Requirements

1. public 模式默认**不挂载** mutation 路由（Router 层面不存在，而非 guard 拦截）；显式开启需 `--public-write` 类参数并强制随机 bearer token（token 文件权限 0600）。
2. 写授权基于 `ConnectInfo<SocketAddr>` 真实 peer IP，而非 HTTP authority；反代场景仅在显式配置 trusted proxy 后才信任 `Forwarded`/`X-Forwarded-For`。
3. 500 响应仅返回 generic code/message/request_id；完整错误链只写 structured log。
4. public 模式 read API 收敛：默认拒绝 raw JSON 与项目路径明文（或提供脱敏 snapshot）；loopback 模式现有行为不变。

## Acceptance Criteria

- [ ] 远程 peer + `Host: localhost` 请求 mutation 路由 → 403，或路由不存在（404/405）。
- [ ] loopback peer 现有本地流程（dashboard 触发 sync/cancel/forget）不回归。
- [ ] public + token 模式：正确 token 成功，无/错 token 拒绝。
- [ ] 未配置 trusted proxy 时，伪造 forwarding header 无效。
- [ ] 所有 API 错误响应不含 `detail` 内部信息；测试断言响应体无路径/SQL 文本。
- [ ] 安全回归测试使用真实 TCP socket（非仅构造 HeaderMap 的单测）。

## Notes

- 审计报告 §1.2 SEC-001 深挖含推荐 `WriteExposure` 枚举草图、攻击复现步骤与验收标准；§6.3 Security 测试矩阵。
- 复杂任务：启动前需补 design.md（暴露策略/参数面）+ implement.md。
- README 与 docs 中 `--public` 的安全说明需同步更新。
