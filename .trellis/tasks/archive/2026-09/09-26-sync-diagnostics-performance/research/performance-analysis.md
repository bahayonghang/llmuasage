# Research: sync 性能、计数与写入协议

- Query: 分析 2026-09-26 sync 的 Codex、Claude、总计耗时与计数；追踪 SourceSyncStats、SyncShard、reset、dedupe、bucket、behavior 和计时边界，给出安全优化方向、基准设计与验收条件。
- Scope: internal；只读源码研究，引用主会话的只读运行状态探针。
- Date: 2026-09-26
- Source version: 主会话核实安装二进制自报 1.3.0；当前源码 HEAD 为 29bde59e84e1149b1d9215e020352586fb0a0ed7，Cargo.toml:3 为 1.4.0。安装二进制的构建提交未确定。本文的代码事实限定当前源码，不能证明全部已存在于运行 1.3.0 的二进制。

## Findings

### 1. 本轮可核实的量与优先级

主会话保存的 research/local-state-probe.json 与 research/local-state-analysis.md:5-25 核实 source_sync_status 更新时间为 2026-09-26T08:33:53Z，计数与用户终端输出一致。研究子会话未重复读取真实数据库。

| 范围 | SEEN | COMMITTED | STORED | BYTES 原始计数 | PARSE ms | WRITE ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Codex | 21737 | 4045 | 267824 | 1341993624 | 6435 | 39553 |
| Claude | 35738 | 16277 | 51033 | 760717760 | 2851 | 63646 |
| 全来源 | 59472 | 22319 | 366201 | 2710299261 | 20375 | 105193 |

- 两源 WRITE 为 103199 ms，占全部 WRITE 的 98.1%；应先细分两源 writer 的成本。
- 全来源 PARSE + WRITE 为 125568 ms，WRITE 占 83.8%。该占比支持优化排序，不能证明磁盘、fsync、索引或某条 SQL 是主要原因。
- run_log 的 started_at=08:31:46Z、finished_at=08:33:53Z、duration_ms=126999，给出约 127 秒的运行基线。时间戳来自秒精度 now_utc；duration_ms 是 julianday 差乘 86400000 后转整数，不能作毫秒精度 profile。依据：src/common/util.rs:12-14；src/store/run_log.rs:35-65。
- 126999 与 125568 相差 1431 ms；该差值包含时间粒度误差与不同计时边界，不能精确归因为额外 SQL 或其它单一开销。
- 用户日志显示 worker lock 成功获取，等待 58 ms；该等待不足以解释约 127 秒。源码中的后续 SQLite BEGIN/commit 等待尚未单独测量。
- 命令前提示符中的“⏱ 6s”不是本轮 sync 的计时证据。

### 2. 计数语义与不能作出的推断

SourceSyncStats 的字段定义在 src/parsers/mod.rs:245-282；终端列映射及 TOTAL 累加在 src/commands/sync_summary.rs:18-29,118-144。

| 列/字段 | 当前实现的语义 | 本轮解释边界 |
| --- | --- | --- |
| FILES / files_processed | 被考虑的候选文件或数据库数 | 不等于导入行数，也不是统一的“文件读取完成数” |
| CHANGED / changed_files | 来源定义的解析工作量；Codex 是触发文件数，Claude 是成功提交批次中重放的文件数 | Claude 307/400 不能解读为 307 个文件的元数据均发生变化 |
| SKIPPED / skipped_files | 未要求重新解析的候选数 | 与 parse issues 的 skipped 行数无关 |
| SEEN / events_seen | 未经逻辑/SQLite 去重的规范化候选事件数；各 parser 计数位置不同 | 不等于原始 JSONL 总行数，也不等于工具调用数 |
| events_replayed | 已有路径重新解析产生的候选数 | Claude 本轮 24589；不能由该量计算净新增事件 |
| COMMITTED / events_inserted | 此轮 INSERT OR IGNORE 成功插入的 usage_event 数 | 包含 reset 删除后的重新插入，不能解读为数据库净增长 |
| STORED / stored_events | 运行末尾查询的来源 usage_event 总数 | 保留的缺失文件/旧口径历史仍在其中；当前 SQL 按 source 计数且未限制 host |
| BYTES / bytes_scanned | Codex/Claude 累加 snapshot.file_size-start_offset | 是解析计划范围的字节数；不含所有 fingerprint/replay-detection 附加读取，也不测物理磁盘 I/O |

