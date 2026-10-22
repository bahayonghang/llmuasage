# Implement — sync 汇总改为近 7 天按日期×来源聚合

## Checklist

1. [x] 读 spec：`source-sync-contracts.md`（上一轮新增的 usage 表契约）、`report-cli-contracts.md`（daily 口径/时区）、`cli-language-contracts.md`。
2. [x] commands 层：`print_summary` 构造 7 天 `ReportFilter`（复用 `apply_daily_default`），调用 `load_daily_reports_by_source`，透视为 `WindowUsage`；查询失败降级。
3. [x] 渲染：`format_sync_report` 接收 `Option<&WindowUsage>`；泛化表格（Date 列、小计、组分隔、TOTAL、窄终端隐藏缓存列）；标题行、空窗口、未定价脚注、`daily` 范围提示。
4. [x] 删除旧 `usage_summary_lines` / `usage_cells` / `has_new_usage` / `No new usage`；概要行 `new` → `written`。
5. [x] i18n：修复提示去反引号（6 处），同步受影响断言。
   - 实际：只改两条可执行修复提示（LegacySkipped、有损重建提示），并去掉命令后的句号；失败说明句中的反引号保留以便阅读。无测试依赖原文本。
6. [x] 单测更新/新增（见 design §6）。
7. [x] 集成测试：两天两来源 fixture，二次 sync 仍显示；TOTAL 与 `daily --json` 一致；`--source` 过滤。
   - 实际：并入 `tests/sync/progress_io.rs::sync_summary_table_is_stdout_only_without_ansi_or_completion_sentence`（单来源 codex、近 1 小时事件；第二次 sync 后 TOTAL 与 `daily --json` 一致；`--source opencode` 不含 codex）。跨两天多来源由单测覆盖。
8. [x] 更新 `tests/sync/progress_io.rs`、`tests/sync/runtime/progress_io.rs`。
9. [x] 文档：README（中英）、`docs/guide/first-sync.md` + zh、`docs/reference/cli.md` + zh，用隔离夹具真实输出替换示例。
10. [x] spec：`source-sync-contracts.md` 更新人读汇总契约（窗口查询、布局、降级）。

## Validation

```
cargo test --locked --all-features --lib sync_summary
cargo test --locked --all-features --test sync --test cli -- --test-threads=1
python scripts/ci-rust.py
node scripts/ci-js.mjs
npm --prefix docs run docs:build
```

手工：隔离 `LLMUSAGE_HOME` 跑两次 `sync`、`sync --source codex`、`NO_COLOR=1`、`COLUMNS=60`、`lang zh`；对比 `daily --json` totals。

## Review gates / Rollback

- Gate A（步骤 2–6）：单测通过、手工输出截图确认后再写文档。
- 回滚：revert 提交；无数据或 schema 变更。
