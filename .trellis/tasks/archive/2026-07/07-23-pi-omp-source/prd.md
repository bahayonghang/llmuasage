# 新增 Pi / Oh My Pi 数据源

> 父任务：`07-23-multi-source-sync-table`。本子任务新增一个 parser-backed 来源（Pi 与 Oh My Pi 合并为一个 `pi` 源），须遵守父任务 `design.md` 的共享来源边界与 token 记账契约。

## Goal

把 Pi 与 Oh My Pi 会话（`~/.pi/agent/sessions`、`~/.omp/agent/sessions`）合并为**一个**稳定 `pi` 源，通过统一来源边界接入，解析 Pi-compatible JSONL 中带 usage 的 assistant message，保留 input/output/cache read/cache write 与权威 total，reasoning 单独保存且不重复计入 output。

## Background And Evidence

- 本机只读采样（2026-07-23，见父任务 `research/upstream-local-evidence.md`）：`~/.pi/agent/sessions` 本机缺失；`~/.omp/agent/sessions` 有 3 个 JSONL、8 条带 usage 的 assistant 消息；usage 键为 `input` / `output` / `cacheRead` / `cacheWrite` / `totalTokens` / `reasoningTokens` / `cost`；观测模型 `gpt-5.5`、`codex-auto-review`；会话文件还含 `title` / `session` / `model_change` / thinking-level 等非 usage 行。
- 参考实现（pinned，见父任务 `research/reference-repositories.md`）：`ccusage` `adapter/pi/paths.rs` 默认 `~/.pi/agent/sessions`、支持 `PI_AGENT_DIR` 与附加 store；`adapter/pi/parser.rs` 过滤 assistant 消息并映射四通道与 `totalTokens`，`reasoningTokens` 保持独立。准入以本仓 `docs/agents/passive-parser-onboarding.md` 为准。

## Requirements

- R1. 枚举 `~/.pi/agent/sessions` 与 `~/.omp/agent/sessions` 两个默认根，对 canonical 路径去重，root 仅用于诊断/路径哈希，不产生重复 source 行。
- R2. 解析 type 缺省或为 `message` 且嵌套 message role 为 `assistant` 且带 usage 的记录；忽略 title/session/control/model_change 元数据行。
- R3. 映射 `input` / `output` / `cacheRead` / `cacheWrite` 到归一通道；可信 `totalTokens` 视为权威 total；`reasoningTokens` 存入 `reasoning_output_tokens`，在有权威 total 时不加入 output 或 total。
- R4. 保留原始模型名（含未来模型）；因并非所有 Pi 记录都暴露稳定 message id，用会话文件名/记录位置 + 路径身份做 dedupe。
- R5. 用 `FileCursor` 的 append/reparse 决策，每个变更文件或有界批次写一个原子 shard。
- R6. 通过统一来源边界接入：新增单个 `SourceKind::Pi` / `pi` 变体、descriptor（含两个 roots、`UsageQuality=precise`、隐私类）、registry 注册、source status/monitor 投影。
- R7. 具备脱敏合成 fixture：两根、缺失 Pi 根、OMP 记录、重复发现去重、损坏 usage、首次/重复/追加/改写、query/report 可见性。

## Acceptance Criteria

- [ ] AC1. Pi/Oh My Pi 获得 parser-backed 接入矩阵，含脱敏 fixture 与两根/缺失根/去重/损坏/首次/重复/追加/改写测试。（父任务 AC2 的 Pi 部分）
- [ ] AC2. Pi 与 Oh My Pi 表现为**一个** `pi` 源（两 roots 合并、无重复 source 行）；模型名在 query/report 保留。
- [ ] AC3. reasoning 单独保存、在有权威 total 时不重复计入；重复 sync 不产生重复事件。
- [ ] AC4. `cargo fmt`、Clippy、Pi 聚焦测试、`cargo test -- --test-threads=1` 跨层（source/query）通过。

## Out Of Scope

- 不做 sync 表格/进度展示改动（属 `07-23-sync-table-output`）。
- 不新增 SQLite migration；不存原始 transcript。
- Reasonix、Grok、Cursor 均不在本任务范围。

## Risks / Open Questions

- **Pi-proper 本机无真实样本**：只有 OMP 文件存在，`~/.pi/agent/sessions` 缺失。Pi 纯格式的接入证据依赖 OMP 样本 + 上游 `ccusage` Pi adapter 格式；须以合成 fixture 覆盖 Pi-only 字段，并在文档标注这一残留证据缺口（OMP 未必覆盖所有 Pi-only 字段）。

## Dependency And Coordination

- **可独立交付**：完整纵向切片（enum 变体 + descriptor + registry + parser + fixtures + query/report 可见性），不依赖 Kimi 子任务。
- **共享文件协调**：会改 `src/domain/models.rs`、`src/domain/source_descriptor.rs`、`src/registry.rs`，以及（若未解耦）`sync_summary.rs` / `sync_progress.rs` 的 `source_label`。与 `07-23-kimi-code-source` 编辑相同文件的不同行，属合并协调而非逻辑依赖，见 `implement.md`。
