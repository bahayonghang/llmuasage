# PRD：时区基础 —— HTTP 层接受 IANA 时区名

父任务：`.trellis/tasks/08-03-dashboard-agentsview-alignment`。轻量任务（PRD-only）。无前置依赖，可与其他子任务并行，但必须在 `08-03-dashboard-session-analytics` 之前完成（后者的 hour_of_week 契约依赖本任务）。

## 背景与决策记录

审阅核验（父任务 `research/review-verification.md` §5）确认：`ReportTimezone` 仅 `Utc/Local/Fixed`，HTTP 解析 `query_timezone`（`src/web/mod.rs:1855`）对未知字符串（含任意 IANA 名如 `Asia/Shanghai`）静默回退 `Local`。而 `src/query/timezone.rs` 的 `ResolvedZone::Iana(chrono_tz::Tz)` 已具备完整 DST 机制且 `chrono-tz 0.10.4` 已是依赖 —— 缺口只在枚举变体与解析层。

决策：支持浏览器传入的任意 IANA 时区（对齐 AgentsView 的 `Intl.DateTimeFormat().resolvedOptions().timeZone` 服务端分桶策略）。理由：零新依赖、机制已就绪、SSH 端口转发/远程查看场景下服务器 Local ≠ 浏览器时区。

## Requirements

1. `ReportTimezone` 新增 `Iana(chrono_tz::Tz)` 变体；`resolved()` 映射到 `ResolvedZone::Iana`（`src/query/filter.rs:164`）。
2. `query_timezone` 解析顺序：utc/Z → local → 固定偏移 → **IANA 名**（`raw.parse::<Tz>()`，chrono-tz 已开 case-insensitive）→ 仍未知则回退 Local（保持现行静默兼容，rustdoc 写明）。
3. 序列化/显示路径（若 `ReportTimezone` 有 Display/Serialize 消费点）同步处理新变体；`cargo build` 全仓穷尽匹配自然暴露遗漏点。
4. 前端 fetch 层在查询参数中携带 `timezone=Intl.DateTimeFormat().resolvedOptions().timeZone`（追加参数，不改既有导出形状；`dashboard-fetch.test.mjs` 补用例）。
5. 行为不变式：未传 timezone、传 utc/local/固定偏移的既有行为逐字节不变（SQL 生成路径不回归）。

## 非目标

- 不做 dow/hour SQL 表达式（session-analytics 在 Rust 侧折叠）。
- 不改 CLI 报表命令的时区参数面（仅 HTTP 层）。

## Acceptance Criteria

- [ ] Rust 单测：`Asia/Shanghai`（+8 无 DST）与 `America/New_York`（DST）经 HTTP 参数产生正确日期折叠；未知名 `Not/AZone` 回退 Local；`UTC+8`/`utc`/`local` 回归不变。
- [ ] `dashboard-fetch.test.mjs` 验证 timezone 参数出现在请求 query。
- [ ] `just ci` 全绿（含 clippy 穷尽匹配）。
- [ ] `heatmap`/`trends_daily` 等既有端点带 IANA 参数时结果按该时区分桶（挑一个端点做集成断言即可）。
