# 接入 Grok Build 被动用量源（grok）

## Goal

llmusage 当前统计 codex / claude / opencode / antigravity / kimi_code / pi 六个来源，缺少 Grok Build（xAI 编码代理 CLI）的 token 用量。本任务把 Grok Build 作为新的**被动只读源** `grok` 接入：sync 后其用量出现在概览、趋势、模型/来源分布与 source-status 中。

**MVP 范围裁定（2026-07-27 审核后确认）**：只统计 token，成本保持 `unpriced`。Grok Build 本地数据只有总量、无 input/output 拆分；现有计价接口（`src/query/pricing.rs` 的 `CostTokens`）只按子通道计算、不读 total，若添加 grok 价格行会得到"价格匹配成功但成本恒为 0"的错误精确性。因此 MVP 不添加 grok 定价行，成本列如实显示未估算。

参考实现：`ref/repo/tokscale`（`crates/tokscale-core/src/sessions/grok.rs`，已支持 Grok Build）；ccusage 明确不支持旧 Grok CLI（SQLite 无 token 数据），仅作反面证据。调研结论见 `research/grok-build-evidence.md`。

## Requirements

### 数据源与发现

- R1. 新增 `SourceKind::Grok`（source id：`grok`），纯被动读取，不写任何第三方配置、不装 hook。
- R2. 发现规则：`GROK_HOME` 环境变量，回退 `~/.grok`；扫描 `sessions/<urlencoded-workspace>/<session-id>/` 会话目录。workspace 目录名是 URL 编码路径，需 percent-decode 得到 workspace 标签（Windows 路径形如 `D%3A%5C...`）。
- R3. 会话目录枚举必须是**固定两级目录遍历**（`sessions/*/*/`），然后在会话目录内直接 join 白名单文件名（`updates.jsonl`、`signals.json`、`summary.json`，可选 `events.jsonl`）；**不得使用递归遍历**（现有 `source_files.rs` 的 `WalkDir` 全递归模式会进入 `terminal/` 子目录，其中含阻塞读取的特殊文件）。实现审查必须确认只存在两层 `read_dir` 且不调用 `WalkDir`；fixture 测试负责证明嵌套 `terminal/` 内容不会进入候选集或产生枚举错误，不把“结果未解析”误当成唯一证明。

### token 语义与取数

- R4. 双路径取数：
  - (a) `updates.jsonl` 中若存在累计 `totalTokens` 计数器（多候选 JSON 路径，见 research），按 `user_message_chunk` 切轮、轮内最大值减基线得逐轮增量；计数器按单调处理，回退值丢弃。
  - (b) `signals.json` 会话级对账：`effective_total = max(totalTokens, totalTokensBeforeCompaction + contextTokensUsed)`，超出 (a) 已计部分的差额补一条对账事件。本机 Grok Build 0.2.112 只有此路径有数据。
- R5. 质量声明：来源描述符（`SourceDescriptor.quality`，`src/domain/source_descriptor.rs`）声明 `UsageQuality::TotalOnly`。事件写入 `total_tokens` 权威总量，input/cache/output/reasoning 子通道为 0（具体归属遵循 token-accounting 契约对 total 权威字段的既有语义）。**不添加 grok 定价行**，`pricing_status` 如实为 `unpriced`；文档中说明成本不可估算的原因。
- R6. 模型归属：updates `_meta.modelId` → `signals.primaryModelId`/`modelsUsed[0]` → `summary.current_model_id` → `grok-unknown` 逐级回退。`UsageEvent.provider_label` 是 CCR 中继归属字段（`src/domain/models.rs:176`），**保持空串**，不得用于持久化 `xai`；provider 身份仅由模型名体现。
- R7. 对账事件时间戳锚定该会话最后一条 update 活动时间，**不得**使用 signals.json 的 mtime（活跃会话 mtime 持续刷新会把大额差额迁移到新的一天）。

### 幂等与重放