证据：Codex 计数 src/parsers/codex.rs:280-297,372-401,414-429,505-523；Claude src/parsers/claude.rs:143-191,250-279,291-306,388-411；STORED 查询 src/sync/engine.rs:299-303,485-500。

Codex COMMITTED/SEEN 为 18.6%，Claude 为 45.5%。两个比例都是候选到成功插入的比值，不能标成“重复率”或“丢失率”。Claude 先在 parser 合并流式/sidechain 候选；writer 还按 event_key 忽略已存在行。reset 与跨文件重复共同影响比值。

Antigravity IDE 的 BYTES 采用数据库逻辑大小累加，来源语义不同；整张表 2.5 GB 不适合直接除以 PARSE 来报告磁盘吞吐率。IDE 的具体保护与短路机会见同目录 local-state-analysis.md:37-59。

### 3. PARSE、WRITE 与总运行边界

1. Driver 按注册顺序串行执行来源，源码不存在 Codex 与 Claude 并行写同一个 Store 的路径。每个 parser 内可以并行解析分片；writer 串行提交。依据：src/parsers/driver.rs:79-99,124-152；src/parsers/codex.rs:321-353；src/parsers/claude.rs:201-233。
2. Codex/Claude 都在发现文件之前启动 parse_started，在返回前使用 total_elapsed.saturating_sub(sum(commit.write_ms)) 作为 PARSE。因此 PARSE 包含目录发现、inventory 写库、cursor 查询、stat/fingerprint、任务调度与等待、解析和一些 writer 预处理。依据：codex.rs:264-290,428-429；claude.rs:124-148,305-306。
3. Codex/Claude 的 inventory 通过独立的 fenced write_transaction 更新全部候选 source_file。这部分 SQLite 写入归入 PARSE。未变更路径必须标记本轮观察到，避免 missing sweep 把它们误报为丢失。依据：src/store/source_file.rs:132-152,281-309。不能通过省略该更新来制造 hot sync 加速。
4. commit_shard_inner 先 apply_host_prefix、dedupe_behavior_facts，再在 src/store/sync_writer.rs:809 启动 WRITE 计时。前两项及开始日志的成本落在 parser residual 中。计时覆盖 provider label 补充、可选操作锁获取、BEGIN IMMEDIATE、permit 校验、SQL 阶段、再次校验、tx.commit；结束于 :856。
5. PARSE/WRITE 之外还有 driver 的 missing sweep、来源最终计数查询、accounting marker/status 写入、remote import、汇总等工作。依据：src/parsers/driver.rs:164-184；src/sync/engine.rs:297-442,470-475。当前 stored_query_ms 的计时范围还跨过 remote import，名称不能证明只测 COUNT 查询（engine.rs:297,399-419）。
6. CLI run_tracked 的起点在 lock、bootstrap、run-log recovery 后，结束在 reporter 收尾与最终表格输出前。依据：src/commands/sync.rs:182-210,234-250；src/sync/engine.rs:25-49。run_log 约 127 秒不覆盖整个 CLI 进程生命周期。
7. Driver 将同一个 lock_wait_ms 复制给每个 source（driver.rs:153-154）。如新增总计展示，只能计一次，不能按来源求和。

### 4. Claude 全文件重放与正确性

当前源码按 projects 下第一层目录分组；任何组内文件需要 rescan，便把该项目所有现存文件纳入计划。计划将 CandidateFile.existing 置 None，确保从 offset 0 重读；已有路径进入 reset_path_hashes。未变化项目跳过，缺失历史路径不出现在现存文件计划中。依据：src/parsers/claude.rs:141-191,322-329。

