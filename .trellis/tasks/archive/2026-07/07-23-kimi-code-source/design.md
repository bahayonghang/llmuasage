# Design — 新增 Kimi Code 数据源

> 遵守父任务 `design.md` 的"共享来源边界"与"token 记账契约"。本文件只写 Kimi 特定的解析与接入设计。

## Architecture And Boundaries

复用既有来源与 parser 管线，不绕过任何边界：

- `src/domain/models.rs`：新增稳定 id `kimi_code`（`SourceKind::KimiCode`），补 `as_str` / 解析路径。
- `src/domain/source_descriptor.rs`（及 `platform_monitor.rs`）：声明 display name、aliases、roots、`UsageQuality::precise`、隐私类；不制造假 usage rows。
- `src/parsers/source_files.rs`：枚举 Kimi 根下的 `wire.jsonl` 候选。
- `src/parsers/kimi_code.rs`（新建，名字是实现选择）：结构化解析、事件身份、游标构造、stats、shard commit，实现 `SourceParser`。
- `src/registry.rs`：仅在通过 onboarding gate 后注册进 `registered_parsers()`；注册即纳入 rebuild/token-accounting 边界。
- 不新增 schema migration。

## Normalized Data Contract（Kimi Code）

1. 在配置的 Kimi sessions 根下发现 `wire.jsonl`。
2. 只解析 `type=usage.record` 且显式 `usageScope=turn`。
3. 映射 `inputOther → input_tokens`、`output → output_tokens`、`inputCacheRead → cache_read_tokens`、`inputCacheCreation → cache_creation_tokens`。
4. 以 `time` 为事件时间戳；保留原始 `model`（含 `kimi-code/k3`，除非既有 model 工具要求仅"展示层"归一）。
5. 事件键 = source id + 隐私安全路径身份 + 文件偏移/记录序号 + 时间戳 + 模型 + 归一 token 元组。`FileCursor` 只在 shard commit 成功后推进。
6. 零 token、损坏、非 turn、session 聚合、重复 `step.end` 记录以逐文件诊断计数忽略，不使整源失败。
7. 用 `src/parsers/file_state.rs` 的 append-vs-full-reparse 状态机判断改写/截断/轮转，不另造并行游标。

## Privacy Boundary

fixture 只含结构化 JSON/JSONL 字段与合成 id/时间戳/模型/token 计数；移除 `content`、prompt/response 正文、tool 参数、凭据、真实工作区路径。不为本源保存原始归档。

## Verification Shape

- 单测：字段映射、K3/未知模型保留、零/非 turn/损坏行忽略计数。
- 集成 fixture：跑两次断言无重复事件；追加；改写/截断；删除文件；缺失根；source status(`passive_ready`/`passive_no_data`)。
- 跨层：query/report 中模型保留、source 状态/monitor 输出。