- R8. **会话 = 原子重放单元**：一个会话的全部事件共享同一 `source_path_hash`（取自会话目录标识）。任一已知 sidecar（updates.jsonl / signals.json / summary.json）变化时，解析器完整重算该会话并通过现有 `reset_path_hashes` 协议（`src/store/sync_writer.rs` 的 `reset_file_events_batch_tx`，claude/codex/kimi_code 已在用）删除旧事件、带聚合桶回退地重插。不做跨次增量续读，`FileCursor` 仅作变化检测（指纹/mtime/size），不做 offset 续读。
- R9. sync 两次（sidecar 无变化）不产生任何删除或插入；updates.jsonl 追加、signals.json 数值增长、文件截断/重建均通过重放收敛到正确总量，无重复、无残留。若一个已被 cursor/source-file inventory 追踪的 sidecar 消失，常规 sync **不得**用不完整会话重放覆盖旧事件；应标记 missing、保留历史，并让显式 rebuild 继续受现有 lossy-rebuild guard 保护。sidecar 恢复后再按完整会话重放收敛。

### 呈现与运维

- R10. `grok` 出现在 sync 汇总、`source-status`/doctor、TUI 与 web 看板的来源列表/筛选/分布中，视觉处理与其他源一致。
- R11. 无 `~/.grok`（或 `GROK_HOME`）目录时报告 `passive_no_data` 类状态，不报错。
- R12. 更新 `docs/agents/passive-source-candidates.md` 中 Grok 行（Monitor-only → 依门禁给出新决定），CLI 行为变化同步 `README.md`、`README.zh-CN.md` 及 docs 对应页面（含 unpriced 说明）。

### 隐私

- R13. 仅持久化归一化用量与会话元数据（session id、模型、workspace 标签、时间戳、token 总量）；不读取、不落库 `chat_history.jsonl` / `system_prompt.txt` / `prompt_context.json`；不存 git remote URL。

## Constraints

- 遵守 `docs/agents/passive-parser-onboarding.md` 门禁：fixture 全部脱敏，来源于本机真实样本结构（见 research）。
- 遵守 `.trellis/spec/llmusage/backend/` 相关契约：source-sync、token-accounting、write-fencing。
- 参考现有最近接入的 `pi` / `kimi_code` 的模块布局与测试形态，不引入新抽象。
- 实施基线：本仓库日常开发在 `dev` 分支，任务 `base_branch` 应为 `dev`（已修正 task.json，原为 main）。

## Out of Scope

- MVP 不添加 Grok 模型价格、成本估算或把 total 猜作 input。
- 不读取或归档聊天、提示词、终端文件及 git remote。
- 不为共享扫描器引入通用递归例外；Grok 使用独立固定两级枚举。
- 不与 `07-27-remove-hook-realtime-sync` 并行实施。
- 本任务依赖 `07-27-remove-hook-realtime-sync` 先完成；实现时直接使用其 passive-only descriptor/source-status 终态，不兼容已经删除的 hook activation 字段。

## Acceptance Criteria

- [x] fixture 解析测试：从脱敏样本（updates 逐轮增量、signals 对账、二者混合、空会话、中断会话）产出正确 `UsageEvent`；descriptor 声明 `TotalOnly`；事件子通道为 0、total 正确。
- [x] sync-twice 集成测试：sidecar 无变化时第二次 sync 零删除零新增。
- [x] 重放回归测试：updates.jsonl **追加**后重扫总量正确且无键碰撞；signals.json 数值增长后重扫收敛到新总量；updates 后续补齐逐轮数据时 signals 对账额被重放正确回收（不双算）；文件截断/重建不重复不丢失。
- [x] 扫描安全门禁：代码审查确认 Grok 枚举器仅两层 `read_dir`、无 `WalkDir`；fixture 含 `terminal/` 嵌套哨兵与 `*.lock`，测试确认候选集仅含会话根白名单文件且嵌套内容不产生枚举错误。
- [x] sidecar 删除回归：已追踪 sidecar 消失时常规 sync 标记 missing、旧事件与聚合保持不变；显式无损 rebuild 被 guard 拒绝；文件恢复后会话重放收敛且无重复。
- [x] source-status/probe 测试：无数据目录 → `passive_no_data`；有样本 → `passive_ready`。
- [x] 真机验证（隔离）：保留真实 `GROK_HOME`，用 `--home <临时目录>` 隔离 llmusage 数据库运行 sync；当前样本写入 2 条 grok-4.5 事件、155,329 token，`pricing_status=unpriced`，Dashboard 来源 API/页面来源清单含 grok；无解析错误。验证后已丢弃临时目录，未触碰默认用户库。
- [x] `docs/agents/passive-source-candidates.md`、README（中英）、docs 页面已更新（含 total_only/unpriced 说明）。
- [x] `just ci` 全绿。
