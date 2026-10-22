# 优化 sync 终端日志：结构化 schema 进度与 token/费用汇总表

## Goal

`llmusage sync` 的人类可读输出（stderr 进度 + stdout 汇总）要让用户一眼看到“这轮同步导入了多少 token、花了多少钱”，而不是一堆内部流水线计数。参照 `npx ccusage` 的表格观感。

## Background（现状问题，来自 2026-10-21 实际运行截图）

1. **Schema 迁移刷屏**：全新库首次同步时，`src/commands/sync_progress.rs` 对每个 `SyncEvent::MigrationFinished` 都换行保留，打印 24 行 `Schema vN <name> completed (0ms)`。对用户无意义。
2. **汇总表内容不对**：`src/commands/sync_summary.rs` 的列是 `FILES / CHANGED / SKIPPED / SEEN / COMMITTED / STORED / BYTES / PARSE / WRITE`，全是管线指标；用户关心的 token 与费用完全缺失。
   - 未检测到的来源（opencode、zcode）占一整行 `-`，全 0 的来源（kimi_code、pi、deepseek_harness）也占行，噪音大。
3. **整体排版**：锁获取、schema、表格、诊断之间缺少分组/层级；诊断块与表格风格不一致；颜色语义（黄/绿）随意。

## Requirements

### R1 Schema / 启动阶段结构化
- 迁移过程中保留单行可覆写进度（TTY），完成后只留**一行摘要**，例如
  `✓ Database schema v0 → v24 (24 migrations, 9ms)`；无迁移时不输出。
- 非 TTY / `LLMUSAGE_PROGRESS=line` 也只输出摘要行，不逐条打印（逐条细节进入结构化文件日志即可）。
- 锁获取：等待 < 1s 且未出现 `LockWaiting` 时不输出永久行；真正等待过才保留一行。
- 定价升级、token accounting 修复等有用户意义的阶段行保留，但统一成同一种前缀样式。

### R2 汇总表改为 token / 费用视角（ccusage 风格）
- 默认列：`Source | Input | Output | Cache Create | Cache Read | Total Tokens | Cost (USD)`，数字带千分位，右对齐；`TOTAL` 行加粗。
- 数值语义：**本轮新增**（`INSERT OR IGNORE` 实际写入的事件）的 token 与费用。费用采用与报表一致的 `cost_with_cache_usd`（含 source-reported cost 优先规则，见 ADR 0016）。
- 已检测但本轮无新增的来源合并为一行说明（如 `No new usage: kimi_code, pi, deepseek_harness`），不再各占一行；未检测到的来源只在诊断区出现。
- 表头下方/表尾给一行管线概要：`185 files scanned (185 changed) · 11,417 events seen · 7,385 new · 358.9 MB · parse 2.3s · write 676ms`。
- 原有 FILES/CHANGED/... 明细通过 `--verbose`（或现有等效开关，见 design）仍可查看，不丢失诊断能力。
- 窄终端：保持“只收缩 SOURCE 列、不截断数字”的既有约定；必要时省略 Cache 列并在表尾注明。
- 无任何新增时：输出 `Up to date — no new usage imported.` 加管线概要行，不画空表。

### R3 整体样式与格式
- 输出分三段，段间一个空行：①启动/阶段行（stderr）②汇总表（stdout）③诊断（stdout）。
- 诊断按严重度归组：错误（红）> 解析故障（黄）> 信息（暗色）；同来源合并在一个小节下，缩进与符号统一（`!`/`i`）。
- 颜色仅表达语义：成功绿、警告黄、错误红、次要信息 dim；遵守 `NO_COLOR` / `LLMUSAGE_NO_COLOR` / 非 TTY 无色。
- 中英文双语保持完整（`src/common/i18n/sync.rs` 与 `text()`），中文表头对齐使用显示宽度。

### 约束
- `--json-events` 输出、`SyncEvent` 序列化保持语言中立且**向后兼容**；新增字段只能加 `#[serde(default)]` 可选字段。
- 不改变同步语义、不新增 schema 迁移（token/费用增量在写入时内存累加，不另查库）。
- TUI 同步面板（`src/tui/sync_control.rs`）与 web 若消费 `SourceSyncStats`，新增字段不得破坏其现有行为。

## Acceptance Criteria

- [ ] 全新 home 首次 `llmusage sync`：stderr 中 schema 相关永久行恰好 1 行；已是最新 schema 时 0 行。
- [ ] 汇总表展示每个有新增来源的 Input/Output/Cache Create/Cache Read/Total/Cost，TOTAL 等于各行之和；与同步后立即运行 `llmusage daily --json` 当日（新库场景）总 token/费用一致（单测或集成测试覆盖）。
- [ ] 重复运行第二次 sync（无新数据）输出 “Up to date” 文案，不画空表。
- [ ] 零新增来源合并为一行；未检测来源不进表格。
- [ ] `--verbose`（或 design 确定的开关）仍可输出旧的管线明细列。
- [ ] `NO_COLOR=1`、管道重定向、`COLUMNS=60` 下输出无 ANSI、数字不截断。
- [ ] 中英文两种语言快照测试通过。
- [ ] `--json-events` 旧字段不变（现有 `tests/sync/progress_io.rs` 等通过），新字段可选。
- [ ] `README.md`、`README.zh-CN.md`、`docs/guide/first-sync.md`、`docs/reference/cli.md` 及中文对应页更新示例输出。
- [ ] `python scripts/ci-rust.py` 与 `npm --prefix docs run docs:build` 通过。

## Out of Scope

- TUI / web 仪表盘的同步视图改版。
- 按模型拆分的汇总（可作为后续 `--breakdown` 任务）。
