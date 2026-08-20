# 技术设计：SSH 远端主机导入与同步

对应 `prd.md` 的 R1–R6 与 D1–D5。本文件是四个子任务共同的设计依据；子任务的 `implement.md` 引用本文件的章节而不重复内容。

## 1. 架构与边界

```
远端主机                                  本地主机
────────                                  ────────
llmusage sync --emit-shards               llmusage remote sync / sync
  ├ EmitParseStore（不打开用户库）           ├ WorkerLock::acquire → fenced_store
  ├ registry::registered_parsers()          ├ store.begin_sync_run()
  ├ 本地解析（含 ProjectResolver）           ├ RemoteImporter::import(host, reader)
  └ NDJSON(stdout)  ──── ssh ────►          │   └ writer.commit_shard(shard.with_host(host))
                                            └ source_sync_status(host_id, source)
```

边界规则：

- 解析只发生在拥有产物的机器上。本地不解析远端文件，不挂载远端目录，不拉取原始会话文件。
- 远端进程只读自己的产物并写 stdout。它不打开用户 `AppPaths.db_path`，不在用户库上取 worker lock，不推进用户库 cursor。parser 对 Store 的 `mark_inventory_seen` / `save_*_cursor` 只能打到 emit 用的隔离 Store。
- 落库只经 `SyncRunWriter::commit_shard`，在 `WorkerLock` 派生的 fenced Store 下执行，遵守 `write-fencing-contracts.md`：不新增控制面例外。
- cost 由本地 pricing catalog 在 `commit_shard` 内计算（`store/sync_writer.rs:97-105`、`:331-338`）。远端不传 cost，跨主机 pricing 版本差异不进入数据。

## 2. host 身份与注册

### 2.1 host 表

```sql
CREATE TABLE host (
    host_id           TEXT PRIMARY KEY,
    label             TEXT NOT NULL UNIQUE,
    transport         TEXT NOT NULL CHECK(transport IN ('local','ssh')),
    ssh_target        TEXT,
    command           TEXT NOT NULL DEFAULT 'llmusage',
    added_at          TEXT NOT NULL,
    last_contacted_at TEXT,
    last_error        TEXT,
    import_watermark  TEXT
);
```

- `host_id` 是内部稳定标识，进入 `event_key` 前缀，注册后不可变。取 `label` 的规范化形式：小写、非 `[a-z0-9_-]` 字符替换为 `-`。与已有 `host_id` 冲突、或等于任一 `SourceKind::as_str()` 时拒绝注册，不自动改名。`host_id` 会写入事件键；与 source 名相同会使 `{host}:` 与 `codex:` 这类原键前缀无法区分。
- `label` 是用户可见名，用于 `--host` 与报表显示。
- 本地行由迁移写入：`host_id='local'`、`label='local'`、`transport='local'`。
- `command` 承载非登录 shell 的 PATH 问题（D3）。允许绝对路径或包装命令。
- `import_watermark` 是本地权威的增量边界（见 4.4）。

### 2.2 remote add 的探测与握手

`remote add <label> <ssh-target> [--command <path>]` 顺序：

1. 校验 `label` 规范化后不与已有 `host_id` 冲突。
2. 执行 `ssh <ssh-target> <command> --version`，超时后判为不可达。
3. 解析版本；执行 `ssh <ssh-target> <command> remote handshake`（新增隐藏子命令）取远端 `latest_schema_version()` 与 shard 协议版本。
4. 兼容规则：远端 shard 协议版本必须等于本地；远端 schema 版本不参与落库，仅用于错误信息中的诊断展示。协议版本不等即拒绝，错误信息给出两端版本与升级指引。
5. 全部通过后写入 `host` 行；任一步失败不写入任何行（R1.4 / AC4 / AC5）。

握手用独立子命令而不是解析 `--version` 文本：版本字符串是展示用输出，不构成契约；独立子命令返回 JSON，可稳定演进。

## 3. schema 迁移 v23

