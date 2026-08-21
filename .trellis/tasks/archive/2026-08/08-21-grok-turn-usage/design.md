# Design：Grok `turn_completed.usage` 与趋势来源表

依据：`research/grok-turn-usage-evidence.md`、`prd.md`。解析器范本：`src/parsers/kimi_code.rs`（turn-scoped 1:1 映射）与现有 `src/parsers/grok.rs`（会话原子重放）。

## 1. 边界

| 路径 | 职责 |
| --- | --- |
| `src/parsers/grok.rs` | 主路径改为 `turn_completed.usage`；无 usage 时回退旧 total_only |
| `src/domain/source_descriptor.rs` | `UsageQuality::Precise` |
| `src/domain/platform_monitor.rs` | grok monitor quality 同步为 Precise |
| `src/store/schema.rs` | `expected_token_accounting_version(Grok) = 3` |
| `.trellis/spec/llmusage/backend/token-accounting-contracts.md` | 重写 Grok 通道契约 |
| `.trellis/spec/llmusage/backend/source-sync-contracts.md` | 补充 usage 主路径，发现/重放不变 |
| `src/web/assets/render/trends.js` | 去掉 `.slice(0, 2)`，按 `PANEL_LIMITS.sources` 展示并补「其他」 |
| `src/web/assets/copy.js` / `src/web/mod.rs` 测试 | 「其他」文案与截断回归 |
| docs / README / `passive-source-candidates.md` | 口径说明 |
| `tests/sync_regression.rs` + `grok.rs` 单测 | fixture 与 sync 回归 |

不改发现器、不改 Store 重放协议、不加定价行、不新增依赖。

## 2. 数据流

```
updates.jsonl
  ├─ turn_completed + usage  →  每条一条 UsageEvent（precise）
  └─ 若会话内 0 条合格 usage
        ├─ _meta.totalTokens 单调轮增量
        └─ signals 差额对账（total_only）
        → 共享 source_path_hash 会话重放
        → usage_event / usage_bucket_30m
        → /api/sources + /api/trends
        → trends.js 来源表
```

有 usage 的会话在重放时删除该 `source_path_hash` 的全部旧事件，包括旧的 `grok:{session}:signals` 与轮增量键。这是期望行为。

## 3. 解析规则

### 3.1 合格 usage

`params.update.sessionUpdate == "turn_completed"`，且 `params.update.usage` 为对象，且 `totalTokens` / `inputTokens` / `outputTokens` 至少一个可解析为非负整数。全 0 跳过。无 usage 的 `turn_completed` 忽略。

### 3.2 通道

与 PRD R3 相同。权威 total 是 `usage.totalTokens`。若缺失则回退 `inputTokens + outputTokens`。`inputTokens < cache_read + cache_creation` 时 input clamp 到 0 并记 accounting anomaly。

不读 `costUsdTicks`。不把 `costUsdTicks` 写入任何 cost 字段。

### 3.3 模型

1. `usage.modelUsage` 若为非空对象：取其插入顺序的第一个 key（serde_json::Map 保序）。
2. 否则现有 `extract_model_id`。
3. 否则 `summary.current_model_id` / `model_id`，值为 `custom` 则视为缺失。
4. 否则 `signals.primaryModelId` / `modelsUsed[0]`。
5. 否则 `grok-unknown`。

多 key 的 `modelUsage`（本机 2/506）仍只写一条顶层 usage 事件，避免把 `grok-4.6-build` 与 `grok-4.6` 加两次。

### 3.4 时间戳

`params._meta.agentTimestampMs`，否则 `timestamp`。沿用现有 `extract_timestamp_ms` / `normalize_unix_timestamp`。缺失时回退 `summary.updated_at`。

### 3.5 事件键

`grok:{session_id}:usage:{prompt_id}`。`prompt_id` 来自 `params.update.prompt_id`。缺失或碰撞时追加 `-:{index}`。回退路径保持 `grok:{session_id}:{turn_index}` 与 `grok:{session_id}:signals`。

### 3.6 会话策略选择

扫描完 updates 后：

- `usage_events.len() > 0` → `output.events = usage_events`，跳过 signals 对账。
- 否则走现有 `parse_updates_file` + signals 对账。

不要混用两条路径。

## 4. Token accounting 版本

`expected_token_accounting_version(SourceKind::Grok) -> 3`。

Codex 已是 3，key 按来源隔离（`token_accounting_version.grok`），数字相同可接受。

`serve` 启动 repair 与 unbounded 正常 sync 已能按 legacy marker 重放无损来源。Grok 会话重放本身无损（输入仍在则重建）。缺失 sidecar 继续阻断自动 reset。

不把 grok 升到与「全源默认 2」绑定的常量；在 match 中显式写 `SourceKind::Grok => 3`。

## 5. 趋势来源表

`render/trends.js`：

- `import { PANEL_LIMITS } from '../data/derive.js'`（或已有 `data.js` re-export）。
- 取 `context.panels.sources` 按 token 已排序的列表。
- `visible = sources.slice(0, PANEL_LIMITS.sources)`。
- `rest = windowTotal - sum(visible.total_tokens)`，`windowTotal = context.totals.total_tokens`。
- `rest > 0` 且存在未展示来源时追加一行，source 文案走 i18n（zh「其他」/ en `Other`）。
- 删除字面量 `.slice(0, 2)`。

`src/web/mod.rs` 增加断言：`trends.js` 不含 `.slice(0, 2)`，含 `PANEL_LIMITS.sources`，含其他行逻辑。

不改 `leaders.source`。不改柱状图最近 10 时段。

## 6. 测试清单

解析器单测（`src/parsers/grok.rs`）：

- 多条非单调 usage 求和。
- 通道映射 AC2。
- usage 优先于 signals。
- 无 usage 回退旧路径。
- `custom` 不覆盖 `modelUsage`。
- `usageIsIncomplete` 仍产出事件。
- 全 0 usage 跳过。

集成（`tests/sync_regression.rs`）：

- 现有 grok replay / missing sidecar / GROK_HOME 测试保持绿。
- 新增：seed `turn_completed.usage` 后总量与通道正确；sync-twice 幂等；追加第二条 usage 后总和增加。
- 新增：legacy marker 2 的 grok 行在无损 rebuild 路径被重放（可复用现有 accounting repair 测试手法）。

Dashboard：

- `web/mod.rs` 现有 spotlight 测试保持。
- 新增趋势来源表截断/其他行断言。

## 7. 兼容与回滚

- 旧 fixture 仍覆盖回退路径，不得删除。
- 用户库：升版本后下一次 unbounded sync 或 `serve` 启动会重放 grok。有缺失 sidecar 的会话保持旧行并进入 rebuild-risk。
- 回滚：恢复解析器与 version=2，再 `--rebuild --source grok`（若已写出 precise 行）。
- 模型分布可能从 `grok-4.6` 变为 `grok-4.6-build`。这是原始 SKU，不在本任务做别名。

## 8. 风险

| 风险 | 处理 |
| --- | --- |
| `numTurns>1` 的记录其实是父级汇总，与后续记录重叠 | 本机样本中后续记录不是父级子集；MVP 按独立段求和。若发现重叠，再改规则。 |
| 活跃会话在第一段 `turn_completed` 前显示 0 | 接受。禁止用上下文占用占位。 |
| precise 子通道被错误匹配到价格行 | 目录无 grok 行；未知模型保持 unpriced。不加 grok 价格。 |
| 自动 rebuild 耗时 | grok 会话原子重放，量级小于 Codex；沿用现有 repair 进度事件。 |
