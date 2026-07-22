# serve HTTP 缓存与压缩（S4，P1）

父任务：`.trellis/tasks/07-22-serve-dashboard-ui-perf`（全局约束 H1–H5 继承自父 PRD）。

## Goal

让内嵌静态资源可被浏览器协商缓存（304），文本资源与 JSON 压缩传输，消除每次页面加载的全量重传与根 HTML 重复生成。

## Requirements

- R4.1：静态资源响应加 `Cache-Control: no-cache` + 基于内容 hash 的 ETag，If-None-Match 命中返回 304（assets/mod.rs:13-16）。资源经 `include_str!` 内嵌，hash 可编译期或启动期计算。不采用 immutable + 版本化 URL（shell 引用路径不变，避免牵动 shell.rs）。
- R4.2：tower-http 启用 `compression` feature，Router 挂 `CompressionLayer`，覆盖 CSS/JS/SVG 与 JSON API（src/web/mod.rs:92-118；Cargo.toml:56）。注意与既有路由/中间件顺序。
- R4.3：根页面 HTML 启动期生成一次并缓存（内容仅依赖编译期版本号与 registry），或至少加 ETag 协商（src/web/mod.rs:168-170、shell.rs live_index_html）。
- R4.4：API JSON 不加强缓存；如顺手可对 `/api/dashboard` 响应头保持 `Cache-Control: no-store` 或现状，语义不变得前提下不新增缓存行为（freshness 属 S2 范围）。

## Acceptance Criteria

- [x] A4.1：curl 证据：静态资源首请求 200 带 ETag/Cache-Control；带 If-None-Match 复请求返回 304 且 body 为空。
- [x] A4.2：curl 证据：`Accept-Encoding: gzip`（及 br，如启用）时 app.js/components.css/`/api/dashboard` 响应有 `Content-Encoding` 且解压后内容一致；记录压缩前后字节数。
- [x] A4.3：连续两次 GET `/` 响应体一致（缓存不改变内容）。
- [x] A4.4：Rust 侧新增/更新测试覆盖 ETag 命中与压缩协商；`just ci` 通过。

## Notes

- 轻量任务，PRD-only。改动面：`src/web/assets/mod.rs`、`src/web/mod.rs`、`Cargo.toml`、对应测试。
- 不引入新第三方运行时依赖（tower-http compression 是既有 crate 的 feature）。

## 验收记录（2026-07-22）

- `/assets/app.js`：200，`Cache-Control: no-cache`，ETag `"b69b4a602f39c1a4"`；携带 `If-None-Match` 后为 304，body 0 bytes。
- gzip 原始/压缩字节：app.js 57,475/14,557，components.css 46,043/7,858，interactive dashboard JSON 4,359/1,217；三者解压后 SHA-256 与 identity body 一致。
- 连续两次 GET `/` 的 SHA-256 一致；Rust ETag/压缩测试与完整 `just ci` 通过。
