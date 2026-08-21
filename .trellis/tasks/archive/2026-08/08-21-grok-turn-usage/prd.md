# 修正 Grok Build 用量少计并补全趋势来源占比

## Goal

用量趋势在当前时间窗口内按真实请求用量排列来源。使用 Grok Build / grok 4.6 时，grok 应出现在窗口来源排序的正确位置，而不是被上下文占用口径压到后列，或被来源表的 2 行截断丢掉。

用户价值：看板「用量趋势」的来源表和观察文案与本机实际使用一致，后续模型分布、概览总量也使用同一套 grok 事件。

## Background

2026-07-27 任务 `07-27-grok-build-passive-source` 按当时 Grok Build 0.2.112 接入 `grok`：本地 sidecar 几乎只有上下文占用（`_meta.totalTokens` / `signals.contextTokensUsed`），因此标记 `total_only` 且 `unpriced`。

2026-08-21 本机样本显示 Grok Build 已在 `updates.jsonl` 的 `turn_completed` 上写出 `params.update.usage`（input / output / cache / reasoning / total，以及 `modelUsage`）。解析器仍走旧路径。证据与测量数字见 `research/grok-turn-usage-evidence.md`。

截图窗口 `7d`：库内 `codex` 346M 与页面 340.3M 一致；库内 grok 29.5M；本机 `turn_completed.usage.totalTokens` 近 7 天求和 1.584B，约为 54 倍少计。

## Confirmed Facts

- Grok 发现规则与会话原子重放仍然正确，不是漏扫。
- `params._meta.totalTokens` 与 `signals.contextTokensUsed` 是上下文窗口占用，不是请求用量。
- `params.update.usage` 出现在带 `usage` 的 `turn_completed` 上；一条记录已聚合 `numTurns`/`modelCalls`；会话内记录非单调，必须逐条入库而不是取最后一条或当累计计数器。
- `totalTokens = inputTokens + outputTokens`；`inputTokens` 含 `cachedReadTokens`；`reasoningTokens` 含在 `outputTokens` 内。
- 无 `usage` 的会话（本机 35/257）仍只能走旧 total_only 回退。
- 趋势页 `render/trends.js` 将 `context.panels.sources` 硬编码 `.slice(0, 2)`。`PANEL_LIMITS.sources` 为 4。
- 定价目录无 grok 行。`costUsdTicks` 单位未验证。

## Requirements

### Grok 用量口径

- R1. 对每个 Grok 会话，若 `updates.jsonl` 中存在至少一条 `sessionUpdate == "turn_completed"` 且 `params.update.usage` 含可用 token 字段，则只从这些记录生成事件。
- R2. 每条合格 `usage` 映射为一条 `UsageEvent`。不要按 `numTurns` 拆行。不要把后续较小的 `totalTokens` 当成计数器回退丢弃。
- R3. 通道映射：
  - `total_tokens` = `usage.totalTokens`（权威）
  - `cache_read_tokens` = `cachedReadTokens`
  - `cache_creation_tokens` = `cacheCreationTokens`
  - `input_tokens` = `inputTokens - cache_read - cache_creation`（饱和减，病态行记 parse issue）
  - `output_tokens` = `outputTokens`（含 reasoning）
  - `reasoning_output_tokens` = `reasoningTokens`（诊断，不另加进 total）
- R4. 同一会话一旦走 R1，禁止再叠加 `_meta.totalTokens` 轮增量或 `signals.json` 对账事件。
- R5. 会话没有任何合格 `usage` 时，保留现有 total_only 回退（`_meta.totalTokens` 单调增量 + signals 对账）。子通道保持 0。
- R6. 模型名：优先 `usage.modelUsage` 的第一个 key；否则 updates 既有 `modelId` 路径；否则 `summary.current_model_id`（值为 `custom` 或空时跳过）；否则 `grok-unknown`。持久化原始标识，不在解析器里把 `grok-4.6-build` 改写成 `grok-4.6`。`provider_label` 保持空串。
- R7. `usageIsIncomplete == true` 的记录仍入库；可增加 parse issue 计数，不得丢弃已给出的 token。
- R8. 来源描述符与 monitor 的质量标签改为 `precise`。无 grok 定价行，成本保持 `unpriced`。不读取、不换算 `costUsdTicks`。

### 重放与存量修复

