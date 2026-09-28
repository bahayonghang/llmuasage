# Research: 写入阶段测量与生成式 replay 基准

- Query: 定位 writer 私有测量接缝，给出同输入、同初态的合成 replay A/B 方案；不实施候选优化。
- Scope: internal；只读当前源码及父任务现有研究，仅写本任务 research/。
- Date: 2026-09-28
- Status: 方案已核对源码；未编译、未运行基准。当前 Cargo.toml:3 为 2.0.0，版本调整由另一代理负责。

## Findings

### 1. 实施结论与实际阶段边界

优先在 `src/store/sync_writer.rs` 增加私有、测试范围内的阶段观测和 ignored 基准。先运行现有算法取得占比，再为一个已证实热点引入候选。公共 `SyncShard`、`ShardCommitStats`、`SourceSyncStats` 和 CLI 参数无需变化。105.2 秒原始 WRITE 尚不能归因到某条 SQL、定价、行为写入或磁盘。

| 接缝 | 精确位置 | 测量和保持要求 |
| --- | --- | --- |
| writer 建立 | `src/store/sync_writer.rs:122-153` | 包括连接、raw 开关、pricing catalog；不在普通 shard WRITE 内，计入基准总耗时 |
| shard 前处理 | `src/store/sync_writer.rs:794-809`；`:1144-1190` | host prefix 和行为 key 去重在 WRITE 起点之前；分别计时，不并入 PARSE CPU |
| provider / operation / BEGIN | `src/store/sync_writer.rs:810-841` | provider 填充、可选临时操作锁、BEGIN IMMEDIATE、初次 permit 校验、OMP 迁移分开；BEGIN elapsed 包含 SQLite 等待和进入事务的工作 |
| reset events / pricing | `src/store/sync_writer.rs:178-364`，pricing 调用 `:355-361` | 保留路径去重、扣减 bucket、删除旧 event、剩余 bucket 定价恢复；把 pricing 从 reset 总时长中分离，避免重复累计 |
| events / pricing / buckets | `src/store/sync_writer.rs:367-485`；`:885-887` | 每 1000 events 调一次批写，全部使用同一个 tx；定价在 INSERT OR IGNORE 之前，只有实际插入者进入聚合 |
| cursor / inventory / raw | `src/store/sync_writer.rs:890-918` | SQL/file cursor、source_file live、可选 raw 各计一段 |
| behavior reset / turns / tools | `src/store/sync_writer.rs:922-939` | 在同一事务中按现有顺序执行，计数含原始与内存去重后条数 |
| fence / COMMIT | `src/store/sync_writer.rs:852-856` | 二次 permit 校验与 tx.commit 分开，原有 write_ms 截止点不移动 |
| writer 收尾 | `src/store/sync_writer.rs:568-570` | finish 消费 writer；连接析构可能发生在 shard WRITE 外，纳入总耗时 |

**实际普通 shard 顺序为** event reset → events/pricing/buckets → cursor → source_file → raw → behavior reset → turns → tools → 二次 fence → commit。不要因为阶段名称的概括排列而重排代码。1000 是同一事务内的事件批次大小；`:30` 的注释不准确，不能据此拆成多个事务。

**Antigravity 使用不同的旧计时范围**：`:645-698` 完成全组前处理、BEGIN 和全部 owner reset；`:700-720` 的每源 write_ms 仅测该源 apply_shard_tx 与 marker；共同 fence/commit 在 `:723-724`，未计入各源 write_ms。其 profile 应有独立 transaction_total 和逐源 apply，不把分源 write_ms 之和解释为全事务时间。此次研究不建议顺带改变公共计时语义。

### 2. 最小私有观测方案

建议分两次实施，避免先制造候选结论。

