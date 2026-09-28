# 写入测量方法独立审查

当前状态（2026-09-28）：候选 3 的 P1–P5 与最终 just ci 均通过。完整结果、分布、身份及父任务验收映射见第七轮；同目录 ci-final* 保存本审查者执行的完整门禁证据。

以下按时间保留各轮记录。第一至第六轮的范围和待办描述属于当时状态；候选 1/2 的失败与证据局限继续保留。

- 日期：2026-09-28。
- 任务：09-26-sync-write-profiling。
- 范围：只读源码、任务设计和实现代理产出的测量日志；只写本报告。未编辑产品、未运行 Cargo、未运行性能基准。
- 阶段：原算法阶段定位。当前尚无优化候选，P1–P4 均未作最终验收。

## 结论

未发现要求停止原算法阶段定位的计时或初态恢复问题。三条详细基线记录中，reset_events 占 WRITE 的 84.44%–87.35%，足以支持继续细分 reset。该结论仅适用于预先固定的合成 Claude replay，不能归因用户运行 1.3.0 时的全部 105.2 秒 WRITE。

发现两项测量证据问题：首轮 profile 的代码身份文件采集时点不匹配；统计 helper 对偶数样本采用上中位数但字段标为 median。两项已通知主代理。正式 A/B 前应修正或明确限制，不需要据此立即重跑已有探索性 profile。

## 已核实的计时边界

1. `src/store/sync_writer.rs::commit_shard_inner` 在 host prefix 和 behavior dedupe 后启动原有 WRITE clock；provider、operation、BEGIN、前后 fence、migration、reset、events、cursor、inventory、raw、behavior 和 commit 保持原顺序。新增 record 最终复用计算公开 write_ms 的同一个 Duration。
2. `src/store/sync_writer/profiling.rs:88-128` 的阶段栈从父段扣除子段 elapsed；`Scope::finish` 在第 167–188 行排除 pre_* 段，用 checked_sub 计算 unclassified。不存在用饱和减法隐藏重叠的逻辑。
3. 对 baseline-profile.log 的六条记录独立重新求和，`sum(non-pre stages) + unclassified == write_ns` 全部成立。三条详细记录 unclassified 分别为 20600、20600、17400 ns。
4. `measure` 同时校验整数 public write_ms 与精确 WRITE 的差小于 shard 数毫秒。writer begin 与 finish 包含于 run total；数据库复制、输入 clone、permit 与 heartbeat 获取位于计时外。begin/finish 尚未分别输出，属于 P2 待补项。
5. `Scope`、`Capture`、`AuditClock` 均使用 Drop 清理状态。源码中的普通错误返回路径会先析构局部 Stage，再清理 Scope；尚未在本审查运行失败或 unwind 测试。
6. Antigravity 保留原有分源 apply + marker write_ms；独立 transaction clock 覆盖 operation 至 shared commit，预处理另列。不能把分源 write_ms 之和作为全事务耗时。本轮日志未测 Antigravity。

## 原始样本核对

来源：`research/baseline-profile.log`，1 个 ignored 测试通过，日志退出码 0。测试由实施代理运行，本审查仅解析日志。

| Round | Detailed | WRITE 秒 | Reset 秒 | Reset / WRITE |
| --- | --- | ---: | ---: | ---: |
| 0 | false | 20.2919753 | 未采样 | — |
| 0 | true | 19.7249920 | 17.2304433 | 87.35% |
| 1 | true | 20.0253024 | 16.9101232 | 84.44% |
| 1 | false | 19.4373321 | 未采样 | — |
| 2 | false | 16.8630825 | 未采样 | — |
| 2 | true | 19.3794829 | 16.8364763 | 86.88% |

profile-off 中位 WRITE 为 19.4373321 秒，范围 16.8630825–20.2919753 秒；profile-on 中位 WRITE 为 19.7249920 秒，范围 19.3794829–20.0253024 秒。现有三组不能把波动精确归因于 profiler 开销。最终收益验收仍须关闭细粒度 profiler，按设计执行至少七组交替 AB/BA，并保存全部原始样本。

审查过程中新增的 reset-profile.log 给出原算法细分：aggregate 6.2735570 秒，delete 6.1977453 秒，WRITE 14.6785615 秒。阶段细分支持继续检查这两条 SQL 的 query plan；单样本不能作为优化收益。

## 固定夹具与初态恢复

- `profiling/tests.rs::replay_fixture` 符合预先固定规模：10 项目 × 10 文件 × 500 = 50000 初始 events；4 项目 × 10 文件 × 525 = 21000 输入，其中 20000 replay、1000 新增；每事件 1 turn、3 tools。日志记录 21000 插入 events、21000 turns、63000 tools，与生成器一致。
- key、token、事件时间来自固定整数运算和 seed 0x20260928；`UsageTurn::from_event` 使用 event_key 派生 turn_key，没有按相同时间合并生成的 turns。
- seed writer 消费结束后释放 heartbeat 和 worker lock，再执行并检查 WAL checkpoint，关闭连接后计算文件 SHA256。每次 measure 先验证 seed SHA，再复制主数据库；所有轮次输入复用同一 source file path。未看到复制活动 WAL 数据库或复用上一轮结束库的路径。
- 固定时钟仅替换 writer audit 时间和 run_started_at；worker lease 仍使用真实时间。source_file live 更新接收固定 run_started_at，因此时间列可完整比较。
- `WorkerLock::drop` 删除协调行；reset profile 的全表比较确认 worker_lock 为 0 行。

## 持久状态比较

`profiling/tests.rs:344-418` 按表读取 schema，按声明主键排序，逐行逐列比较。无主键表按全部列排序。所有 cost_* REAL 使用 1e-9 绝对误差，其余 SQLite 类型与值 exact。比较不忽略 timestamp、NULL、token 或 key。

reset profile 已调用该 helper 比较原算法的 profiler-off/on 副本，两个 digest 相等，最大成本误差 0。输出包含 events 51000、turns 51000、tools 153000、buckets 40、projects 10、cursors 100、source_file 100；run_log、source_sync_status、raw、worker_lock 均为 0 行。该结果证明当前测量开关样本一致，尚未证明候选优化等价。

digest 使用每个单元格的表名、列名、类型和值序列。成本容差通过但末位不同的两个库可以具有不同 digest；不能用 digest 相等替代带容差的逐列 oracle。

## Findings (fixed)

无产品修复。当前派遣限制为只读审查，产品和 Cargo 由实施代理独占。本报告是唯一写入。

## Findings (not fixed)

### M1：首轮 profile 的源码身份标识不对应测量版本

- 文件：`research/baseline-code-identity.json`、`research/baseline-profile.log`。
- 证据：身份文件记录 writer SHA256 为 `4423e9be75d9896fa11da82a34929b84a32809e84d1aa9ed475adae9de85fc00`，与当前包含 reset_aggregate、reset_bucket_update、reset_delete 的 writer 一致；首轮日志三个 detailed 样本都没有这些阶段。首轮日志最后写入 07:00:21Z，writer 更新于 07:02:03Z，身份文件更新于 07:02:49Z。
- 影响：身份文件不能声称是首轮 profile 编译输入。算法仍是原算法，已有热点比例不因此失效。
- 建议：将后采集身份明确标为细分阶段或未绑定首轮测量；正式 A/B 在编译前记录源码/工作区 patch 摘要，编译后记录测试二进制 SHA，运行后检查相关文件未变。保留原始日志。
- 未修原因：证据文件由实施代理维护，本审查仅可写本报告。

### M2：统计 helper 对偶数样本返回上中位数

- 文件：`src/store/sync_writer/profiling/tests.rs:420-429`，调用点第 469 行。
- 证据：`values[values.len() / 2]` 对两个样本返回较大值；reset_profile 的两个 total 为 14.8707236 与 15.1078582 秒，字段 median_ns 等于 15.1078582 秒。常规定义的中位数为 14.9892909 秒。MAD 同样使用上中位数。
- 影响：正式七组样本为奇数，不受该边界影响；现有偶数样本统计字段及未来采用偶数组数时不准确。
- 建议：实现奇偶兼容的 median，并据此计算 MAD，或将该字段明确命名为 upper_median。避免混合不同 profiler 模式的两个样本来报告单一总体分布。
- 未修原因：产品文件仍由实施代理独占。

