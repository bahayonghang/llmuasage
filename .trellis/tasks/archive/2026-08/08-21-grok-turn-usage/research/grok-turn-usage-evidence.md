# Grok Build `turn_completed.usage` 与趋势页来源表证据

调研时间：2026-08-21。对象：本机 `~/.grok/sessions`（257 个会话）与只读 `~/.llmusage/llmusage.db`。只读用量/模型/时间戳字段，不读取对话正文。

## 用户现象

用量趋势页（窗口 `7d`）来源表只显示两行：

| 来源 | Token | 占比 |
| --- | --- | --- |
| `codex` | 340.3M | 29.9% |
| `kimi_code` | 256.2M | 22.5% |

观察文案写「当前主来源为 `codex`」。窗口总量约 1.1B。用户判断最近应以 Grok Build 的 grok 4.6 为最高。

## 原因 1：解析器把上下文占用当成用量

`src/parsers/grok.rs` 当前只读取：

- `params._meta.totalTokens`（按 `user_message_chunk` 切轮、单调计数器求增量）
- `signals.json` 的 `effective_total = max(totalTokens, totalTokensBeforeCompaction + contextTokensUsed)`

2026-07-27 接入时（Grok Build 0.2.112）本机 `updates.jsonl` 几乎没有 token 字段，只有少数 `signals.json` 带 `contextTokensUsed`。该路径被写成 `UsageQuality::TotalOnly` 契约。

当前本机 Grok Build 已经在 `sessionUpdate == "turn_completed"` 上写出精确用量对象 `params.update.usage`：

```json
{
  "inputTokens": 372055,
  "outputTokens": 2903,
  "totalTokens": 374958,
  "cachedReadTokens": 333824,
  "cacheCreationTokens": 0,
  "reasoningTokens": 1814,
  "modelCalls": 12,
  "numTurns": 12,
  "costUsdTicks": 2657920000,
  "modelUsage": {
    "grok-4.6-build": { "...same channels..." }
  }
}
```

本机核对：

| 指标 | 数值 |
| --- | --- |
| 会话数 | 257 |
| 含 `turn_completed.usage` 的会话 | 222 |
| 无 usage 对象的会话 | 35（走旧路径） |
| `_meta.totalTokens` / `signals.contextTokensUsed` 合计（解析器等价物） | 34.9M |
| 全部 `turn_completed.usage.totalTokens` 求和 | 1.897B |
| 近 7 天 `turn_completed.usage.totalTokens` 求和 | 1.584B（423 条，其中 9 条 `usageIsIncomplete=true`） |
| 近 7 天其中 grok-4.6 / grok-4.6-build | 1.584B（全部） |

`params._meta.totalTokens` 与 `signals.contextTokensUsed` 的取值范围对齐上下文窗口占用（单会话峰值约 10 万–60 万，`contextWindowTokens=1000000`）。`usage.totalTokens` 是按段累计的请求用量，可到千万级。

## 原因 2：库内 grok 行仍是旧口径

只读查询 `usage_event`（`event_at >= now-7 days`）：

| source | events | tokens |
| --- | --- | --- |
| `codex` | 3012 | 346.4M |
| `kimi_code` | 2586 | 279.2M |
| `zcode` | 1596 | 171.0M |
| `claude` | 1048 | 169.6M |
| `deepseek_harness` | 835 | 162.8M |
| `grok` | 418 | **29.5M** |
| `antigravity` | 408 | 17.7M |
| `pi` | 42 | 2.6M |

`codex` 346M 与截图 340.3M 一致。库内 grok 近 7 天 29.5M，与解析器等价物 29.6M 一致，与真实 `usage` 求和 1.584B 相比约 **54 倍少计**。

若按真实口径重算 7d 窗口：总量约 `1.1B - 29.5M + 1.584B ≈ 2.65B`，grok 约占 60%，超过 `codex`。

全库 grok 模型行：`grok-4.6` 443 条 / 31.7M，`grok-4.5` 61 条 / 3.2M。这是旧路径从 `summary.current_model_id` / `_meta.modelId` 写下的标签，总量仍是上下文占用。

## `usage` 语义

