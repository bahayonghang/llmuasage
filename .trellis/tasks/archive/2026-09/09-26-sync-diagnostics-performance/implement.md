# 实施顺序与验证

## 当前阶段

用户于2026-09-26在最终规划后明确授权："请按照trellis任务规划顺序开始实施"。三个子任务按既定顺序进入实施，父任务负责协调与整体验收。真实数据库重建、安装和发布仍不在授权范围。

## 顺序

- [x] 先完成用户补充授权的门禁修复：定位并修复三个本地时区相关 lib 失败；明确并实施 v1.2.0 公开 API 差异的版本/兼容性处理，再执行正式 semver 与跨表面门禁。保留此前失败证据。
- [x] 复核当前HEAD仍为29bde59；产品源码无新增变化，规划证据继续适用。安装版1.3.0的构建提交仍未知，不将当前源码等同于已安装二进制。
- [x] 先启动并完成 diagnostics-contracts 子任务；用其最终 DTO/错误分类契约约束后续工作。
- [x] 完成 antigravity-replay-preflight 的全阻断快速路径及回归。
- [x] write-profiling 先保存独立基准，再逐个改变实测热点。共享 DTO 变更在 diagnostics-contracts 完成后进行。
- [x] 每个子任务独立检查，父任务审核 R1–R8 与 AC1–AC7。
- [x] 更新对应源码契约及 README/双语 CLI 文档；执行跨表面最终门禁。
- [x] 保存命令、退出码、基准环境与未验证项，用户未授权前不执行真实修复/安装/发布。

## 规划交付检查（本轮）

对父任务和三个子任务运行 task.py validate；验证 task.json 互相关联、planning 状态、三份文档和非空真实 jsonl context；校验研究证据路径及链接、无占位项、无产品源码改动。只验证本轮规划，不把未来 AC 误标为完成。

## 实施门禁（后续）

- 针对子任务指定的 lib / sync / store / cli / remote 测试先运行。
- Rust 修改运行 python scripts/ci-rust.py。
- CLI 行为文档修改按 AGENTS.md 同步 README.md、README.zh-CN.md、docs/reference/cli.md、docs/zh/reference/cli.md。
- Rust 与 docs 跨表面最终执行 just ci；不执行 cargo update、不手改 lockfile。
- 性能基准单独记录，时间阈值不加入不稳定的常规 CI 测试。

## 回滚与停止条件

任一 totals/cost/bucket/cursor/marker 对照不一致即停止性能优化，回滚本次候选。若需要逐文件部分提交 Antigravity 或新 schema，返回规划。不得通过删除原始输入、更新 accounting marker 或 --allow-lossy-rebuild 消除失败。

## 门禁范围扩展（2026-09-27）

用户明确指示："先扩大范围修复这些门禁 ,然后继续"。本轮纳入原有三个时区相关 lib 失败，以及 `cargo semver-checks --baseline-rev v1.2.0` 报告的 11 类既有公开 API 差异。继续按 diagnostics → preflight → profiling 顺序实施；门禁修复插入 diagnostics 验收前。

时区修复由 query/filter 与 web 测试各自的时间契约约束。先区分实现缺陷与测试隐含本地时区假设，再修改最小所属层。不得通过全局修改测试进程时区、删断言或忽略测试使门禁通过。

API 修复先记录完整差异、现有来源/主机/成本功能和可行方案。不得切换到 HEAD 基线、忽略 lint 或使用 release-type 参数掩盖正式结果。是否需要主版本边界由现有 API 事实与兼容方案决定；不能把诊断任务的增量兼容检查代替正式门禁。实际产品版本、发布和安装为不同动作；本轮不发布、不安装。

2026-09-28：用户接受推荐的 2.0.0 版本边界，批准按该方案继续。先完成版本入口、迁移说明与正式 SemVer，再按既定顺序完成 Antigravity 预检及 writer profiling；不再重复询问该版本决策。

## 最终验收（2026-09-28）

三个子任务的独立审查完成，D1–D5/G1–G3、A1–A5、P1–P5 及父任务 AC1–AC7 全部通过。最终 `just ci` 于 09:35:09Z–09:39:13Z 执行，耗时 244.4963957 秒，退出码 0；四个 lockfile 前后字节及 SHA 一致。Root lib 922 passed / 19 ignored，八个集成 target 247 passed；Dashboard JS 66、Desktop frontend 65、端口脚本 4、Desktop Rust 33 通过。fmt、Clippy、TypeScript、构建及双语文档检查通过。

性能结果为 124 对正式 A/B、136 次完整状态比较一致、9 项幂等断言通过；主 WRITE 减少 86.4842%，最大控制 total 退化 6.8478%。完整证据见 writer 子任务的 `check-report.md`、`research/ci-final-verification.json` 和 `research/candidate-3-results.md`。门禁后 324 个构建输入及冻结 release exe 身份仍一致。

本轮仅更新验收与交付记录，不重复测试、不操作真实 usage 数据。用户已确认 Phase 3.4 方案，两个工作提交与三个子任务归档已完成。父任务完成状态记录在 task.json，最终 journal 记录完整交付。