## 正式验收前的待补项

下列内容是已计划但未完成的测量控制，不作为既有产品缺陷。

- P1：最终七组交替 A/B、双方各一轮丢弃预热、全部 per-pair ratio、明确统计规则、运行前后代码身份；元数据中的 PRAGMA user_version=0 不能替代实际 `meta('schema_version')`。CPU、Windows、D:卷与磁盘型号已有环境记录，SSD 介质独立确认目前明确未完成。
- P2：writer begin/finish 分项、Instant 有效精度的实测记录、错误/unwind guard 回归。细分 reset 后仍应保留 non-pre stages 与精确 WRITE 的恒等式断言。
- P3：候选 A/B 全列比较、重复 replay 幂等、未参与 source/host 和 run_log/status 的非空保护哨兵、raw-on、双 host 相同 key、SQLite cursor、shared-bucket pricing/metadata、失败/取消/fencing/rollback。当前 oracle 仅比较持久表 schema，需同时断言持久 index/trigger/view 未变。
- P4：insertion、250k 历史放大、Codex append、90% duplicate/behavior、shared-bucket、parser hot/replay 控制逐一验收。不得把尚未实现的控制记为通过，或用局部 WRITE 改善替代每个控制的 total 阈值。
- Codex 控制生成器复用 `event` 时应按既有会计规范使用 cache_creation_tokens=0；当前主场景是 Claude，因此不影响已采样主基线。
- parser 控制需遵守依赖边界；当前源码未新增 store→query 依赖。不能为了复用测试 collector 把上层查询或命令依赖引入 store。

## Verification

- Lint：本审查未执行。
- TypeCheck：本审查未执行。
- Tests：本审查未执行；只读核对实施代理 baseline-profile.log 和 reset-profile.log，均记录 1 passed、退出码 0。未将两个 ignored 测试等同于正式门禁。
- 只读数值复核：六条原始计时记录的精确加总恒等式成立；预先固定规模与日志计数一致；reset profile 全表 oracle 零差异。
- 本审查未运行真实 sync、rebuild、用户数据库访问、编译、重 CPU/IO 基准、Git 写操作。
- P1–P5 最终检查：尚未进行；候选与最终测量矩阵未完成。

## 审查记录身份

- baseline-profile.log SHA256：`8e767deb64b81544ea986bcc08bf5f1b3e779aeaa38f4df185e194b14e6a15b3`。
- 审查时 profiling.rs SHA256：`6ba748778a8731c8e737c41025678a6922f2b08f12e48d3297088d99a3ca514d`。
- 审查时 profiling/tests.rs SHA256：`07e1d07f09a6fc209862562a4b0819a3ffa2bdd0909c8155f193d5b5623c210e`。
- 行号和源码哈希对应当前只读快照。实施代理继续修改同一文件，后续候选需重新核对相关变更。

## 第二轮只读审查：正式 A/B 与 host 偏斜

日期：2026-09-28。派遣更新后仅核对新代码和 reference-adaptation.md，未重复运行或重新分析探索性 profile。产品和 Cargo 仍由实施代理独占。以下状态更新覆盖首轮报告中相应的待补项，其余历史证据保留。

### 已补齐的测量设施

- M1：baseline-code-identity.json 已追加 identity_caveat，明确哈希在粗粒度 profile 完成后采集，不能标识首轮可执行文件输入。首轮精确源码身份无法由后采集哈希补证；该限制现在已正确披露。
- M2：distribution 已按奇偶样本计算常规 median 和 MAD，并增加 distribution_uses_average_of_middle_pair 测试。源码断言为 median 14989290900 ns、MAD 118567300 ns。本审查未运行该测试。
- metadata 现读取 meta('schema_version')；measure_variant 现单列 writer_begin_ns 和 writer_finish_ns。
- 完整 schema oracle 现比较 sqlite_schema 的 type/name/tbl_name/sql，涵盖 table/index/trigger/view。全表逐列比较规则保持。
- sized_fixture 现写入非空 run_log、source_sync_status 和 meta 哨兵。全表 A/B 比较涵盖这些行，但仍不等同于单独的 seed→output 不变性断言。

### 正式 A/B 结构核查

`profiling/tests.rs::run_acceptance` 为每个场景执行 Baseline、Candidate 各一轮预热，预热不进入正式分布。预热先验证全状态相等及 Candidate 二次 replay 幂等。正式七组按 AB、BA 交替，每对测量后执行全状态 oracle，保留每个原始 WRITE/total、配对比值及分布。主场景按 median WRITE 比值 ≤0.80，控制按 median total 比值 ≤1.10 判断。源码在所有正式 measure 调用关闭 detailed profiler。

`VariantGuard` 选择两条 reset SQL，原 SQL 保存在测试私有模块中，默认使用 Candidate；guard 的 Drop 恢复先前选项。当前 Candidate 仅在 aggregate/delete 添加 INDEXED BY idx_usage_event_source_path_hash，host/source/path WHERE 条件保留。seed 创建使用 Baseline，双方均从该不可变 seed 恢复。未发现把 Candidate 自己生成的优化结果用作某一侧初态的路径。

主 acceptance 入口目前覆盖 primary replay、250k history、insertion、Codex append、duplicate/behavior、shared-bucket 六种场景。host-skew 通过独立 ignored 测试入口执行。parser hot/replay 控制当前仍待补。仅主入口成功不能据此验收完整 P4。

### 多 host 共享 path 控制

- 代码事实：migrations.rs 的 idx_usage_event_source_path_hash 键为 (source, source_path_hash)，不含 host_id；idx_usage_event_host_source_event_at 键为 (host_id, source, event_at)。apply_host_prefix 只改 event/turn/tool/raw/cursor identity，不改 source_path_hash。
- `host_skew_fixture(50000)` 保留 local host 的 500 条 events，再加入 generated-remote 的 50000 条 events；双方均使用 fixture-000-00。remote 的前 500 个原始 event_key 与 local 相同，经过 host prefix 后仍分别存储。输入仅 reset/replay local 的 525 条 events。该形状能覆盖选中 host 小、共享 path 的其它 host 历史大这一风险。
- 强制 path 索引会先命中两个 host 的共享 path 范围，再过滤 host；无 hint 的基线可选择 host/source 范围。当前审查没有运行查询或计时，不能将可能退化标为已实测回归。
- 待补证据：在相同 host-skew seed 上输出两种 SQL 的 EXPLAIN QUERY PLAN；现有 plan 单测只检查 Candidate 在小型普通夹具上的索引选择。正式总结果必须包含独立 sync_writer_host_skew_ab_acceptance 的退出码、七组 total 比值与全表 oracle。
- host-skew 的全表 A/B 比较会涵盖 remote events/buckets 及 host 行，可发现候选改变跨 host 持久结果；建议同时明确报告 remote 50000 行保留的 seed→output 断言，避免仅用聚合行数描述隔离。

### 新发现与未完成控制

1. **F1：Codex 控制的 token 生成值不符合已有 source 契约。** `event()` 仍无条件使用 cache_creation = 10 + n % 7；insertion 与 codex_append 已调用该 helper 创建 Codex events。既有 token-accounting-contracts 要求 Codex cache_creation_tokens=0。建议在长测前增加 source 分支，并重新生成这两个控制的 seed。Claude 主场景的固定输入保持。该问题影响控制代表性，未发现实际产品会计变更。
2. **F2：host-skew manifest 的 seed 峰值计数未随扩展更新。** sized_fixture(1,1,500,...) 设置 seed_max_retained_events=500，host_skew_fixture 后续按 5000 条 event 的 shard 生成 remote 历史。该 manifest 字段应更新为 5000。seed 构建位于计时外，因此不改变现有 WRITE/total 边界。
3. 完整 state oracle 当前只返回 maximum_cost_error，没有逐项记录容差内的成本差异；若最终 A/B 出现非零差异，应保留差异列、主键与绝对误差，满足研究方案的可追溯要求。
4. raw-on、SQLite cursor、价格 tier/source-reported 混合的 shared-bucket、失败/取消/lease guard 等小型 controls 尚需对应测试证据。已有常规回归可以复用，不要求为本报告重跑已成功且未受后续修改影响的检查。

