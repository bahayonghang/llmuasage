# 审阅核验：真实契约事实清单

对 Codex 审阅报告（2026-08-03）逐条源码核验的结果。**所有 10 条均成立**，本文件固化核验出的真实契约，供各子任务 design/implement 直接引用，避免再出现"步骤 0 再核对"的推迟。

## 1. 会话报表真实签名（审阅条 1 ✅）

`src/query/reports.rs:948`：

```rust
pub fn load_session_report(
    store: &Store,
    filter: &ReportFilter,
    session_id_filter: Option<&str>,   // 子串匹配，lowercase contains
) -> Result<SessionListReport>
```

`ReportFilter`（reports.rs:26）字段：`since/until: Option<NaiveDate>`、`order: SortOrder`、`timezone: ReportTimezone`、`locale: String`、`source: Option<SourceKind>`、`project: Option<String>`（标签字符串，非 hash）、`breakdown: bool`。**无 model、无 project_hash**；且直接吃 `Store`，绕过 `Dashboard` 门面（无信号量/超时包装点）。逐事件内存扫描（`visit_filtered_events`），非 SQL 聚合。

→ 结论：Top Sessions 不复用 `load_session_report`，新建 `Dashboard::top_sessions`（SQL 聚合 `usage_event` GROUP BY session_id，完整 `QueryFilter`，SQL 侧 ORDER BY + LIMIT，session_id 作稳定 tiebreak）。

## 2. LogsQuery 真实能力（审阅条 2 ✅）

`src/query/logs.rs:18`：字段仅 `filter: QueryFilter`、`page_size`（默认 50 / 上限 500）、`cursor`、`include_total`、`include_raw_json`。

- **无 session_id / session_label / event_key 过滤**。
- `include_raw_json` 是**整页**开关，非单记录。
- 游标 = base64url JSON `(event_at, event_key)`，newest first。

→ 结论：会话联动与按需 raw 需要服务端契约扩展：`session` 过滤参数（匹配 session_id 或 session_label）+ 单记录详情（`event_key=` 精确取一条含 raw）。

## 3. 快照 DTO（审阅条 3 ✅）

`DashboardSnapshot`（`src/query/mod.rs:798` 起）现有字段：overview、sync_command_center、day/week/month/all_trends、models、sources、projects、costs、activity、tools、optimize、compare...。**无 home_overview / heatmap / trends_daily**。`src/export/mod.rs:22` 仅序列化该 DTO 为 `snapshot.json`。

→ 结论：ready-widgets 的"零后端改动"不成立；快照 DTO 扩展（三个可选字段 + `snapshot()` 组装 + 旧快照兼容）必须纳入该任务的后端范围。

## 4. 面板加载生命周期（审阅条 4 ✅）

- `src/web/assets/load-state.js:1`：`SECONDARY_SECTIONS = Object.freeze(['activity','tools','optimize','explorer','compare'])`；`CORE_SLOW_MS 2000 / CORE_TIMEOUT_MS 6000`；`runLoadersWithConcurrency`；`createDashboardLoadState(generation, ...)` 状态机。
- `app.js:394` 起 `loadDashboardProgressive`：`reloadGeneration` 递增 + `AbortController` + generation 不匹配丢弃（latest-wins）+ `secondaryLoadingPayload` 占位。
- CI：`scripts/tests/dashboard-load-state.test.mjs`、`dashboard-render-lifecycle.test.mjs` 直接 import 这些模块。

→ 结论：所有新面板必须注册进 `SECONDARY_SECTIONS`（或等价扩展点）并走 generation 守卫；独立裸 fetch 会产生旧响应覆盖/进度提前完成。相应 node 测试必须同步扩展。

## 5. 时区真实现状（审阅条 5 ✅，修复成本低于审阅预估）

- `ReportTimezone` 枚举仅 `Utc / Local / Fixed(FixedOffset)`；HTTP 解析 `query_timezone`（`src/web/mod.rs:1855`）：未知字符串（含任意 IANA 名）**静默回退 Local**。
- 但 `src/query/timezone.rs` 已有完整 DST 机制：`ResolvedZone::{Fixed, Iana(chrono_tz::Tz)}`、`FN_LOCAL_DATE/MONTH/WEEK` SQL 函数、DST 双边界处理、Asia/Shanghai 等测试。`ReportTimezone::Local.resolved()` 已解析**服务器机器**的 IANA 区。
- `chrono-tz 0.10.4` 已在 `Cargo.toml:63`（case-insensitive feature）。
- **无 dow/hour SQL 表达式**（只有 date/month/week）。

