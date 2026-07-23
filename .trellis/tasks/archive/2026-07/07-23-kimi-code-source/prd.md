# 新增 Kimi Code 数据源

> 父任务：`07-23-multi-source-sync-table`。本子任务新增一个 parser-backed 来源，须遵守父任务 `design.md` 的共享来源边界与 token 记账契约。

## Goal

把 Kimi Code 会话（`~/.kimi-code/sessions/**/wire.jsonl`，含 K3 及未来模型）通过统一的 `SourceKind` / descriptor / parser registry / `FileCursor` / `SyncShard` / query-report 链路正式接入，只采集 `usageScope=turn` 的 `usage.record`，保留原始模型标识，不因发现目录就写入 usage rows。

## Background And Evidence

- 本机只读采样（2026-07-23，见父任务 `research/upstream-local-evidence.md`）：22 个 `wire.jsonl`、1099 条有效 `usage.record`，全部 `usageScope=turn`；观测模型 `kimi-code/k3`；usage 键为 `inputOther` / `output` / `inputCacheRead` / `inputCacheCreation`；记录无稳定 id。
- 参考实现（pinned，见父任务 `research/reference-repositories.md`）：`tokscale` `crates/tokscale-core/src/sessions/kimi.rs` 记录了 `usage.record` / `usageScope=turn` / camelCase / 时间戳 / 模型归一，并有不依赖用户 transcript 的测试；`ccusage` kimi adapter 同时识别 `~/.kimi` 与 `~/.kimi-code`。参考仓库只作字段映射与游标策略证据，准入以本仓 `docs/agents/passive-parser-onboarding.md` 为准。

## Requirements

- R1. 读取 `~/.kimi-code/sessions/**/wire.jsonl`，支持 `KIMI_CODE_HOME` 或等价显式根。
- R2. 只计 `type=usage.record` 且显式 `usageScope=turn`；忽略 session 聚合、重复 `step.end`、零 token、非 turn、损坏行——以逐文件诊断计数处理，不让整源失败。
- R3. token 映射：`inputOther → input_tokens`、`output → output_tokens`、`inputCacheRead → cache_read_tokens`、`inputCacheCreation → cache_creation_tokens`；以 `time` 作事件时间戳。
- R4. 保留原始 `model` 字符串（含 `kimi-code/k3` 及未来后缀），不做硬编码白名单过滤；模型在 fixture 与 query/report 中可见。
- R5. 事件身份由 source id + 隐私安全的路径身份 + 文件偏移/记录序号 + 时间戳 + 模型 + 归一 token 元组组成；`FileCursor` 只在 shard commit 成功后推进。
- R6. 通过统一来源边界接入：新增 `SourceKind` 变体、descriptor（display name / aliases / roots / `UsageQuality=precise` / 隐私类）、registry 注册、source status/monitor 投影。
- R7. 具备脱敏合成 fixture：K3、未知模型后缀、零/非 turn 记录、损坏行、首次 sync、重复 sync 幂等、追加、改写/截断、缺失根、source status。

## Acceptance Criteria

- [ ] AC1. Kimi Code 获得 parser-backed 接入矩阵，含脱敏 fixture 与首次/重复/追加/改写/损坏/缺失测试。（父任务 AC2 的 Kimi 部分）
- [ ] AC2. K3 及未来原始模型在 fixture 与 query/report 中保留，不被硬编码白名单过滤。（父任务 AC3 的 Kimi 部分）
- [ ] AC3. 重复 sync 不产生重复事件；改写/截断由 `FileCursor` 正确处理；游标只在 commit 后推进。
- [ ] AC4. `cargo fmt`、Clippy、Kimi 聚焦测试、`cargo test -- --test-threads=1` 跨层（source/query）通过。

## Out Of Scope

- 不做 sync 表格/进度展示改动（属 `07-23-sync-table-output`）。
- 不新增 SQLite migration；现有事件键与 source-scoped dedupe 足够承载 Kimi 记录，不存原始 transcript。
- Reasonix、Grok、Cursor 均不在本任务范围。

## Dependency And Coordination

- **可独立交付**：本任务是完整纵向切片（enum 变体 + descriptor + registry + parser + fixtures + query/report 可见性），不依赖 Pi 子任务。
- **共享文件协调**：会改 `src/domain/models.rs`（`SourceKind`）、`src/domain/source_descriptor.rs`、`src/registry.rs`，以及 `sync_summary.rs` / `sync_progress.rs` 的 `source_label` match（除非 `07-23-sync-table-output` 已改为从 descriptor 取显示名）。与 `07-23-pi-omp-source` 编辑相同文件的不同行，属合并协调而非逻辑依赖，见 `implement.md`。