重读与以下现存逻辑有关：

- 以 message.id + requestId 匹配；存在 sidechain 时允许 message.id 跨 request 匹配：claude.rs:582-607。
- 优先保留非 sidechain 的元数据，合并各 token 通道最大值；最终 total 至少覆盖 input/cache/output 组成项：claude.rs:637-686。
- cross-request sidechain 可令最终 event_key 从 message+request 改为 message-only：claude.rs:603-605,619-633,689-705。
- writer 对已有 event_key 执行 INSERT OR IGNORE，不会自动把此前 streaming 小值替换为后续大值：sync_writer.rs:393-400,421-469。

因此只把 Claude 改成文件尾 append 会在既有协议下漏掉 streaming 更新，或让后出现的 sidechain 与旧逻辑键重复。安全的局部增量方案需要持久化候选状态、逻辑键迁移与 bucket/behavior 更新协议；该方案超出本次最小性能优化范围。保留当前项目重放边界。

当前已有两个优化，规划不得将它们写成待实现的新功能：

1. 只重放变更项目；其它项目不重读。测试 tests/sync/sources/codex_claude.rs:218-338 断言只重放 2/3 文件、字节数等于变更项目大小。
2. 同一个有界并行 batch 的多个项目已经合成一个 SyncShard 提交：claude.rs:233-279。default parallelism 为 available_parallelism 上限 4（src/sync/types.rs:95-99）。增加 parallelism 同时改变项目 batch 形状，不能预先宣称能缩短 WRITE。

Codex 当前已经按持久化 cursor 选择 Append/Reparse，使用旧 offset 附近的签名判断安全追加（file_state.rs:49-103），测试 codex_append_scans_only_changed_file 明确断言只扫描追加范围（tests/sync/sources/codex_claude.rs:125-175）。因此 Codex 1.2 GB 不能单凭数值判定为“缺少增量读取”；需先测 append、new-file、replace、partial-tail 各自的数量与字节。

### 5. SyncShard 的事务与已存在的成本

SyncShard 同时携带 reset、events、cursors、seen_file_paths、raw、turns、tool_calls 及 OpenCode/ZCode 游标，定义见 src/store/mod.rs:786-848。ShardCommitStats.events_inserted 与 write_ms 仅代表 usage_event 插入数和整个 commit 范围时间，另有 turns_inserted/tool_calls_inserted（mod.rs:1161-1173）；CLI 没展示行为行数。

Claude 在每条候选解析时生成 turn/tool facts（claude.rs:545-553），之后才在项目结果中合并 usage events（:398-411）。行为表采用自身的 stable key 去重，不能要求行为行数等于逻辑 usage_event 数。优化验收应逐表比较既有 key/内容语义，不能顺带改变行为口径。

单 shard 使用一个 BEGIN IMMEDIATE 事务。EVENT_WRITE_BATCH_SIZE=1000 仅把事件切为循环内批次；各批共享同一个 tx，未进行每 1000 条一次独立 commit。常量注释的“单 transaction 最大事件数”与当前调用边界有偏差，不能依靠该注释推断事务数。依据：sync_writer.rs:30-35,839-853,885-888。

