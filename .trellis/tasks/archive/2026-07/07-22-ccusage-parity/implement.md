# ccusage parity — 执行编排（父任务）

父任务本身**无直接实现**；负责编排子任务顺序、跨子任务门禁与集成评审。各子任务的具体清单在其
自身 `implement.md`。

## Build order（严格依赖）

1. **C1 `daily-by-agent-report`（unified-agent-report）** — 基础：统一 Agent 列渲染器 +
   camelCase JSON DTO + `--by-agent`，覆盖 daily/monthly/session。**先完成 design/implement → 评审 →
   start → 实现**。
2. **C2 `weekly-report`** — 依赖 C1：把 Weekly kind 接入统一模型，周键=周起始日期。
3. **C3 `no-cost-flag`** — 依赖 C1：文本列省略 + JSON cost strip，覆盖所有报表（含 C2 weekly）。
4. **C4 `sections-composite-report`** — 依赖 C1、C2：`--sections` 编排层。
5. **C5 `source-subcommands`** — 依赖 C2：聚焦能力矩阵，无 Agent 列。
6. **C6 `date-filter-format`** — 无依赖，可任意时点并入。

## 每子任务开工门（Trellis 复杂任务规则）

- 复杂子任务（C1–C5）在 `task.py start` 前必须具备 `prd.md`+`design.md`+`implement.md`；C6 可 PRD-only。
- **目前仅 C1 已具三件套**；C2–C5 已修正 PRD 契约，但 design/implement 在各自开工前补齐。
- 每个子任务实现为原子提交，避免默认表出现新旧格式混合的中间态。

## 集成评审（全部子任务归档后）

- [ ] 跑 `just ci` 全量门禁（fmt / clippy / `cargo test --test-threads=1` / `cargo doc` /
      node 脚本检查 / `docs:build`）。
- [ ] 手动 smoke：`llmusage daily|weekly|monthly|session`、各 `--by-agent --json`、`--no-cost`、
      `--sections daily,weekly,monthly,session [--json]`、`claude daily`/`codex daily`/
      `opencode weekly`、`--since 2026-04-25`。
- [ ] 核对父 PRD P1–P9 与"破坏性变更面"迁移清单逐项完成。
- [ ] 更新 `README.md` + `README.zh-CN.md` + `docs/` 命令页与示例（AGENTS.md 要求）。
- [ ] 在 `.trellis/spec/llmusage/backend/` 落一份报表 CLI surface 契约文档。

## 破坏性变更迁移核对

- [ ] 默认 `daily/monthly/session` 文本快照测试已按 Agent 列新结构更新（非保留旧断言）。
- [ ] CLI `--json` 断言已改为 camelCase `{<period>:[...],totals}`；旧 snake_case CLI JSON 断言移除。
- [ ] 确认未改动 `TokenTotals` 等内部结构 Serialize；dashboard/web/export/TUI 测试逐字节不变（P8）。

## 验证命令

```powershell
just ci
# 或分步（编辑 .rs 后先 cargo fmt，避免全局 formatter hook 重排 import 脏化）
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features -- --test-threads=1
```