当前最新版本为 22（`store/migrations.rs:46-108`）。新增 `m_023_add_host_dimension`。

### 3.1 结构变更

- `CREATE TABLE host`，插入 `local` 行。
- `ensure_column` 追加 `host_id TEXT NOT NULL DEFAULT 'local'`：`usage_event`、`usage_turn`、`usage_tool_call`、`source_file`、`source_cursor`、`source_sync_status`。`usage_turn` 与 `usage_tool_call` 需要该列才能让 behavior 与 Activity 视图按主机过滤（见第 6 节）。
- 主键变更的四张表用建表复制法（与 `m_014` 的 `usage_bucket_30m__v14` 同形，`store/migrations.rs:672-710`）：`usage_bucket_30m`、`source_file`、`source_cursor`、`source_sync_status`。`usage_event` 的主键仍是 `event_key`，只需追加列与重写键值，不必建表复制。
- 索引：`usage_event(host_id, source, event_at)`、`source_file(host_id, source, state)`；保留既有索引。

### 3.2 event_key 重写（D2）

在同一迁移事务内按下列顺序执行，全部使用统一前缀 `'local:'`：

```sql
UPDATE usage_event_raw SET event_key = 'local:' || event_key;
UPDATE usage_event     SET event_key = 'local:' || event_key;
UPDATE usage_turn      SET turn_key  = 'turn:local:' || substr(turn_key, 6);
UPDATE usage_tool_call SET
    event_key = 'local:' || event_key,
    turn_key  = 'turn:local:' || substr(turn_key, 6)
  WHERE event_key IS NOT NULL OR turn_key IS NOT NULL;
UPDATE usage_tool_call SET
    tool_call_key = 'tool:' || source || ':local:'
                 || substr(tool_call_key, length('tool:' || source || ':') + 1);
```

要点：

- 前缀对所有行一致，主键唯一性天然保持，不需要冲突处理。
- `turn_key` 的 `turn:` 前缀长度不变，`substr(turn_key, 6)` 的 join 与表达式索引 `idx_usage_turn_event_key_expr`（`store/migrations.rs:764-765`）在重写后继续成立。
- `usage_tool_call.turn_key` 可为 NULL，`substr` 对 NULL 返回 NULL，需要 `WHERE` 或 `COALESCE` 保护，避免把 NULL 写成非 NULL。
- opencode 的 `tool_call_key` 形如 `tool:opencode:{key_seed}`（`parsers/opencode.rs:646`），与 behavior 派生形式共享 `tool:{source}:` 前缀，上面的 `substr` 表达式对两者都成立。
- `usage_bucket_30m` 不含 event_key，只在建表复制时补 `host_id` 并纳入主键。

### 3.3 reset_for_source 的 raw 删除

`Store::reset_for_source` 现用 `usage_event_raw.event_key LIKE '{source}:%'`（`store/schema.rs:262-270`），前缀化后失效。改为在删除 `usage_event` 之前先按子查询删除 raw：

```sql
DELETE FROM usage_event_raw
WHERE event_key IN (SELECT event_key FROM usage_event WHERE source = ?1 AND host_id = ?2);
```

因此 `reset_for_source` 需要新增 host 参数，并把 raw 删除提到 `usage_event` 删除之前。函数其余删除语句同样追加 `host_id` 条件（R2.6 / AC3）。

### 3.4 前缀施加点（R2.7）

parser 保持 host 无关。`commit_shard` 在写入前对 shard 内的键统一施加前缀：

- `UsageEvent.event_key` → `{host_id}:{key}`
- `UsageTurn.turn_key` → `turn:{host_id}:{key}`
- `UsageToolCall.event_key` / `turn_key` / `tool_call_key` 按同一规则改写

`SyncShard` 增加 `host_id: String` 与 `host_prefix_applied: bool`（默认 `false`）。`RawRecord.event_key` 同步处理。`commit_shard` 入口：若 `host_prefix_applied` 已为 true 则跳过改写；否则按字段规则改写一次并置 true。不要用 `starts_with("{host}:")` 检测：`turn_key` / `tool_call_key` 不以 `{host}:` 开头，且 `host_id` 一旦等于 source 名会误跳过 `event_key`。`host_id` 与 source 名的冲突由注册阶段拒绝（R1.6）。九个 parser 与远端二进制都不需要知道 host。

