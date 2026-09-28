# 实施与验收汇总

日期：2026-09-28。主任务及三个子任务已获实施授权；门禁修复和 2.0.0 版本边界已获补充授权。验收基线为 `29bde59e84e1149b1d9215e020352586fb0a0ed7`；工作改动已按确认方案提交为 `cc7eed3` 和 `6389ceb`。提交前后产品文件字节不变。

## 已完成的行为改动

| 原始问题 | 当前实现与证据 |
| --- | --- |
| legacy accounting 与缺失 Antigravity 库难以区分 | 源级原因独立持久化，保留历史，避免增加 malformed 行数。缺失、范围外、不可读分别使用闭集原因码。见 diagnostics-contracts 的 `check-report.md`。 |
| Codex 跳过记录原因不明确，13 条只显示 8 条 | 已识别的超长非 usage 记录显示 `oversized_non_usage_record`；摘要保留总计、8 条上限及省略数。保留 basename 与 byte offset，默认输出不包含正文和完整私有路径。 |
| Grok `usage_incomplete` 缺少位置 | 原生可用用量继续保留，增加安全位置；原生 usage 与 fallback 排他，不用默认 0 消除诊断。 |
| 锁日志与警告交错 | TTY、非 TTY 和 NDJSON 的输出边界已回归；stdout NDJSON 保持纯净。 |
| 全部选中 Antigravity 产品已阻断仍发生解码 | 在完成发现与 fingerprint 后执行覆盖预检；全部阻断时实际 usage decoder 调用为 0。部分阻断保留跨 root 解码和身份归属。见 replay-preflight 的 `check-report.md`。 |
| 原有门禁失败 | 校正本地 IANA/DST 契约与 UTC fixture，修复桌面测试格式；按批准的主版本边界处理既有公开 API 破坏性差异。 |

以上行为由隔离 fixture 和正式门禁验证。本次没有重新运行用户原始数据的 sync，因此没有新的真实来源写入数量或端到端运行时间。原始缺失路径的原因仍未查明，旧会计版本和缺失历史仍须按显式修复边界处理。

## 版本与已通过门禁

2.0.0 的根 crate、desktop crate、Tauri、npm 和双语文档版本一致。迁移说明位于 `docs/reference/migration-v2.md` 和 `docs/zh/reference/migration-v2.md`。第三方依赖结构未变化。

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| `cargo semver-checks --baseline-rev v1.2.0` | exit 0；真实 `1.2.0 → 2.0.0` 主版本边界；0 checks、254 skip | diagnostics-contracts 的 `semver-2.0.log`、`version-boundary-evidence.md` |
| 版本阶段 `just ci` | exit 0，276.111 秒；四个 lockfile 字节不变 | diagnostics-contracts 的 `ci-v2-result.json`、`ci-v2.log` |
| 预检阶段 `just ci` | exit 0，251.635 秒；lib 915 passed / 12 ignored，集成 247 passed；四个 lockfile 字节不变 | replay-preflight 的 `ci-preflight-result.json`、`ci-preflight.log` |
| 最终集成 `just ci` | exit 0，244.4963957 秒；lib 922 passed / 19 ignored，集成 247 passed；四个 lockfile 字节不变 | write-profiling 的 `research/ci-final-result.json`、`ci-final-verification.json`、`ci-final.log` |

SemVer 结果验证主版本边界符合工具规则，不表示恢复 v1.2.0 源码兼容性。早期网络、工具及门禁失败日志均保留，并与后续成功记录分开。最终 writer 集成门禁已由独立 checker 执行并通过，没有使用此前门禁替代。

最终门禁于 2026-09-28 09:35:09Z–09:39:13Z 执行，使用原生 Cargo 1.97.0。Root fmt、Clippy `-D warnings`、rustdoc 均通过；八个集成 target 为 api 3、architecture_dependencies 12、CLI 34、query 9、remote 8、store 2、sync 144、TUI 35。Dashboard/scripts JavaScript 66、Desktop frontend 65、端口脚本 4、Desktop Rust 33 全部通过；TypeScript、Vite 和 VitePress 构建通过。19 项 ignored 保持显式基准策略，正式性能运行单独取证。成功编译仍有 MSVC `linker_messages`，未修改 lint 或链接配置。

## 写入优化验收

