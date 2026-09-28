# Writer profiling 实施证据

日期：2026-09-28。状态：候选 3 四个正式入口均 exit 0；主 WRITE 中位数减少 86.484208%，所有控制 total 比值 ≤1.068477886。独立复算一致，136 次完整状态比较通过、最大成本误差 0。P4 数值门槛通过；最终独立审查、同步引擎取消/跨域集成及 just ci 由 checker 确认，PRD 验收尚不由实施会话勾选。候选 1/2 失败证据全部保留。

## 测量支持的改动

原有普通 shard WRITE 中，event reset 占三次详细样本的 84.44%–87.35%。进一步分解的单次 WRITE 为 14.6785615 s，其中 aggregate 为 6.2735570 s、DELETE 为 6.1977453 s、bucket update 为 0.0013973 s、reset pricing 为 0.0353728 s。原始 aggregate 与 DELETE 查询计划均选用 `idx_usage_event_host_source_event_at`，每个路径重复扫描该 host/source 的历史范围。`baseline-profile.log` 与 `reset-profile.log` 保留原始结果。

候选 1 固定使用已有 `idx_usage_event_source_path_hash`。在 500 个 local 事件和 50000 个 remote 事件共享路径的控制中，median total 从 351.5009 ms 增至 713.7432 ms，比例为 2.03055867，超过 1.10 上限。候选 1 已拒绝，完整失败记录位于 `host-skew-candidate-1.log`。

候选 2 在一次 reset 批次中先通过已有覆盖索引计数当前 host/source 的事件，再对每个路径进行有上限的 source/path 计数。路径候选数严格少于当前 host/source 数时使用 path index；否则保留原查询计划。每次删除后扣减当前 host/source 计数。零事件时跳过路径计数，但仍执行原 aggregate/DELETE。

优化不新增持久索引、schema、cache 或 batch，不更改 path hash、host predicate、路径顺序、价格恢复、bucket 算法、事务顺序、durability、generation fence、公共 DTO 或默认 CLI 输出。原 reset 函数在测试私有模块中保留为 Baseline，同一 release 可执行文件内选择两种算法。

## 已固定的比较边界

- 种子 `0x20260928`。主场景初态为 10 项目 × 10 文件 × 500 = 50000 事件；输入重放 4 项目的 40 路径，包含 20000 重放和 1000 新增事件，每事件 1 turn、3 tools。
- writer 主场景和常规控制各 7 组交替 AB/BA；host skew 和三个 parser 控制各 15 组。每种变体各预热一次，预热不计入统计。轮数在候选 2 测量前固定，不删除任何不利轮次。
- 每轮验证不可变 seed 文件 SHA256，恢复同一初态。数据库生成、checkpoint、关闭、复制、输入 clone 和 worker lock 获取在计时外。精确 total 包含 writer begin、被测工作和 finish。
- P4 关闭 Stage 的详细时钟与阶段累计，保留相同的外层 Instant Duration、结构计数及实际 reset 算法记录。两变体共享该机制，Candidate 另记录选路数量；剩余计数开销未单独测量，不能宣称 collector 开销为零。公开 WRITE 保持原边界；私有精确 WRITE 与公开整毫秒值的截断误差小于每条 record 1 ms。无 parser writer record 时两者均为零。
- 主场景要求 median WRITE 比例 ≤0.80；每个控制要求 median total 比例 ≤1.10。报告全部 paired ratios、min/max、IQR 和 MAD；偶数样本中位数取中间两项平均。
- 基准只生成脱敏 fixtures。临时数据库全部位于 D: 工作区 `target/writer-benchmark/` 的 TempDir，退出时清理。未读取、复制或同步真实用户库。

## 构建身份与环境

正式候选 2 release exe SHA256：`7ac3a8cf6e50303035edf18535dc0e7d1edf9cc241c2848aa16c4b57f6510c29`。编译成功并精确列出 1 项测试后运行。

`candidate-2-build-identity.json` 记录 Git HEAD、324 个源码/测试/配置文件 SHA256、exe SHA256、命令和时间。`candidate-2-source.patch` 是直接从 subprocess 获取的完整 tracked source diff；`candidate-2-new-sources/` 保存新增源码的原始字节。所有输入哈希在编译后复核一致。`candidate-2-runs.jsonl` 保存各正式运行命令、开始/结束时间、退出码和相同 exe 身份。

`candidate-2-final-observation.json` 是三次正式运行结束后的单次末次观察：324 个文件哈希、完整 patch 和 exe SHA256 均与冻结身份一致，同时保存 18 份原始 log 的 SHA256。各正式运行前检查过 exe 哈希；没有补造逐次运行前后的源码观察。