1. 在 writer 模块内定义测试私有 `WriterProfile` 和作用域 collector，采用单调 `Instant`/`Duration`，保留纳秒数；只在 `#[cfg(test)]` 构建启用。collector 以每个 shard/transaction 一条记录输出，不逐事件输出，不输出正文、路径、原始 key 或用户 hash。现有 `AFTER_COMMIT_SHARD` hook（`:82-119`）只能借鉴 guard 恢复模式；不要用该 hook 做计时，因为启用时 `:588-590` 会克隆整个 shard，污染时间与内存。
2. 第一轮只测互斥大阶段：prefix、behavior_dedupe、provider、operation、begin、fence_before、migration、reset_events、reset_pricing、event_batch_total、cursor、inventory、raw、behavior_reset、turns、tools、fence_after、commit。event_batch_total 中如需深查 pricing/INSERT/project_flush/bucket_flush，再开探索性子计时；子计时不与父阶段求和。
3. `write_unclassified = precise_write_elapsed - sum(exclusive_write_stages)`；用 checked subtraction 发现边界错误，不用饱和减法隐藏重叠。另报 preprocess_total、writer_begin、writer_finish、run_total。计时器与 profiler bookkeeping 未覆盖的开销留在 unclassified。整数 `write_ms` 每 shard 截断，校验其与精确 WRITE 的误差小于 shard 数毫秒。
4. 同时保存结构计数：候选/实际插入 events、reset paths、批次数、cursor/seen path 数、原始/去重后 turn/tool 数、raw 数、transaction 数。reset 删除行数/touched bucket 数由现有结果累加，不为获取计数额外扫描全表。
5. 用同一基准比较 profile 开/关开销；最终 P4 使用关闭细粒度 profile 的轮次。精确外层 WRITE 的两个 clock read 在 A/B 两侧一致。测试仅观测层不修改正常产物 ABI，也不引入产品 CLI 开关。若未来需要用户诊断日志，另在既有 tracing 下评审输出契约。

`ShardCommitStats` 公开形状位于 `src/store/mod.rs:1161-1173`；无需添加字段。事务内部函数私有，阶段参数或测试 collector 仍由 writer 拥有。`rusqlite` 已有 trace feature（Cargo.toml:74），暂不需要新依赖或 statement-level tracing；每条 SQL 的 tracer 会引入额外开销。

### 3. A/B 选择方式

- 先只添加 profile 与 fixture，记录原算法的阶段结果。**尚未选定候选，不能预先加入缓存、索引或批次更改。**
- 热点确认后，在同一个 release 测试二进制中，用 `#[cfg(test)]` 的私有 `WriterBenchmarkVariant::{Baseline, Candidate}` 及作用域 guard 选择准确热点函数。Baseline 保留优化前该私有函数的原实现；正常非测试构建直接调用 Candidate。只在每批/阶段入口判断一次，不在每条 event 外加模式查询。
- 测试默认走 Candidate；Baseline 只用于 ignored 基准和等价性 oracle。不得通过环境变量、公开参数、新 feature 或公开 DTO 选择算法。模式 guard 在错误和 unwind 时恢复，禁止并行基准。若改动只是索引，则像现有基准一样在各独立副本中选择 schema；必须额外审阅 migration/query-plan 面。
- 第一阶段为 writer 单元基准。完整 parser 控制场景可在同一个 lib 测试内用 `SourceParser::sync` 和单线程 Tokio runtime 调用 Codex/Claude，以便复用私有 test guard；parser 的 blocking worker 只解析，writer 仍在调用线程。不要为了跨 integration-test 边界公开测试 API。现有 `tests/sync` 测试继续验证真实执行路径。

### 4. 可复用设施与生成负载

已有 helper：`build_event`/`build_cursor`（`:1490-1528`）、`build_tool_call`/`build_behavior_turn`（`:1887-1920`）、带全部写表面的 replacement shard（`:1923-1947`）。现有 index 基准（`:1950-2116`）使用 TempDir、fenced Store、8×500 event、7 轮交替 A/B 和 median；**仅覆盖无 reset/behavior 的 insertion**。

