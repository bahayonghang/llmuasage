# Token 统计口径增强（Child A）

Parent: `07-01-token-stats-tui-uplift`

## Goal

补齐 llmusage 统计口径中经代码核对确认为**真缺口**的指标；不重复已实现的 burn
rate / 预算 / daily-monthly-session 报告。

## 范围界定（核对结论）

- ✅ 已实现，**本任务不做**：burn rate、projected、token-limit 预算（`src/query/reports.rs`
  + `blocks.rs`）；daily/monthly/session CLI 报告；current streak / active days /
  best day / cache efficiency（Health 面板 `stats.rs:85-156`）。
- ❌ 本任务要做的真缺口：见 Requirements。

## Requirements

### A1 事件级 context window 利用率（P1，核心）
- 采集每个事件（或每 turn 末次消息）的**上下文占用**与**模型最大上下文窗口**，
  计算 `context_percent = used / model_max`。
- 数据来源：各 parser 已有的 token 字段可近似 used（input + cache_read +
  cache_creation + output + reasoning 的某个口径，需在 design 中定稿）；model_max
  需一张"模型 → 上下文窗口"表（可复用/扩展 pricing catalog）。
- 展示：至少在 Overview 或 Health 面板给出"最近会话上下文占用峰值/均值"。

### A5 longest streak（P2，小增量）
- 在现有 `current_streak` 基础上补"历史最长连续活跃天数"，Health 面板显示为
  `current/longest`（对标 token-tracker `12/45d`）。

### A3 session 级 active vs span（P3，候选，需先确认）
- 先确认 `src/commands/session.rs` / `reports.rs` 是否已计算 session 时长。
- 若无：实现 gap-capped active-time —— 相邻事件间隔 > 30min 视为挂机，不计入
  active；span = 首尾事件差。展示为 `active / span`。
- 若已覆盖：本项标记为 N/A 并从任务移除。

## Acceptance Criteria

- [ ] A1：查询层能返回 context window 利用率（结构体字段 + query 方法 + 单测）；
      至少一个 TUI 面板或 CLI 报告可见该指标；无模型窗口数据时优雅降级（显示 `-`，不 panic）。
- [ ] A5：Health 面板显示 `current/longest` streak；longest 计算有单测覆盖边界
      （全零、单段、多段、末尾连续）。
- [ ] A3：完成"已覆盖"确认；若实现则 active-time 有单测（含 >30min gap 用例）。
- [ ] 无回归：现有 token 统计与报告数值不变。
- [ ] `cargo fmt --check` / `clippy -D warnings` / 相关 `cargo test` 通过。

## 约束

- 改 parser/schema 前读 `.trellis/spec/llmusage/backend/source-sync-contracts.md`。
- context window 若需新 schema 字段 → 走 `src/store/migrations.rs` 加迁移（当前 v13）。
- 最小改动，不顺手重构；`.rs` 编辑后跑 `cargo fmt`。