环境见 `measurement-environment.json`：Windows 11 10.0.26200，Intel Core Ultra 9 275HX，D: NTFS，磁盘型号 ZHITAI TiPlus7100 2TB。磁盘介质类型未独立核实。Rust/Cargo 1.97.0；SQLite 3.53.2。各基准 manifest 记录实际 schema version、SQLite PRAGMA 和最小观测非零计时间隔。

`baseline-code-identity.json` 是在初始探索 profile 之后采集的身份，包含后续 instrumentation，不能作为初始粗粒度 profile 的精确编译输入。候选 2 正式测量身份独立固定，不受该限制。

## 阶段观测与精度

阶段 collector 仅在测试编译中存在，正常构建的阶段宏直接执行原表达式。嵌套阶段从父阶段扣除子阶段时长，WRITE 的未归类时长通过 checked subtraction 核算。`pre_host_prefix` 和 `pre_behavior_dedupe` 位于既有 WRITE 之前，单独报告。BEGIN 包括 SQLite 等待与事务进入工作，不能解释为纯等待时间。

Antigravity 独立记录整个 transaction total 和各 source apply。各 source 的公开 write_ms 不包括共同 BEGIN、全组 reset 或共同 commit，不能将逐源数值相加当作整个事务时间。

初始三组 off/on WRITE 秒数依次为 20.2919753/19.7249920、19.4373321/20.0253024、16.8630825/19.3794829。开关差值混合了运行和后台负载波动，不能作为纯 profiler 开销。阶段累计加未归类等于精确 WRITE；三个粗粒度样本的未归类分别为 20.6、20.6、17.4 微秒。正式 P4 不读取详细阶段时钟，仍保留结构计数。

`stage-closure-check.json` 独立重算四个详细 profile 的累计，核算误差均为 0 ns。计时精度仍受实际时钟步长和插桩开销限制；正式运行最小观测非零间隔为 100 ns。

原 reset profile 的两个 total 为 14.8707236/15.1078582 s，正确中位数为 14.9892909 s，MAD 为 0.1185673 s。旧原始日志保留；distribution helper 已修正，并有偶数样本回归测试。

## 状态等价与恢复

每组比较完整 sqlite_schema，以及全部 16 张持久表的全部列和行，包括 sqlite_sequence、run_log、source_sync_status、空 worker_lock。比较不排除时间字段；仅 `cost_*` REAL 允许绝对误差 1e-9，其余类型和值严格一致。私有固定 audit clock 只用于审计字段，lease clock 保持真实时间。

普通回归覆盖 raw pricing、未知模型/模型层级/来源成本、其它 source、remote host 保护、重复同步幂等，以及 reset/events/cursor/source_file/raw/behavior_reset/turns/tool_calls 八个 failpoint 在 Baseline 与 Candidate 下的完整初态回滚。parser 控制检查扫描字节、changed files、插入数和实际进入的 reset 算法。

正式三个运行合计 128 次完整状态比较（含预热）全部通过，所有 digest 相等，最大成本误差为 0。writer 各场景另通过重复同步幂等检查。完整分布及逐组值见 `candidate-2-results.json`；便于阅读的范围表见 `candidate-2-results.md`；三份同名 JSON 提取文件保留每条原始结构记录。

## 验收结果与检查状态

主场景 median WRITE 为 16.9689661→2.6668043 s，减少 84.284%。历史放大、host skew、insertion、Codex append、duplicate behavior 和全部 parser 控制通过各自阈值。shared_bucket_reset median total 为 406.7318→471.0081 ms，退化 15.803%；Baseline 范围 381.8877–1025.5468 ms，Candidate 范围 333.2166–1219.2782 ms。

该单项失败使完整 P4 失败。退化原因未查明；离散程度不构成忽略失败的依据，不改用 paired median，不删除样本，也不选择性重跑得到通过结论。候选 2 尚不能作为验收成功的生产优化交付。

| 检查 | 结果 | 证据与范围 |
| --- | --- | --- |
| writer 正确性回归 | 34 passed / 6 ignored，exit 0 | `writer-final-tests.log`；在最后 reset provenance 与 parser 截断断言补充之前运行 |
| Clippy all-features / all-targets | exit 0 | `clippy.log`；最后 parser 截断断言补充前运行 |
| cargo fmt --check | exit 0 | `fmt.log`；冻结 release 前运行 |
| release 精确入口列表 | 1 test，exit 0 | `final-release-list.log`、`parser-candidate-2-list.log`、`writer-candidate-2-list.log` |
| host-skew 独立 15 组 | exit 0 | `host-skew-candidate-2.log` |
| 三个 parser 控制，各 15 组 | exit 0 | `parser-candidate-2.log` |
| 完整 writer 矩阵 | exit 101 | `writer-candidate-2-ab.log`；7 场景均执行，shared_bucket_reset 超过上限 |
| 最终全仓门禁 | 尚未执行 | 父会话 checker 统一执行 `just ci`；本实施会话不重复集成 targets / ci-rust |

