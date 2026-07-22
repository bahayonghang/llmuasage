# --no-cost - 实施计划（C3）

## 实施顺序

- [ ] 在 `ReportCommonArgs` 增加 `--no-cost`，不修改 `to_filter` 或 `ReportFilter`。
- [ ] 给统一命令的 text/JSON 分支透传 `no_cost`；DTO 在最终 `Value` 做递归 strip。
- [ ] 为 daily `--instances` 和 blocks 的 legacy table/JSON 补同样的成本投影。
- [ ] 添加 daily/weekly/monthly/session 的 text 与 JSON 组合回归，覆盖 compact、breakdown 和
      by-agent 嵌套层。
- [ ] grep 证明 query 层没有 `no_cost`，并在父任务文档收尾中记录 flag 语义。

## 验证命令

```powershell
cargo test --test report_commands no_cost -- --test-threads=1
cargo test tui::report_table --lib -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## 回滚

纯输出投影，无数据迁移；移除 flag 和渲染/DTO 投影即可。
