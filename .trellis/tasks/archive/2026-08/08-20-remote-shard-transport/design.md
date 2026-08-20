# C2 技术设计

设计依据：父任务 `design.md` 第 1、2.2、4 节。本文件只记录父设计未覆盖的子任务级细节。

## 模块归属

新增 `src/remote/` 模块：

| 文件           | 职责                                                                                |
| -------------- | ----------------------------------------------------------------------------------- |
| `mod.rs`       | 对外导出 `RemoteHost`、`RemoteImporter`、`ShardStream`                              |
| `transport.rs` | `ssh` 子进程调用与超时；不含任何解析逻辑                                            |
| `protocol.rs`  | NDJSON 记录类型（`header` / `shard` / `trailer`）、协议版本常量、逐行解析与容错跳过 |
| `importer.rs`  | 拉取到 `commit_shard` 的编排、watermark 推进、`source_sync_status` 写入             |

`tests/architecture_dependencies.rs` 禁止非 `commands` 模块依赖 `commands`。`src/remote/` 不得 `use crate::commands::*`；CLI 层的 `src/commands/remote.rs` 单向依赖 `src/remote/`。

## 传输实现

用 `std::process::Command` 调用系统 `ssh`，不引入 SSH 库（父 design.md §8）。

```
ssh -o BatchMode=yes -o ConnectTimeout=<n> <ssh_target> <command> sync --emit-shards [--since <ts>]
```

- `BatchMode=yes` 阻止交互式口令提示挂住 CLI。需要口令的主机由用户自行配置 agent 或密钥。
- stdout 流式逐行读取，不整批缓冲：远端全量首次导入可能很大。
- stderr 单独收集，上限截断后作为 `host.last_error` 与告警内容。
- 退出码非零且未收到 trailer 视为失败。

不使用 shell 拼接。`command` 字段允许包含空格（例如 `docker exec c1 llmusage`），按空白切分为 argv 传给 `ssh` 的远端命令位置，不经本地 shell 解释，避免注入。

## 协议类型

```rust
pub const SHARD_PROTOCOL_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ShardRecord {
    Header { shard_protocol: u32, llmusage_version: String, schema_version: u32, emitted_at: String },
    Shard { shard: SyncShard },
    Trailer { sources: Vec<SourceSyncStats>, parse_issues: ParseIssues },
}
```

未知 `kind` 靠 `serde` 反序列化失败识别，与非 JSON 行走同一条跳过路径并计数。跳过计数留在本地 `ShardStream` 上，写入告警，不放进 trailer。这样新增记录类型时旧本地二进制不会崩，只会跳过。

解析规则：先跳过无法反序列化的行；第一条成功反序列化的记录必须是 `Header`。header 之前的 motd 走跳过路径。`shard_protocol` 不匹配则立即失败、不提交 shard。

`SyncShard` 的 `raw_records` 标记 `#[serde(skip)]`：序列化时不写出，反序列化时取默认空 vec。这从协议层保证 R3.4，不依赖调用方自觉。

## 导入编排

`RemoteImporter::import(host, store, writer)`：

1. 计算 cutoff：`host.import_watermark` 减 48 小时；无 watermark 则不传 `--since`。
2. 启动传输，跳过前导非 JSON 后读取第一条有效记录；必须是 header 且协议版本匹配，否则立即返回错误，不提交任何 shard（AC5b）。
3. 逐条 `Shard` 记录调用 `writer.commit_shard(shard)`，shard 的 `host_id` 由本地设为该 host，忽略远端可能填写的值。远端不可信地决定自己的 host_id。
4. 收到 trailer 后写 `source_sync_status(host_id, source)`，记录 `host.last_contacted_at`，推进 `import_watermark` 为本批事件的最大 `event_at`。
5. trailer 缺失：保留已提交 shard，不推进 watermark，返回错误（AC5c）。

watermark 取本批事件最大 `event_at` 而不是当前时间：远端时钟与本地时钟可能不同步，用事件时间避免跳过窗口。

## 远端侧 emit 实现

`sync --emit-shards` 走一条独立路径，不复用 `run_once_locked`。parser 的 `parse` 同时接收 `&Store` 与 `&mut SyncRunWriter`，且会在 `commit_shard` 之外写库（`mark_inventory_seen`、`save_opencode_cursor`、`save_zcode_cursor`）。无 permit 的 `write_transaction` 会自动取用户库 worker lock。因此只做 collect-only writer 不够。

两层机制，不改 `SourceParser` trait：

1. `EmitParseStore`：不打开用户 `AppPaths.db_path`。`load_file_cursors` / `load_opencode_cursor` / `load_zcode_cursor` / `tracked_paths` 返回空；`mark_inventory_seen`、`save_*_cursor` 与其它 `write_transaction` 为空操作，且不调用 `acquire_worker_lock`。空 cursor 使每个候选文件都被解析；`seen_file_paths` 仍进入 shard。
2. `Store::begin_collect_run()`：`commit_shard` 把 shard 交给回调而不写 SQLite，分批与顺序协议保持不变。

`--since` 转为 `recent_cutoff` 传给 driver（`parsers/source_parser.rs:51`），不新增过滤逻辑。

断言必须打在用户库上（AC13），不能只断言 collect-only writer：夹具里先写入已知行，跑 emit-shards，再断言用户库行数、`schema_version` 不变，且未取得用户库 worker lock。

## 测试策略

`ci-toolchain-contracts.md` 约束子进程测试。远端交互不能依赖真实 SSH，测试用三层替身：

- `transport.rs` 抽出 `ShardSource` trait（`Read` 来源加退出状态），测试注入内存或临时文件替身，覆盖 header 校验、header 前非 JSON 跳过、trailer 缺失、退出码非零。
- `remote add` / handshake 抽出可注入的命令运行器（与 `ShardSource` 同级），覆盖 AC4（命令不存在）、AC5（协议版本不等）、AC5（协议相同而 schema 不同则允许）、AC5d（`host_id` 冲突与 source 名冲突）。不调用真实 `ssh`。
- 端到端用本机 `--emit-shards` 输出重定向到文件，再由 importer 读入，验证 shard 往返与 host 隔离（AC6）。另用已有用户库夹具验证 AC13。不调用 `ssh`。
