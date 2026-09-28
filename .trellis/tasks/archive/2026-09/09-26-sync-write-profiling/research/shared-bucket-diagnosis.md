# shared_bucket_reset 失败控制只读诊断

日期：2026-09-28。范围：读取冻结源码与已有候选 2 日志；未修改产品代码，未新增计时运行。原始数据提取见 `shared-bucket-diagnosis.json`。

## 结论

P4 仍失败。shared_bucket_reset 的 total 中位数为 406.7318→471.0081 ms，比例 1.158031；WRITE 中位数为 383.2649→436.8118 ms，比例 1.139713。退化原因未查明。

可确认两变体的计时边界、持久结果和业务操作计数一致。唯一已识别的产品工作差异位于 reset 选路：Candidate 新增选择率探测，并将 aggregate/DELETE 从默认 host/source 范围切换至 path index。当前正式测量没有阶段耗时，不能将 64.2763 ms 的 total 中位数差直接归因于某条 SQL、commit、缓存或后台负载。

最小后续候选应收缩优化适用范围：单输入 reset path 沿用原 SQL 和原准备过程，跳过选择率探测及额外 path statement 准备；多路径批次保持自适应算法。该方案直接移除单路径场景中新增的规划工作，保留已测得主收益的 40 路径场景。该方案尚未实施或计时，不能宣称能通过 P4。

## 边界核对

1. 两变体均执行 `measure_variant` 的同一计时函数。复制 seed、获取 worker lock、clone 输入位于 `Instant::now()` 之前；total 覆盖 begin_sync_run、commit_shard 和 finish_sync_run。checkpoint 位于 total 取值之后。数据库等价比较位于两个测量返回之后。位置：`src/store/sync_writer/profiling/tests.rs:288-319,728-743`。
2. 两变体的 WRITE 共用 `commit_shard_inner`。host prefix 和 behavior dedupe 位于 WRITE 起点前；起点为第 979 行，终点为第 1037 行，覆盖同一事务 commit。位置：`src/store/sync_writer.rs:957-1044`。
3. 每份精确 WRITE 与公开 write_ms 的累计截断误差断言均通过；正式样本详细阶段字典全部为空。位置：`src/store/sync_writer/profiling/tests.rs:312-315`；详细时钟开关：`src/store/sync_writer/profiling.rs:95-105`。结构计数仍存在，位置 `profiling.rs:211-223`。
4. 每个样本计算 total−WRITE 后，Baseline 的残差中位数为 23.4669 ms，Candidate 为 24.0819 ms。WRITE 自身也超过 10% 比例。分项中位数不能相加还原总量中位数，因此不将两者中位数之差作为逐阶段归因。

## 输入和结构计数

初态固定为 1 项目、2 文件、4000 events、4000 turns、12000 tools。输入只 reset 1 path，包含 2050 events、2050 turns、6150 tools。共享 bucket 的另一路径保留。fixture 位置：`src/store/sync_writer/profiling/tests.rs:597,643-649`。

| 计数 | Baseline 每样本 | Candidate 每样本 |
| --- | ---: | ---: |
| event_batches | 3 | 3 |
| events / events_inserted | 2050 / 2050 | 2050 / 2050 |
| events_deleted | 2000 | 2000 |
| turns_raw / deduped / inserted | 2050 / 2050 / 2050 | 2050 / 2050 / 2050 |
| tools_raw / deduped / inserted | 6150 / 6150 / 6150 | 6150 / 6150 / 6150 |
| cursors / seen_paths / reset_paths | 1 / 1 / 1 | 1 / 1 / 1 |
| touched_bucket_candidates | 4 | 4 |
| reset 算法实际入口 | baseline: 1 | adaptive: 1 |
| 选用 path index | 无新增选路计数 | 1 |

各变体七个样本的计数完全稳定。八次完整状态比较（含预热）全部通过，所有 digest 相等、成本误差 0。没有输入数量、行为去重、bucket 数量或持久输出差异的证据。

## SQL 计划与新增工作

计划来自正式 manifest，未重跑查询：

