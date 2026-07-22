# weekly 周报 - 实施计划（C2）

## 实施顺序

- [ ] 在 `reports.rs` 增加周起始日期 helper、weekly aggregate loader 和按来源 loader。
- [ ] 接通 `PeriodKind::Weekly` 到 C1 的 `load_unified_report`，不新增 renderer/JSON DTO。
- [ ] 在 `report_args.rs` 增加 `WeeklyArgs`，在 `commands/mod.rs` 加命令和 dispatch，在
      `commands/weekly.rs` 调用共享路径。
- [ ] 添加 query 单测：周一、跨年、固定时区、All/agents 不变式，以及 weekly/daily totals 相等。
- [ ] 添加 CLI 测试：`weekly --json` camelCase、`--by-agent`、文本 `Week`/Agent/Detected。
- [ ] 更新 README、中文 README 和中英文命令文档的 weekly 示例（与父任务文档收尾一起核对）。

## 验证命令

```powershell
cargo test query::reports::tests::weekly --lib -- --test-threads=1
cargo test --test report_commands weekly -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## 回滚

无数据迁移。回滚该子任务的命令、week-start loader 分支和测试即可；C1 的统一模型保持不变。
