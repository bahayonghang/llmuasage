# codex-tracer HTTP 边界收口

## Goal

tracer dashboard 的刷新改为非 GET；错误体不再带文件系统路径；查询 `limit` 有上限并用绑定参数。

## Background

监听 `127.0.0.1`（`src/commands/codex_tracer/server.rs:110`）。GET `/api/refresh` 会重新 ingest（`:241-310`）。404/500 把 `err.to_string()` 放进 body（`:142-148`, `:224-233`）。`LIMIT` 字符串插值且未夹紧（`store.rs:341-342`）；index 一次拉 10000 条（`:137-139`）。主 dashboard 已要求 `error.detail` 不出现（SEC-003 测试在 `web/mod.rs`）。

## Requirements

- R1. 刷新改为 POST（或等价非 GET）。GET `/api/refresh` 返回 405 或不再挂载。
- R2. JSON/HTML 错误体使用稳定 code + 短消息，不包含 rollout 路径或 rusqlite 原文。原因写 tracing。
- R3. `limit` 用绑定参数，并夹到明确上限（建议 ≤ 500 列表 API；index 启动查询单独写预算）。
- R4. 仍只绑定 127.0.0.1。

## Acceptance Criteria

- [ ] AC1. GET `/api/refresh` 不触发 ingest。
- [ ] AC2. 失败响应 JSON 不含绝对路径。
- [ ] AC3. 超大 `limit` 被夹紧，SQL 不再 `format!("... LIMIT {limit}")`。
- [ ] AC4. 现有 tracer 解析/存储测试仍通过。

## Out of scope

- 把 tracer 并入主 `llmusage.db`。
- 主 dashboard CSRF（`09-03-loopback-write-csrf`）。