| 阶段 | 已验证的源码动作 | 验证定位 |
| --- | --- | --- |
| Reset events/buckets | 每个 unique path 查询旧事件聚合，逐旧 bucket 扣减 token/cost/count，删除空 bucket 与旧事件 | sync_writer.rs:178-364 |
| Reset pricing | temp.llmusage_reset_bucket 去重 touched keys；一次 source-range event 扫描 join temp keys，恢复剩余 bucket 的 pricing metadata | sync_writer.rs:37-55,1366-1469 |
| Insert events | 每个候选先算 pricing 再 INSERT OR IGNORE；只有 changed>0 才进入 project/bucket 内存聚合；每个 1000 条批次 flush | sync_writer.rs:367-485 |
| Cursor 与库存 | 同一事务写 file/SQLite cursor，再写该 shard seen paths 的 source_file | sync_writer.rs:890-910 |
| Raw | raw_archive_enabled 且有 raw_records 才写；当前 Codex 明确传 Vec::new，Claude combined shard 也未填 raw | sync_writer.rs:912-917；codex.rs:389-397；claude.rs:233-259 |
| Behavior reset | 构建临时 path key 表，集合删除 usage_tool_call 与 usage_turn | sync_writer.rs:965-1009 |
| Behavior insert | 对预去重的 turns/tool_calls INSERT OR IGNORE | sync_writer.rs:930-939,1012-1104,1182-1190 |
| Commit/fencing | 事务前后校验 generation/lease，最后 commit | sync_writer.rs:839-853；src/store/lock.rs:183-198 |

由源码可以确认以下成本存在：候选级 pricing 与 SQL、reset 的旧聚合读写、行为表更新、cursor/inventory 更新和事务提交。现有本轮日志没有各阶段毫秒数、受影响行数、SQL query plan 或 WAL/checkpoint 数据，因而尚不能排出各项占比。

现有 SQLite 设置是 WAL、synchronous=NORMAL、foreign_keys=ON、temp_store=MEMORY，默认 busy_timeout=30 秒（src/store/connection.rs:26-56）。不能再把“启用 WAL/事务批写”列为新优化；也不能从 30 秒 timeout 推断实际等待。关闭约束或持久化保证不在建议范围。

行为 reset 已使用集合 SQL，并已有 (source, source_path_hash) 索引（src/store/migrations.rs:794-819）。pricing refresh 已有单 source scan + temp bucket join 的回归测试（sync_writer.rs:2706-2811），不能把“每个 bucket 一次全来源扫描”当作当前未修复问题。

### 6. 待测假设与最小改进范围

以下项目按测量价值排列，全部是待验证假设，不是本轮已证明的性能原因。

| 假设 | 需要取得的证据 | 若成立的最小改进方向 |
| --- | --- | --- |
| 项目重放带来的 reset/重复写占较大比例 | 每 shard reset_paths、旧删除行数、候选/去重后/成功插入行数、reset/pricing/behavior/commit 的独立计时 | 在 writer 内优化已测出的阶段，保留项目候选完整性 |
| 高重复率候选仍做不必要 pricing | event_count、SQL ignored_count、pricing 独立计时与价格 tier/cache 情形 | 评估只在确定可插入时计算价格，或局部价格 lookup 复用；需要正确性与 A/B 证据，不能默认加缓存 |
| 多 shard 反复扫描同一来源的剩余事件 | source 历史行数、触及桶数、reset shard 数、refresh SQL 查询计划与耗时 | 先对当前已集合化实现做规模测试，再判断是否需要改变扫描边界/索引；不得恢复每 bucket 扫描 |
| SQL insert/索引维护或 commit/checkpoint 占较大比例 | 插入与 tx.commit 分段耗时、索引列表、同一 workload 有/无目标索引的隔离对照 | 只修改证实有价值的 index/batch 细节；禁止根据一次 wall 值删索引 |
| inventory 与重复 seen upsert 放大 hot/多文件成本 | inventory 单独计时、候选数、shard seen 写行数，hot baseline | 保留全部候选 last_seen_at 与 resurrect/missing 契约，在有测量支持时消除重复工作 |
| Codex 大字节数来自新文件或签名触发 reparse | append/new/reparse/partial-tail 文件数与各自逻辑扫描字节 | 先增加原因可观测性；当前 append 正确性已覆盖，未定位前不改变 cursor 判定 |

建议先增加私有 writer/parser 分段观测或基准钩子，输出计数、稳定来源 id、毫秒数与查询计划；不输出消息正文、原始 JSONL 或完整用户路径。计时扩展不需要立即改公共 SourceSyncStats/schema。若最终产品要公开指标，应另行明确 JSON 兼容、CLI/TUI/Web 映射和字段语义。