### reference-adaptation.md 核查

两参考仓库当前 HEAD 分别为 732c7a6362f3d86a4992d2ad7071b6532161a396 与 1d9a9395418efc6952944b794097935d7d6fa1e8，符合报告。逐项核对所引 Codex 筛选、Grok 原生 usage/modelUsage、Antigravity 只读事务、DB/WAL fingerprint、dirty key 分组和成功后清理、Codex 全前缀 hash、Antigravity 字段映射，没有发现实质技术错误。

文案精度建议：C 的“空快照”明确限定为 page_count=0 的零页未初始化库；parser.rs:290-317 的判断不是所有零 usage 库。当前适配保留 llmusage 完整快照和历史保护边界，不应扩大 C 的 None 语义。本审查未修改主会话维护的参考适配文件。

### 第二轮验证边界

只读审查、源码比较、引用行核对已完成。未运行 Cargo、lint、typecheck 或任何性能测试；未改变产品、实施代理日志及主会话证据。本报告新增的建议已发送主会话，待明确 handoff 后再执行获准的产品修复或正式门禁。

## 第三轮只读审查：自适应候选与 parser 控制

日期：2026-09-28。范围仅为新增自适应选择、冻结 Baseline 函数、parser 三控制和测试支持的依赖方向。没有重新审查已核实的探索性 profile 或参考仓库。

### 新自适应选择

未发现要求停止长测的新增会计或计时缺陷。reset 在同一事务内按 host/source 做一次 covering COUNT；每个去重 path 的 source/path COUNT 使用当前 host 行数作为 LIMIT；随后选择 path hint 或原默认 SQL。实际 DELETE 的行数从 host_candidates 扣除，下一 path 使用更新的界限。读探针、选择和 DELETE 都位于公开 WRITE 区间，计时不会把新增选择成本排除。

将 profiling.rs::reset_file_events_baseline_tx 与 HEAD 的原 reset_file_events_batch_tx 逐段归一化比较后，看到的差异为函数名称/可见性、原 SQL 抽取为常量、writer audit seam、私有阶段计时及计数。原路径去重、聚合字段、bucket 扣减、零 bucket 条件、逐 path DELETE、剩余 bucket pricing 恢复均保留。基线未共享 Candidate 新增的 COUNT 和选择成本。

两项表述需要与真实执行区分：

1. host_candidates=0 当前只跳过 path probe，代码仍执行默认 aggregate/delete。若后续报告称“零 host 跳过 reset”，需要限定为跳过探针或据实际后续代码说明。
2. RESET_EVENT_AGGREGATE_HOST_SQL / RESET_EVENT_DELETE_HOST_SQL 没有 INDEXED BY host。reset_host_index_paths 只记录选择原默认 SQL，不能证明 SQLite 最终采用 host/source 索引。建议将计数命名为 default-plan 路径，或记录同一初态上的实际 query plan。该命名问题不改变 SQL 结果；性能是否满足门槛仍由正式控制测量判断。

### parser 控制的线程、时钟与依赖

- `src/parsers/writer_benchmark.rs::parser_control` 使用 Tokio new_current_thread；`run_parser` 在调用线程安装 VariantGuard、AuditClock、Capture，然后 block_on SourceParser::parse。
- Codex 的 spawn_blocking 只执行 parse_codex_shard；await 返回后在同一调用线程调用 writer.commit_shard。Claude 的 parse_claude_shard 同样返回数据，由调用线程合并并 commit_shard。当前控制没有在线程池内执行 writer，因此三个 thread-local 状态不会丢失。
- 文件 cursor 在解析 worker 中生成，updated_at 会使用真实时间。新增 writer 测试 audit seam 在持久化时使用固定时间；只有 cfg(test) 且 AuditClock 启用时生效，正常产物仍使用 cursor.updated_at。source_file inventory 使用 writer.run_started_at；worker lease 保持真实时间。没有冻结协调时钟。
- parser 测试引用 store 的 cfg(test) pub(crate) 支持模块，方向为 parser→store；store 未反向依赖 parser、query 或 commands。正常构建不导出该支持模块，未增加公共产品 API。

### 三个 parser 场景的输入和初态

- Codex hot：2 文件各 500 条，共 1000 初始 events；输入不变，断言 changed=0、bytes_scanned=0、inserted=0。
- Codex append：同样初态，仅第一个文件追加 25 条；断言 changed=1、扫描量等于追加字节、inserted=25。session_meta 中 model=gpt-5 被实际解析路径识别。原生 input=100、cached=20、output=30、total=130，规范化后 input=80、cache_read=20、cache_creation=0，reasoning 不重复加总。
- Claude project replay：3 项目 × 4 文件 × 250 = 3000 初始 events；仅项目 0 的文件 0 追加 25 条，断言 changed=4、扫描该项目四个文件全部字节、inserted=1025。message/request/tool identity 含 project/file/index，不会意外跨文件去重。另两个项目不应重放。

每个场景只生成一次临时源树，再用原算法建立 seed。追加在 seed 建立后一次性完成；所有 A/B 轮次复用同一源树并恢复同一 seed 文件。HOME、USERPROFILE、CODEX_HOME 定向至临时目录，并由 guard 恢复。当前两个 source 的 discovery 实现只使用这些入口，因此不会读取真实用户的 Codex/Claude 日志。

parser 初读代码为每场景八对样本；本轮末次复读已增加至十六对，round=0 为两边各一次预热，正式十五对从 BA 开始交替，满足至少七对的要求。每一对都执行完整 schema 和全表 oracle，预热不进入性能统计。输入创建、数据库 copy、permit/heartbeat 获取在 total 之外；total 从 writer begin 之前覆盖整个 parser.parse 到 writer finish。SourceFileStore::mark_inventory_seen 的独立事务包含于该 total，符合 source 控制范围。

### 实际分支证据已补齐

本轮初读时 parser_sample 只输出声明的 variant、WRITE 总和和 parser 计数，Capture records 在求和后丢弃。提出建议后，实施代理已保留并输出 records，增加 Record.reset_algorithms，由两个实际函数入口分别累计 baseline 和 adaptive。Claude replay 根据 variant 断言相应入口出现；Codex hot/append 断言无 reset 调用。结合已核实的线程调用链，该证据覆盖当前控制中的 TLS 传播风险。没有为满足分支断言改变源场景。

同时建议对有提交的 parser 场景核对 precise WRITE 与 stats.write_ms 的截断误差；hot 的 WRITE=0 合理。该校验与保留 records 可防止今后 writer 被移动到其它线程后输出错误的零 WRITE。当前未确认发生了这种丢失。

### 本轮状态

F1 已在源码修正为 Codex cache_creation=0；F2 的 host-skew seed_max_retained_events 已更新为 5000。host-skew 已接入主 writer acceptance 入口；新增小型 host-skew 正确性测试比较完整状态并断言 remote 1000 events 保留。以上是源码审查结果，本审查未运行测试。

本轮没有新增阻断性发现。默认计划计数命名、零 host 的准确描述以及 parser 分支日志建议已通知主会话。未运行 Cargo、lint、typecheck、性能测试；未修改产品或实施代理证据。

## 第四轮只读审查：candidate 2 正式日志与构建身份

日期：2026-09-28。审查对象为已完成的 `host-skew-candidate-2.log`、`parser-candidate-2.log`，以及仍在运行的 `writer-candidate-2-ab.log` 中已完整输出的 `claude_replay_primary`。本轮读取现有文件并独立计算，未运行 Cargo、基准或产品命令。

### 配对、预热、统计与门槛

