# Implementation Plan

父任务不直接修改产品代码；它按以下顺序协调 children 并执行最终集成。

## Phase 0 — Freeze Evidence

- [x] 复核 `research/code-quality-review.md` 的每个行锚点和当前 HEAD。
- [x] 记录所有 child 的 baseline commit、命令、fixture 规模和证据级别。
- [x] 确认任务开始前 worktree，保护无关改动。

## Phase 1 — P1 Data/Execution Boundaries
- [x] 完成并归档 `08-24-codex-tracer-bounded-ingestion`。
- [x] 完成并归档 `08-24-sync-application-boundary`。
- [x] 在第二个 child 完成后重跑 Codex sync/tracer 与 public `JobRegistry` 交叉 fixture。

## Phase 2 — Query/Report Boundaries
- [x] 完成并归档 `08-24-report-project-filter-performance`。
- [x] 完成并归档 `08-24-dashboard-query-modularization`。
- [x] 基于最终文件布局更新 architecture gates；不得通过复制 query helper 消除冲突。

## Phase 3 — Parent Integration Gate

- [x] 运行 public API 与 architecture targets。
- [x] 运行 focused tracer/sync/report/query/web/TUI tests。
- [x] 运行代表性性能 harness；没有可用备份的项目保留 `UNVERIFIED`，不能用 synthetic 冒充。
- [x] 运行 `python scripts/check-ci-gate.py`、`cargo fmt --check`、严格 clippy、串行全测试、rustdoc、Node checks 和 docs build，最终以 `just ci` 为准。
- [x] 汇总 child commits、before/after 指标、跳过项与回滚点。
- [x] 经用户确认后再提交父级仅规划/集成记录并归档；不 push，除非另行要求。

