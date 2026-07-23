# Design — 新增 Pi / Oh My Pi 数据源

> 遵守父任务 `design.md` 的"共享来源边界"与"token 记账契约"。本文件只写 Pi/OMP 特定的解析与接入设计。

## Architecture And Boundaries

复用既有来源与 parser 管线，Pi 与 Oh My Pi 合并为**一个** `pi` 源：

- `src/domain/models.rs`：新增稳定 id `pi`（`SourceKind::Pi`），一个 id 覆盖 Pi 与 Oh My Pi。
- `src/domain/source_descriptor.rs`（及 `platform_monitor.rs`）：声明 display name、aliases、**两个 roots**、`UsageQuality::precise`、隐私类。
- `src/parsers/source_files.rs`：枚举两根并对 canonical 路径去重（root 仅用于诊断/路径哈希）。
- `src/parsers/pi.rs`（新建，名字是实现选择）：结构化解析、事件身份、游标、stats、shard commit，实现 `SourceParser`。
- `src/registry.rs`：通过 onboarding gate 后注册。
- 不新增 schema migration。

## Normalized Data Contract（Pi / Oh My Pi）

1. 枚举两个默认根，canonical 路径去重，root 只用于诊断/路径哈希。
2. 解析 type 缺省或 `message` 且嵌套 message role 为 `assistant` 且带 usage 的记录；忽略 title/session/control/model_change。
3. 映射 `input` / `output` / `cacheRead` / `cacheWrite` 到归一通道；可信 `totalTokens` 视为权威 total；`reasoningTokens` 存入 `reasoning_output_tokens`，有权威 total 时不加入 output/total。
4. 保留原始模型名（含未来模型）。因非所有 Pi 记录暴露稳定 message id，用会话文件名/记录位置 + 路径身份做 dedupe。
5. 用 `FileCursor` append/reparse 决策，每个变更文件或有界批次写一个原子 shard。

## Privacy Boundary

fixture 只含结构化字段与合成 id/时间戳/模型/token 计数；移除 `content`、prompt/response 正文、tool 参数、凭据、真实工作区路径。不保存原始归档。

## Verification Shape

- 单测：字段映射、reasoning 独立且不重复计入、模型保留、损坏 usage 忽略。
- 集成 fixture：两根合并为一个源、缺失 Pi 根、OMP 记录、重复发现去重、跑两次无重复事件、追加、改写。
- 跨层：query/report 模型保留、source 状态/monitor 输出。
- **Pi-only 覆盖缺口**：以合成 fixture 补齐 OMP 样本未覆盖的 Pi-only 字段，并在文档标注证据缺口。
