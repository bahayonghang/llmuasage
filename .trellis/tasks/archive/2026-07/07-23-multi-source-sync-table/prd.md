# 新增多来源统计并表格化 sync 输出（父任务 / 伞任务）

> 本任务是**伞任务**：拥有来源需求全集、研究证据、跨子任务验收标准、共享契约与最终集成评审。实现工作在三个子任务中完成，父任务本身不作为主要实现目标（除集成/文档收尾外无直接产品代码）。

## Goal

让 `llmusage sync` 在不破坏现有 parser/store/query、SQLite 写入和增量同步契约的前提下，正式采集 Kimi Code（含 K3）与 Pi 兼容会话（含 Oh My Pi），并把成功终态收敛成一张可扫描、稳定、可测试的表格。Reasonix 不阻塞本轮交付。

## Task Map

| 子任务 | 目录 | 职责 | 承接需求 | 独立性 |
| --- | --- | --- | --- | --- |
| 表格化 sync 输出 | `07-23-sync-table-output` | 展示层：单表 + `TOTAL`、去重复完成句、宽度自适应 | R7, R8 | 对现有 3 个来源即可完整验收 |
| Kimi Code 来源 | `07-23-kimi-code-source` | `kimi_code` parser 纵向切片 | R3, R6（Kimi）| 完整纵向切片，独立验收 |
| Pi / Oh My Pi 来源 | `07-23-pi-omp-source` | 单一 `pi` parser（两 roots）纵向切片 | R4, R6（Pi）| 完整纵向切片，独立验收 |

父任务保留：R1（研究）、R2（统一来源边界，跨子任务契约）、R5（Reasonix monitor-only / 排除）、R9（文档与 SQLite 兼容，跨子任务）。

> 依赖关系不由树位置隐含。子任务间无逻辑依赖；三者编辑若干相同文件的不同行（见 `design.md` 共享文件表），属**合并协调**，具体协调写在各子任务 `implement.md`。

## Background And Evidence

- 用户要求结合 `ref/repo/ccusage`、`ref/repo/tokscale` 和网络搜索建立 Trellis 任务；证据集中在 `research/`。
- 重复输出根因：`src/commands/sync_progress.rs:253-263` 与 `:532-538` 的 `SourceFinished` 永久完成句，与 `src/commands/sync.rs:283-287` 的最终摘要并行；表格渲染器已存在于 `src/commands/sync_summary.rs`。
- 现架构用 `SourceKind`（`src/domain/models.rs`）、descriptor（`src/domain/source_descriptor.rs`）、parser registry（`src/registry.rs`）、`SourceParser`（`src/parsers/source_parser.rs`）、`FileCursor`（`src/store/mod.rs`）承载来源接入；新增来源不得绕过这些边界。
- 准入门槛见 `docs/agents/passive-parser-onboarding.md`。
- 本机只读检查（2026-07-23）：Kimi Code 22 个 `wire.jsonl` / 1099 条 turn usage；Oh My Pi 3 个 JSONL / 8 条 assistant usage；`~/.pi/agent/sessions` 本机缺失；Reasonix 当前会话无 usage-bearing 行。

## Shared / Cross-Cutting Requirements（父任务持有）

- R1. 研究材料记录每个候选来源的 artifact 路径、格式、模型标识、token/费用字段、事件身份、游标可行性、隐私边界和证据日期。（见 `research/`）
- R2. 新来源必须通过统一 `SourceKind` / descriptor / parser registry / source-file state / `SyncShard`/cursor / query-report 链路；禁止只因发现目录就写入 usage rows。
- R5. Reasonix 默认 monitor-only；只有证明当前稳定会话格式携带可重放 usage 语义并通过准入门槛，才另开后续范围，不得把旧 telemetry 聚合文件当作当前会话事件。
- R9. 更新 `README.md` / `README.zh-CN.md` 与对应 VitePress 页面；保留现有来源与 SQLite 数据兼容性，除非验证证明需要迁移，否则不新增 schema migration。

> 逐来源需求 R3/R4/R6/R7/R8 已下放到对应子任务 `prd.md`。

## Cross-Child Acceptance Criteria（集成级）

- [ ] AC1. `research/` 以本地代码、两个 pinned reference commits 和可追溯官方 URL 证明每个来源的路径/格式/语义，并标注 2026-07-23 证据日期、不确定项和样本缺口。
- [ ] AC2. K3 及未来 Kimi 原始模型、Pi 未来模型在跨层 query/report 中保留；Reasonix 保持排除且不阻塞其他交付。
- [ ] AC3. 三子任务合并后，现有来源回归、新来源聚焦测试、`cargo fmt`、Clippy、Rust tests、文档构建与 `just ci` 全绿。
- [ ] AC4. README 与 VitePress 同步说明来源路径、质量标签、游标/幂等限制、Reasonix 已知缺口及新的 sync 表格契约。
- [ ] AC5. 三个子任务各自的 AC（见其 `prd.md`）全部满足。

## Out Of Scope

- Cursor：当前没有可由 `llmusage` 从本地直接读取的 source-owned usage artifact。
- Grok Build：本地会话只持久化可回退的上下文计数，不持久化可可靠累计的逐请求 token usage ledger（见 `research/upstream-local-evidence.md`）。
- 不把目录探测、旧 telemetry 汇总或外部 OTEL 指标当作稳定 usage parser。
- 不在规划审查前运行 `task.py start`、修改产品代码、执行破坏性 rebuild、提交或推送。