新增生成器使用固定 seed `0x20260928`，通过明确的整数序列生成 id、token、项目和时间，不用随机 UUID、当前时间或 HashMap 遍历来确定输入顺序。按 `(project, file, event_index)` 排列，固定 UTC event_at。扩展 helper 时填五个 token 通道、authoritative total、项目、session 和 source_path_hash。使用测试已确认的 `gpt-5`、`claude-sonnet-4-5`，另设 unknown model、priced tier、source-reported-cost 的小型正确性夹具。不要仅测全部 None 的 project 或全部零 cache。

| 场景 | 初态 / 输入 | 目的 |
| --- | --- | --- |
| 插入控制 | 空库；复用 8 shards×500 events，无 behavior | 与既有基准同规模，观察普通导入退化 |
| **主要 Claude replay** | 初态 10 项目×10 文件×500=50000 events；改变 4 项目，reset 40 paths，重放 20000 旧 events 并新增 1000，合成一个 21000-event shard | 对应 bounded parallel batch 为 4 的项目重放形状；主 P4 场景提前固定 |
| 历史放大控制 | 初态 50 项目×10 文件×500=250000；保持上述 40 reset paths/21000 输入不变 | 测剩余历史规模对 reset/pricing 的影响 |
| Codex append 控制 | 初态 250000，输入 4000 新 events，8 shards；无 reset | 覆盖原问题的较大历史规模和增量路径 |
| 重复/behavior 压力 | 初态 50000；20000 candidates，其中90%为已有 key；每 event 一 turn、三种不同 tool key，输入另附25%相同 behavior key | 压力分布明确标为合成；不能用原 COMMITTED/SEEN 推导该比例 |
| shared-bucket reset | 两文件共享 provider/model/project/半小时 bucket，仅 reset 一文件，保留另一文件的 tier/pricing | 放大 reset-pricing，检查剩余桶 metadata；先小型 oracle，再按规模扩展 |
| hot/parser 控制 | 固定源树先导入，再原样同步；另测单 Codex 文件追加、Claude 一个项目变化 | 记录全 source wall、changed、scanned bytes、WRITE，确认没有通过少读必要数据获得提升 |

主要 replay 的新旧唯一 events 各有一 turn、三条不同 tool calls；初态 50000 turns/150000 tools，输入 21000 turns/63000 tools。所有倍数直接写入输出 manifest。增加 raw 开启、两个 host 同 key、SQLite cursor 的小型正确性控制；大性能默认 raw 关闭。250000 为合成历史放大规模，不声称重现真实数据分布。

Parser 夹具参考 `tests/sync/main.rs:774-843`（完整临时 HOME/env 恢复）、`:845-926`（Codex seed/append/replace）、`:1090-1119`（Claude 项目文件）；`tests/sync/sources/codex_claude.rs:218-338` 已验证变更项目重放且其它项目跳过。避免直接搬一个 Fixture 到产品层。

### 5. 完全相同初态与计时外准备

1. 所有源文件、seed Store、算法输入在计时外生成。每个 workload 创建唯一 immutable seed。seed 由正常 fenced writer 创建；确认所需 accounting marker、host、inventory、cursor 均已存在。源文件固定在同一 TempDir 下，A/B 不能因为不同绝对源路径产生不同 identity。
2. 关闭 seed writer、heartbeat、lock 与连接后，以新连接执行 `PRAGMA wal_checkpoint(TRUNCATE)`，确认成功并关闭连接；再复制稳定主 DB 到每轮独立目标目录。不要复制运行中的 `.db` 而遗漏 WAL。每轮记录源 seed 文件 SHA256，必要时同时比较规范化表快照；copy/生成/bootstrap/预热均在被测区间外。无须开启 rusqlite backup feature。
3. 每轮使用全新数据库副本、完全相同预生成 shards 的 clone。clone 在计时前完成。获取 permit/heartbeat 也在计时前；所有轮次采用相同配置。主要 writer_total 从 begin_sync_run 前到 finish_sync_run 返回后。精确 WRITE 覆盖现有 `:809-856`，preprocessing 单列。
4. 每场景先各做一次 A、B 预热并丢弃，随后至少7组：AB、BA、AB、BA、AB、BA、AB。每次都从 immutable seed 恢复，不用上轮结束库。记录缓存状态为预热的 OS 缓存；空库导入不等同于物理磁盘冷缓存。
5. 记录全部原始 samples、median、min/max、IQR 或 MAD，并记录 paired ratio。主要验收为 `median(candidate WRITE)/median(baseline WRITE) <= 0.80`；**每个**控制场景的 `median(candidate total)/median(baseline total) <= 1.10`。不得挑最快轮或事后更换主场景；不达标保留任务未完成并撤回无收益候选。
6. 环境记录 OS/CPU、磁盘型号和介质、profile、Rust/SQLite版本、schema版本、PRAGMA、并行度、raw/provider/pricing 设置、代码来源及工作区补丁摘要。变更 batch/cache 时同时记录最大 retained events/估计 bytes 或进程 peak RSS；输入规模不随算法变化。

