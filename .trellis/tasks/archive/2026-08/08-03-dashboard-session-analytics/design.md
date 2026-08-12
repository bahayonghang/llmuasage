# Design：会话分析

真实契约依据：父任务 `research/review-verification.md`（§1 §2 §5 §7 §8）。

## 后端

### `Dashboard::top_sessions`（新查询，`src/query/top_sessions.rs`）

```rust
pub struct TopSessionsQuery {
    pub filter: QueryFilter,
    pub sort: TopSessionsSort,   // Tokens | Duration | Cost
    pub limit: u32,              // clamp(1..=50)，0 → 10
}
pub struct TopSessionRow {
    pub session_id: String,
    pub session_label: Option<String>,
    pub project_label: Option<String>,
    pub source: Option<String>,      // 跨源会话 → None
    pub total_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub span_minutes: i64,
    pub active_minutes: i64,
    pub event_count: i64,
}
```

- SQL：`usage_event` GROUP BY session 标识（session_id 取值语义对齐 `logs.rs` 的现行做法），WHERE 完整走 `QueryFilter`（含 model / project_hash —— 这是不复用 `load_session_report` 的核心原因）。
- span/active 分钟：span = MAX(event_at)-MIN(event_at)；active 采用与 `reports.rs::session_time_span` 相同的间隙规则。tokens/cost 可在 SQL 排序后只精算 TopN；duration 必须读取过滤范围内全部会话候选，并用一次批量事件时间查询在 Rust 精算 active 后重排截断。固定 `3×N` span 粗排不正确：长 idle span 会挤掉 span 较短但 active 更高的真实 TopN。
- 稳定排序：`ORDER BY <key> DESC, session_id ASC`；Rust 重排同样带 tiebreak。
- 错误映射：handler 内 `Result` → 现有分节降级 payload 词汇（`degraded` + error_key），不透传 500。
- 路由：`loopback_router()` 挂 `GET /api/sessions`，走既有信号量 + 5s 超时包装；public 无此路由（测试断言 404）。

### `LogsQuery` 扩展（`src/query/logs.rs`）

```rust
pub struct LogsQuery {
    // ...既有字段不变...
    pub session: Option<String>,     // session_id 精确（大小写不敏感）或 session_label 子串
    pub event_key: Option<String>,   // 单记录模式：恰返回一条 + raw_json；与 cursor/page_size 互斥
}
```

- `session` → SQL `WHERE lower(session_id) = lower(?) OR instr(lower(session_label), lower(?)) > 0`（label 判空处理）。
- `event_key` 模式：忽略 cursor/page_size/include_total，强制 include_raw_json 语义仅对该条生效；未命中 → 空 records（非错误）。互斥规则 rustdoc + 参数解析处理。
- 既有分页行为逐字节不变（新增字段 default None）。

### `Dashboard::hour_of_week`（新查询，`src/query/hour_of_week.rs`）

- 第一步 SQL：`SELECT hour_start, SUM(total_tokens), SUM(event_count) FROM usage_bucket_30m WHERE <QueryFilter> GROUP BY hour_start`（一年 ≤ 17520 行）。
- 第二步 Rust：每行 `hour_start`（UTC epoch）经 `filter.timezone.resolved()`（`ResolvedZone`）转本地取 `(weekday, hour)` 累加 —— DST 语义自动正确，无需新 SQL 函数。
- 输出：7×24 零填充 `Vec<HourOfWeekCell { dow: u8 /* Monday=0 */, hour: u8, total_tokens: i64, event_count: i64 }>`，rustdoc 写明 dow 约定与桶归属（30m 桶按桶起始时刻归属）。
- 时区：依赖 timezone-iana 任务的 `ReportTimezone::Iana`；该任务未合入前本查询按 Local/Utc/Fixed 工作（代码不依赖新变体，仅行为受益）。
- 路由 `GET /api/hour_of_week` 仅 loopback。

