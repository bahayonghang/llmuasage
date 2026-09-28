# Sync 写入测量与定向优化：独立检查记录

日期：2026-09-28。任务：09-26-sync-write-profiling。审查代理：diagnostics_check_resume。

## 当前候选 3：完整独立验收通过

P1–P5 与最终跨表面门禁通过。未发现未修复的产品缺陷。候选 3 相对候选 2 仅修改 writer reset 选路范围及测试：单个不同路径（含重复列出）沿用默认 SQL，多个不同路径保留有界自适应选择。其余 322 个构建输入 raw SHA 不变；公开 DTO、事务、host 条件、bucket、behavior、marker 和隐私路径未新增变更。

四次正式运行、12 个运行/场景、124 对交替 A/B 全部通过。136 条 A/B 全库比较（含预热）digest 一致，最大成本误差 0；另 9 项 Candidate 幂等断言通过。主场景 WRITE 中位数下降 86.4842%；最大控制 total 中位数退化 6.8478%，低于固定 10% 上限。

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

主 WRITE 门槛 ≤0.80，其余各场景 total 门槛 ≤1.10。独立/矩阵的 shared 和 host 分别判断，不合并或择优。所有原始轮次、离群样本、min/max/median/IQR/MAD、配对比值及 ratio of medians 均已独立复算；最终 results.json 的 12 条汇总字段也与原始样本一致。详细分布见 research/profile-review.md 第七轮。

### Findings (fixed)

本轮没有修改产品、fixture、算法、测量边界或阈值。曾发现规范把未准备 unused statements 写成动态测试证明；主代理已改为静态惰性控制流证明，动态测试只证明阶段/选择/状态行为。canonical source-sync SHA 为 80f4d0a2245c000044ed5773cb2993dae946ad6acda12fc8cf7acd24aeb62eff；688 条规范摘录逐行及 SHA 匹配。两份自有报告现已更新为完整结果，历史失败记录保留。

### Findings (not fixed)

- 没有当前产品或正式门禁阻断项。候选 1/2 已拒绝结果保留；候选 2 shared 退化原因仍未查明，原始混合换行布局归档限制保留。
- 结果限于固定生成负载、单机 warm OS cache 和记录环境；未运行真实用户源 sync/rebuild/reset，未测量生产 500 个 Antigravity DB 的收益。
- 最终 cargo test 编译仍输出 MSVC 创建 import library 的 linker_messages 警告；Clippy -D warnings 与全部 recipe 均通过。未更改 lint/链接配置来消除环境输出。
- 先前间歇 CLI 日志断言失败及 MBX wrapper 启动访问冲突的原因未查明；本轮原生 Cargo 完整通过不构成旧故障的原因证明。

### Verification：最终 just ci

主代理释放 Cargo 后，本审查者只执行一次原始 just ci，未额外重跑 sync/store/remote、ci-rust 或 SemVer。区间为 2026-09-28 09:35:09.197594Z–09:39:13.693987Z，244.4963957 秒，退出码 0，error=null。命令范围 PATH 首项为 C:/Users/lyh/.cargo/bin；实际 Cargo 为该目录的 cargo.EXE（rustup 链接），版本 1.97.0。

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

证据：research/ci-final.log、ci-final-env.json、ci-final-result.json、ci-final-locks-before.json、ci-final-locks-after.json、ci-final-verification.json。四锁前后字节及 SHA 一致。门禁后重算 324 个输入 raw/LF SHA 全匹配；release exe SHA 仍为 427d7a7bbe0ca0b945611a0fd4b36706a4e6f04485b5c8639b20463b7809b385。35 份归档日志 size/SHA、7 份 supporting artifacts SHA 全部匹配。

19 项 ignored 保持显式基准策略。本任务四个性能入口已通过独立 release 运行，不能把常规 CI 的 ignored 行当作性能运行。此前 2.0.0 SemVer gate 已退出 0（0 checks、254 skipped），本轮无新公共 API，未重复运行；该结果只验证已批准 major 边界，不证明 v1.2.0 源兼容。