- Baseline aggregate 和 DELETE：`SEARCH usage_event USING INDEX idx_usage_event_host_source_event_at (host_id=? AND source=?)`。
- Candidate aggregate 和 DELETE：`SEARCH usage_event USING INDEX idx_usage_event_source_path_hash (source=? AND source_path_hash=?)`。
- 两个 aggregate 都使用临时 B-tree 完成 GROUP BY。
- Candidate 另执行一个 host/source 覆盖索引 COUNT，及一个有上限的 source/path 覆盖索引 COUNT。相关生产位置：`src/store/sync_writer.rs:283-296,347-379`。
- Candidate 目前提前准备 path/default 两组 aggregate/DELETE 及 path count；Baseline 只准备原 aggregate/DELETE。后续 bucket 更新、空 bucket 删除和 pricing refresh 的结构一致。位置：`src/store/sync_writer.rs:283-346`；`src/store/sync_writer/profiling.rs:313-365`。

由固定 fixture 可知，host/source 候选为 4000 行，当前 path 为 2000 行，因此 Candidate 的 `path_candidates < host_candidates` 成立。数量比较没有计入探测与额外准备成本。在单路径场景中，一次 host COUNT 无法由后续多个路径摊销。

这里的 4000/2000 是输入及查询谓词确定的候选行数。日志未记录实际 SQLite VM steps、cache misses 或每条 SQL 耗时，不将候选行数等同于执行成本。

## 波动证据和未证实解释

| 组 | Baseline total ms | Candidate total ms | 配对比例 |
| ---: | ---: | ---: | ---: |
| 0 | 706.7212 | 1219.2782 | 1.725261 |
| 1 | 628.2719 | 787.0631 | 1.252743 |
| 2 | 1025.5468 | 471.8878 | 0.460133 |
| 3 | 395.9343 | 471.0081 | 1.189612 |
| 4 | 397.0555 | 422.6561 | 1.064476 |
| 5 | 406.7318 | 344.9595 | 0.848125 |
| 6 | 381.8877 | 333.2166 | 0.872551 |

两个变体均出现较大的早期耗时，后续样本较低；不能仅凭这一序列断定后台负载、设备状态或 warmup 是原因。配对比例中位数为 1.064476，仅供诊断；正式预先固定的指标是两个 total 中位数之比 1.158031，判定仍为失败。

需要区分的假设：

- H1：单路径新增 COUNT/statement 准备成本抵消 path index 的收益。源码和计划证明新增工作存在，但没有该控制的阶段耗时证明它占据退化差值。
- H2：path index 改变行访问顺序及缓存行为，影响 aggregate/DELETE 或后续工作。计划证明访问路径不同；缓存效应未测量。
- H3：运行时波动影响中位数。时序与 BEGIN/run-entry 数据存在波动；具体外部原因未测量。

## 最小修正与验证建议

产品改动范围只应涉及 `reset_file_events_batch_tx` 的选择与准备逻辑。单输入 path 使用原默认 aggregate/DELETE；在进入该分支前跳过 host COUNT、path COUNT 及未用 path statement 准备。保留原 per-path 循环、bucket、pricing、行为、事务和 host predicate。多输入路径保留当前有界自适应计数。不得添加按 4000 events 等 fixture 数字硬编码的阈值，不扩大到 parser 或 schema。

该候选基于优化范围收缩，保留单路径的原有访问策略；并不等于确认 H1。需要新增或调整私有正确性断言，证明单路径执行真实 Candidate 入口且未做选择率探测，多路径仍选择合适索引，remote host 不受影响。

在提出该方案之后，下一轮应先用相同 shared_bucket fixture 的详细阶段 profile 区分 reset_selectivity、reset_aggregate、reset_delete、reset_pricing、event/behavior 和 commit；该诊断运行独立保存，不作为 P4 通过样本。随后若实施新候选，须固定新构建身份，保持原 fixture、轮数、AB/BA、20%/10% 指标，再执行完整验收矩阵。候选 2 的失败证据持续保留，不能被新结果覆盖。