- 出现位置：仅 `params.update.sessionUpdate == "turn_completed"` 且带 `params.update.usage`。存在无 `usage` 的 `turn_completed`（早期会话），必须跳过。
- 不是会话级单调计数器：76/222 个有 usage 的会话非单调。后一条可以远小于前一条。不得再套用「小于前值则丢弃」。
- 一条记录覆盖 `numTurns` / `modelCalls`（可为 1，也可为数十）。该对象已经是这一段的合计，映射为 **一条** `UsageEvent`，不要按 `numTurns` 拆行。
- 通道恒等式（222/222 有 usage 的会话最后一条）：`totalTokens = inputTokens + outputTokens`；`inputTokens >= cachedReadTokens`；`reasoningTokens <= outputTokens`。
- `cacheCreationTokens` 本机 506/506 条为 0，字段仍应映射。
- `costUsdTicks` 465 条非 0，单位未验证，本任务不用于计价。
- `usageIsIncomplete`：11/506 条为 true；近 7 天 9 条。仍应入库，不丢弃已给出的 token。
- `modelUsage`：504/506 条只有一个 key；2 条同时有 `grok-4.6-build`+`grok-4.6` 或 `grok-4.5-build-free`+`grok-4.5`。MVP 用顶层 `usage` 通道写一条事件，模型取 `modelUsage` 的第一个 key。不要按 key 拆行，避免重复计数。
- `summary.current_model_id == "custom"` 的会话有 38 个；其 `modelUsage` 仍是 `grok-4.6-build` / `grok-4.6`。不要把 `custom` 当模型名持久化。
- 活跃会话可能还没有 `turn_completed.usage`（本会话 `01a021f5-…` 截至采样时无 `signals.json`、无 `usage`）。在第一段完成前该会话合法为 0 事件。

建议映射（对齐 Codex / ZCode 的 cache-inclusive 输入）：

| 通道 | 公式 |
| --- | --- |
| `total_tokens` | `usage.totalTokens`（权威） |
| `cache_read_tokens` | `cachedReadTokens` |
| `cache_creation_tokens` | `cacheCreationTokens` |
| `input_tokens` | `inputTokens - cache_read - cache_creation`（饱和减） |
| `output_tokens` | `outputTokens`（含 reasoning） |
| `reasoning_output_tokens` | `reasoningTokens`（诊断，不另加进 total） |

有任意一条 `turn_completed.usage` 的会话：**不要**再叠加 `_meta.totalTokens` 增量或 `signals.json` 对账，否则会把上下文占用和请求用量加在一起。

无 `usage` 的 35 个会话：保留现有 total_only 回退。

## 原因 3：趋势页来源表硬截断为 2 行

`src/web/assets/render/trends.js`：

```javascript
const sourceRows = (context.panels.sources || []).slice(0, 2)
```

`PANEL_LIMITS.sources` 在 `derive.js` 中是 4，来源分布页用 4，趋势页却写死 2。截图两行占比 29.9% + 22.5% = 52.4%，其余来源被丢掉。即使 grok 按旧口径排第 6（29.5M），用户也看不见。

观察文案的「主来源」取 `context.leaders.source`（完整排序第一名），与被截断的表一致地指向 `codex`，因为库内 7d 确实是 `codex` 最高。解析器修正后该文案会改为 grok，无需改文案逻辑。

## 非原因

- 发现规则 `~/.grok/sessions/*/*/` 仍能枚举到 257 个会话，不是漏扫。
- `SourceKind::Grok` 已注册，sync 有写入，不是未接入。
- 7d 窗口包含 08-15 的 319M 峰值，会抬高历史来源；即便如此，按真实 grok 1.584B 重算后 grok 仍应第一。窗口定义本身不是主因。
- 模型页「模型用量分布」与来源表是不同维度。当前库内 grok 模型总量只有 31.7M，模型页同样被旧口径压低。

## 与既有契约的冲突点

- `token-accounting-contracts.md` 仍写：Grok 由累计计数器 + signals 对账得到 total，子通道全 0。
- `source_descriptor.rs` / `platform_monitor.rs`：`UsageQuality::TotalOnly`。
- `expected_token_accounting_version(Grok) == 2`。语义变更后若不抬版本，`serve` 启动自动 repair 不会重放已有 grok 行，看板会继续显示 29.5M。

## 隐私与安全

扫描器已只读会话根白名单 sidecar。`turn_completed.usage` 位于 `updates.jsonl`，不需要读取 `chat_history.jsonl` / `system_prompt.txt` / `prompt_context.json` / `terminal/`。
