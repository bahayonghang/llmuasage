# Sync 警告诊断与增量同步优化

## 目标与用户价值

让 sync 准确说明哪些来源已更新、哪些来源保留历史并暂停导入，以及记录级问题的原因和位置；减少无法提交的重复解码，并用可比测量降低主要写入成本。

规划阶段已完成。用户于 2026-09-26 明确要求按任务规划顺序实施，并再次要求继续。当前授权包含三个子任务的产品实现、相关验证及文档更新；真实用户数据库重建、删除历史、产品安装或发布仍不在范围内。

## 已确认事实

- 本轮为 2026-09-26 08:31:46–08:33:53 UTC，运行记录 success，约 127 秒。安装二进制自报 1.3.0，当前 HEAD 29bde59 的 Cargo 版本为 1.4.0；二进制构建提交未确定。
- 总 PARSE=20375 ms，WRITE=105193 ms；Codex 与 Claude 合计占 WRITE 的 98.1%。WRITE 占两阶段合计的 83.8%；具体耗时原因尚未测量。
- CLI accounting=2，被 ordinary sync 排除，保留 3477 条事件。IDE accounting=3，缺成员导致整组保留 9591 条事件，本轮没有新增写入。
- 08:59 UTC 只读核查发现 IDE 跟踪的 501 个成员有 147 个路径缺失，库存状态仍全部为 live。文件消失原因未查明。
- Codex 保存的 8 条样本均为超过 4 MiB 的合法 response_item/custom_tool_call_output；13 为总计、8 为样本上限。Grok 当前对应文件的 13 条 usage 明确带 usageIsIncomplete=true；本轮保存了其中 8 条样本。
- 参考仓库已更新：ccusage 732c7a6362f3d86a4992d2ad7071b6532161a396；tokscale 1d9a9395418efc6952944b794097935d7d6fa1e8。

证据：research/local-state-analysis.md、local-state-probe.json、parse-diagnostics.md、parse-probe-results.json、performance-analysis.md、upstream-comparison.md。

## 需求

| ID | 可观察需求 | 所属任务 |
| --- | --- | --- |
| R1 | legacy、输入不完整、记录跳过与 accounting anomaly 分别报告，默认保留历史 | diagnostics-contracts / replay-preflight |
| R2 | 有原因码的样本仍可定位，显示总数、样本数与省略数，默认输出无正文、完整私有路径或 path_hash | diagnostics-contracts |
| R3 | 源级阻断持久化且不增加 malformed 行数；旧诊断 JSON 可读，所有错误安全检查保持有效 | diagnostics-contracts |
| R4 | 全部选中 Antigravity 产品已阻断时不再解码 usage；保留跨 root 归属与完整组原子性 | replay-preflight |
| R5 | 区分持久库存状态与本次观察，不将旧 live 标志解释为当前文件存在 | diagnostics-contracts / replay-preflight |
| R6 | 测量写入各阶段，在相同输入和初始库下优化实际热点；结果与原实现一致 | write-profiling |
| R7 | TTY、non-TTY 和 NDJSON 的警告边界正确；普通 sync 退出策略保持现有兼容性 | diagnostics-contracts |
| R8 | 参考机制必须通过本仓库 accounting 与原子性契约筛选，记录接受和拒绝理由 | 全部 |

## 验收标准

- [x] AC1：覆盖 R1/R3/R5。缺成员用例显示源级阻断、准确数量及观察时点；malformed=0；重启后仍有诊断；events、buckets、cursors、markers 不因诊断改变。
- [x] AC2：覆盖 R2。Codex 超长非 usage 记录保持有界跳过；Grok usageIsIncomplete 保留可用原生值并报告不完整；reason、位置和安全定位同时存在。13 个问题/8 个样本明确显示还有 5 个未展示。ParseIssues::total() 继续只统计 malformed+oversized；省略数使用全部四类诊断总计。
- [x] AC3：覆盖 R3/R7。旧 JSON 与所有现有消费者兼容；新源级问题不能绕过远端认证、accounting marker 或健康检查；stdout NDJSON 不混入警告，交错 stderr 输出保持完整行。
- [x] AC4：覆盖 R4/R5。全部选中产品阻断的用例 usage decode 调用数为 0，历史及 cursor 不变；部分阻断、跨 root 副本、WAL 变化、取消、bounded 与恢复后重放均通过回归。
- [x] AC5：覆盖 R6。提交同输入前后基准、阶段分解、重复次数和离散程度；优化目标是目标场景中位 WRITE 至少下降 20%，非目标场景中位总耗时退化不超过 10%。目标是验收预算，不是已证实收益。达不到时记录结果并保持任务未完成，不以改变语义通过验收。
- [x] AC6：覆盖 R8。每条采纳机制有参考提交、文件行号、本仓库适配点与反例；原生 protobuf 字段、usage/fallback 排他性、事务与 bounded 语义均保持。证据：`research/upstream-comparison.md`、write-profiling 的 `research/reference-adaptation.md` 与独立核查。写入性能验收仍按 AC5 单独判断。
- [x] AC7：三个子任务完成后执行相应门禁和父任务集成检查，文档与 CLI 行为一致，无真实使用数据进入 Git。

## 任务结构与顺序

1. 09-26-sync-diagnostics-contracts（P1）：分类、持久化、定位、摘要和输出边界。
2. 09-26-antigravity-replay-preflight（P2）：依赖 1 的来源级诊断契约，完成低成本阻断预检。
3. 09-26-sync-write-profiling（P2）：可先研究基准；涉及共享 DTO/计时字段的实现排在 1 后。按实测热点提交最小修改。

父任务负责 R1–R8、跨子任务验收和最终报告，不作为产品实施目标。

## 范围外

不运行真实用户数据的 sync/rebuild，不清理旧数据，不自动升级 marker，不新增来源、不改商业模型映射、不加 RPC 收集器，不替换 SQLite，不以未证实的上游做法改变 token 总量。完整保留历史同时持续导入缺成员组属于另一个状态模型变更，不在本计划中。

## 已决策事项与未知项

沿用 ordinary sync 的历史保护和成功退出契约；用附加状态说明来源降级。沿用有界 8 样本和当前大记录限制。性能验证先于 SQL 修改。没有阻止本轮规划完成的用户决策。

未确定安装二进制提交、文件消失原因、stderr 混行的具体竞态、writer 内部时间分布。实施时用隔离证据核实；不据此先行修改 accounting 或数据恢复政策。
