# Design — sync 汇总改为近 7 天按日期×来源聚合

## 1. 数据：查询而非累加

- 汇总在 `src/commands/sync.rs::print_summary` 中、sync 成功且释放 worker lock 之后读取（只读查询，不与写入争锁）。
- 构造 `query::reports::ReportFilter`：
  - `filter.timezone = ReportTimezone::Local`，`since = today_for_timezone(local) - 6`，`until = today`（与 `unified_report::apply_daily_default` 同口径；直接调用该函数，`commands → commands` 内部可见）。
  - `filter.source = options.source`；`host_id = None`（所有主机，等同 `daily` 默认）。
  - `order = SortOrder::Desc`。
- 调用 `reports::load_daily_reports_by_source(conn, &filter)` → `Vec<(SourceKind, DailyReport)>`；连接用 `query::Dashboard::open(store)?.connection()`，与 `commands/daily.rs` 相同。
- 在 commands 层把结果透视为纯数据结构，交给纯渲染函数，保持 `sync_summary` 可单测：

```rust
pub(crate) struct WindowUsage {
    pub since: NaiveDate,
    pub until: NaiveDate,
    /// Date desc; each day's rows sorted by cost desc.
    pub days: Vec<DayUsage>,
}
pub(crate) struct DayUsage { pub date: String, pub rows: Vec<SourceUsage> }
pub(crate) struct SourceUsage { pub source: &'static str, pub totals: TokenTotals, pub unpriced: bool }
```

- 查询失败：`warn!` 一次 + stderr 一行 `Usage summary unavailable: <reason>`，表格部分省略，概要行与诊断照常输出；sync 退出码不变。

## 2. 渲染（`src/commands/sync_summary.rs`）

- `format_sync_report(.., detail, window: Option<&WindowUsage>)`；`SummaryDetail::Usage` 使用 `window`，`Pipeline` 忽略它。
- 替换上一轮的 `usage_summary_lines` / `usage_cells` / `has_new_usage` 与 `No new usage` 行；保留 `group_digits`、`format_usd`、`pipeline_line`、`diagnostic_lines`。
- 表格泛化：现有 `usage_table` 改为接受 `columns: &[Column]`（header、对齐、是否可隐藏）与行类型枚举：

```rust
enum TableRow { Data([String; N]), Subtotal([String; N]), GroupBreak, Total([String; N]) }
```

  - Date 列只在组首行填值；`Subtotal` 行 dim 样式，Source 列写 `(total)` / `（小计）`；`GroupBreak` 渲染 `├┼┤` 分隔线；`Total` 行黄色加粗。
  - 窄终端：先收缩 Source 列（Date 固定 10 宽），仍溢出则隐藏 Cache Create / Cache Read 并提示。
- 标题：`Usage · last 7 days (since → until, local time)` / `用量 · 最近 7 天（since → until，本地时间）`，置于 `Sync finished:` 下一行。
- 未定价：行 `unpriced == true` → 费用加 `*`；任一行带 `*` 时表尾 `* Includes unpriced events; cost is a lower bound.`。
- 空窗口：`No usage in the last 7 days.` / `最近 7 天没有用量。`；表尾提示 `Other ranges: llmusage daily --since YYYYMMDD --until YYYYMMDD`。

## 3. 概要行

- `pipeline_line` 文案 `N new` → `N written` / `新增 N 条` → `写入 N 条`。

## 4. 修复提示

- `src/common/i18n/sync.rs` 中人读 notice 的 6 处 `` `llmusage …` `` 去掉反引号；检查 `tests/` 与 `src/` 中是否有断言依赖原文本并同步。

## 5. 兼容性

- `SourceSyncStats.inserted_usage` 与写入层累加保留（NDJSON 字段、未来用途），只是不再驱动人读表。
- `--verbose`、`--json-events`、`--emit-shards` 不变。
- 架构：`commands → query` 已被允许（`commands/daily.rs` 先例）；`sync_summary` 不直接依赖 `query`，只接收透视后的结构（`TokenTotals` 为 `query::reports` 类型，允许在 commands 内使用）。

## 6. 测试

- 单测（纯函数）：多来源日期小计、单来源无小计、日期倒序/组内费用降序、空窗口、未定价 `*`、窄终端、中英双语、color 剥离后一致。
- 集成测试（`tests/sync/`）：隔离 home，fixture 写入跨两天的 codex + claude 事件（时间戳在最近 7 天内，按本地时区计算），sync 两次；第二次 stdout 仍含两个来源；表格 TOTAL 与 `llmusage daily --json` `totals` 一致。
- 现有 `progress_io` 两个测试按新输出更新。

## 7. 回滚

纯展示层改动 + 文案；revert 提交即可。
