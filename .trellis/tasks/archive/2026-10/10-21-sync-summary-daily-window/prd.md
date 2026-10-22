# sync 汇总改为近 7 天按日期×来源聚合

## Goal

`llmusage sync` 结束时的人读汇总应回答"最近一周用了多少、花了多少"，并按日期分组，而不是只列"本轮新写入"的来源。口径与 `llmusage daily` 一致。

## Background（2026-10-22 实测问题）

上一轮任务（`archive/2026-10/10-21-sync-log-token-summary`）把汇总改成了"本轮新写入事件"的 token/费用表，实际使用暴露两个问题：

1. **来源缺失**：codex、omp、grok 等本轮没有新事件就不进表，用户看不到它们最近的用量。
2. **数字误导**：claude 有 71 个文件变更，按文件重放（reset + 重新插入），"本轮新增"把重放事件也算进来，显示 6 亿 token / $215，看起来像刚产生的用量。
3. **没有时间维度**：用户需要按日期查看，默认最近一周。

另外：
- grok 被判为旧版 token 计数而跳过：根因已由 `7605399`（已合入 `dev`）修复——记录级解析错误不再阻止写入 accounting 标记。已被误标的库仍需执行一次 `llmusage sync --rebuild --source grok`。
- 修复提示把命令包在反引号里（`` Repair: `llmusage sync --rebuild --source grok`. ``），用户整段复制后带上了反引号，导致 `unknown_source`。

## Requirements

### R1 汇总数据来源与时间窗
- sync 成功后，从数据库读取**最近 7 个自然日**（含今天，本地时区，`today-6 ..= today`）的用量，与 `llmusage daily` 默认窗口和时区（`--timezone local`）一致。
- 复用 `query::reports::load_daily_reports_by_source`，不新写 SQL；所有主机（与 `daily` 默认一致）。
- `sync --source X` 时只统计该来源。
- 不新增命令行参数；表尾提示其他范围用 `llmusage daily --since/--until`。

### R2 表格布局（日期 + 来源子行）
- 列：`Date | Source | Input | Output | Cache Create | Cache Read | Total Tokens | Cost (USD)`；中文表头对应翻译。
- 日期倒序（最新在上）；同一日期内来源按费用降序；日期只写在该组第一行。
- 同一日期有 ≥2 个来源时，追加一行该日小计（`(total)` / `（小计）`）；只有 1 个来源时不加小计。
- 日期组之间用分隔线；末尾 `TOTAL` 行为窗口合计。
- 没有用量的日期不出行；整个窗口无用量时输出 `No usage in the last 7 days.`，不画空表。
- 标题行说明窗口：`Usage · last 7 days (2026-10-16 → 2026-10-22, local time)`。
- 未定价事件：费用后加 `*` 并在表尾说明为下限（沿用现有口径；判断依据见 design）。
- 窄终端：先隐藏两个缓存列并提示；数字不截断；`NO_COLOR` / 非 TTY 无 ANSI。

### R3 本轮同步信息保留在概要行
- 管线概要行保留（文件数、事件数、字节、解析/写入耗时）。
- "新增"改称"写入"（`7,385 written` / `写入 7,385 条`），因为重放的文件会重新写入事件。
- 删除 `No new usage:` 行；`Not detected:` 行与按来源诊断保留。

### R4 修复提示可直接复制
- 终端人读输出中的命令不再包反引号（`Repair: llmusage sync --rebuild --source grok`）；改动范围限 `src/common/i18n/sync.rs` 中 sync 人读文案。

### 约束
- `--verbose` 管线明细表行为不变。
- `--json-events` / `SourceSyncStats.inserted_usage` wire 形状不变（保留字段，供 NDJSON 使用方）。
- 查询失败时不得让 sync 失败：降级为只打印概要行，并在 stderr 给一条警告。

## Acceptance Criteria

- [ ] 第二次 sync（无新数据）仍显示最近 7 天按日期×来源的表，包含 codex/claude/grok 等有用量的来源。
- [ ] 表格合计与同一时刻 `llmusage daily --json` 的 `totals`（默认 7 天、本地时区）的 token 与费用一致（集成测试覆盖）。
- [ ] 多来源日期有小计行，单来源日期没有；日期倒序，组内费用降序。
- [ ] `sync --source codex` 只统计 codex。
- [ ] 无用量窗口输出 "No usage in the last 7 days." 且不画表。
- [ ] 窄终端（`COLUMNS=60`）隐藏缓存列、数字完整；`NO_COLOR` 下无 ANSI；中英文快照通过。
- [ ] 修复提示中无反引号；`--verbose`、`--json-events` 现有测试通过。
- [ ] README / 中英文首次同步指南 / CLI 参考 / source-sync 规范更新。
- [ ] `python scripts/ci-rust.py`（已知与时间相关的 `web::tests::api_dashboard_applies_window_to_snapshot_sections` 例外需单独说明）、`node scripts/ci-js.mjs`、`npm --prefix docs run docs:build` 通过。

## Out of Scope

- 新增 `--days` 等窗口参数。
- TUI / web 同步视图。
- `api_dashboard_applies_window_to_snapshot_sections` 的时间敏感问题。