## 4. shard 传输契约

### 4.1 序列化

`SyncShard` 与 `RawRecord` 增加 `Serialize` / `Deserialize`（R3.2）。`raw_records` 标记 `#[serde(skip)]`，从协议层保证不跨机器传输（R3.4 / AC10）。

### 4.2 线格式

NDJSON，一行一条记录，复用 `sync --json-events` 的 stdout 先例：

```
{"kind":"header","shard_protocol":1,"llmusage_version":"1.2.0","schema_version":22,"emitted_at":"..."}
{"kind":"shard","shard":{ ...SyncShard... }}
{"kind":"trailer","sources":[{"source":"codex","events_seen":12,...}],"parse_issues":{...}}
```

- 本地解析器跳过非 JSON 行与未知 `kind` 并计数。第一条成功反序列化的 `ShardRecord` 必须是 header；`shard_protocol` 不匹配即中止导入、不提交任何 shard。登录横幅、`motd` 出现在 header 之前时走跳过路径，不触发「首条记录不是 header」失败（R3.6 / AC11 / AC5b）。
- 跳过行数由本地解析器持有，写入告警；不放进远端 trailer。
- trailer 形状为 `sources` 加 `parse_issues`（远端 parser 的解析问题）。trailer 缺失视为远端异常终止：已提交的 shard 保留（`commit_shard` 逐 shard 事务），但该 host 的 `import_watermark` 不推进。

### 4.3 远端侧命令

`llmusage sync --emit-shards [--since <RFC3339>]`：

- 不打开、不写入用户 `AppPaths.db_path`，不在用户库上取 worker lock，不推进用户库 cursor（R3.1 / AC13）。
- `--since` 复用既有 `recent_cutoff` 机制（`parsers/source_parser.rs:51` 的 `recent_cutoff` 参数），parser 已支持按 cutoff 过滤，不需要新增过滤逻辑。`recent_cutoff` 为 `Some` 时各 parser 会清空 shard 内 `cursors`（例如 `parsers/codex.rs:368-369`）；增量导入因此不刷新本地 cursor，watermark 仍是增量权威。
- shard 的 `cursors` 与 `seen_file_paths` 按 parser 产出输出，本地按 host 落库，用于 diagnostics 与 `missing` 判定。
- parser 的 `parse` 同时接收 `&Store` 与 `&mut SyncRunWriter`。`commit_shard` 不是唯一写入口：文件源调用 `mark_inventory_seen`（如 `parsers/codex.rs:269-273`），OpenCode / Zcode 调用 `save_opencode_cursor` / `save_zcode_cursor`（`parsers/opencode.rs:114`、`:305`；`parsers/zcode.rs:126`）。无 permit 的 `write_transaction` 会自动取 worker lock（`store/lock.rs:180-192`）。因此 collect-only 的 `commit_shard` **不够**。
- 机制分两层，且不改 `SourceParser` trait：
  1. `EmitParseStore`：不指向用户 `db_path`。cursor 读取返回空；`mark_inventory_seen`、`save_*_cursor` 与其它 `write_transaction` 为空操作，且不调用 `acquire_worker_lock`。parser 因此对每个候选文件做全量解析（空 cursor），`seen_file_paths` 仍进入 shard。
  2. `Store::begin_collect_run()`：`commit_shard` 把 shard 交给回调而不写 SQLite，分批与顺序协议保持不变。
- 断言必须同时覆盖两层：用户库连接上的 SQLite 写计数为 0；用户库 `schema_version` / 事件行 / `source_file` / `source_cursor` 与运行前相同（AC13）。只断言 collect-only writer 会漏掉 parser 经 Store 的写入。

