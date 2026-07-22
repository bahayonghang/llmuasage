# 聚焦来源子命令 - 实施计划（C5）

## 实施顺序

- [ ] 增加 source host Clap 树及 source-injection/conflict helper。
- [ ] 增加 source-filtered unified-to-focused projection，确保不改 query loader。
- [ ] 在 `report_table.rs` 实现参数化 focused renderer（无 Agent/Detected，支持 compact/no-cost）。
- [ ] 在 CLI DTO 实现 focused 单段与 sections 有序 JSON（无 agent/agents）。
- [ ] 接通 daily/weekly/monthly/session，支持 C4 sections 和 C3 no-cost；拒绝不适用的
      focused daily instances 组合并给出明确错误。
- [ ] 添加 parse/dispatch、等价 JSON、text、冲突和矩阵回归；更新 help/docs/README。

## 验证命令

```powershell
cargo test commands::focused --lib -- --test-threads=1
cargo test --test report_commands focused -- --test-threads=1
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## 回滚

无数据迁移。删除 source host、focused projection/renderer/DTO 即可，旧 `--source` 命令完全不变。
