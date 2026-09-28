# 候选 3 固定矩阵结果

日期：2026-09-28。四个正式入口均 exit 0，P4 数值门槛全部通过。最终独立审查及 just ci 由 checker 执行；本文件不代替 PRD 验收勾选。

## 改动与边界

候选 3 仅在存在多个不同路径时使用有界自适应选路；单个不同路径（含重复列出同一路径）使用原默认 aggregate/DELETE，并跳过 host/path COUNT 与未用 path SQL 准备。原 HashSet 去重、路径顺序、host predicate、bucket、pricing 和事务顺序保持。

候选 2 的正式 shared-bucket 退化 15.80% 仍是失败结果，原因未查明。后续诊断并未证明 COUNT 是原因。候选 3 是将产品变化限制在已证明主要收益的多路径重放，不能写成已定位单路径退化根因的修复。候选 1/2 的所有失败日志继续保留。

## 全部结果

以下比值均为 Candidate 中位数 / Baseline 中位数。主场景要求 WRITE 比值不超过 0.80；每个控制要求 total 比值不超过 1.10。独立 shared/host 与矩阵内同名控制分别报告；writer 与 parser 的 codex_append 分别报告。

| 运行 | 场景 | 组数 | Baseline WRITE（s） | Candidate WRITE（s） | WRITE 比值 | Baseline total（s） | Candidate total（s） | total 比值 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| shared-bucket | shared_bucket_reset | 7 | 0.3231858 | 0.3178331 | 0.983437701 | 0.3416735 | 0.3382685 | 0.990034346 |
| writer | claude_replay_primary | 7 | 19.1082996 | 2.5826381 | 0.135157924 | 19.1937894 | 2.6688979 | 0.139050077 |
| writer | claude_replay_history | 7 | 73.3968021 | 2.4078120 | 0.032805407 | 73.4709747 | 2.4816807 | 0.033777702 |
| writer | host_shared_path_skew | 15 | 0.2045105 | 0.2018653 | 0.987065701 | 0.2166205 | 0.2118398 | 0.977930528 |
| writer | insertion | 7 | 0.0777858 | 0.0742708 | 0.954811804 | 0.0983227 | 0.0964142 | 0.980589426 |
| writer | codex_append | 7 | 0.4103390 | 0.4149150 | 1.011151755 | 0.4307421 | 0.4360335 | 1.012284381 |
| writer | duplicate_behavior | 7 | 0.4593654 | 0.4823132 | 1.049955439 | 0.5444855 | 0.5635625 | 1.035036746 |
| writer | shared_bucket_reset | 7 | 0.2670508 | 0.2833261 | 1.060944584 | 0.2817844 | 0.3010804 | 1.068477886 |
| host | host_shared_path_skew | 15 | 0.2617791 | 0.2590820 | 0.989697038 | 0.2743279 | 0.2718105 | 0.990823391 |
| parser | codex_hot | 15 | 0.0000000 | 0.0000000 | 不适用（两者为 0） | 0.0188215 | 0.0187191 | 0.994559413 |
| parser | codex_append | 15 | 0.0044407 | 0.0044649 | 1.005449591 | 0.0346546 | 0.0343473 | 0.991132490 |
| parser | claude_project_replay | 15 | 0.1822738 | 0.1540040 | 0.844904753 | 0.2231974 | 0.1903080 | 0.852644341 |

主场景 WRITE 中位数 19.1082996 → 2.5826381 s，减少 86.484208%。全部控制通过；控制中最大的 total 比值为 1.068477886。

## total 分布

单位为 ms。IQR 使用排序后 n/4 与 3n/4 位置之差；MAD 为相对中位数的绝对偏差中位数。所有原始样本、配对比值及 WRITE 分布另存于 candidate-3-*.json 和 candidate-3-results.json。