远端不写用户库的后果：每次从零解析候选文件，没有 cursor 加速。`--since` 把扫描窗口收窄，代价可接受；如果后续实测过慢，可让远端保留自己的 `~/.llmusage` 并只做增量，但那会把增量权威从本地移到远端，本期不做。

### 4.4 增量边界（R3.5）

本地是唯一权威：

- `host.import_watermark` 记录上次成功导入的最大 `event_at`。
- 下一次请求 `--since (watermark - OVERLAP)`，`OVERLAP` 取固定 48 小时。重叠窗口内的重复事件由 `event_key` dedupe 吸收（`INSERT OR IGNORE`，`store/sync_writer.rs:316`）。
- 首次导入不带 `--since`，取全量历史。
- 只有收到 trailer 且全部 shard 提交成功后才推进 watermark。

## 5. 生命周期语义（R5）

### 5.1 missing 扫描按 host 限定

`SourceFileStore` 的 `sweep_missing`、`counts`、`tracked_paths`、`mark_inventory_seen`、`lossy_rebuild_risk`、`delete_for_source_in_tx`、`upsert_live_in_tx`、`update_missing_with_conn`（`store/source_file.rs:90-306`）全部追加 host 参数。

`driver.rs` 的扫描（`parsers/driver.rs:121-128`）只对本轮实际解析过的 host 执行。远端导入成功后对该 host 执行同样的扫描；导入失败或主机不可达则完全跳过（AC7）。

### 5.2 lossy rebuild 风险

`lossy_rebuild_risks`（`commands/sync.rs:807-819`）排除 `transport='ssh'` 且本轮未联系成功的 host 的 `missing` 行。本轮是否联系成功用内存中的 `RemoteRunOutcome.contacted` 判定，不用 `host.last_contacted_at` 与墙上时钟比较。`last_contacted_at` 只用于 `source-status` / `remote list` 展示。同一秒连续 sync 下时间戳比较不可靠，见 `common/util.rs:16-26`。调用点两处：`sync --rebuild` 守卫（`commands/sync.rs:793`）与自动 token-accounting 修复守卫（`commands/sync.rs:768`），都必须吃同一集合。

### 5.3 sync 编排

`run_once_locked`（`commands/sync.rs:443`）在本地 driver 完成后追加远端阶段：

1. 读取 `host` 表中 `transport='ssh'` 的行。
2. 串行处理每台：探测、拉取、逐 shard `commit_shard`、写 `source_sync_status(host_id, source)`、推进 watermark。
3. 单台失败：记录 `host.last_error`、发出 `SyncEvent::RemoteHostSkipped`、继续下一台，不改变整次 sync 的退出码（D5 / AC7）。
4. `remote sync [--host <label>]` 由 C2 实现为只走 importer、不跑本地 driver；C4 把同一 importer 接到 `run_once_locked` 的远端阶段，不重写该子命令。

`SyncEvent` 新增 `RemoteHostStarted` / `RemoteHostFinished` / `RemoteHostSkipped`，供 `--json-events` 与 dashboard job 展示。

### 5.4 source-status

`source-status` 是只读命令，走 `require_initialized()`，没有本轮 `RemoteRunOutcome`。主机状态只从持久化字段推导三态：

| 状态 | 判据 |
|---|---|
| `never_contacted` | `last_contacted_at` 为 NULL |
| `unreachable` | `last_error` 非空 |
| `idle` | 已联系成功且 `last_error` 为空 |

`live`（本轮联系成功且有新事件）只出现在 sync 的 `RemoteHostFinished` / `--json-events`，不进入 `source-status`（R5.3 / AC7d / AC7e）。

## 6. 读取层（R4）

