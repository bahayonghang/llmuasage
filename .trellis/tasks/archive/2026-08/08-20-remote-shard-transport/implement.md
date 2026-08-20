# C2 执行清单

设计依据：父 `design.md` §1、§2.2、§4；本任务 `design.md`。

前置：C1 完成并通过 G1。

## 顺序清单

- [ ] 1. `src/store/mod.rs`：`SyncShard` 与 `RawRecord` 增加 `Serialize` / `Deserialize`；`raw_records` 标记 `#[serde(skip)]`。
- [ ] 2. `src/store/sync_writer.rs`：新增 collect-only 模式（`Store::begin_collect_run()`），`commit_shard` 在该模式下回调而不写 SQLite。
- [ ] 3. 新增 `EmitParseStore`（建议 `src/store/emit.rs` 或 `src/remote/emit_store.rs`）：不打开用户 `db_path`；cursor 读空；inventory/cursor 写为空操作且不取 worker lock。
- [ ] 4. 新建 `src/remote/protocol.rs`：`SHARD_PROTOCOL_VERSION`、`ShardRecord`（trailer 含 `parse_issues`）、逐行解析、本地跳过计数、第一条有效记录必须是 header。
- [ ] 5. 新建 `src/remote/transport.rs`：`ShardSource` trait 与 `ssh` 子进程实现；可注入的命令运行器供 `remote add` / handshake 使用；`BatchMode=yes`、`ConnectTimeout`、stderr 截断收集、argv 按空白切分不经 shell。
- [ ] 6. 新建 `src/remote/importer.rs`：拉取到 `commit_shard` 的编排、host_id 强制由本地设定、`source_sync_status(host_id, source)` 写入、watermark 推进规则、本地跳过行数写入告警。
- [ ] 7. `src/lib.rs` 注册 `remote` 模块；确认 `tests/architecture_dependencies.rs` 通过（`src/remote/` 不得依赖 `src/commands/`）。
- [ ] 8. `src/commands/remote.rs`：`remote add` / `list` / `remove` / `sync` / 隐藏 `handshake`；`add` 的探测与协议握手按父 design.md §2.2 五步，含 R1.6。`remote sync` 取 WorkerLock、派生 fenced Store、只走 importer。
- [ ] 9. `src/commands/mod.rs`：注册 `Remote` 子命令与 `ExportCommand` 同级的 `RemoteCommand` 枚举。
- [ ] 10. `src/commands/sync.rs`：新增 `--emit-shards` 与 `--since`，parser 使用 `EmitParseStore` + collect-only writer，不打开用户库。
- [ ] 11. 测试：协议替身覆盖 header 校验、header 前非 JSON、trailer 缺失、退出码非零；handshake 替身覆盖 AC4 / AC5 / AC5d；用户库夹具覆盖 AC13；`--emit-shards` 文件回灌 importer 覆盖 AC6。
- [ ] 12. `cargo fmt`，然后跑验证命令。

## 验证命令

```bash
cargo test --test architecture_dependencies
cargo test --test sync_regression -- --test-threads=1
cargo test --test local_flow -- --test-threads=1
cargo test --all-features -- --test-threads=1
cargo clippy --all-targets --all-features -- -D warnings
```

## 风险与回滚

| 风险                                | 位置             | 处置                                      |
| ----------------------------------- | ---------------- | ----------------------------------------- |
| collect-only 挡不住 parser 经 Store 的写入 | `EmitParseStore` + AC13 | 用户库夹具断言行数与 lock 不变；禁止只测 writer |
| collect-only writer 自己写库                 | `sync_writer.rs` | writer 层 SQLite 写计数为 0 |
| 远端自填 host_id 污染本地数据       | `importer.rs`    | 本地强制覆盖 shard 的 host_id，忽略远端值 |
| `command` 字段经 shell 解释造成注入 | `transport.rs`   | 按空白切分为 argv，不拼 shell 字符串      |
| 交互式口令提示挂住 CLI              | `transport.rs`   | `BatchMode=yes` 加 `ConnectTimeout`       |
| 首次全量导入内存占用过高            | `importer.rs`    | stdout 流式逐行处理，不整批缓冲           |
| `raw_records` 意外进入线格式        | `store/mod.rs`   | `#[serde(skip)]` 加 AC10 断言             |

本子任务不含不可逆迁移。回滚为撤销代码改动；已导入的远端行可用 host 版 `reset_for_source` 清除。