逐项读取 JSON 原始样本，独立重算 min、max、median、IQR、MAD、每对 Candidate/Baseline 比值及两个中位数之比。全部结果与 summary 一致。IQR 遵循日志声明的排序索引 n/4 和 3n/4；median/MAD 使用常规奇偶中位数规则。没有遗漏或重复的正式 round。

| 场景 | 正式 A/B 对数 | 验收指标 | Baseline 中位数 ms | Candidate 中位数 ms | Candidate/Baseline | 门槛 |
| --- | ---: | --- | ---: | ---: | ---: | --- |
| host_shared_path_skew | 15 | total | 285.7078 | 281.7658 | 0.9862026868 | ≤1.10，通过 |
| codex_hot | 15 | total | 19.3187 | 19.6439 | 1.0168334308 | ≤1.10，通过 |
| codex_append | 15 | total | 35.0247 | 35.3445 | 1.0091306992 | ≤1.10，通过 |
| claude_project_replay | 15 | total | 245.6370 | 208.8850 | 0.8503808465 | ≤1.10，通过 |
| claude_replay_primary | 7 | WRITE | 16968.9661 | 2666.8043 | 0.1571577363 | ≤0.80，通过 |

host 和主 writer 的正式 round 从 0 开始，按 AB、BA 交替；预热单独执行并排除。三个 parser 的 round 0 各含两侧一次预热，正式 round 1–15 按 BA、AB 交替，全部预热样本未进入分布。host 和 parser 日志均包含精确测试成功及 EXIT_CODE=0。主 writer 当时尚无最终退出码，不能将单场景 summary 当成整个测试成功。

主场景 WRITE 中位数下降 84.2842%；total 中位数为 17048.4719→2756.0227 ms，比值 0.1616580487。WRITE 范围为 Baseline 14991.0365–24255.6536 ms、Candidate 2174.5581–3130.2931 ms；MAD 为 1090.1140→252.2603 ms，IQR 为 3565.4682→657.0274 ms。结果只描述固定合成 fixture，不直接推算用户真实 sync 的收益。

### 状态与实际分支

五场景共 72 次全状态比较（含预热）均为两侧 digest 相同、maximum_cost_error=0。每场景各轮表行数相同。最终 usage_event 行数分别为 host 50525、hot 1000、append 1025、parser Claude 3025、主 replay 51000；主 replay 同时包含 51000 turns 和 153000 tool calls。当前没有容差内非零成本差异需要另附逐项记录。全 schema 和全列 oracle 的实现已在前轮核对。

实际 reset 函数入口与声明 variant 一致：host 正式样本各 15 次 baseline/adaptive，Candidate 的 15 个 path 均走 default-plan；Claude parser 各 15 次 baseline/adaptive，Candidate 的 60 个 path 均走 path index；主 replay 各 7 次 baseline/adaptive，Candidate 的 280 个 path 均走 path index。两个 Codex 场景没有 reset 调用。host manifest 的 EXPLAIN QUERY PLAN 另外证实默认 aggregate/delete 在该初态使用 host/source 索引；default-plan 计数本身不承诺其它初态的 SQLite 选路。

每条样本的 Record.write_ns 合计等于样本 write_ns；阶段合计加 unclassified 等于每条 Record.write_ns，writer begin/finish 与 WRITE 合计不超过 total。正式 A/B 的详细阶段计时关闭，stages_ns 为空，但结构计数和 reset_algorithms 仍记录，两变体共享该机制，Candidate 另外记录选路次数。不能把关闭详细阶段计时表述为采集器开销为零。

`src/parsers/writer_benchmark.rs:121-127` 已补公开 write_ms 与精确 WRITE 的截断误差断言：非空 records 时差值小于 records.len()×1 ms；空 records 时两者都为零。该断言包含在冻结源码和成功的 parser 正式入口中，前轮建议已落实。

### 构建身份与重建材料

独立重新计算 `candidate-2-build-identity.json` 记录的 324 个源码输入 SHA256，无缺失或差异；当前 HEAD 与记录均为 `29bde59e84e1149b1d9215e020352586fb0a0ed7`。scope 中所有当前 tracked/untracked 文件均有记录。

- `candidate-2-source.patch` 为 180942 字节，SHA256 为 `910cbf83006c5ca57208dd0b34cc31263b37995ba295bdbbf031f6fcd1617c79`，与当前 `git diff --binary HEAD -- <scope>` 逐字节相同；`git apply --reverse --check` 退出 0。
- `candidate-2-new-sources/` 完整保存四个新增源文件，其 SHA256 分别与当前源文件和身份记录一致。HEAD、完整 patch 与这四个文件组成当前 scope 的重建材料。
- release 测试可执行文件为 32057344 字节，SHA256 为 `7ac3a8cf6e50303035edf18535dc0e7d1edf9cc241c2848aa16c4b57f6510c29`，与构建身份及两个已完成 run ledger 一致。未执行该可执行文件。
- `final-release-list.log` 编译成功，精确列出 host-skew 一项测试，EXIT_CODE=0。两个已完成 run ledger 分别绑定命令、开始/结束时间、可执行文件哈希、日志路径及退出码 0。

证据限制：本轮没有进行清洁重建，不能宣称不同机器上的二进制逐字节可复现。当前 run ledger 未保存日志自身 SHA256，也未分别记录每次运行前后的独立源码/二进制哈希。现有验证确认读取时源码、patch、快照、二进制与记录一致；长矩阵闭合后宜冻结日志摘要及最终身份，不能补写未实际采集的历史观测。

### 契约同步与剩余验收

source-sync-contracts.md:101-107 与当前自适应代码一致，明确有界探针、默认 SQL、实际删除数递减及零 host 只跳过探针。write-fencing-contracts.md:50-53 明确固定 audit 时间不替代 lease 时钟。两份 source excerpt 共 674 条引用行与权威源逐行相同，SHA256 均为 `aad504cd5ea5b979cff7d1858d08f33cb2cfaff61847fde66d50d6bd28062d07`。

本轮未发现新增阻断性缺陷。P4 的主场景和四个短控制已独立通过；其余 writer 控制、整个长测试退出码及最终正式门禁尚未完成，因此 P1–P4 整体仍保持未验收。现有 fmt/clippy 与 34 项 writer 单测日志为较早成功记录，不替代最终冻结版本的规定门禁。详细状态写入任务根目录 `check-report.md`。

## 第五轮只读审查：candidate 2 完整失败证据

日期：2026-09-28。候选 2 明确拒绝，P4 失败。完整 writer 矩阵退出码 101，shared_bucket_reset total 中位数比值 1.1580311645167651，退化 15.8031%，超过固定 10% 上限。退化原因未查明。前轮的部分通过状态由本节更新，原始结果及失败样本全部保留。

### 冻结范围与统计复算

仅读取 candidate-2 的 build identity、记录 HEAD、source.patch、四个新增源文件快照、三份正式原始日志及最终汇总。本轮未运行 Cargo、基准或 just ci，未读取候选 3 的当前源码作为候选 2 证据，未修改产品或实施代理证据。

三次运行共 11 个运行/场景、117 对正式 A/B。三个提取 JSON 与原始日志逐条相等。独立核对每个 round 的两侧各一次、AB/BA 顺序、预热排除、正式数量，并重算全部 min/max/median/IQR/MAD、配对比值和中位数比值；与 candidate-2-results.json 全部相同。仅 shared_bucket_reset 门槛失败。

| 运行 / 场景 | 正式对数 | A/B 状态比较数（含预热） | 验收指标 | Candidate/Baseline | 结果 |
| --- | ---: | ---: | --- | ---: | --- |
| 独立 host / host_shared_path_skew | 15 | 16 | total | 0.9862026868 | 通过 |
| parser / codex_hot | 15 | 16 | total | 1.0168334308 | 通过 |
| parser / codex_append | 15 | 16 | total | 1.0091306992 | 通过 |
| parser / claude_project_replay | 15 | 16 | total | 0.8503808465 | 通过 |
| writer / claude_replay_primary | 7 | 8 | WRITE | 0.1571577363 | 通过 |
| writer / claude_replay_history | 7 | 8 | total | 0.0325325955 | 通过 |
| writer / host_shared_path_skew | 15 | 16 | total | 1.0430564312 | 通过 |
| writer / insertion | 7 | 8 | total | 1.0058567816 | 通过 |
| writer / codex_append | 7 | 8 | total | 1.0449678454 | 通过 |
| writer / duplicate_behavior | 7 | 8 | total | 0.9276269806 | 通过 |
| writer / shared_bucket_reset | 7 | 8 | total | 1.1580311645 | **失败** |