- dashboard / explorer：`QueryFilter` 增加 `host_id: Option<String>`（`query/filter.rs:29-41`）。host 条件加在共用实现 `sql_filter_with_model_column`（`query/filter.rs:98-104`），这样 `bucket_filter` / `event_filter` / `turn_filter` / `tool_filter` 四条入口都会带上。不要只改包装函数 `sql_filter`，否则 `turn_filter` 不会带 host（AC8d）。
- CLI 报表：`ReportFilter`（`query/reports.rs:26-35`）增加 `host_id`。`ReportCommonArgs::to_filter`（`commands/report_args.rs:67-81`）把 `--host <LABEL>` 解析为 `host_id`；未注册 label 报错并列出候选。`push_bucket_filter` 与 `visit_filtered_events` 追加 host 条件。daily / weekly / monthly / session / blocks / focused 都走 `to_filter`，不走 `QueryFilter`。
- 每主机行沿用 `load_daily_reports_by_source` 的形状（`query/reports.rs:597`），新增 `load_daily_reports_by_host` 等对应函数（R4.3）。该组函数不替代 `--host` 过滤。
- dashboard 新增独立主机分组 payload 字段，不改造现有 source 分组（D4）。遵守 `dashboard-performance-contracts.md` 的 payload 与查询预算约束。

## 7. 兼容性与回滚

- 迁移不可逆。v23 前的库升级后 `event_key` 全部带 `local:` 前缀；降级到旧二进制会命中 `SchemaTooNew`（`store/schema.rs:41-46`），不会静默错读。
- 升级前自动备份：现有机制只在 v0 老库升级时备份（`store/schema.rs:124-126`、`store/schema.rs:310-322`）。v23 因为重写主键，在 `Store::bootstrap` 里、打开 v23 迁移事务之前，若磁盘库 `schema_version == 22`，则先 `PRAGMA wal_checkpoint(TRUNCATE)` 再复制到 `backups/llmusage.db.pre-0.23-host`（文件已存在则不覆盖，与 pre-0.5.0 同形）。`MigrationFn` 只接收 `&Transaction`（`store/migrations.rs:13-14`），运行器在 `BEGIN IMMEDIATE` 之后才调用迁移（`store/migrations.rs:209-214`），备份不能放进 `m_023`。内存库 / `run_migrations_for_test` 不走 `bootstrap` 备份分支，备份失败不得中止内存迁移。磁盘升级路径备份失败则中止，不进入 v23。
- `remote remove` 默认保留该主机的用量行（R1.5）。清除路径复用 `reset_for_source` 的 host 版本，并要求显式确认。

## 8. 权衡与否决

- **否决 sshfs 挂载加环境变量：** Claude 无环境变量覆盖（`parsers/source_files.rs:62-67`）；project 归属依赖本地工作树；SQLite over sshfs 不可靠。
- **否决拉取原始会话文件到本地解析：** project 归属仍错误，且把 prompt 正文复制到本地磁盘，相对现有就地只读姿态是退步。
- **否决每主机独立 DB 加读取时 ATTACH：** 读取层改动量与本方案相同，但引入跨库 schema 版本与 pricing 版本一致性问题，且 DB 文件可能含 raw archive。
- **否决只给远端加 event_key 前缀：** 迁移风险更低，但用户选择全局统一前缀以保持语义一致（D2）。
- **否决引入 `russh` 等 SSH 库：** 需要自行处理 host key 校验、密钥与 agent，依赖树显著变大。调用系统 `ssh` 直接继承用户的 ssh config、ProxyJump、agent 与 known_hosts，并与文档既有的 SSH 使用方式一致。

## 9. 需要更新的 spec 契约

- `token-accounting-contracts.md`：Codex 逻辑身份从 source-scoped 变为 host 加 source 作用域；说明同一份产物在两台已注册主机上会计为两条。
- `source-sync-contracts.md`：新增远端 host 阶段、`SyncEvent` 新事件、`source_sync_status` 的 host 维度、`source-status` 的只读三态（`idle` / `unreachable` / `never_contacted`）。`live` 只出现在 sync 事件。
- `write-fencing-contracts.md`：明确远端导入走 `commit_shard` 与 fenced Store，不构成新的控制面例外。
- `report-cli-contracts.md`：`--host` 参数与每主机行的 JSON 形状。
- `dashboard-performance-contracts.md`：主机分组 payload 字段。