### P1–P5 与父任务验收映射

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

以下为候选 2 历史结论。其失败、局限与当时未执行最终门禁的状态均保留，不代表候选 3 当前状态。

## 候选 2 历史验收

**候选 2：拒绝，P4 失败。** 完整 writer 矩阵退出 101，唯一失败控制为 shared_bucket_reset：total 中位数退化 15.8031%，超过固定 10% 上限，原因未查明。以下历史结论来自冻结候选 2 的 HEAD、patch、新增源码快照及既有日志；候选 3 的审查状态单独记录。

## Findings (fixed)

本轮没有新增产品修复。报告已由部分结果更新为完整失败结论。以下前轮问题已由实施代理修正，已核对候选 2 冻结源码或正式日志：

- File: `src/store/sync_writer/profiling/tests.rs`
  - Issue: 偶数样本 median/MAD 使用上中位数；Codex 生成夹具带有 cache_creation；host-skew 峰值 manifest 未随 seed 分片更新。
  - Fix: 使用常规偶数中位数并增加回归；Codex cache_creation=0；host seed_max_retained_events=5000。正式统计独立复算一致。
- File: `src/store/sync_writer.rs`
  - Issue: host 分支计数名称容易被解读为强制使用 host 索引。
  - Fix: 冻结版使用 `reset_default_plan_paths`；默认 SQL 保留 SQLite 选路。host manifest 单独记录实际查询计划。
- File: `src/parsers/writer_benchmark.rs`
  - Issue: 原控制只报告 variant，缺少 reset 实际入口及精确 WRITE 与公开 write_ms 的一致性证据。
  - Fix: 输出 records/reset_algorithms，并断言实际入口；新增每 Record 不到 1 ms 的公开值截断误差检查。正式 parser 测试退出 0。

## Findings (not fixed)

1. **P4 失败。** shared_bucket_reset 的 Baseline/Candidate total 中位数为 406.7318/471.0081 ms，比值 1.1580311645。WRITE 比值亦为 1.1397125069。该失败不被主场景收益抵销。两侧输入、业务计数、计时边界和持久结果一致；退化原因未查明。本代理没有修改产品或重跑测试。
2. **冻结输入的原始换行布局还原限制。** 324 条 build/final 源码 SHA 记录一致。HEAD、32 文件 patch 和四个新源快照可直接还原 296/324 个文件的记录字节。按主代理补充授权，仅对 raw SHA 仍等于候选 2 的文件读取当前字节：其余 28/28 均满足该条件，均为 CRLF/LF 混合；将 CRLF 归一 LF 后，28/28 与 HEAD+patch 还原内容逐字节一致。因此全部 324 个文件的规范化内容均已覆盖，没有非换行内容遗漏证据；现有归档材料仍不保留 28 项原始混合换行布局。关键 sync_writer.rs 的记录 SHA 可按 CRLF 精确还原。本代理未修改换行或借用候选 3 变更。
3. **正式门禁尚未通过。** 较早 fmt、clippy、编译及 writer 单测成功不能覆盖本次性能失败。本轮没有运行 Cargo、性能测量或 just ci；下一候选仍须独立验收。
4. **历史观察边界。** final-observation 是三次运行结束后的单次观察，不能反推每次运行前后的源码状态。首轮探索 profile 的事后身份采集限制继续保留。当前证据未证明任何一次运行中存在源码或 exe 漂移。

## Verification

