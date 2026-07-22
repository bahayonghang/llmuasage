# --sections - 实施计划（C4）

## 实施顺序

- [ ] 增加 `UnifiedReportArgs`/`ReportSectionArg` 与 `requested_sections` 单测。
- [ ] 在 `commands/unified_report.rs` 增加段级 filter/default helper、ordered sections JSON 和文本
      runner；复用 `load_unified_report`、DTO 和 renderer。
- [ ] 让 daily/weekly/monthly/session 用共享 runner；保留 daily `--all` 和 instances 例外。
- [ ] 添加 sections 的 text/JSON/invariant integration tests，包括 monthly 当前优先、重复移除、
      session 无 agents、no-cost 递归 strip。
- [ ] 更新 README、中文 README、文档命令页和顶层 help（最终文档收尾一并核对）。

## 验证命令

```powershell
cargo test commands::report_args::tests --lib -- --test-threads=1
cargo test --test report_commands sections -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## 回滚

无数据迁移。删除统一参数、runner 和 sections serializer 即可，单段 C1/C2 行为保持独立。