| 运行 / 场景 | Baseline 范围 | Candidate 范围 | Baseline IQR / MAD | Candidate IQR / MAD |
| --- | ---: | ---: | ---: | ---: |
| shared-bucket / shared_bucket_reset | 256.5127–1156.3380 | 248.5766–347.9825 | 32.3547 / 5.4555 | 20.5268 / 9.4778 |
| writer / claude_replay_primary | 17634.7163–19520.1335 | 2534.3568–2871.9024 | 1239.2088 / 326.3441 | 204.1192 / 84.1913 |
| writer / claude_replay_history | 71298.8990–81404.0827 | 2314.4269–2772.0705 | 9324.7633 / 2172.0757 | 291.9028 / 145.9102 |
| writer / host_shared_path_skew | 195.2245–262.5530 | 192.4243–254.8583 | 24.5550 / 13.4400 | 20.4172 / 9.6109 |
| writer / insertion | 90.9098–112.4834 | 93.9226–106.7161 | 17.2870 / 7.4129 | 8.0304 / 2.4916 |
| writer / codex_append | 331.3532–655.7124 | 331.0290–461.9109 | 118.6940 / 61.4583 | 109.6540 / 25.8774 |
| writer / duplicate_behavior | 467.8713–590.2045 | 465.1018–573.0387 | 46.7311 / 17.0366 | 23.4115 / 8.8218 |
| writer / shared_bucket_reset | 254.4531–322.2229 | 273.3423–324.1298 | 38.7523 / 22.4832 | 18.1609 / 8.7027 |
| host / host_shared_path_skew | 197.3386–287.0382 | 228.7431–318.4955 | 39.5838 / 12.7103 | 21.4227 / 10.4025 |
| parser / codex_hot | 16.8594–19.8708 | 16.7999–21.6837 | 1.1924 / 0.4484 | 2.0036 / 0.8756 |
| parser / codex_append | 25.8470–43.3263 | 26.6501–43.1181 | 4.0866 / 1.7915 | 7.4991 / 2.0516 |
| parser / claude_project_replay | 180.9740–251.8578 | 151.4293–215.8513 | 30.1584 / 14.8747 | 39.9127 / 15.4272 |

全部样本均保留。例如先行 shared 的 Baseline total 最大值 1156.3380 ms，没有删除。未合并独立/矩阵控制，未使用 paired median 替换预定的 ratio of medians，未通过重跑选择较好分布。

## 状态与测量检查

136 次完整状态比较通过：先行 shared 8 次、完整 writer 64 次、独立 host 16 次、parser 48 次，均含预热。比较覆盖 sqlite_schema 与全部 16 表的所有行和列；digest 全部相等，最大成本误差为 0。仅 cost_* REAL 允许 1e-9，未排除时间字段；私有 audit clock 固定，lease clock 保持真实。

独立脚本 summarize-candidate-3.py 从每个计量样本重算 median、min/max、IQR 和 MAD，与基准输出完全一致；核对原 fixture、轮数、SQLite 设置与交替顺序。writer 计量轮次从 0 开始；parser 的预热为 round 0，计量轮次为 1–15，与候选 2 的协议一致。

P4 关闭详细 Stage 时钟，保留结构计数和真实 reset 算法来源；不能宣称剩余计数开销为零。全部临时库均在 D: 的 target/writer-benchmark/ TempDir，使用生成 fixture，未读取、复制或同步真实使用数据。

## 固定身份与运行记录

release exe SHA256：427d7a7bbe0ca0b945611a0fd4b36706a4e6f04485b5c8639b20463b7809b385。

candidate-3-build-identity.json、candidate-3-source.patch 与 candidate-3-new-sources/ 保存 HEAD、完整 tracked diff、新增源字节、324 个输入文件的 raw/LF SHA256。candidate-3-runs.jsonl 保存四个正式入口的精确命令、开始/结束时间、exit 0，以及实际前后身份观测。candidate-3-final-observation.json 在 09:32:17Z 进行单次末次观测，所有输入与 exe 一致，并保存 35 份原始 log SHA256。该末次文件不冒充逐次监控。

路径核对确认当前 C:/Users/lyh/.cargo/bin/cargo.exe 是 rustup 链接，Cargo 1.97.0。早先检查的裸 cargo 调用未在启动时采集真实路径；mbx 输出来源未确定。未清缓存或重跑已通过门禁。四个正式性能入口直接运行固定 exe，不经过 Cargo。

## 普通检查与交接

candidate-3-writer-tests.log：35 passed / 8 ignored，exit 0。candidate-3-fmt.log：exit 0；candidate-3-clippy.log：all-features / all-targets、-D warnings，exit 0。首次 fmt 提示的三个格式差异已作最小修正，失败日志保留。candidate-3-release-list.log：精确匹配 1 项，exit 0。

Cargo 独占已释放。实施会话不再编译或启动基准；最终 just ci、独立审查与 PRD 验收由主会话/checker 完成。没有 commit、安装、发布或真实数据库操作。