独立 host 与矩阵 host 是两次不同运行，各 15 对、各自判定；未合并为一组分布，也未选择较优比值。矩阵顺序为 primary→history→host→insertion→codex_append→duplicate_behavior→shared_bucket_reset，与冻结入口一致。writer 类 round 0–N-1 从 AB 开始交替；parser 的 round 0 为预热，round 1–15 从 BA 开始交替。

128 条有日志的 A/B 全库比较（117 正式+11 预热）全部 digest 相同、maximum_cost_error=0，逐场景表行数稳定。另有 8 个 writer 类场景的 Candidate 二次 replay 幂等断言成功，不能把这些未单独输出 digest 的内部断言混入 128 条 A/B 日志计数。三条 run ledger 退出码分别为 0、0、101。

### shared 控制结构与公平性

冻结 fixture 为 1 项目、2 文件、4000 初始 events；输入重放 1 个 path，含 2050 events、2050 turns、6150 tools。每侧七个正式样本的业务计数均一致且稳定：删除 2000 events，插入 2050 events，3 个 event batches，4 个 touched_bucket_candidates，1 cursor/seen_path/reset_path。两侧均保留另一条路径；八次 A/B 全库比较通过。

Baseline 每样本实际进入 baseline 一次；Candidate 每样本进入 adaptive 一次，并累计 reset_path_index_paths=1，没有 default-plan 选择。manifest 的原 aggregate/DELETE 计划使用 host/source 索引，Candidate path 语句使用 source/path 索引。冻结源码证明 Candidate 另做一次 host COUNT、一次有界 path COUNT，并提前准备额外 path/default 语句。结构差异可确认，耗时归因不可据此确定。

为避免使用候选 3 源码，本轮从记录 HEAD 与 source.patch 在内存重建 sync_writer.rs。采用记录对应的 CRLF 后，其 SHA256 为 `b924120ac44c6cdef6a10072b9c516795798e49bf3aaa158acb73a3e74c96ca9`，与 candidate-2 identity 精确相同。只读核对该冻结源码及四份已验证快照：

- measure_variant:288-319 对两侧使用同一函数。seed copy、lock、输入 clone 在 total 起点之前；total 覆盖 writer begin、commit_shard、finish；checkpoint 和状态比较在 total 之后。
- sync_writer.rs:957-1044 的两侧 WRITE 起止点一致：host prefix/behavior dedupe 在起点前，provider/事务/reset/event/behavior/fence/commit 在区间内。新增选择率探针和 statement 准备没有被移出 WRITE。
- profiling/tests.rs:694-751 的预热不入分布，正式每轮重新复制相同 seed；两侧每对之后执行 oracle。主入口:772-790 累计所有场景结果，最后对任一失败拒绝候选。
- 正式 records 的 stages_ns 全部为空，结构计数仍启用。每个 sample 的 Record WRITE 合计相符；BEGIN/finish 加 WRITE 不超过 total。公开毫秒值截断误差断言包含于已执行入口。

shared total 中位数 406.7318→471.0081 ms；WRITE 中位数 383.2649→436.8118 ms。逐样本 total−WRITE 的中位数为 23.4669→24.0819 ms。分项中位数不具可加性，因此不能用差值将退化归因于某一阶段。配对 total 比值中位数 1.0644761249 与正式中位数之比是不同统计量；不替换指标。

shared-bucket-diagnosis.json 的 manifest、summary、14 条样本、每侧计数、残差中位数和配对诊断值均与原日志一致，其 raw_log_sha256 匹配。diagnosis.md 对 COUNT/额外准备、访问顺序及运行波动均保持假设标签，没有证据证明具体原因。本审查不把该文档的后续候选建议视为已经实施或验证。

### 最终日志与身份

candidate-2-final-observation.json 于三次运行结束后记录一次观察，明确不回填逐次前后源码状态。其 324 个 expected/observed 输入哈希与 build identity 一致，patch 前后哈希与冻结文件实算一致，三条 run ledger 的 exe SHA 和退出码也一致。四份新增源码快照再次实算全部匹配。18 份已冻结日志的文件大小及 SHA256 全部匹配 final-observation。三份正式日志为：

- host-skew-candidate-2.log：33102 字节；`3e8c6b7661d8df191f8adf81c397c07d3e52b42ecaa5880aabd26425d1885aac`。
- parser-candidate-2.log：85699 字节；`c659d531a15fe68a55a96fa4072e5f7cbf406aec52d32ec6c370028a0f4e95f5`。
- writer-candidate-2-ab.log：202709 字节；`e6443ce9cf3cef5dd4a7664b839e0a566c5c34737216b9861d920093edd4352f`。

**补充字节还原限制：** 前轮已在候选 2 仍冻结时直接核对当前 324 个输入哈希全部一致。本轮进一步从记录 HEAD 的 320 个 Git blob、32 文件 patch、4 个新源快照在内存还原；第一步不读取当前源码。分别尝试原字节、LF、CRLF 后，296/324 个输入能匹配记录哈希，28 个不能匹配。经下述有条件的针对性核对，28 项差异全部确定为 CRLF/LF 混合布局；规范化后不存在内容遗漏。现有归档仍不能独立恢复这些文件的原始混合换行位置，前轮“重建材料完整”仅适用于规范化内容。没有清洁重建或跨机器二进制复现验证。

28 个未能字节还原的记录路径：

```text
src/commands/diagnostics.rs
src/commands/export.rs
src/commands/help.rs
src/common/util.rs
src/domain/platform_monitor.rs
src/domain/source_descriptor.rs
src/parsers/claude.rs
src/parsers/mod.rs
src/query/filter.rs
src/runtime/mod.rs
src/sync/executor.rs
src/tui/report_table.rs
src/tui/sync_control.rs
src/web/assets/charts.css
src/web/assets/components.css
src/web/assets/mod.rs
src/web/assets/render/hero.js
src/web/assets/render/models.js
src/web/assets/render/projects.js
src/web/assets/render/sync-command-center.js
tests/api/facade.rs
tests/cli/main.rs
tests/sync/jobs.rs
tests/sync/progress_io.rs
tests/sync/runtime/jobs.rs
tests/sync/runtime/mod.rs
tests/sync/sources/kimi.rs
tests/sync/sources/zcode.rs
```

主代理随后授权对这 28 项做针对性核对：先读取当前文件并计算 raw SHA，只有仍等于 candidate-2 冻结记录的文件才可作为该候选字节证据；再验证 UTF-8，将 CRLF 转为 LF，保留其余字节，与 HEAD+patch 的 LF 内容逐字节比较。结果为 raw SHA 相同 28/28、规范化内容相同 28/28、CRLF/LF 混合 28/28、裸 CR 为 0、因 raw SHA 改变而排除的文件为 0、非换行内容差异为 0。没有修改任何源码或补入候选 3 改动。结合前述 296 项，候选 2 的规范化输入内容 324/324 已核实；原始混合换行布局的归档还原限制保留。该换行结论与 shared 性能退化无因果证据关联，性能原因仍未查明。

### 阶段闭合及最终状态

stage-closure-check.json 的三条详细 baseline profile 和一条详细 reset profile，均从冻结原始日志独立复算。四条记录的 WRITE−互斥阶段合计−unclassified 均为 0；pre-WRITE 时间独立记录。该算术验证不消除首轮 profile 事后源码身份的既有局限，也不提供 shared 控制的阶段归因。

本轮只更新两份自有审查报告。候选 2 的 P4 失败未修复；字节还原差异已查明为混合换行，原始换行布局的归档限制保留，规范化内容核对完成。既有 lint/typecheck/单测日志成功，完整性能门禁失败；未运行 just ci。候选 3 需接收独立最终检查派遣，不能继承候选 2 的部分通过来完成整体任务。

