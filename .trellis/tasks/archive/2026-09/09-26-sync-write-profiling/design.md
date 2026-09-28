# 写入性能设计

## 先测量现有链

SyncRunWriter::commit_shard / commit_antigravity_snapshot 继续拥有写协议。在现有 tracer/调试测量机制上增加必要的内部阶段计时与计数，优先用于基准而不扩默认表格。阶段边界覆盖 provider mapping、BEGIN IMMEDIATE 等待、reset、event/pricing/bucket、behavior、cursor/inventory 和 commit。

host prefix 与行为去重可能发生在既有 WRITE 计时之前，需要单独记录，避免把 PARSE 残差当解析 CPU。各计时必须单调且范围不重叠；精度与采样开销一起披露。

## 基准场景

1. 冷启动空库导入；2. 无变化热同步；3. Codex 追加；4. Claude 单文件变化导致项目组 replay；5. 多项目 replay / 重复 events / 大量 behavior facts；6. reset 后 bucket reconcile；7. 取消或事务失败正确性。

输入来自生成或脱敏固定 fixtures。真实库若用于独立追加研究，必须在临时隔离副本中执行；本任务默认用可分发 fixtures。每轮恢复相同初始 SQLite 数据、cursor 与输入。不得把第一轮导入与第二轮 no-op 作为前后性能比较。

## 候选选择规则

以阶段 profile 和 query plan 选一个热点。优先复用已存在的批处理与 set-oriented reset；先检查 prepared statements、临时 key 表和 bounded batch 是否已覆盖方案。事件存在查询、provider 处理或 behavior 写入只是候选，未测量前不确定改动。

优化应位于拥有该成本的层，并保持当前普通 shard 的实际顺序：event reset → events/pricing/buckets → cursor → source_file → raw → behavior reset → turns → tools → fence → commit。不要用 per-file Claude cursor 替换项目级 winners；不要并行打开多个 writer。每个候选单独测量、可独立回滚。

## 实施前固定的测量边界

2026-09-28 研究 `research/implementation-seams.md` 已定位准确接缝。普通 shard 的精确 WRITE 对应现有公开 write_ms 起止点；host-prefix 与 behavior dedupe 单独计量。Antigravity 的分源 write_ms 只覆盖 apply 与 marker，不含全组 reset、BEGIN 和共同 commit，不能相加当作整个事务耗时。分别报告私有 transaction total 和逐源 apply；公共 DTO 和已有 write_ms 语义保持。

P4 主场景预先固定为合成 Claude replay：10 项目 × 10 文件 × 500 = 50000 个初始事件，重放 4 个项目的 40 个路径，输入 20000 个重放事件和 1000 个新增事件。每事件有 1 个 turn、3 个 tool calls。生成种子为 `0x20260928`，时间和键固定。主要场景选择发生在热点测量之前，后续不得选择性更换为收益更大的场景。历史放大控制采用 250000 初始事件；其余控制覆盖 insertion、Codex append、duplicate/behavior、shared-bucket reset 与 parser hot/replay。

阶段观测优先使用测试私有 collector；性能验收关闭细粒度 collector，A/B 只保留一致的精确外层计时。数据库准备、checkpoint 和初态复制位于计时外。每个测量轮次使用同一初态，先比较完整持久状态，再汇总至少 7 组交替 A/B 的 WRITE 和总耗时。完整方案及资源上限记录要求见研究文件。

正式冻结构建的关闭范围是 Stage 时钟与阶段累计。结构计数和实际 reset 算法路线仍记录，两变体共用该机制；Candidate 额外记录选路次数。计数开销保留在测量中，不宣称 collector 开销为零，也不在长测中途修改构建。

2026-09-28，在第二个候选开始测量前固定短控制轮数：多 host 共享 path，以及 parser hot、Codex append、Claude project replay，各执行 15 组交替 A/B。其余 writer 场景保持 7 组。依据是第一个已拒绝候选的 host 控制存在较大离散程度；不改变 fixture、主场景或 20%/10% 阈值，不删除不利样本。第一个固定 path hint 候选的 host median total 为 351.501→713.743 ms（退化 103.06%），原始失败证据保留。

## 候选 3 前的架构优先核对

2026-09-28：目标仍为主场景 WRITE 中位数至少减少 20%，每个控制 total 中位数退化不超过 10%。成本由 `reset_file_events_batch_tx` 拥有；事实来源是已冻结候选 2 的完整日志与 SQL 计划。shared_bucket_reset 失败原因未查明，128 次完整状态等价比较及失败样本全部保留。

第一阶段为测试私有测量扩展：同一 `control_fixture("shared_bucket_reset")`，每变体一次预热、3 组交替详细 profile；复用相同恢复初态、计时边界、状态比较与 query-plan 收集函数。这些详细 profile 不参与 P4 通过判定。产品代码在该阶段保持候选 2 不变。

后续产品改动类别为 reset 选路范围收缩。只有阶段证据支持时，单一不同路径保留原 default-plan，跳过选择率 COUNT 和未用 path SQL 准备；多路径保持 adaptive。修改只限 writer 内部及其私有测试，不扩 parser、公共输出或 schema，不添加事件数阈值。需要覆盖单路径、多路径、重复 path 输入、完整状态及 host 隔离。若 profile 与该方向冲突，先报告证据，由主会话决定后续方案。

候选 3 正式冻结后先运行原 shared-bucket 7 组短门禁，再执行原完整 writer、host、parser 矩阵。原有 fixture、轮数、交替顺序与 20%/10% 指标不变；保留每轮和所有失败。最终全仓门禁由 checker 统一执行。

2026-09-28 详细 profile 后的主会话决定：selectivity 中位 0.9186 ms，而 Candidate 全部 reset 阶段合计中位由 81.8881 ms 降至 42.1345 ms。该证据不足以将原正式 15.80% 退化归因于 COUNT，且单路径回退可能放弃约 39.75 ms 的诊断收益。原失败原因仍未查明，详细 profile 不替代 P4。主会话仍选择候选 3，依据是单路径优化尚未通过正式控制，将变化边界收缩至已证明主要收益的多路径重放。

实现边界为“单个不同路径”，包括同一路径重复列出。使用 first 与 `iter().skip(1).any(...)` 的短路比较，不新增集合或路径 clone；原去重循环及路径执行顺序不变。单个不同路径保留原默认 SQL/准备过程；只有多个不同路径才执行有界选择率探测与 path SQL 准备。

## 成功证据与限制

比较完整持久输出及查询聚合结果，再比较时间分布。常规 CI 验证语义与操作边界，性能报告单独保存环境和7组以上交替 A/B 结果。SQLite busy/设备后台负载与 profiler 本身的开销须记录。未取得收益或发生输出差异则回滚候选，保留测量证据并更新任务未知项。
