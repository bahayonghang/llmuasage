# 写入性能实施计划

- [x] 读取 performance-analysis.md 与 upstream-comparison.md，核对 writer 现有优化和原子性测试。
- [x] 先行只读研究形成 `research/implementation-seams.md`，定位现有 writer 顺序、Antigravity 计时差异、私有 profile、固定生成 fixtures 与全状态等价比较方案。尚未测量热点或实施候选。
- [x] 构建固定 fixtures/初态与可重跑基准；跑基线并保存精确命令、退出码和原始计时。
- [x] 增加最小内部阶段观测，确认时长边界不重叠、默认 CLI/NDJSON 契约不变。
- [x] 依据测量选择一个热点，先补准确 seam 的回归，再实施最小候选。
- [x] 比较全量持久输出，运行7组以上交替 A/B 匹配场景并记录分布；候选 3 四个入口通过，136 次全状态比较一致。候选 1/2 已拒绝并保留失败证据；checker 已确认 P3 同步引擎取消与跨域门禁。
- [x] 检查无变化、append、project replay、reset、取消、remote/host 隔离；确认无数据或隐私输出进入 Git。
- [x] 独立审阅测量结果与 P1–P5，更新对应 source-sync/write-fencing 契约。

~~~text
cargo test --locked --all-features --lib store::sync_writer::tests -- --test-threads=1
cargo test --locked --all-features --test sync -- --test-threads=1
cargo test --locked --all-features --test store -- --test-threads=1
cargo test --locked --all-features --test remote -- --test-threads=1
python scripts/ci-rust.py
~~~

复用已有 ignored insertion 基准检查测量工具（不将其视为 replay 基准）：

~~~text
cargo test --locked --all-features --release --lib activity_cost_index_sync_throughput_regression_stays_within_ten_percent -- --ignored --test-threads=1 --nocapture
cargo test --locked --all-features --release --lib top_sessions_cover_index_sync_throughput_regression_stays_within_ten_percent -- --ignored --test-threads=1 --nocapture
~~~

新增 replay/behavior 基准命令由准确入口确定后立即写回本文件，记录实际匹配测试数量。若本任务修改用户文档或其它表面，最终门禁升级为 just ci。时间阈值不放入易抖动的常规 CI。临时基准文件必须放入 tempfile 或既有忽略目录，不能复制用户数据库到仓库。每次成功门禁后不无理由重复执行。

## 已实现的私有基准入口（2026-09-28）

以下入口须确认精确匹配一项后运行。实际命名位于 profiling::tests。

```powershell
cargo test --locked --all-features --release --lib store::sync_writer::profiling::tests::sync_writer_replay_profile -- --list --exact
cargo test --locked --all-features --release --lib store::sync_writer::profiling::tests::sync_writer_replay_profile -- --exact --ignored --test-threads=1 --nocapture
```

TempDir 固定在工作区 target/writer-benchmark/ 下，测试退出自动清理。原始 JSON 从测试 stderr 捕获到本任务 research/。阶段一只执行原算法，尚未引入候选。

## 精确测试入口更新 2026-09-28

运行基准前采用相同过滤器 --list --exact，必须匹配一项；实际命名如下。

~~~text
cargo test --locked --all-features --release --lib store::sync_writer::profiling::tests::sync_writer_reset_profile -- --exact --ignored --test-threads=1 --nocapture
cargo test --locked --all-features --release --lib store::sync_writer::profiling::tests::sync_writer_host_skew_ab_acceptance -- --exact --ignored --test-threads=1 --nocapture
cargo test --locked --all-features --release --lib parsers::writer_benchmark::writer_parser_ab_acceptance -- --exact --ignored --test-threads=1 --nocapture
cargo test --locked --all-features --release --lib store::sync_writer::profiling::tests::sync_writer_replay_ab_acceptance -- --exact --ignored --test-threads=1 --nocapture
cargo test --locked --all-features --lib store::sync_writer -- --test-threads=1
~~~

首轮stage profile、细分reset profile exit0。原始reset两条total为14.8707236/15.1078582s，正确偶数中位数14.9892909s、MAD0.1185673s；旧原始log保留，新helper修正。baseline-code-identity.json是后采集，未声称对应首轮编译输入。固定path-index候选在50,000 remote/500 local共享path控制总耗时比2.03056，已拒绝；自适应候选无持久schema、cache、batch改变。

## 候选 2 正式测量状态

固定编译身份、完整原始 patch、新增源码位于 `research/candidate-2-build-identity.json`、`research/candidate-2-source.patch`、`research/candidate-2-new-sources/`。运行命令、时间、退出码位于 `research/candidate-2-runs.jsonl`。每个 ignored 入口均先使用 `--list --exact`，确认精确匹配 1 项。

2026-09-28：host-skew 15 组通过（median total ratio 0.986203）；三个 parser 控制各 15 组通过（hot 1.016833、append 1.009131、Claude project replay 0.850381）。完整 writer 矩阵退出码 101：主场景 7 组 median WRITE 16.9689661→2.6668043 s，减少 84.284%；shared_bucket_reset median total ratio 1.158031，超过 1.10 上限，P4 未完成。其余 writer 控制通过；合计 128 次全状态比较全部通过，成本误差 0。