## 第六轮只读审查：candidate 3 冻结源码与先行结果

日期：2026-09-28。静态审查未发现新的产品缺陷。candidate 3 仅有独立 shared 控制和 writer 主场景完成统计复核，整体 P4 和最终门禁尚未完成。本轮不运行 Cargo、性能测量或产品命令，不修改实施代理证据。

### 冻结身份与范围

构建 identity 于 09:01:10Z 保存，明确为成功 release 构建后、测量前的观察。记录 HEAD 仍为 `29bde59e84e1149b1d9215e020352586fb0a0ed7`；source.patch 为 182038 字节，SHA256 `d456f5d153f52869786a01eb2f18076b515495330a6a88a0950f462c95ccec09`。release exe 为 31528448 字节，SHA256 `427d7a7bbe0ca0b945611a0fd4b36706a4e6f04485b5c8639b20463b7809b385`，读取时与记录一致。

324 个构建输入的 raw SHA 和 normalized-LF SHA 均与读取时的冻结源文件匹配。独立从记录 HEAD 的 Git blob、完整 patch 及四份新增源码快照，在内存重建全部输入，324/324 normalized-LF SHA 匹配。新增快照 raw SHA 亦匹配。候选 3 显式记录 normalized-LF SHA，解决前轮对规范化内容重建身份的表述边界；该改进不反向抹除候选 2 的原始混合换行布局限制。

候选 2→3 仅两个输入 raw SHA 变化：`src/store/sync_writer.rs` 与 `src/store/sync_writer/profiling/tests.rs`。其余 322 个输入完全相同。产品差异的 normalized-LF SHA 为 `d15af1bca145128b92d3f039fb8dc3d8728c1fc87fe3f58e2378a545a66f4712`，本轮分析使用冻结 HEAD+patch 还原源码及已验证的新源快照。

### 单路径与多路径执行证明

`reset_file_events_batch_tx` 在既有 HashSet 去重前增加短路比较：`iter().skip(1).any(|path| path != &path_hashes[0])`。比较语义为是否存在第二个不同路径，不依据原始列表长度，重复同一字符串不会启动自适应选择。长度小于 2 时闭包不执行，也不会索引空列表。未增加路径 clone 或新集合。

单个不同路径的 `use_adaptive_plan=false`，host_candidates 为 None，三个 `bool::then` 闭包均不执行，因此不 prepare path aggregate、path count、path DELETE，也不查询 host COUNT。默认 aggregate/DELETE 及原 bucket 更新准备照常执行。prefer_path 落入 false，实际沿用原默认 SQL。多不同路径时 Option 与三个 statement 同时创建，沿用一次 host COUNT、有界 path COUNT、严格小于选择 path、按实际删除数递减的逻辑。Option 中的 expect 仅在对应 Some/adaptive 条件下可达。

冻结差异未改变 SQL 字符串、source/host/path 谓词、路径处理顺序、HashSet 去重、bucket 扣减与剩余 pricing 恢复、behavior 写入、事务/fence/commit、source marker 或公开 DTO。选择和准备仍在原事务及 WRITE 内。没有 schema、cache、batch、并行 writer 或依赖改动。

### 测试与非循环证明

`reset_plan_depends_on_distinct_paths_and_preserves_complete_state` 使用四个显式 fixture 组合：1/2 个不同路径，各自含/不含重复路径。断言来自 fixture 的预定路径数和固定初态，没有复用产品的 use_adaptive_plan 判断函数。实际函数入口记录必须为 adaptive；预期删除数为路径数×5；selectivity 阶段、default/path 计数分别满足单/多路径期望。对照侧仍经 VariantGuard 进入冻结的原 reset 函数，未共享候选新增的选择率代码。

每组随后比较完整 schema 与持久表，并对 Candidate 二次 replay 比较幂等。host-skew 回归分别覆盖单路径及附加 absent 第二路径，除了完整 Baseline/Candidate 比较，还直接断言 generated-remote 保留 1000 events。上述固定外部预期和实际入口记录减少了声明 variant 正确但执行了同一算法的风险。

35 项 writer 测试通过，8 项 ignored，0 failed；其中既有完整 raw/pricing/host/各回滚阶段、SQLite cursor 原子写入、共享桶 pricing、stale generation、Antigravity 跨来源与 marker 保护均仍在该成功日志。新测试没有降低既有断言或更改 fixture/阈值来接受失败。

覆盖边界曾通知主代理：测试动态证明没有 selectivity 阶段、只走 default-plan，以及多路径/重复路径选择正确；没有独立 SQL-prepare 探针，unused path statements 的未准备由惰性控制流静态证明。主代理已据此将 source-sync 的测试规范明确为动态断言与静态审查两部分，没有改产品或测试，没有打断冻结运行。该表述问题已闭合。

### 已完成子任务及版本边界的漂移核查

预检任务 `reviewed-source-hashes.json` 的六个最终文件与候选 3 raw SHA 全部相同：source_files、Antigravity parser/tests/decode、source_diagnostics、Antigravity sync 集成测试。诊断 DTO、CLI/NDJSON 输出、remote importer/protocol、source status、privacy sanitizer、engine marker 与 sweep guard 等文件相对已检查候选 2 的字节均未变化。writer 新差异位于私有 reset 选择，不增加公开 API。未发现已完成诊断与预检行为的新漂移。

当前 13 个版本文件的 15 个既有字段全部仍为 2.0.0；四个 lockfile 与 2.0.0 gate 及预检 gate 保存的最终 SHA 均相同。版本文档/desktop 文件不属于 writer 构建 identity，本项为针对既有版本字段和锁快照的只读核对，不宣称整份文档已被 writer 构建快照覆盖。原 SemVer major 边界通过但跳过兼容性检查的含义保持，不声称 v1.2.0 源兼容。

源数据 fixture 仍由原生成器构造。parser 控制的隔离 HOME/USERPROFILE/CODEX_HOME、真实用户源访问边界及公开 DTO 私有诊断排除路径均未改变。新增 profile/acceptance 入口仅在 cfg(test) 测试模块，没有向默认产品输出暴露 benchmark 数据。

### 规范与状态

先读到的 source-sync SHA 为 `27d33a89c23fad744232b64b213129ab9f350bbc8c694ada526267eeb5c12502`。测试证据措辞收紧后，当前 canonical SHA 为 `80f4d0a2245c000044ed5773cb2993dae946ad6acda12fc8cf7acd24aeb62eff`。单路径/重复路径默认 SQL、多不同路径探针、零 host 行为、失败控制保留和统计量不可替换等条款均与冻结代码及验收一致。两份 context 摘录为 26911/23109 字节，共 360+328=688 条引用行，逐行内容及 SHA 全部一致。

父任务 AC5/AC7、子任务 P1–P4 仍未勾选。候选 2 完整失败和各原始样本保留。候选 3 的决定文档明确：单路径范围收缩是范围决定，没有将先前退化归因于已知 COUNT 成本；更好的诊断样本不覆盖候选 2 的正式失败。

### 已完成的候选 3 测量

独立 shared 入口于 09:01:11Z 开始、09:01:22Z 完成，退出 0。run ledger 记录同一冻结 exe 及运行前后的 raw/LF 源码、patch、exe 相符观察。7 对正式 AB/BA 加 1 对预热，预热排除，所有分布与配对比值独立复算一致。total 中位数 341.6735→338.2685 ms，比值 0.9900343456545503；WRITE 比值 0.9834377005425362。8 次全库比较 digest 相同、成本误差 0，幂等通过；每条 Candidate 样本实际进入 adaptive 入口，但只选 default-plan 一次，未选 path index。

主 writer 的 claude_replay_primary 7 对已完成。WRITE 中位数 19108.2996→2582.6381 ms，比值 0.13515792373278468，下降 86.4842%；total 比值 0.13905007731302918。全部原始分布、配对比值及顺序复算一致，8 次 A/B 全状态比较及幂等通过。Candidate 每样本 40 个 reset path 均选 path index，21000 events/21000 turns/63000 tools 输入计数保持。主入口尚无最终退出码，其余控制仍待完成，不能据此通过整体 P4。