### 6. 持久等价性与事务验证

按各表主键排序，比较所有被 writer 触及的列：`usage_event`、`usage_bucket_30m`、`project_dim`、`usage_turn`、`usage_tool_call`、`usage_event_raw`、`source_cursor`、`source_file`、`meta`、`host`；另外确认未参与的 source/host 以及 run_log/source_sync_status 等不受影响。事件键、五通道 token、total、每事件 cost、pricing_status/source/rate、bucket token/count、behavior link/key、cursor offset/fingerprint、live/missing 状态和 accounting marker 必须一致。计数相同不是充分证明。

writer 目前使用 wall-clock audit 时间（`:268,389,1030,1078,1118,1260,1336`），writer.run_started_at 另由毫秒 UTC 产生（`:148`）。建议在**writer 测试内部**增加作用域固定 audit clock，并在测试构造后设定固定 run_started_at；不要冻结 worker lease 的全局时钟。这样可以全列比较 writer 表，不必泛化忽略所有 timestamp。drop heartbeat 和 lock 后 worker_lock 应为空；协调 owner/generation 不作为业务内容差异。时钟 seam 默认行为必须仍调用现有 util。

SQLite REAL 成本遵循现有 `1e-9` 容差（`:3132`），整数/字符串/NULL 完全相等。每个浮点差异记录绝对误差，不能把 token 或 status 差异纳入容差。主键缺失、多余行、表/schema变化立即失败。正确性 snapshots、SHA256 和第二次幂等 replay 全部在计时外完成。幂等允许 replay 再次插入已 reset 的行；要求最终状态相同，不能断言 COMMITTED=0。

已有必复用检查：所有8个 failpoint rollback（`:2252-2300`），共享 bucket pricing 与单 source scan query plan（`:2706-2811`），host prefix once（`:3195-3237`），SQLite cursors 与 events 原子（`:2391`），generation fencing（`:2120`），Antigravity 跨源/marker rollback（`:1669-1700`）。取消是 parser 边界协议，见 `tests/sync/runtime/jobs.rs:79-165` 和 Codex/Claude cancel 检查；不要为基准改成事务中途提交。

### 7. 建议命令与交付顺序

以下命令由实施代理在获准使用编译资源后运行；本研究没有执行。新增测试名先写入代码，再用 `--list --exact` 确认匹配一项，防止零测试被误报通过。