- Lint: 冻结的 `research/fmt.log`、`research/clippy.log` 退出 0；本轮仅核对日志及 SHA。
- TypeCheck: `research/final-release-list.log` 的 all-features release lib-test 编译成功且退出 0；本轮未运行 Cargo。
- Tests: 既有 writer 单测 34 passed、6 ignored、0 failed；独立 host/parser 两入口各 1 passed、退出 0；完整 writer 性能入口 1 failed、退出 101。
- 独立复算：3 次运行、11 个运行/场景、117 对正式 A/B；预热排除、交替顺序、所有原始样本及 min/max/median/IQR/MAD、配对比值和门槛全部一致。三个提取 JSON 与原始日志逐记录相同。
- 状态比较：128 条 A/B 全库比较记录（含预热）均 digest 相同、最大成本误差 0；另有 8 个 writer 类场景的 Candidate 二次 replay 幂等断言通过。实际 reset 入口符合 variant。
- 身份与日志：18 份日志的大小与 SHA 均符合 final-observation；三条 run ledger 的 exe SHA、退出码和汇总一致；完整 patch 和四份新增源码快照的 SHA 匹配。324 条前后源文件身份记录一致，规范化源内容 324/324 已覆盖；28 项混合换行布局的原始字节还原限制见 Findings (not fixed)。
- 计时闭合：stage-closure-check 的四条详细记录均从原日志独立复算，WRITE=互斥阶段+未归类时间，误差 0；pre-WRITE 另列。
- 前轮契约核查继续有效：source-sync 自适应选择、write-fencing audit/lease 边界及当时 674 条规范摘录一致。本轮未将新源码或新契约回填到候选 2。

| 运行 / 场景 | 正式对数 | 指标 | Baseline 中位数 ms | Candidate 中位数 ms | 比值 | 结果 |
| --- | ---: | --- | ---: | ---: | ---: | --- |
| 独立 host / host_shared_path_skew | 15 | total | 285.7078 | 281.7658 | 0.986203 | 通过 |
| parser / codex_hot | 15 | total | 19.3187 | 19.6439 | 1.016833 | 通过 |
| parser / codex_append | 15 | total | 35.0247 | 35.3445 | 1.009131 | 通过 |
| parser / claude_project_replay | 15 | total | 245.6370 | 208.8850 | 0.850381 | 通过 |
| writer / claude_replay_primary | 7 | WRITE | 16968.9661 | 2666.8043 | 0.157158 | 通过 |
| writer / claude_replay_history | 7 | total | 100676.5598 | 3275.2698 | 0.032533 | 通过 |
| writer / host_shared_path_skew | 15 | total | 277.7239 | 289.6817 | 1.043056 | 通过 |
| writer / insertion | 7 | total | 123.2076 | 123.9292 | 1.005857 | 通过 |
| writer / codex_append | 7 | total | 426.1445 | 445.3073 | 1.044968 | 通过 |
| writer / duplicate_behavior | 7 | total | 909.7907 | 843.9464 | 0.927627 | 通过 |
| writer / shared_bucket_reset | 7 | total | 406.7318 | 471.0081 | 1.158031 | **失败** |

主场景 WRITE 门槛为 ≤0.80，其余 total 门槛为 ≤1.10。独立 host 15 对和矩阵 host 15 对各自判断，没有合并或选择较优结果。主场景 WRITE 中位数下降 84.2842%，但候选 2 因 shared 控制失败而拒绝。详细证据见 `research/profile-review.md` 第五轮及原始日志。

正式 A/B 关闭详细 Stage 时钟与阶段累计，保留轻量结构计数和 reset_algorithms。Candidate 另外记录选路次数。报告不把关闭详细阶段计时等同于采集器开销为零。

shared 控制每样本均为 2050 events、2050 turns、6150 tools、删除 2000 events、3 个 event batches、4 个 touched bucket candidates。Baseline 的默认计划使用 host/source 索引；Candidate 的七个正式样本均进入 adaptive 并选择 path index。两侧测量使用同一函数与起止点。新增 COUNT 和 statement 准备工作存在，但无该控制的详细阶段耗时，无法据此解释 64.2763 ms 的 total 中位数差。配对比值中位数 1.064476 仅为诊断量，不能替换预先固定的两个 total 中位数之比。