- R9. 继续以会话为原子重放单元：任一白名单 sidecar 变化则按共享 `source_path_hash` 删除并重插。`FileCursor` 只做变化检测。
- R10. 将 `expected_token_accounting_version(Grok)` 从 2 升到 3，使已有 grok 行被既有 unbounded 正常 sync / `serve` 启动 repair 识别为 legacy 并在无损前提下重放。有缺失 sidecar 的 grok 会话仍走现有 lossy-rebuild guard，不得静默删历史。
- R11. 事件键稳定且会话内唯一。推荐 `grok:{session_id}:usage:{prompt_id}`，`prompt_id` 缺失时回退会话内序号。回退路径的 signals 对账键保持 `grok:{session_id}:signals`。

### 趋势页来源表

- R12. 用量趋势来源表不得写死 2 行。展示上限与 `PANEL_LIMITS.sources`（当前 4）对齐。
- R13. 窗口内来源数超过展示上限时，增加一行「其他」，token 与占比等于窗口总量减去已展示行。展示行加「其他」的占比应覆盖窗口总量（舍入误差除外）。
- R14. 观察文案继续使用窗口内完整排序的第一来源（`leaders.source`），不改为按「最近一天」重排。

### 文档与隐私

- R15. 更新 token-accounting / source-sync 契约中的 Grok 段落、`docs/agents/passive-source-candidates.md`、README 与 first-sync 文档：说明主路径是 `turn_completed.usage`，旧路径仅作无 usage 会话的回退，质量为 `precise`，成本仍 `unpriced`。
- R16. 解析器仍只读会话根白名单 sidecar。不读取 `chat_history.jsonl`、`system_prompt.txt`、`prompt_context.json`、`terminal/`。

## Acceptance Criteria

- [ ] AC1. 单元/fixture：含多条非单调 `turn_completed.usage` 的会话，事件数等于合格 usage 条数，`total_tokens` 为各条 `usage.totalTokens` 之和，而不是最后一条或 `_meta.totalTokens` 峰值。
- [ ] AC2. 单元/fixture：`inputTokens=1000, cachedReadTokens=400, cacheCreationTokens=0, outputTokens=50, reasoningTokens=20, totalTokens=1050` 映射为 `input=600, cache_read=400, cache_creation=0, output=50, reasoning=20, total=1050`。
- [ ] AC3. 同一会话同时有 `usage` 与 `signals.contextTokensUsed` 时，不产生 signals 对账事件，也不把上下文占用加进总量。
- [ ] AC4. 无 `usage`、仅有 `_meta.totalTokens` / signals 的会话仍产出 total_only 事件，子通道为 0。空会话仍为 0 事件。
- [ ] AC5. 模型名为 `grok-4.6-build`（来自 `modelUsage`）；`summary.current_model_id=custom` 不得覆盖该名称。
- [ ] AC6. sync-twice：sidecar 无变化时第二次 sync 零删除零新增。updates 追加一条新的 `turn_completed.usage` 后重放收敛到新总和。
- [ ] AC7. `expected_token_accounting_version(Grok)==3`。未升版本的存量库在无损前提下会重放 grok；缺失 sidecar 时拒绝有损自动 rebuild 并保留旧行。
- [ ] AC8. 来源描述符质量为 `precise`；grok 事件在无定价行时 `pricing_status=unpriced`。
- [ ] AC9. 趋势页来源表渲染不再 `.slice(0, 2)`。来源多于 4 个时可见 4 行加「其他」；测试锁定该行为。
- [ ] AC10. 文档与契约已更新。隐私边界未扩大。
- [ ] AC11. 针对本任务的 Rust 测试与 dashboard JS 检查通过。完整 `just ci` 在实现阶段结束时执行。

## Out of Scope

- 不根据 `costUsdTicks` 或猜测费率给 grok 定价。
- 不在解析器或 catalog 中把 `grok-4.6-build` 改名为 `grok-4.6`。
- 不改变 24h / 7d / 30d / 全部 窗口定义，也不按「最近一天」重排主来源。
- 不改发现规则，不改为递归扫描，不读取终端或对话文件。
- 不把无 `usage` 的活跃会话用上下文占用先占位。
- 不修改 Codex / Kimi / 其他来源的记账。
- 不把趋势页做成完整来源分布页；完整列表仍在「来源分布」。

## Technical Notes

- 实现边界见 `design.md`。执行顺序见 `implement.md`。
- 本机测量不可提交；fixture 必须脱敏，只保留 usage/model/timestamp 结构。
- 实施基线分支：`dev`。