## 前端

### TopSessions（`render/top-sessions.js`）

flex 列表 + `.seg` 三态排序（重新 fetch）；行点击 → `session` 参数写入日志过滤 + `location.hash='#logs'`。并入 `SECONDARY_SECTIONS`（沿用 ready-widgets 模式，`secondaryTotal` 断言同步）。

### LogsViewer（`render/logs-viewer.js`）

- `#logs` section（shell.rs + 侧栏导航 + copy 键）。
- 状态：cursor、累计 rows、过滤签名（全局过滤或 session 过滤变更 → 重置）；"加载更多"页 50。
- 行展开：`<details>` toggle 时按 `event_key` 单记录拉 raw（R2 新契约），缓存已拉取记录。
- 快照模式：live-only 提示。
- **不并入** `SECONDARY_SECTIONS`（日志是按需分页视图，不属于 dashboard 快照面板；首次进入 `#logs` 锚点时懒加载第一页，同样带 generation 守卫防 stale）。

### HourOfWeek（`render/hour-of-week.js`）

SVG 常量照 AgentsView（CELL 17/gap 2/rx 2/489×155/行标签 29/列标签 18）；行序 remap `[6,0,1,2,3,4,5]`（Sun 置首，后端 Monday=0）；分档客户端 max×25/50/75%；`--hm-l*` + `.chart-tooltip`。挂贡献日历同 `.wide` 卡（分隔线组合）。并入 `SECONDARY_SECTIONS`。

### CSV（`assets/csv-export.js`）

`exportAnalyticsCsv(snapshot, locale)` 纯函数 + 下载壳分离（纯函数可 node 测试）：

```js
function escapeCsvCell(value) {
  let v = String(value);
  if (/^[=+\-@\t\r\n]/.test(v)) v = `'${v}`;      // 公式注入防护（对齐 agentsview csv-export.ts:19）
  if (/[",\n\r]/.test(v)) v = `"${v.replaceAll('"', '""')}"`;
  return v;
}
```

BOM 头、多段（段标题 + 空行分隔）、zh/en 表头查 copy.js。测试：注入样例（`=cmd()`、`+1`、`-1`、`@x`、制表符开头）、引号/换行转义、多段结构。

## 快照 DTO

沿用 ready-widgets 的 Option 字段模式：`DashboardSnapshot` 再加 `top_sessions: Option<Vec<TopSessionRow>>`（默认 sort=tokens 前 10）与 `hour_of_week: Option<Vec<HourOfWeekCell>>`；logs 不进快照。

## 资产与清单

新 JS：top-sessions、logs-viewer、hour-of-week、csv-export → `ASSET_MANIFEST` +4（29 → 33）。

## 测试

- Rust：`tests/web_sessions_endpoint.rs`、`tests/hour_of_week.rs`、logs 扩展并入现有 logs 测试文件。用例见 prd 验收清单（含 public 404、排序 tiebreak、时区偏移、event_key 互斥）。
- JS：csv 纯函数、新 section stale 丢弃、logs 懒加载重置。

## 性能预算与实测

- sessions/hour_of_week：热身后 ≤ 400ms、≤ 128 KiB（`curl -w` 实测记录进 research）。
- logs 分页 < 30ms/页（integration-prd v1.1:651 既有预算，扩展 session 过滤后复测）。
- duration 全候选批量精算路径单独计时断言 < 100ms @ 代表库。

## 风险

1. session_id 取值语义（`event_session_id` 的空/合成规则）需与 `logs.rs`/`reports.rs` 保持一致 —— 实现步骤 0 读这两处并把规则写进 top_sessions rustdoc。
2. duration 排序全量精算的代表库预算必须保持 `<100ms`；使用单次批量事件时间查询避免按会话 N+1。
3. `usage_bucket_30m` 的 `hour_start` 单位（epoch 秒/毫秒/字符串）实现前核对一行样本。