```powershell
# 已存在的参考基准；只检验测量设施，不替代 replay 验收。
cargo test --locked --all-features --release --lib store::sync_writer::tests::activity_cost_index_sync_throughput_regression_stays_within_ten_percent -- --exact --ignored --test-threads=1 --nocapture
cargo test --locked --all-features --release --lib store::sync_writer::tests::top_sessions_cover_index_sync_throughput_regression_stays_within_ten_percent -- --exact --ignored --test-threads=1 --nocapture

# 建议新增入口；第一项仅原算法 profile，第二项为完整7组A/B矩阵。
cargo test --locked --all-features --release --lib store::sync_writer::tests::sync_writer_replay_profile -- --list --exact
cargo test --locked --all-features --release --lib store::sync_writer::tests::sync_writer_replay_profile -- --exact --ignored --test-threads=1 --nocapture
cargo test --locked --all-features --release --lib store::sync_writer::tests::sync_writer_replay_ab_acceptance -- --list --exact
cargo test --locked --all-features --release --lib store::sync_writer::tests::sync_writer_replay_ab_acceptance -- --exact --ignored --test-threads=1 --nocapture

# 聚焦正确性，再运行正式门禁。
cargo test --locked --all-features --lib store::sync_writer::tests -- --test-threads=1
cargo test --locked --all-features --test sync -- --test-threads=1
cargo test --locked --all-features --test store -- --test-threads=1
cargo test --locked --all-features --test remote -- --test-threads=1
python scripts/ci-rust.py
```

基准输出建议每轮一条 JSON，包含 workload/seed/scales/variant/round/order、精确 WRITE 与 total/stages、计数、完整状态 digest，最后输出统计与阈值结果。原始计时保存到本任务 research/；大型临时 DB、输入文件和原始资料仅在 TempDir。运行前用 `rustc -Vv`、`Get-CimInstance Win32_Processor`、`Get-CimInstance Win32_OperatingSystem`、`Get-PhysicalDisk` 获取环境；SQLite版本和 PRAGMA 在 benchmark 连接中查询，避免记录不同 SQLite 二进制的版本。

实施顺序：profile + deterministic fixture → 原算法基线及热点证据 → 一个候选与同二进制 A/B seam → 全状态等价/回滚 → 7组交替性能 → 独立审阅。共享产品修改继续等待前序 diagnostics 和 Antigravity preflight 完成。

## Files Found

- `src/store/sync_writer.rs`：writer 协议、定价/reset、行为/游标、existing ignored throughput tests、failpoint。
- `src/store/mod.rs:748-783,786-848,1161-1173`：writer 私有字段与公开 payload/stats；公共形状不需修改。
- `src/store/connection.rs:26-56`：30秒 busy timeout、WAL/NORMAL/foreign_keys/temp_store。
- `src/store/source_file.rs:273-310`：按 run_started_at 的库存 live 更新，不能省略。
- `src/parsers/claude.rs:233-279`：多个解析项目共享一个 bounded-batch shard；project replay 不能改 append。
- `tests/sync/main.rs`、`tests/sync/sources/codex_claude.rs`、`tests/sync/runtime/jobs.rs`：隔离输入生成、incremental/project replay 和取消边界。

## External References / Related Specs

- 复用父任务 `research/performance-analysis.md` 与 `research/upstream-comparison.md`。参考锚点 ccusage `732c7a6362f3d86a4992d2ad7071b6532161a396`、tokscale `1d9a9395418efc6952944b794097935d7d6fa1e8` 来自既有已更新研究，未重新 pull 或运行上游。
- tokscale `message_cache.rs:2582-2606,2675-2702,2934-2955` 的受影响集合和提交成功再清 dirty 原则可借鉴；bincode cache、全前缀 hash 与 llmusage SQLite materialization 不构成等价速度对照。
- `.trellis/spec/llmusage/backend/source-sync-contracts.md:51-57,90-93`；`write-fencing-contracts.md:9-21,25-44,75-93`；token-accounting-contracts；ADR0002；ADR0017。

## Caveats / Not Found

- 本研究未执行 Cargo、真实 sync/rebuild、用户库读写、安装或 Git 操作；未编辑产品代码、spec 或其它任务。
- 没有测得 writer 热点或20%收益；以上 workload/scales 是明确的待执行方案。
- 未发现现成的 writer 固定 audit clock 或全表等价性 helper；这两项需要最小测试实现。现有 index 基准只比较 event_count，不能直接当 P3 oracle。
- 精确源码行号可能因并行工作变化；实施前只核对相关函数，不需重复广泛研究。