候选 3 的 fmt、Clippy `--locked --all-features --all-targets -- -D warnings`、release 构建与精确列表日志退出 0。本轮仅审查这些日志，未自行编译。最终 just ci 只能在主代理明确释放 Cargo 并授权后执行；届时重新采集四个 lockfile 的运行前后哈希，不能把本轮静态锁核对当作最终门禁前后记录。


## 第七轮最终检查：candidate 3 完整矩阵与跨表面门禁

日期：2026-09-28。本轮继续已有冻结构建和四次正式运行，未重新运行性能基准。主代理授权独占 Cargo 后仅运行一次原始 just ci，通过后已归还 Cargo。没有修改产品、fixture、计时或阈值。P1–P5 和最终集成验收证据齐备，状态勾选由主代理管理。

### 完整统计复核

独立解析四份原始 JSON 日志，合计 12 个运行/场景、124 对正式 A/B：独立 shared 7、writer 57、独立 host 15、parser 45。四个入口均 exit 0。预热排除、AB/BA 次序、样本数、业务计数、reset 实际入口、全部 min/max/median/IQR/MAD、配对比值及 ratio of medians 均一致。最终逐字段核对 candidate-3-results.json；parser 补充 WRITE 分布从 sample 重算，零 Baseline WRITE 的比值为 null。

| 运行 / 场景 | 正式对数 | 指标 | Baseline 中位数 ms | Candidate 中位数 ms | 比值 | 结果 |
| --- | ---: | --- | ---: | ---: | ---: | --- |
| 独立 shared / shared_bucket_reset | 7 | total | 341.6735 | 338.2685 | 0.9900343457 | 通过 |
| writer / claude_replay_primary | 7 | WRITE | 19108.2996 | 2582.6381 | 0.1351579237 | 通过 |
| writer / claude_replay_history | 7 | total | 73470.9747 | 2481.6807 | 0.0337777021 | 通过 |
| writer / host_shared_path_skew | 15 | total | 216.6205 | 211.8398 | 0.9779305283 | 通过 |
| writer / insertion | 7 | total | 98.3227 | 96.4142 | 0.9805894264 | 通过 |
| writer / codex_append | 7 | total | 430.7421 | 436.0335 | 1.0122843808 | 通过 |
| writer / duplicate_behavior | 7 | total | 544.4855 | 563.5625 | 1.0350367457 | 通过 |
| writer / shared_bucket_reset | 7 | total | 281.7844 | 301.0804 | 1.0684778859 | 通过 |
| 独立 host / host_shared_path_skew | 15 | total | 274.3279 | 271.8105 | 0.9908233905 | 通过 |
| parser / codex_hot | 15 | total | 18.8215 | 18.7191 | 0.9945594134 | 通过 |
| parser / codex_append | 15 | total | 34.6546 | 34.3473 | 0.9911324903 | 通过 |
| parser / claude_project_replay | 15 | total | 223.1974 | 190.3080 | 0.8526443408 | 通过 |

验收统计量为两侧中位数之比。主 WRITE 下降 86.4842%，最大控制 total 退化 6.8478%。shared/host 独立运行及 writer 矩阵运行单列，不合并或择优。每个离群样本保留。

| 运行 / 场景 | 变体 | 验收指标 | min ms | max ms | median ms | IQR ms | MAD ms |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| 独立 shared / shared_bucket_reset | Baseline | total | 256.5127 | 1156.3380 | 341.6735 | 32.3547 | 5.4555 |
| 独立 shared / shared_bucket_reset | Candidate | total | 248.5766 | 347.9825 | 338.2685 | 20.5268 | 9.4778 |
| writer / claude_replay_primary | Baseline | write | 17547.8885 | 19439.6372 | 19108.2996 | 1232.7925 | 331.3376 |
| writer / claude_replay_primary | Candidate | write | 2456.9660 | 2791.3529 | 2582.6381 | 202.2010 | 83.7365 |
| writer / claude_replay_history | Baseline | total | 71298.8990 | 81404.0827 | 73470.9747 | 9324.7633 | 2172.0757 |
| writer / claude_replay_history | Candidate | total | 2314.4269 | 2772.0705 | 2481.6807 | 291.9028 | 145.9102 |
| writer / host_shared_path_skew | Baseline | total | 195.2245 | 262.5530 | 216.6205 | 24.5550 | 13.4400 |
| writer / host_shared_path_skew | Candidate | total | 192.4243 | 254.8583 | 211.8398 | 20.4172 | 9.6109 |
| writer / insertion | Baseline | total | 90.9098 | 112.4834 | 98.3227 | 17.2870 | 7.4129 |
| writer / insertion | Candidate | total | 93.9226 | 106.7161 | 96.4142 | 8.0304 | 2.4916 |
| writer / codex_append | Baseline | total | 331.3532 | 655.7124 | 430.7421 | 118.6940 | 61.4583 |
| writer / codex_append | Candidate | total | 331.0290 | 461.9109 | 436.0335 | 109.6540 | 25.8774 |
| writer / duplicate_behavior | Baseline | total | 467.8713 | 590.2045 | 544.4855 | 46.7311 | 17.0366 |
| writer / duplicate_behavior | Candidate | total | 465.1018 | 573.0387 | 563.5625 | 23.4115 | 8.8218 |
| writer / shared_bucket_reset | Baseline | total | 254.4531 | 322.2229 | 281.7844 | 38.7523 | 22.4832 |
| writer / shared_bucket_reset | Candidate | total | 273.3423 | 324.1298 | 301.0804 | 18.1609 | 8.7027 |
| 独立 host / host_shared_path_skew | Baseline | total | 197.3386 | 287.0382 | 274.3279 | 39.5838 | 12.7103 |
| 独立 host / host_shared_path_skew | Candidate | total | 228.7431 | 318.4955 | 271.8105 | 21.4227 | 10.4025 |
| parser / codex_hot | Baseline | total | 16.8594 | 19.8708 | 18.8215 | 1.1924 | 0.4484 |
| parser / codex_hot | Candidate | total | 16.7999 | 21.6837 | 18.7191 | 2.0036 | 0.8756 |
| parser / codex_append | Baseline | total | 25.8470 | 43.3263 | 34.6546 | 4.0866 | 1.7915 |
| parser / codex_append | Candidate | total | 26.6501 | 43.1181 | 34.3473 | 7.4991 | 2.0516 |
| parser / claude_project_replay | Baseline | total | 180.9740 | 251.8578 | 223.1974 | 30.1584 | 14.8747 |
| parser / claude_project_replay | Candidate | total | 151.4293 | 215.8513 | 190.3080 | 39.9127 | 15.4272 |

IQR 按排序后 n/4 与 3n/4 的整数索引差计算；MAD 为离中位数绝对偏差的常规中位数。表中列验收指标；WRITE/total 两套完整分布在原始日志及 results.json。

136 条 A/B 全库比较包含预热（shared 8、writer 64、host 16、parser 48），digest 全一致，最大成本误差 0。另 9 项 Candidate 二次 replay 幂等断言通过，不混入 136 条显式记录。schema、完整表/列、event key、五通道 token/total/cost、bucket/pricing、raw/behavior、cursor/inventory、source/host/marker 按 oracle 比较。最终 sync 门禁覆盖取消边界及 recovery。

### 身份与证据完整性

| 文件 | bytes | SHA-256 |
| --- | ---: | --- |
| candidate-3-shared-bucket.log | 17618 | fe247b007c08becba2347e8f9f342f69a29fe99b0c470031aede16eb609bd902 |
| candidate-3-writer.log | 202216 | 556d4e8c9d0b25218031f90589653d03b04a40916dab060ff9c506613a7b22b6 |
| candidate-3-host.log | 33105 | 921cc8c590bc4343a41f979ff6223aba1afa76c11411b62406ee47a713eb6534 |
| candidate-3-parser.log | 85707 | 57fa770a817b2a4da38e7bb1cd29f92bb6582f8fd089a88fd9565acb2af8c823 |

