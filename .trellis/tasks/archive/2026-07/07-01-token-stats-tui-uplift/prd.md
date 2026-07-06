# Token 统计与 TUI 观感优化（Parent）

## Goal

对标参考项目 `ref/token-tracker`（Python + Rich），补齐 llmusage 在 **token 统计口径(A)** 与
**TUI 展示观感(B)** 上的差距。本 parent 统筹需求集与跨子任务验收；实现落在两个 child。

## 现状核对（避免重复造轮子）

先行代码核对结论 —— **query/统计层已相当完整，主要差距在"已有数据未进交互 TUI"和少数真缺口**：

| 能力 | 状态 | 证据 |
| --- | --- | --- |
| burn rate / projected / token-limit 预算 | ✅ 已实现（CLI） | `src/query/reports.rs:764-793`、`src/commands/blocks.rs`、`report_args.rs` `--token-limit` |
| daily / monthly / session 报告 | ✅ 已实现（CLI） | `src/commands/{daily,monthly,session}.rs` |
| 贡献热力图 | ⚠️ 单行 strip，非日历网格 | `src/tui/panels/stats.rs:159 render_contribution`（只写 `inner.y` 一行） |
| current streak / active days / best day / cache% | ✅ 已在 Health 面板 | `src/tui/panels/stats.rs:85-156` |
| 事件级 context window 利用率 | ❌ 主管线缺失（仅 Codex Tracer JS 有） | `grep context_window src/domain src/query/mod.rs src/store` 为空 |
| longest streak（历史最长连续） | ❌ 只有 current streak | `stats.rs:366 current_streak` |
| 多主题系统 | ❌ 单套硬编码调色板 | `src/tui/theme.rs` 全 `const Color` |
| 进度条/柱状图分级着色 | ⚠️ 单色 green | `stats.rs:321 render_bar` 固定 green |
| Blocks/burn-rate/Sessions 进交互 TUI | ❌ 仅 CLI，8 面板无此视图 | `src/tui/app.rs` `Panel` 枚举 8 项无 Blocks/Sessions |

## Requirements（拆分到 child）

- **Child A — `07-01-stats-metrics`**：补齐统计口径真缺口
  - A1 事件级 context window 利用率（parser → schema → query → 展示）
  - A5 longest streak（历史最长连续活跃天数）
  - A3（候选，需二次确认）session 级 active vs span（>30min gap 视为挂机，gap-capped active-time）
- **Child B — `07-01-tui-uplift`**：提升展示观感与已有数据可见性
  - B1 贡献热力图升级为 GitHub 风格 7×N 日历网格 + 分位分档着色
  - B2 多主题系统（运行时切换，≥2 套：默认 + Catppuccin/Dracula 之一）
  - B3 进度条/柱状图三阶着色（绿/黄/红）+ 榜首高亮 + 长尾压暗
  - B4（可选）把已有 blocks/burn-rate 与 sessions 报告接入交互式 TUI 面板

## 跨子任务验收（Parent AC）

- [ ] A、B 两 child 各自 AC 全绿并归档
- [ ] `cargo fmt --check` + `cargo clippy --all-targets --all-features -- -D warnings` 通过
- [ ] `cargo test -- --test-threads=1` 通过（跨 source/query/TUI 变更）
- [ ] 无回归：现有 8 面板与 CLI 报告命令行为不变（除非本次显式增强）
- [ ] 新增指标/视图有对应单元测试或渲染快照

## 约束与非目标

- 遵循 `CLAUDE.md`：最小改动、不顺手重构无关代码、匹配现有风格。
- 遵循 `.trellis/spec/llmusage/backend/`：改 parser/source 前读 Source Sync Contracts。
- **非目标**：Web dashboard 认证、ML 时序预测、桌面 GUI、i18n（本轮不做）。
- 编辑 `.rs` 前后按项目记忆运行 `cargo fmt`，避免 import 重排污染无关文件。

## Notes

- 本 parent 本轮**只出规划**（prd/design/implement），不进入实现，待用户 review。
- A3 的 gap-capped active-time 需在 child A 规划中先确认 session 报告是否已覆盖，避免重复。
