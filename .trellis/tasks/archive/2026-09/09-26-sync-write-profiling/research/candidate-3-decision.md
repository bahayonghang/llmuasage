# 候选 3 的证据、依据与范围

日期：2026-09-28。目标保持主场景 WRITE 中位数至少减少 20%，每个控制 total 中位数退化不超过 10%。候选 2 正式 shared_bucket_reset 退化 15.80%，原因仍未查明；所有失败样本和 128 次状态等价结果保留。

## 详细 profile 结果

复用完全相同的 `control_fixture("shared_bucket_reset")`，每变体一次预热后执行 3 组交替 AB/BA。每轮恢复不可变 seed，准备与比较位于计时外，详细时钟开启。该运行仅作诊断，不能代替正式 P4 控制。

| 指标（ms，中位数） | Baseline | Candidate 2 |
| --- | ---: | ---: |
| reset_selectivity | 不执行 | 0.9186 |
| reset_aggregate | 26.0437 | 4.9164 |
| reset_delete | 48.8864 | 31.0226 |
| reset_pricing | 4.7956 | 5.0022 |
| reset_events 未细分部分 | 0.1553 | 0.2615 |
| 全部 reset 阶段逐样本合计后的中位数 | 81.8881 | 42.1345 |
| behavior_reset | 63.0014 | 64.1116 |
| commit | 108.0896 | 100.2262 |
| WRITE | 386.1234 | 329.7978 |
| total | 408.0843 | 352.2421 |

选择率阶段范围为 0.8789–1.7841 ms。Candidate 2 的 reset 在三个配对中均更快，详细 profile 的 WRITE 也均更快。阶段核算误差为 0 ns，4 次完整数据库比较（含预热）通过。查询计划与原正式失败控制相同：Baseline 使用 host/source index，Candidate 2 使用 path index，另执行两个覆盖索引计数。

这些结果反对将 COUNT 成本认定为原正式退化的已知原因，也不支持声称回退默认计划能修复已定位的性能瓶颈。正式 7 组结果仍失败，诊断 3 组的较好结果不覆盖失败结果。

原始及提取数据：`shared-bucket-candidate-2-profile.log/.json`、`shared-bucket-candidate-2-profile-summary.json`。命令、时间和 exit 0 记录于 `shared-bucket-profile-run.json`；该次产品 writer SHA 与冻结候选 2 一致，见 `shared-bucket-profile-identity.json`。

## 主会话的范围决定

主会话在读取上述相反证据后仍明确选择候选 3。依据是单路径优化未通过正式 P4 控制，因此将产品变化限制在已证明主要收益的多路径重放。该决定接受单路径回退可能放弃诊断样本中约 39.75 ms 的 reset 收益；不将该范围决定写成已确定根因的修复。

成本归 `reset_file_events_batch_tx`。单个不同路径（含同一路径重复列出）保留原默认 aggregate/DELETE、跳过 host/path COUNT 和未用 path SQL 准备。多个不同路径保留候选 2 的有界自适应选择。first + `iter().skip(1).any(...)` 只做短路比较，不新增集合或路径 clone；原 HashSet 去重循环、路径顺序、事务、bucket、pricing 与 host predicate 保持。

不按事件数量添加阈值，不替换 fixture，不改变 schema、cache、batch、公共 DTO、parser 或默认输出。新回归覆盖单路径、多个路径、重复路径、真实 Candidate 入口、选择率阶段有无、全状态等价、重复同步幂等，以及单/多路径下 remote host 保护。

## 验证顺序

1. focused writer tests、fmt、Clippy。
2. 冻结完整源码、原始字节 SHA256、normalized-LF SHA256、patch 和 exe SHA256。
3. 原 shared-bucket 控制独立执行 7 组作为先行门禁。
4. 同一冻结 exe 执行原完整 writer、host 和 parser 矩阵；轮数、交替顺序和阈值保持，不删除不利轮次。
5. 全部结果保存后交接，最终 `just ci` 由 checker 统一执行。

## 本轮启动异常

release 编译完成后首次通过 Cargo 列出 profile 测试再次报 `0xc0000005 STATUS_ACCESS_VIOLATION`，shell 记录 EXIT_CODE=5。未重建的同一 exe 直接重试成功精确列出 1 项，随后实际 profile 成功。原因未查明。原始 `shared-bucket-profile-list.log` 与 `shared-bucket-profile-list-retry.log` 均保留，不能将直接重试成功表述为首次 Cargo 调用成功。
