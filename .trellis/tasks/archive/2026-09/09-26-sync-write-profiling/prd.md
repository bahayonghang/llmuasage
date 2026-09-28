# Sync 写入测量与定向优化

## 目标

确定主要写入时间的实际来源，并以同输入、同初态的测量证明最小优化有效。父任务：09-26-sync-diagnostics-performance；覆盖 R6/R8。

## 事实

本轮 Codex WRITE=39553 ms，Claude=63646 ms，占全部 WRITE 的98.1%。WRITE 包含事务、reset、events/pricing/buckets、behavior、cursor/inventory 与 commit；PARSE 是总时长减 WRITE 的残差，含扫描与其他工作。Claude 项目级 replay、批量 shard、临时 key 表和 prepared statements 已存在。瓶颈原因未查明。

## 需求

- 测量应能定位阶段，不改变事务顺序、会计结果或默认输出稳定性。
- 用完全一致的输入及初始库比较优化前后；准备与复制数据库的时间不计入被测 sync。
- 只实现实际测量支持的热点优化，不根据源码循环或 INSERT 数量推断瓶颈。
- 保留 Claude 项目级逻辑去重、Codex 增量 cursor、行为事实、成本、bucket 与恢复语义。

## 验收

- [x] P1（R6）：提供脱敏可重跑基准，包含版本、OS/CPU/存储、SQLite 设置、初始库、输入规模、预热及至少7组交替 A/B 测量；所有轮次都重置为相同初态。
- [x] P2（R6）：记录互斥 writer 阶段及未归类耗时，区分 BEGIN 等待与 work、commit、计时外行为；累计误差在明确记录的测量精度范围内。
- [x] P3（R6）：候选优化前后事件键/五通道 token/总量/成本、bucket、behavior、cursor、source/host 和 marker 一致，重复同步幂等，失败/取消回滚通过。
- [x] P4（R6）：目标基准中位 WRITE 至少减少20%，其它场景中位总耗时退化不超过10%；同时给范围或离散程度。未达到时保留任务未完成，拒绝无收益候选。
- [x] P5（R8）：记录与参考机制的差异，不复制 tokscale 全前缀哈希来宣称尾读优化，不移植不存在相同 store 链的性能结论。证据：`research/reference-adaptation.md`；独立只读核查：`research/profile-review.md` 第二轮审查。

2026-09-28：P1–P4 由候选 3 验收。四次正式运行、12 个运行/场景、124 对交替 A/B 全部通过；136 次完整数据库比较一致、最大成本误差 0，另有 9 项幂等断言通过。主场景 WRITE 中位数下降 86.4842%，最大控制 total 退化 6.8478%。证据：`research/candidate-3-results.json`、`research/candidate-3-final-observation.json`、`research/profile-review.md`。候选 1/2 失败记录保留。最终跨表面 CI 由父任务 AC7 单独验收。

## 范围外与依赖

可先准备隔离基准；共享 DTO/时间字段的实施依赖 diagnostics-contracts 完成。禁止将 Claude 改为未证明安全的逐文件 append、取消 durability/fence、增加并发 SQLite writer、删除 behavior、自动 rebuild 或改变 token 语义。没有证据时不添加缓存、索引或数据库依赖。改变 batch/cache 时记录峰值保留 events/bytes 或 RSS，继续满足有界资源契约。

## 未知项

provider mapping、reset、event/bucket、behavior 或 commit 的占比需要测量。这是实施第一阶段的技术问题；20%/10% 是待验证的目标，非预期收益事实。