### 7. 最小可复现基准设计

本节是待执行设计。本研究未编译基准，也未运行真实 sync/rebuild 或 Cargo 测试。

#### 7.1 复用现有能力与隔离方式

- 现有 writer 吞吐测试位于 src/store/sync_writer.rs:1950-2051,2054-2117：TempDir + Store bootstrap + generation-fenced writer，8 shards × 500 events，7 轮交替 A/B，取 median，允许对照场景 10% 以内回归。两项测试标记 ignore，需显式运行。
- 这些基准仅覆盖 insertion 与索引成本；turn/tool/reset 为空。不得把其通过当作 Claude replay 或本轮真实负载的性能证据。
- Parser 集成测试通过 tests/sync/main.rs 的 Fixture 创建隔离源文件与库，串行测试防止进程环境变量串扰。遵循原有环境恢复模式。
- 不复制真实用量库，不采集用户消息正文。只创建合成 tokens、stable ids、timestamps 与可控填充字段。版本、profile、CPU/存储、SQLite 版本、PRAGMA、fixture 参数和 raw 开关都记录在结果中。
- A/B 固定 baseline commit 与 candidate commit，统一 --release、parallelism、相同 fixture 和同一台机器。每轮重建隔离种子状态；交替 A/B 顺序，至少 7 组，以 median 为主并保留原始样本。不要混入编译时间或 fixture 生成时间。

#### 7.2 正确性最小样例

| 样例 | 合成输入/动作 | 必须观测的结果 |
| --- | --- | --- |
| C0 hot | 先同步两个 Codex 文件，再无变更同步 | changed=0、events_inserted=0、扫描范围字节=0；现存文件保持 live |
| C1 append | 仅为其中一个文件追加完整记录 | changed=1、bytes_scanned 等于追加字节；全部累积总量一致 |
| C2 replacement/tail | 替换已有文件；另测无换行 EOF 和 oversized/截断行 | 正确 reset；cursor 只越过 durable boundary；重试幂等 |
| L0 project replay | 两个 Claude 项目；A 有 main+sidechain，B 无变更；只追加 A/main | 只重放 A 的 2 个文件；B 跳过；项目字节与计数正确 |
| L1 streaming update | 同 message/request 的低 usage 后追加高 usage | 合并后的通道与总量等于独立 oracle；重复运行不增量 |
| L2 key migration | main/request-A 后新增同 message 的 sidechain/request-B | message-only usage 逻辑键收敛且 bucket 不双计；behavior 保持基线键与自身去重契约 |
| L3 missing history | A 的已导入文件缺失，另一现存文件变化 | 缺失历史保留，当前库存状态与保护语义不变 |
| L4 bounded | recent cutoff 排除旧行，随后 full sync | bounded 不重置整文件、不推进 full cursor；后续 full 能恢复旧行 |

#### 7.3 性能样例与计量层

1. Writer 层：使用预生成 SyncShard 隔离解析成本。至少分别测 insert-only、duplicate-heavy、reset+shared-bucket、reset+behavior 四个场景。每个场景固定 candidate/inserted/reset-path/touched-bucket/turn/tool 数量。
2. 起步规模复用 8×500；增加 50000 和 250000 条历史种子，保持本轮改变规模固定，测历史规模放大后的 reset/pricing 成本。将 90% 候选 key 已存在的重复场景作为合成压力条件，注明不代表真实数据分布。
3. Parser 层：扩展 C0/C1/L0，各增加一个无变更大项目和一个多文件变更项目，确保吞吐提升没有来自少读必须重放的文件。不要把任意 GB 文本填充的结果等同于真实 JSON 结构。
4. 记录 monotonic source_total、discovery/inventory/cursor/plan/parse_wait、writer preprocessing、BEGIN wait、reset_event、reset_pricing、event_price_insert、cursor/inventory、behavior_reset/insert、commit 与 engine_tail。并行 parse task 的 CPU 时间不得相加当 wall clock。
5. 对关键 reset SQL 在合成库执行 EXPLAIN QUERY PLAN；保留 source-range scan 与 temp bucket probe 的现有断言。SQLite statement profile 如使用，应记录自身开销，并用关闭 profile 的 A/B 样本判断优化收益。
6. end-to-end wall 作为最终性能判据；各 stage 用于解释。单次真实运行用于确定优先级，不能当已控制变量的前后对照。

