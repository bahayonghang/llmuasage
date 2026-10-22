# Design — sync 日志结构化与 token/费用汇总

## 1. 数据：本轮新增 token / 费用从哪来

写入路径 `src/store/sync_writer.rs::commit_shard_inner` 在 shard 事务里逐条执行
`INSERT OR IGNORE INTO usage_event(...)`，此时已经算好 `pricing::cost_for_event(...)`，
并能拿到 `changed`（1 = 新插入）。因此：

- 给 `ShardCommitStats` 增加 `inserted_usage: UsageDelta`（新类型）：
  `input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, reasoning_output_tokens, total_tokens: i64`、`cost_usd: f64`、`unpriced_events: usize`。
  仅在 `changed == 1` 时累加。
- `collect_sink` 分支（远程/测试收集模式）按 shard.events 计算同样的值，保持语义一致。
- 各 parser（`src/parsers/{codex,claude,grok,kimi_code,opencode,dsh,antigravity,...}.rs`）已把 `commit.events_inserted` 累加进 `SourceSyncStats`；同处累加 `inserted_usage`。
  建议在 `SourceSyncStats` 上加 `fn absorb_commit(&mut self, &ShardCommitStats)` 统一做，减少 8 处重复。
- `SourceSyncStats` 新增 `#[serde(default)] pub inserted_usage: UsageDelta`。`PartialEq, Eq` 派生：f64 不满足 `Eq`，用 `cost_micro_usd: i64`（微美元整数）存储，展示时再换算，避免浮点比较与 Eq 问题。
- `--rebuild` 时“新增”= 重建后写入的全部事件，语义自洽，表头副标题标注 `(rebuild)`。

不新增 schema 迁移，不在同步后额外查库。

## 2. 进度输出（stderr）

位置：`src/commands/sync_progress.rs`（`LineRenderer` 与 `BarRenderer` 两套）+ `src/common/i18n/sync.rs::progress`。

- `MigrationStarted/Finished`：渲染器内部累计 `first_from_version`、`latest_version`、`count`、`elapsed_ms`；TTY 下单行覆写 `Upgrading schema … (k/N)`，**不再**在 `MigrationFinished` 上 `writeln`。
  在第一个非迁移事件（或 `finish`）到来时 flush 一行摘要。非 TTY 只输出摘要行。
  需要 `MigrationStarted` 里的 `latest_version`（已有）；起始版本若事件里没有，摘要写成 `schema → v24 (24 migrations)`，不新增事件字段。
- `LockAcquired`：仅当之前收到过 `LockWaiting` 或 `wait_ms >= 1000` 才保留永久行。
- 新增统一的阶段行格式 helper：`✓ <msg>`（成功，绿）、`! <msg>`（警告，黄）、`✗`（错误，红）；在 `i18n::sync` 产出纯文本，渲染器负责着色。

## 3. 汇总（stdout）

位置：`src/commands/sync_summary.rs`。保持 `format_summary_lines_with_notices` 为纯函数，签名新增 `detail: SummaryDetail { Usage, Pipeline }`。

- `Usage`（默认）：
  - 行集合 = `!absent && inserted_usage.total_tokens > 0`（或 events_inserted > 0）的来源，按 cost 降序。
  - 列：Source / Input / Output / Cache Create / Cache Read / Total Tokens / Cost (USD)。数字千分位（新增 `fn group_digits`，中文同样用逗号），费用 `$1,234.56`；有未定价事件时 Cost 加 `*` 并在表尾注明。
  - 零新增来源 → 一行 dim 文本；无任何行 → “Up to date” 文案。
  - 表尾管线概要行由现有 `total_cells` 的数据生成。
- `Pipeline`：现有表格原样保留（逻辑不动，只换入口），由 `--verbose` 触发。先确认 `SyncRunOptions` 是否已有 verbose/详细开关；没有则在 `src/commands/sync.rs` 的 clap 定义里加 `--verbose`，并更新 help（`src/commands/help.rs`）与 i18n。
- 窄终端：沿用 `fit_source_column`；若仍溢出，依次省略 Cache Create、Cache Read 列，表尾注明 `Cache columns hidden; widen terminal or use --json`.
- 边框继续用现有 box-drawing，表头 dim 改为 bold+cyan（与 ccusage 观感接近），TOTAL 行 bold 黄。

## 4. 诊断区

- 收集每个来源的 notices / last_error / parse_issues，按严重度排序后输出；`absent` 来源汇成一行 `Not detected: opencode, zcode`（信息级），不再每个来源一个小节。
- 小节头统一 `Diagnostics · <source>`，条目统一两空格缩进 + 级别符号。

## 5. 兼容性

- `SyncEvent` JSON：只新增可选字段（`SourceFinished.stats.inserted_usage`），老消费者忽略即可。`tests/sync/progress_io.rs`、`tests/sync/runtime/progress_io.rs` 需要确认是否断言了 stderr 文本，按新格式更新。
- TUI `src/tui/sync_control.rs` 引用 Migration 事件：只改 CLI 渲染器，不改事件，TUI 行为不变。
- `doctor.rs`、`source_status.rs` 复用 `notice_lines` / `parse_issue_lines`，诊断重排时保持这两个函数签名与输出不变，新排序只在 sync 汇总层做。

## 6. 回滚

纯展示层 + 一个累加字段；回滚即 revert 提交。无数据迁移。
