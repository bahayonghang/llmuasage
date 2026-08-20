# C4 执行清单

设计依据：父 `design.md` §5、§9；本任务 `design.md`。

前置：C2 完成并通过 G2。

## 顺序清单

- [ ] 1. `src/commands/sync.rs`：引入 `RemoteRunOutcome`（本轮成功与跳过的主机集合）；远端阶段串行执行，单台失败记录原因并继续。
- [ ] 2. `src/parsers/driver.rs`：扫描接收本轮 host 集合，逐主机执行；保持 `stats.last_error.is_some()` 的既有豁免。
- [ ] 3. `src/commands/sync.rs`：`lossy_rebuild_risks` 增加 host 过滤，两处调用点（`--rebuild` 守卫与自动修复守卫）都接线。
- [ ] 4. `src/parsers/mod.rs`：`SyncEvent` 新增三个远端变体。
- [ ] 5. `src/commands/sync_progress.rs`、`src/commands/sync_summary.rs`：渲染新事件与跳过告警。
- [ ] 6. `src/commands/source_status.rs`：主机状态三取值（`idle` / `unreachable` / `never_contacted`），按本任务 design.md 的只读推导规则。
- [ ] 7. 确认 C2 的 `remote sync [--host <label>]` 仍只走 importer；本子任务不重写该命令。`llmusage sync` 的远端阶段复用 C2 importer。
- [ ] 8. 确认 dashboard job 前端对未知 `SyncEvent` tag 不崩溃；必要时补容错。
- [ ] 9. 测试：不可达主机的 AC7、AC7b、AC7c、AC7d、AC7e、AC7f。
- [ ] 10. 新增 ADR 到 `docs/adr/0014-*.md`，登记 `docs/adr/index.md`。
- [ ] 11. 文档修正：按本任务 design.md 的位置表逐处处理，含 `docs/zh/` 对应页。
- [ ] 12. spec 更新：`token-accounting-contracts.md`、`source-sync-contracts.md`、`write-fencing-contracts.md`、`report-cli-contracts.md`、`dashboard-performance-contracts.md`。
- [ ] 13. `cargo fmt`，然后跑 `just ci`。

## 验证命令

```bash
cargo test --test sync_regression -- --test-threads=1
cargo test --test source_file_state -- --test-threads=1
cargo test --test local_flow -- --test-threads=1
cargo test --all-features -- --test-threads=1
just ci
```

## 风险与回滚

| 风险                                                                  | 位置                                          | 处置                                     |
| --------------------------------------------------------------------- | --------------------------------------------- | ---------------------------------------- |
| 用 `last_contacted_at` 与墙上时钟比较判定本轮联系，同秒连续 sync 判错 | `commands/sync.rs`                            | 用内存集合判定，持久化字段只用于展示     |
| 只接线一处 lossy 守卫，另一处仍被远端 `missing` 阻断                  | `commands/sync.rs:768`、`:793`                | AC7b 与 AC7c 分别覆盖两处                |
| 远端不可达导致整次 sync 退出码失败                                    | `commands/sync.rs`                            | AC7 断言退出码                           |
| 新 `SyncEvent` 变体使 dashboard job 前端崩溃                          | `src/web/assets/`                             | 步骤 8 先确认容错；改 JS 用 Bash heredoc |
| 文档只追加不修正，留下自相矛盾的声明                                  | `docs/index.md:21`、`docs/safety/index.md:29` | 按位置表逐处处理，验收含文档项           |

本子任务无不可逆改动。