最终候选仅对多个不同路径的 reset 批次执行有界自适应选路。单个不同路径（包括重复列出）保留原 SQL 和准备过程。没有增加持久索引、schema、cache、batch 或并发 writer；host 隔离、路径顺序、bucket/pricing、行为事实、事务和 token 语义保持。

固定主场景为 50000 个初始事件，重放 20000 个事件并新增 1000 个事件，涉及 40 个路径及相应 turns/tools。原算法详细 profile 中 reset 占 WRITE 的 84.44%–87.35%。该定位适用于合成场景，不直接解释用户安装版 1.3.0 的全部 WRITE。

| 场景与指标 | Baseline 中位数（秒） | Candidate 中位数（秒） | 变化 | 验收 |
| --- | ---: | ---: | ---: | --- |
| 主重放 WRITE | 19.1082996 | 2.5826381 | 减少 86.48% | 达到至少减少 20% |
| 250000 事件历史控制 total | 73.4709747 | 2.4816807 | 减少 96.62% | 通过 |
| 矩阵内共享 bucket total（最差控制） | 0.2817844 | 0.3010804 | 增加 6.85% | 低于 10% 上限 |

四次正式运行、12 个运行/场景、124 对交替 A/B 全部通过。每侧一次预热并排除预热后计算统计；主场景和常规 writer 控制为 7 对，host 和 parser 控制为 15 对。先行 shared、矩阵 shared、独立 host、矩阵 host、writer/parser 同名场景分别报告，不合并或选择有利结果。全部 min/max、IQR、MAD、配对比值和原始样本见 write-profiling 的 `research/candidate-3-results.md`、`candidate-3-results.json` 及四份原始日志。

136 次完整 SQLite schema 与 16 表全行全列比较均通过，digest 一致、最大成本误差 0；另有 9 项幂等断言通过。私有阶段累计闭合、公开 WRITE 截断精度和实际 reset 分支已独立复核。324 个源码/配置输入的 raw/LF SHA、HEAD、完整源码 patch、新增源快照和 release exe 在各正式运行前后均一致，末次观测为 `research/candidate-3-final-observation.json`。

候选 1 的 host 控制退化 103.06%，候选 2 的 shared 控制退化 15.80%，均按失败保留。候选 2 退化原因未查明；后续详细 profile 不支持把 COUNT 直接认定为原因。候选 3 的依据是收缩优化适用范围，保留单路径原查询。早期粗 profile 身份采集时点、候选 2 原始混合换行还原、历史裸 Cargo 进程路径等证据限制均保留，未用新观测补造旧记录。

三个子任务的 D1–D5/G1–G3、A1–A5、P1–P5 和父任务 AC1–AC7 已验收。参考机制的提交、精确源码位置、适配点及反例见 `research/reference-adaptation.md`，两个参考仓库提交已复核未变。最终跨表面 `just ci` 已通过，AC7 已勾选。门禁后 324 个构建输入 raw/LF SHA、冻结 release exe、35 份日志和 7 份支持材料身份核对一致。

## 本地交付记录

用户于 2026-09-28 确认两组本地工作提交及任务归档、journal 记录。工作提交：

- `cc7eed395b56d3c9ee524aa69cc8a3a1778046e0`：fix(测试): 🐛 校正时区回归与桌面格式门禁（4 文件）。
- `6389cebb9a5f60e624da3122eb3ac18371e450eb`：feat(同步)!: ✨ 完善诊断与重放写入并准备 2.0 版本（63 文件）。

子任务按 diagnostics、preflight、profiling 顺序归档：

- `82929bb32a39ec25c4de51d837923c91b8b8e190`：09-26-sync-diagnostics-contracts。
- `ef62682ce0971805d9725e26a39ac74c682c360c`：09-26-antigravity-replay-preflight。
- `09e2944a28d76ca0093ba77d5796bb2addfcda39`：09-26-sync-write-profiling。

父任务完成状态记录在 task.json；最终 journal 记录归档及工作区核对结果。context 的任务内路径同步到 archive/2026-09，历史日志、构建身份、失败候选和门禁证据保持原内容。归档脚本自动提交故障及限定目录恢复见 `research/archive-recovery.md`。

未执行真实数据 sync/rebuild/reset、删除历史、安装、发布、tag 或 push。已识别的工作和任务材料均按授权范围提交，未纳入 SQLite、WAL、SHM 或二进制文件。