→ 结论（采纳审阅建议）：新增小型基础子任务 —— `ReportTimezone::Iana(Tz)` 变体 + HTTP 解析接受 IANA 名 + `resolved()` 映射；无新依赖。hour_of_week 的 dow/hour 折叠在 Rust 侧做（按 `hour_start` 聚合后逐行 `ResolvedZone` 转换，一年 ≤ 17520 行，无需新 SQL 函数）。

## 6. home_overview 真实字段（审阅条 6 ✅）

`src/query/home_overview.rs:36`：

- `summary: HomeOverviewSummary { total_sessions, total_requests, total_tokens, total_cost_usd, cache_efficiency, active_days, platforms }`（全 i64/f64）。
- `by_platform: BTreeMap<String, HomeOverviewPlatformStats { sessions, requests, tokens }>`（键 = source id，覆盖全部注册源）。
- `series: Vec<HomeOverviewSeriesItem>`：**固定四平台键** `claude/codex/antigravity/opencode` —— kimi_code/pi/grok 不在 series 中。
- 另有 `bootstrap`（ccr-ui 引导提示）、`archive`（诊断）、`last_updated`。
- 性能契约：冷读，本地 10k 事件种子测试 **80ms 预算**（debug+release，需连续 3 次通过；`dashboard-performance-contracts.md:492` 起）。

→ 结论：汇总卡只用 `summary` + `by_platform`（全源覆盖）；**不用** `series`（四平台键残缺）。设计映射表按真实字段名写死，无"实现时二选一"。

## 7. 性能预算（审阅条 7 ✅）

- `dashboard-performance-contracts.md:12`：点击反馈 p95 ≤ 100ms、交互 API p95 ≤ 400ms（一次热身后）、交互 JSON ≤ 128 KiB。
- 同文 :443：Dashboard 连接 busy_timeout 1500ms；:492 起 home_overview 80ms×3。
- `docs/prd/llmusage-integration-prd-v1.1.md:651`：**logs 分页 < 30ms/页**；同文 372（v1）：overview < 50ms @ 10 万事件。

→ 结论：三个子任务验收都补"继承性能预算"条目；新端点（sessions/hour_of_week）纳入 400ms/128KiB；日志查看器纳入 30ms/页。

## 8. CSV 公式注入（审阅条 8 ✅）

`ref/repo/agentsview/frontend/src/lib/utils/csv-export.ts:19`：`/^[=+\-@\t\r\n]/` 命中则前缀单引号，再做逗号/引号/换行的标准转义。项目名、模型名、会话标签均为不可信输入。

→ 结论：CSV 设计必须含同等防护；纯函数测试从"若可行"改为**必选**。

## 9. 源标记真实类名（审阅条 9 ✅）

`src/web/assets/render/sources.js:45`：现有类为 `.src-name`（另有 `.src-meta/.src-bar-*/.src-value/.src-pct`），markup 不携带 data-source 属性 —— 每源着色**纯 CSS 做不到**。

→ 结论：visual-system 边界修正为"CSS + shell.rs + 渲染器最小 markup 改动（加 `data-source` 属性/徽章 class，不动数据流）"；并显式定义 token 名供后续任务引用：
`--source-claude/--source-codex/--source-opencode/--source-kimi-code/--source-pi/--source-antigravity/--source-grok`、分类色 `--chart-cat-1..6/--chart-cat-other`、热力标尺 `--hm-l0..l4`。

## 10. 规划收敛（审阅条 10 ✅）

- 所有 8 个 jsonl 均残留 `_example` 行 → 删除。
- 父任务 manifest 0 条真实条目 → 补研究文件条目。
- 父任务缺最终集成审查/归档顺序清单 → 补进父 prd。
- 可当场回答的仓库事实不再推迟到实现步骤 0 → 本文件即答案，各 design 直接引用。