四个 run ledger 各有实际运行前后源码、patch 和 exe 观察；最终观察于 09:32:17.898072Z。HEAD 为 29bde59e84e1149b1d9215e020352586fb0a0ed7；patch SHA 为 d456f5d153f52869786a01eb2f18076b515495330a6a88a0950f462c95ccec09；release exe SHA 为 427d7a7bbe0ca0b945611a0fd4b36706a4e6f04485b5c8639b20463b7809b385。

324 个文件 raw/LF SHA、完整 patch、新增源和 exe 全匹配。HEAD+patch+新增源重建后 324/324 normalized-LF SHA 一致。35 份日志逐项 size/SHA、7 份 supporting artifacts 逐项 SHA 全匹配。just ci 后重算 324 个源输入 raw/LF SHA 和 release exe，全部仍匹配冻结身份。没有用后续源码或编译产物回填先前运行身份。

### 最终 CI 与前后锁

原始 just ci 于 09:35:09.197594Z–09:39:13.693987Z 执行，244.4963957 秒，exit 0，error=null。recipe 和参数未改。Cargo 为 C:/Users/lyh/.cargo/bin/cargo.EXE，链接至 rustup.exe；版本 1.97.0，x86_64-pc-windows-msvc，Windows 11 10.0.26200。实际解析路径与版本输出保存在 ci-final-env.json。

| 检查 | 最终结果 | ci-final.log 行号 |
| --- | --- | --- |
| CI contract self-test / contract | 通过 | 1–4 |
| Root Rust fmt / Clippy | 通过，-D warnings 保持 | 6–9 |
| Root Rust lib | 922 passed、19 ignored、0 failed | 63–1008 |
| 八个 integration target | 247 passed：api 3、architecture 12、CLI 34、query 9、remote 8、store 2、sync 144、TUI 35 | 1016–2169 |
| Root rustdoc | 构建通过；doc-tests 0 | 2171–2180 |
| Dashboard / scripts JavaScript | 66 passed；两个脚本语法检查通过 | 2181–2262 |
| Desktop frontend | 18 文件、65 tests passed | 2263–2292 |
| Desktop dev-port scripts | 4 passed | 2294–2308 |
| Desktop TypeScript / Vite | tsc --noEmit 与 production build 通过 | 2309–2323 |
| Desktop Rust | lib 18、acceptance 9、quota 6 passed；main/doc-tests 0 | 2352–2413 |
| VitePress docs | 构建通过，4.54 秒 | 2415–2425 |

四锁前后 SHA 与字节数相同，门禁后独立读取当前文件仍一致：

| 文件 | bytes | SHA-256 |
| --- | ---: | --- |
| Cargo.lock | 103508 | f88651176c1d0f1b64b949f697217a95e23b3cdb18cef7b1c0b09401a7dce9ba |
| desktop/src-tauri/Cargo.lock | 175564 | 93dd2b3a506284617486806b5a0e375362c396bd77545cf1ab506b5e5e64b358 |
| desktop/package-lock.json | 113097 | 95378d7b1409b56b957d325f78ad2ba2b879766ef2fa31ab42a5de72ef312ad5 |
| docs/package-lock.json | 88365 | 9a4fe421531cda95ffe058f3a66b7b917ab519bea194881818f62a9ba2bf7173 |

ci-final.log SHA-256 为 01c23956725f011ae4707b3bccb509f4bed47ce19c648bb1d3182fc2d2b95640。ci-final-result.json 与完整输出保留精确退出码、起止、耗时。没有失败重试或重复 SemVer。19 项 ignored 包含本任务四个已单独完成的正式 release 入口；其余显式测量未执行。编译日志保留 MSVC import library linker_messages；Clippy -D warnings 与整个 recipe 成功，未修改 lint/链接设置。

### P1–P5 / AC1–AC7 证据映射

| 验收 | 独立结论与证据 |
| --- | --- |
| P1 | 通过。固定 seed/输入与 checkpoint 后初态复制、每变体预热、7/15 对交替；环境、SQLite PRAGMA、版本、硬件与计时边界保存于 implementation-evidence、四个 manifest 和 build/run identities。完整规范化源码可从 HEAD+patch+新源快照重建。 |
| P2 | 通过。WRITE 阶段互斥，BEGIN elapsed 包含等待及进入事务，pre-WRITE 单列；四条详细记录闭合误差 0。Antigravity transaction total 与 source apply 分开。公开 write_ms 每 Record 截断误差小于 1 ms；正式细分时钟关闭但结构计数保留。见 stage-closure-check 与前轮审查。 |
| P3 | 通过。136 条 A/B 全库比较一致，成本最大绝对误差 0；另 9 项 Candidate 幂等断言。35 项 writer 测试覆盖完整 raw/pricing/host/SQLite cursor、八阶段失败回滚、stale generation 和跨源 marker；最终 144 项 sync 与 8 项 remote 覆盖取消及跨层语义。 |
| P4 | 通过。四次运行、12 个运行/场景、124 对正式 A/B；主 WRITE 比值 0.1351579237，所有控制 total 比值 ≤1.0684778859。所有样本保留，独立与矩阵同名场景分别判断。 |
| P5 | 通过。reference-adaptation.md 与父任务 upstream-comparison.md 固定参考提交和行号；保留原生 accounting、Claude 整组 replay、单 writer 事务、bounded 约束，不复制全前缀哈希或上游非等价 store 性能结论。 |

| 父验收 | 证据关系与结论 |
| --- | --- |
| AC1 | diagnostics-contracts 的 typed record/source error 九类回归、持久化重启检查；preflight A2/A5 的 missing/unreadable/out-of-scope 数量和独立观察时点。最终 sync 通过，events/buckets/cursor/marker 保护保持。 |
| AC2 | diagnostics-contracts 的 Codex bounded oversized、Grok native incomplete、reason/安全位置/8 样本与省略数检查；ParseIssues::total() 语义保持。最终 lib/CLI 通过。 |
| AC3 | diagnostics-contracts 的旧 JSON/default/私有持久化、remote 认证与 marker、TTY/non-TTY/NDJSON 分流、警告整行写入。最终 remote 8、CLI 34、sync 144 通过。公开 Rust 源兼容边界按已批准 2.0.0 处理。 |
| AC4 | preflight check-report A1–A5；真实 decoder 入口计数 0，partial/cross-root/WAL/bounded/recovery/cancel，以及 root discovery red/green。六个已审核源码 raw SHA 未漂移，最终 sync 通过。 |
| AC5 | 本轮 P1–P4 完整通过。不能将合成基准外推为用户 2026-09-26 那次 sync 的节省秒数。 |
| AC6 | 参考更新提交、采用/拒绝机制和反例继续成立；本轮 P5 及前两个子任务独立检查完成。 |
| AC7 | 原始 just ci 退出 0；文档构建通过，CHANGELOG 限定 multi-path replay。324 个构建输入门禁后仍匹配冻结 SHA，四锁前后相同。新测试使用合成输入，未执行真实 sync/rebuild/reset；Git 工作区路径列表无 SQLite/DB 数据文件候选。父任务勾选和交付状态由主代理管理。 |

### 最终限制与交接

本轮未发现未修复产品问题，无失败门禁。候选 1/2 原始拒绝记录完整保留，候选 2 shared 退化原因仍未查明。首轮 profile 事后身份、候选 2 混合换行归档限制保留，不能用候选 3 身份回填。

结果限于固定合成负载与记录环境，不外推用户安装版 1.3.0 的实际运行。未运行真实 sync/rebuild/reset、安装或提交；未修改 Windows ACL 或执行 native UI 人工验收。Git 工作区候选路径无 SQLite/DB 文件；该路径检查不替代父会话最终提交范围核对。

Cargo 已归还主代理。本审查仅更新 check-report.md、profile-review.md 与 ci-final* 证据，未修改 writer 实施证据、PRD 勾选、父总结或提交状态。
