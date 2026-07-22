# 统一 Agent 列报表 — 实施计划（C1）

## 实施顺序（测试先行）

- [ ] 阅读参考 `adapter/all/{mod,report,types}.rs`，固定文本布局与 JSON schema 的目标断言。
- [ ] `src/query`：补失败单测——`load_unified_report`（daily 起）的 `All=Σagents`（token 精确、
      cost 1e-9）、`detected`、`order`、session 无 `All` 层。
- [ ] 实现 `PeriodKind`(Date/Month/Session，预留 Weekly)、`UnifiedRow`/`UnifiedReport`、
      `load_unified_report`（复用 `load_*_report` + `load_daily_reports_by_source` pivot）。
- [ ] `src/tui/report_table.rs`：实现 `render_unified_table(report, compact, no_cost 形参,
      color_mode)`——Agent 列、All+来源子行、Period 首行、Total、compact 列集合、breakdown 归属、
      `Coding (Agent)…\nDetected:` 标题；加渲染快照测试。
- [ ] 新增 camelCase CLI JSON DTO（`report_json`/`row_json`/`totals_json`，`--by-agent`→agents），
      **不改**内部结构 Serialize；加 JSON 形状/键/agents/ session 测试。
- [ ] `report_args.rs`：加 `--by-agent`（`-A`，JSON-only）；删除旧 daily-only Open Decision 分支。
- [ ] 接线 `daily.rs`/`monthly.rs`/`session.rs`：默认文本走统一渲染器、`--json` 走 DTO；保留
      daily `--instances`。
- [ ] **迁移既有测试**：改写 daily/monthly/session 文本快照与 `--json` 断言为新契约（不保留旧断言）。
- [ ] **P8 红线**：确认 dashboard/web/export/TUI payload 测试逐字节不变；grep 确认未改内部结构
      Serialize。
- [ ] 更新 `README.md`+`README.zh-CN.md`+`docs/` 的 daily/monthly/session 示例（Agent 列 + camelCase JSON）。

## 验证命令

```powershell
cargo test query::  --lib
cargo test tui::report_table --lib
cargo test --lib commands
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features -- --test-threads=1
```

编辑 `.rs` 提交前先 `cargo fmt`（全局 formatter hook 会重排 use-import，脏化无关文件）。

## Review Gates

- start 前具备 prd+design+implement 且经评审。
- 聚合不变式：token 精确、cost 1e-9；先失败后通过。
- **P8 红线**：内部 payload 不变、内部结构 Serialize 未改——回归失败即停。
- 文本/JSON 迁移是**有意破坏**：必须更新旧断言为新契约，不得保留旧格式断言"绕过"。

## Rollback Point

无数据迁移。回滚 = 撤销本任务原子提交（统一模型、渲染器、DTO、命令接线、迁移测试、docs）。
C1 是 C2–C5 前提，回滚会连带阻塞下游子任务。