## 资源与解释限制

事件 batch 保持 1000；单 writer；未增加缓存。各 manifest 记录 seed 单 shard 最大事件数、单份输入事件数以及 turns/tools。主场景每份输入为 21000 events/21000 turns/63000 tools；fixture 保留一份输入，计时前 clone 另一份，因此测试 harness 可同时保留两份输入。该数字不是进程峰值 RSS，也不包括 writer 已有的临时结构。host-skew seed 单批最大为 5000 events。完整数据库比较按主键排序流式读取，不将数据库全表装入内存。

结果来自固定合成 fixtures 和暖 OS cache。不能将合成收益直接换算为用户此前 105.2 s 的真实同步时间。未记录进程 RSS，因为生产 batch/cache 未改变。

候选 1 的首次 release `--list --exact` 曾以 `0xc0000005 STATUS_ACCESS_VIOLATION` 退出；相同可执行文件的直接重试成功列出 1 项，后续实际基准运行至阈值断言。原因未查明。原始 `host-skew-list.log` 和 `host-skew-list-retry.log` 均保留。候选 2 的精确列表和正式短控制均正常退出。

## 候选 3 与 P1–P4 证据映射

2026-09-28：候选 3 的四个正式入口均 exit 0。主场景 WRITE 中位数 19.1082996 → 2.5826381 s，减少 86.484208%；所有控制 total 比值不超过 1.068477886。136 次全状态比较全部通过，最大成本误差 0。完整数据与独立复算见 candidate-3-results.json/.md；原始 log 不改写、不择优。

| 条款 | 实施证据 | 验证边界 |
| --- | --- | --- |
| P1 可重跑同初态基准 | measurement-environment.json；candidate-3-build-identity.json；candidate-3-source.patch；candidate-3-new-sources/；candidate-3-runs.jsonl；四份 candidate-3 正式 log | 原 seed/fixture/SQLite 设置与轮数保持；7 或 15 对交替，每变体一次预热，准备/复制/clone/lock 获取在计时外；324 个文件 raw/LF 与 exe 在各入口前后核对一致 |
| P2 互斥阶段与精度 | baseline-profile.log；reset-profile.log；stage-closure-check.json；shared-bucket-candidate-2-profile.log/.json | 四份详细 profile 阶段核算误差 0 ns；BEGIN 区间单独记录，包含 SQLite 等待和 BEGIN 调用工作，不冒称纯等待；commit、未归类、计时外去重/prefix 与 Antigravity source apply 边界独立说明；最小观测计时间隔 100 ns |
| P3 持久等价、幂等与回滚 | candidate-3-writer-tests.log；candidate-3-{shared-bucket,writer,host,parser}.log/.json | 136 次比较覆盖 schema 与 16 表全部行列，时间字段不排除；35 项 writer 测试含八阶段失败回滚、幂等、pricing/raw、host 隔离和 stale generation 拒绝提交。同步引擎取消与跨域集成的最终门禁由 checker 的 just ci 确认 |
| P4 主收益与控制门槛 | candidate-3-results.json/.md；summarize-candidate-3.py；candidate-3-final-observation.json | 主 WRITE 比值 0.135157924；全部控制 total 比值 ≤1.068477886；每组分布保留并独立复算，先行/矩阵 shared、独立/矩阵 host、writer/parser 同名 workload 分开；数值门槛通过，PRD 由独立 checker 后确认 |

候选 3 的范围决定见 candidate-3-decision.md：单路径优化没有通过候选 2 的正式控制，因此保留单路径原计划，只在多个不同路径上使用有界自适应选路。候选 2 的退化原因仍未查明，不能将该范围决定写为已定位 COUNT 根因。

最终源/exe 观察位于 candidate-3-final-observation.json（09:32:17Z，324 文件 raw/LF、HEAD、patch、新增源快照、exe 全部一致，35 份 log SHA）。当前原生 Cargo 路径/版本已记录；历史裸 cargo 进程路径未采集，mbx 来源未确定。正式测量直接调用固定 exe。Cargo 已释放；实施会话不再编译或运行基准，最终 just ci 由 checker 执行。