全部结果见 `research/candidate-2-results.json` 与 `research/candidate-2-results.md`。末次源码、patch、exe 身份观察和原始日志 SHA256 位于 `research/candidate-2-final-observation.json`。不删除不利样本，不更改冻结产品；下一步先只读诊断 shared_bucket_reset。最终全仓 `just ci` 由父会话 checker 统一执行，本实施会话不重复整套集成 targets / ci-rust。详见 `research/implementation-evidence.md`。

## 候选 3 继续实施

经主会话授权，已完成同一 shared-bucket fixture 的 3 组详细阶段诊断。COUNT 中位 0.9186 ms，Candidate 2 reset 总阶段中位减少约 39.75 ms，不能认定 COUNT 导致正式退化。主会话据“单路径优化尚未通过正式控制”选择保守收缩适用范围，允许放弃该单路径诊断收益。完整依据与启动异常保留于 `research/candidate-3-decision.md`。

候选 3 按不同路径数量选路：单路径及重复同路径保留原默认 SQL/准备，多个不同路径保留 adaptive。已补单/多/重复路径、全状态、幂等与 host 隔离回归；正在执行 focused tests/fmt/Clippy。正式冻结后，先 shared-bucket 7 组，再原完整 writer/host/parser 矩阵；不改变正式样本数、fixture 或阈值。

## 候选 3 检查与固定身份（2026-09-28）

focused writer 检查为 35 passed / 8 ignored；fmt 和 Clippy all-features/all-targets 均 exit 0。未重跑已通过门禁。release 精确 list 匹配 1 项，exit 0。固定身份见 research/candidate-3-build-identity.json，包含 raw SHA256、normalized-LF SHA256、HEAD、完整 patch、新增源快照及 exe SHA256。

独立 shared-bucket 先行门禁完成原 7 对 AB/BA，total 中位数 341.6735 → 338.2685 ms，比值 0.99003435；WRITE 比值 0.98343770，exit 0。8 次完整状态比较通过，成本误差 0；Baseline 最大 total 1156.338 ms 保留。分布独立重算见 research/candidate-3-shared-bucket-check.json。完整 writer 矩阵于 09:04:02Z 启动，host/parser 尚待执行，不能据短控制宣告 P4 通过。

当前路径核对确认 C:/Users/lyh/.cargo/bin/cargo.exe 为 rustup 链接，Cargo 1.97.0。原检查 ledger 使用裸 cargo，未在进程启动时记录实际可执行路径；历史 mbx 日志的包装来源未确定，不补写为已证实的原生调用。后续正式性能运行直接调用冻结 exe，不触发 Cargo 构建。

## 候选 3 实施交接（2026-09-28）

四个正式入口均 exit 0：先行 shared 7 对、完整 writer（主/常规各 7 对，host 15 对）、独立 host 15 对、parser 三场景各 15 对。主 WRITE 19.1082996 → 2.5826381 s（减少 86.484208%）；全部控制 total 比值 ≤1.068477886。136 次完整状态比较全部通过。详细分布、原始样本和独立复算见 research/candidate-3-results.json/.md。所有独立/矩阵及 writer/parser 同名分布分开保存。

P1–P4 实施证据映射已写入 research/implementation-evidence.md。最终 raw/LF、HEAD、patch、新增源和 exe 观察通过，见 research/candidate-3-final-observation.json。Cargo 独占已释放；不再编译或启动基准。常规未变化/append/project replay/reset、host 隔离、八阶段失败回滚与 stale generation 拒绝提交已有对应证据；完整同步引擎取消、跨域集成、最终 just ci、独立审阅与 PRD 验收由主会话/checker 完成。

## 最终独立验收（2026-09-28）

checker 已完成 P1–P5 及父 AC1–AC7 的证据映射，未发现当前产品或正式门禁阻断项。最终 `just ci` 退出码 0，耗时 244.4963957 秒；其中 lib 922 passed / 19 ignored、八个集成 target 247 passed（sync 144、remote 8、store 2），其余跨表面检查均通过。精确数量、日志位置、源码身份和四个 lockfile 不变的结果见 `research/ci-final-verification.json` 与 `check-report.md`。

最终性能为 124 对正式 A/B、136 次完整状态比较、9 项幂等断言；主 WRITE 减少 86.4842%，最大控制 total 退化 6.8478%。先前候选的失败与证据限制保留。P1–P5 全部勾选；尚未提交或归档，不执行真实数据 sync/rebuild/reset。

## 提交与归档交接（2026-09-28）

用户已确认交付方案。工作提交 `cc7eed3`、`6389ceb` 已完成，产品文件字节保持不变。验收及门禁沿用已通过证据；本轮不重复测试。任务随后使用 Trellis 归档脚本标记 completed 并归档，context 路径已同步到同月归档位置。
