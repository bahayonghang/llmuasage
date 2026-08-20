# C2：SSH 传输与远端 shard 导入

父任务：`.trellis/tasks/08-20-ssh-remote-host-import`

## Goal

让用户注册 SSH 远端主机，把远端解析出的规范化 shard 经 SSH 传回本地并落库，事件带正确的 `host_id`。

## Scope

覆盖父任务 R1.3、R1.4、R1.5、R1.6 与 R3（全部），以及 R5.5 的 `remote sync` 命令本身。不含 `missing` 扫描按 host 限定、`source-status` 主机状态、`llmusage sync` 自动包含远端（属 C4），也不含读取层 host 过滤（属 C3）。

前置：C1 必须完成并通过 G1。

## Requirements

- R1.3 新增 `llmusage remote add <label> <ssh-target> [--command <path>]`、`remote list`、`remote remove <label>`。
- R1.4 `remote add` 执行探测与 shard 协议握手；探测失败或 shard 协议版本与本地不等时拒绝注册且不写入 `host` 行，错误信息含两端协议版本、远端 schema 版本与修复指引。schema 版本差异单独不得成为拒绝条件。
- R1.5 `remote remove` 默认保留该主机已导入的用量行，并提示清除方式；清除需显式确认。
- R1.6 `host_id` 与已有 `host_id` 冲突、或等于任一 `SourceKind::as_str()` 时拒绝注册。
- R3.1 远端侧 `llmusage sync --emit-shards [--since <RFC3339>]`，NDJSON 到 stdout。第一条成功反序列化的记录必须是 header。该命令不打开、不写入用户 `AppPaths.db_path`，不在用户库上取 worker lock，不推进用户库 cursor。parser 对 Store 的写入必须打到 `EmitParseStore`，collect-only 的 `commit_shard` 单独不够。
- R3.2 `SyncShard`、`RawRecord` 增加 `Serialize` / `Deserialize`；`raw_records` 标记 `#[serde(skip)]`。
- R3.3 本地经 `ssh` 子进程调用远端命令，逐行反序列化，经 `commit_shard` 在 fenced Store 下落库，并写入该 host 的 `source_sync_status`。
- R3.4 传输内容仅规范化字段。
- R3.5 增量边界由本地权威：`host.import_watermark` 加 48 小时重叠窗口；首次导入不带 `--since`；只有收到 trailer 且全部 shard 提交成功后才推进 watermark。
- R3.6 本地导入器对非 JSON 行与未知 `kind` 计数后跳过，不中止导入。跳过行数由本地解析器计入告警。
- 新增隐藏子命令 `remote handshake`，返回 JSON 形式的 shard 协议版本与 schema 版本。
- 新增 `remote sync [--host <label>]` 子命令，在 fenced Store 下只走 importer，不跑本地 driver（R5.5）。C4 把同一 importer 接到 `llmusage sync`，不重写本命令。

## Acceptance Criteria

- [ ] AC4 `remote add` 指向未安装 llmusage 的目标时返回明确错误，且 `host` 表不新增行。
- [ ] AC5 `remote add` 指向 shard 协议版本与本地不等的远端时拒绝注册，错误信息含两端协议版本；仅 schema 版本不同而协议相同必须允许注册。
- [ ] AC6 远端与本地存在相同绝对路径的会话文件时，`source_file` 与 `source_cursor` 各自独立成行，cursor 不互相覆盖。
- [ ] AC10 shard NDJSON 序列化结果不含 `raw_records` 字段，且不含 prompt 正文（断言方式沿用 `parsers/mod.rs:341` 的 secret 断言模式）。
- [ ] AC11 远端 stdout 在 header 之前或记录之间混入非 JSON 行（登录横幅）时导入仍成功；被跳过的行数来自本地解析器并出现在告警中。
- [ ] AC5b 第一条成功反序列化的记录不是 header、或 `shard_protocol` 不匹配时，导入中止且不提交任何 shard。header 之前的非 JSON 行不触发该失败。
- [ ] AC5c trailer 缺失时已提交的 shard 保留，但 `host.import_watermark` 不推进。
- [ ] AC5d `remote add` 的 `label` 规范化后与已有 `host_id` 冲突、或等于任一 `SourceKind::as_str()` 时拒绝注册，不自动改名。
- [ ] AC5e `remote remove` 默认不删除该主机的 `usage_event` 行。
- [ ] AC13 在已有用户库夹具上跑 `sync --emit-shards` 后，该库的 `schema_version`、`usage_event` / `source_file` / `source_cursor` 行数与运行前相同；测试替身证明未在用户库上取得 worker lock。
- [ ] `cargo test --all-features -- --test-threads=1` 通过。

## Out of Scope

- 并发同步多台远端（本期串行）。
- 远端二进制自动安装或推送。
- `llmusage sync` 自动包含远端（属 C4）。
- 读取层 `--host` 过滤（属 C3）。