### 8. 建议验收条件

这些是规划建议，需由主会话写入优化子任务的 PRD/design。未给出未经实测的“127 秒降到 N 秒”承诺。

1. 计数与边界：文档明确 SEEN、COMMITTED、STORED、replayed、logical scanned bytes；Claude trigger-files 与 replay-files 可分辨；一次 lock wait 不重复累加。分段计时能解释源码范围，毫秒精度监测采用 monotonic clock。
2. 正确性：C0-C2、L0-L4 的 event key、各 token 通道、总量、bucket event_count/cost、cursor 和 source_file 保持基线语义；浮点成本使用现有 1e-9 容差。reset/behavior、generation fencing 和失败回滚测试通过。
3. 性能：优化前先保存可复现 baseline；只选择测得的主成本实施。建议 primary replay workload 的 paired median 至少下降 20%，所有控制场景 median 不超过 baseline 的 1.10 倍；阈值是待纳入任务的验收目标，尚无证据证明可达。若 baseline 测得主成本不足以支持 20%，在实施前用测量结果收敛范围和目标，不为过门槛改变 workload。
4. 资源：保留有界并行与完整 shard 原子性；不得用全来源无界缓存换速度。若改变 batch/cache，记录峰值 retained events/bytes 或 peak RSS，并说明边界；不能只报 wall 改善。
5. 数据保护：不删除 missing history、不自动 --allow-lossy-rebuild、不推进失败/取消/bounded 的 full cursor、不重写 accounting marker 以消除警告、不改变 authoritative totals。
6. 版本：源码优化的收益限定实施版本；在未识别安装 1.3.0 的构建提交前，不声称修复了该二进制中某个已确认 SQL 根因。

### 9. 可复用测试与执行入口

| 合同 | 现有测试/位置 |
| --- | --- |
| hot、append、库存存活 | tests/sync/sources/codex_claude.rs:4,67,125 |
| Claude 项目重放范围 | claude_changed_project_does_not_replay_other_projects，tests/sync/sources/codex_claude.rs:218 |
| recent/full cursor | claude_recent_window_preserves_full_history_cursor_and_later_recovers_old_event，tests/sync/sources/codex_claude.rs:346 |
| replacement 与 missing-history gate | tests/sync/sources/codex_claude.rs:409,451 |
| 跨来源 accounting、streaming/sidechain 与 query/bucket 一致 | ccusage_token_semantics_are_consistent_across_sources_and_queries，tests/sync/accounting.rs:26 |
| legacy mixed/hot 幂等 | ordinary_sync_skips_only_legacy_in_a_mixed_run_and_stays_idempotent，tests/sync/accounting.rs:275 |
| shard 所有阶段回滚 | commit_shard_rolls_back_every_stage_on_failure，src/store/sync_writer.rs:2252 |
| reset → events → cursor 原子顺序 | commit_shard_runs_reset_then_events_then_cursor，src/store/sync_writer.rs:2314 |
| SQLite cursor 同事务 | commit_shard_persists_sqlite_cursors_with_events_in_one_transaction，src/store/sync_writer.rs:2391 |
| behavior reset、shard 内去重 | src/store/sync_writer.rs:2466,2565 |
| reset pricing 与单 source-range scan | src/store/sync_writer.rs:2591,2706 |
| stale writer generation | stale_generation_cannot_commit_next_shard_transaction，src/store/sync_writer.rs:2120 |
| reset source 隔离与失败回滚 | tests/sync/runtime/reset.rs:6,116 |
| durable EOF/oversized 边界 | src/parsers/file_state.rs:518-735；src/parsers/claude.rs:911 |
| behavior reset 索引迁移 | migration_v15_adds_part_cursor_and_behavior_reset_indexes，src/store/migrations.rs:2913 |

