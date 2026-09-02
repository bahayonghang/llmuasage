# Technical Design

## Boundaries

本任务不改变数据库、Rust 日志查询或 API schema。修复集中在 embedded shell、浏览器导航和事件日志展示层：

```text
usage_event
  -> Dashboard::logs(现有 cursor contract)
  -> GET /api/logs?page_size=20&cursor=...
  -> fetchLogs
  -> logs-viewer state
  -> bounded table viewport + on-demand raw detail
```

运行状态继续消费现有 `insights-card` 与 `failures-card` 渲染结果；只改变其 section ownership 和响应式 composition，不改变数据来源或状态判定。

## Shell And Navigation Structure

- 从 `#cost` 的 `.cost-status-grid` 中移出 `#status`，为它增加独立的 section head、标题、说明和全宽 panel。
- 将三个运行区块按侧栏顺序排列为 `#cost`、`#status`、`#logs`。`setupNavigation` 使用相同顺序观察锚点。
- `#status`、`#logs-viewer`、`#insights-card`、`#failures-card` 等现有 id 保持不变，避免破坏渲染器和深链接。
- 运行状态内部只为两个真实子区块建立两列；在窄断点降为一列。

## Logs Presentation Contract

- `fetchLogs` 显式发送前端展示常量 20；后端仍接受现有 0/1..500 范围，其他调用者行为不变。
- viewer 继续持有 `rows`、`cursor`、`raw` 和 generation/signature。加载更多仍 append，筛选或会话变化仍 reset。
- `.logs-table-wrap` 同时承担横向和纵向局部滚动，使用 viewport-relative 最大高度、`overscroll-behavior` 和 sticky table header；容器外保留筛选条和“加载更多”按钮。
- 为时间、来源、模型、会话、Token、成本和项目列增加语义 class。时间通过现有 `formatDateTime` 格式化；长文本 cell 内使用可截断 span，原始值放入 `title`。
- 可交互事件行继续使用 `tabindex=0`，并补齐 Enter/Space 激活、`aria-expanded` 和 detail row 的稳定关联。原始 JSON 仍通过单条 event-key 请求按需读取。

## Responsive And Visual Rules

- 桌面：运行状态双列且等宽；日志表固定关键数值列，把可变宽度优先分配给会话和项目。
- 窄屏：运行状态单列；日志保留最小表宽并仅在容器内横向滚动，不把 main/body 撑宽。
- sticky header 使用现有 surface/border 变量；light/dark 主题共享变量，不引入固定主题色。
- 原始记录保留受限高度和内部滚动；展开状态通过背景、边框或 disclosure 指示建立层级。

## Compatibility

- live 与 snapshot 继续复用同一 shell 和 asset manifest；snapshot 仍显示原有“事件日志仅在实时看板中可用”空态。
- `#status`、`#logs` 深链接保持不变。DOM ownership 改变不影响现有 Rust 查询或 JSON payload。
- public read-only Dashboard 不暴露日志/诊断详情；本任务不扩展路由 allowlist。
- 不改变 `sync_command_center`、`health.recent_failures` 或 `diagnostics.recent_failures` 的语义。

## Testing Strategy

- Rust embedded-shell 回归断言运行区块顺序、`#status` 为独立 section、必要 i18n key 和 embedded asset wiring。
- Node fetch 测试断言 `/api/logs` 使用 20 条首批请求并保留 filters/cursor/event key。
- Node logs-viewer 测试覆盖格式化/截断所需纯 helper、generation/signature 和键盘激活状态；必要时将现有日志测试纳入 `just ci`。
- `node --check`、聚焦 Node tests、`cargo test web::tests -- --test-threads=1` 后运行完整 `just ci`。
- 对正在运行的 `http://127.0.0.1:37421/` 在 2048×1120、1440×900、390×844 验证锚点、高亮、空白、局部滚动、展开交互、i18n、页面溢出和控制台日志。

## Rollback

改动仅涉及 shell、静态 CSS/JS、文案和测试，没有迁移或持久状态。若视觉回归，可按这些文件整体回退；API 与数据库无需回滚。
