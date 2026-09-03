# 剩余安全收口

## Goal

补上审查里成本低、彼此独立的安全缺口：订阅 HTTP 重定向、explorer 绑定过滤、导出快照脱敏、浏览器隔离头。

## Background

- SEC-004 `subscription/http.rs:6-10` 默认跟随重定向，Bearer 可能跟到其他 Host。
- SEC-005 `explorer.rs:1123-1141` `tool_name`/`tool_kind` 用 quote-doubling 拼 SQL。
- SEC-006 `export` 的 `snapshot.json` 含 projects/hosts/`archive_root`/失败文案；`docs/safety/index.md` 只写聚合标签。
- SEC-007 静态资源与 tracer HTML 无 CSP / frame-deny / nosniff。
- `uninstall --purge` 对任意 `--home` 做 `remove_dir_all`（低，本地操作者）。本任务可选：打印解析后的路径；不强制交互确认（CLI 无 TTY 协议）。

## Requirements

- R1. 订阅 `reqwest::Client` 禁用跨主机重定向（`Policy::none()` 或只允许同 host）。
- R2. explorer `tool_name` / `tool_kind` 用 `?` 绑定，与 `SqlFilter` 一致。
- R3. 安全页写明导出快照可能含项目标签、host、`archive_root`、失败字符串。另提供或默认剥离 `archive_root` 与 `recent_failures` 原文（design 选“默认剥离”或“文档 + 可选 flag”；推荐默认剥离路径字段，保留聚合表）。
- R4. loopback 与 public 的 HTML/资产响应加 `X-Content-Type-Options: nosniff` 与 `X-Frame-Options: DENY`（或 CSP `frame-ancestors 'none'`）。
- R5. 每项有测试或响应头断言。

## Acceptance Criteria

- [ ] AC1. 订阅客户端 builder 可见 redirect policy；测试用 302 到外主机不得带原 Authorization 发出（可用 mock）。
- [ ] AC2. explorer SQL 对 tool 过滤不再出现 `a.tool_name = '...'` 字面量。
- [ ] AC3. `docs/safety/index.md` 与 `docs/zh/safety` 描述真实 snapshot 字段；若默认剥离，导出 JSON 无 `archive_root`。
- [ ] AC4. `GET /` 或 `/assets/app.js` 响应含 nosniff 与 frame 限制。

## Out of scope

- loopback CSRF、SSH `--`、tracer GET refresh。
- `--public` 加认证。