只在实施阶段运行的建议命令：

~~~text
cargo test --locked --all-features --test sync -- --test-threads=1
cargo test --locked --all-features --lib store::sync_writer::tests -- --test-threads=1
cargo test --locked --all-features --lib parsers::file_state::tests -- --test-threads=1
cargo test --locked --all-features --release --lib activity_cost_index_sync_throughput_regression_stays_within_ten_percent -- --ignored --test-threads=1 --nocapture
cargo test --locked --all-features --release --lib top_sessions_cover_index_sync_throughput_regression_stays_within_ten_percent -- --ignored --test-threads=1 --nocapture
python scripts/ci-rust.py
~~~

以上入口覆盖 Rust 面；如果实施同时变更 docs 或其它面，按 AGENTS.md 改为完整 just ci。新增集成测试必须挂在现有 8 个 Cargo target 下；根 Cargo.toml:6 关闭自动发现。

### 10. Files found、相关规格与外部参考

- src/parsers/mod.rs：SourceSyncStats 与进度统计契约。
- src/parsers/codex.rs：文件级增量计划、分片并行解析、分片串行提交。
- src/parsers/claude.rs：项目级 replay、streaming/sidechain 逻辑去重、batch 合并。
- src/parsers/file_state.rs：fingerprint/cursor replay 判定与 durable JSONL 边界。
- src/parsers/driver.rs：来源串行编排、missing sweep、进度汇报。
- src/store/mod.rs、sync_writer.rs：原子 shard 数据与写协议。
- src/store/connection.rs、lock.rs、source_file.rs、run_log.rs：SQLite 连接、写 fencing、库存写入与运行计时。
- src/sync/engine.rs、types.rs：应用编排、末尾状态/计数与默认并行度。
- src/commands/sync.rs、sync_summary.rs：CLI 生命周期与终端字段投影。
- .trellis/spec/llmusage/backend/source-sync-contracts.md:51-57,90-96,368-395：Claude 项目 replay、Codex 增量、set-oriented reset 与对应回归。
- .trellis/spec/llmusage/backend/token-accounting-contracts.md:59-74,133-160：Codex fork、Claude streaming/sidechain、host identity 与旧历史保护。
- .trellis/spec/llmusage/backend/write-fencing-contracts.md:14-40：lease/generation 与事务前后 fencing。
- docs/adr/0002-sync-shard-as-commit-protocol.md:29-99：commit 协议所有权、原子性、否决在 parser 拆分写阶段。
- docs/adr/0006-source-file-state-machine.md：live/missing/deleted 状态与 JSONL durable offset。
- External references: 本子研究未访问外部资料，也未运行 git；ccusage/tokscale 的更新与对照由主会话和其它研究文件负责。本文不对参考项目速度作未经测量的比较。

## Caveats / Not Found

- 已验证：用户日志与主会话的只读状态、当前源码控制流、计数映射、已有测试定义。
- 未测量：真实 1.3.0 writer 的阶段耗时、实际 commit 数、reset path 数、turn/tool 行数、SQL 查询计划、WAL/checkpoint、物理 I/O、CPU 与内存峰值。
- 未执行：Cargo tests、性能基准、编译、真实 sync/rebuild/reset、真实日志正文读取、任何 Git 操作。本文仅新增本研究文件。
- 不把现有断言当作本轮已经执行通过；不把所有 SEEN-COMMITTED 差额解释为丢失；不把已存在的 WAL、prepared SQL、Claude batch 合并和 set-oriented reset 重新列为功能缺口。
- 不改变 Claude 项目级全重放契约；不建议未经候选状态/逻辑键迁移证明的 append-only 优化。
