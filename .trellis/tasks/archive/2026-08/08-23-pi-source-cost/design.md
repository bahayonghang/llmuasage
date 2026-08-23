# 设计：接入 Pi 源上报成本

## 边界

新增一条「源上报成本」通路，与目录定价并列。改动落在
`src/domain/models.rs`（事件载体）、`src/query/pricing.rs`（状态与折算）、
`src/store/sync_writer.rs`（写入选择 + 桶状态解码）、`Store` 重算路径与
`src/parsers/pi.rs`（读取）。schema 不加列。

## 决定 1：成本载体放在 UsageEvent，不加数据库列

`UsageEvent` 新增：

```rust
/// Cost reported by the source itself, in USD, when the source records one.
/// `None` for sources that do not report cost.
#[serde(default)]
pub source_cost: Option<SourceCost>,
```

`SourceCost` 携带 `total` 与可选的 `input` / `output` / `cache_read` / `cache_write`
分项，用于推导 `cost_without_cache_usd`。它只在内存与远端 shard 传输中存在，
落库时被折算进既有的 `cost_with_cache_usd` / `cost_without_cache_usd` /
`pricing_status` / `pricing_rate` 四列。

理由：`pricing_status='source_reported'` 已经足够让重算路径识别这些行，不需要再持久化
原始分项。少一列就少一次迁移。代价：改折算公式必须重放该源，重放通路已存在。

`#[serde(default)]` 保证旧版本远端主机发来的 shard 仍可反序列化（AC3.9）。

## 决定 2：状态取值与判定优先级

`PricingStatus` 新增 `SourceReported`，`as_str()` 返回 `"source_reported"`。

判定顺序（`src/store/sync_writer.rs` 的成本选择点）：

1. `event.source_cost` 存在且 `total > 0` → 源上报成本，状态 `source_reported`。
2. 否则走 `compute_cost_with(catalog, source, model, tokens)`，状态由目录决定。

即源上报优先于目录。理由：真源知道自己实际按哪个 provider、哪个套餐计费
（本机同时存在 oauth 与 API key 两类 provider），目录只能按模型名猜。
这与 ccusage 的 `auto` 模式同序。

`total == 0` 落到分支 2 而不是硬写 `unpriced`。这是与 PRD R3.2 一致的措辞：
默认内置目录没有 omp 行，结果就是 `unpriced`；但 `load_snapshot` / `merge_overlay`
允许用户目录包含 `omp` 行（`src/query/pricing_catalog.rs:302`、`:325`），
此时按目录结果标 `static`/`snapshot` 是正确行为——用户显式提供了费率，
不应该被本任务屏蔽。AC3.3 用一例 overlay 测试固定这条语义。

## 决定 3：cost_without_cache_usd 的推导

既有语义是「把全部 prompt token 按 input 单价计价」（`src/query/pricing.rs:107`）。
源上报只给金额，不给单价，所以：

- 当 `input_tokens > 0 且 cost.input > 0`：`input_rate = cost.input / input_tokens`，
  `without_cache = (input + cache_read + cache_creation) * input_rate + cost.output`。
- 否则：`without_cache = cost.total`，并在 `pricing_rate` JSON 里写
  `{"source":"pi_usage_cost","without_cache":"fallback_equals_total"}`。

`pricing_source` 写 `"source-reported"`。`pricing_rate` 同时记录分项原值，便于审计。

reasoning 不参与推导：真源 1198/1198 记录满足
`totalTokens == input + output + cacheRead + cacheWrite`，`reasoningTokens <= output`，
即 reasoning 已含在 output 里，与 `ReasoningPolicy::IncludedInOutput` 一致。

## 决定 4：重算必须同时处理事件行与聚合桶

只给事件 UPDATE 加 `pricing_status <> 'source_reported'` 是不够的。现有重算的两阶段：

1. 阶段一逐页读事件、对每条算目录成本、写事件行、并把该成本累加进内存 `buckets`
   （`src/store/mod.rs:491`、`:545`、`:565`）。
2. 阶段二 `reconcile_pricing_buckets` 用 `buckets` 覆盖 `usage_bucket_30m`，
   并删除不在 rollup 里的桶（`src/store/mod.rs:598`、`:641`、`:665`）。

因此设计为：

- 阶段一的读取语句增加 `pricing_status`、`cost_with_cache_usd`、`cost_without_cache_usd`、
  `pricing_source`、`pricing_rate` 五列。
- 当行的状态是 `source_reported`：**跳过** UPDATE，并把该行**已持久化的**成本与状态
  累加进 `buckets`（而不是目录算出的值）。这样桶 rollup 仍然覆盖全部事件，
  第二阶段既不会清零也不会把桶当孤儿删除。
- 桶的状态聚合沿用既有「多状态混合 → `mixed`」规则；纯 `source_reported` 桶保持
  `source_reported`。

被否方案 A：让重算把 `source_reported` 行按目录重新定价。否决理由：默认目录没有
omp 行，重算等于把已知成本清零。
被否方案 B：在阶段一直接跳过 `source_reported` 行（不读、不入 rollup）。否决理由：
该行所在的桶会因为 rollup 缺失而被阶段二当孤儿删除。

## 决定 5：桶状态解码补全

`src/store/sync_writer.rs:1143` 的路径级 reset 解码目前只识别 `"static"` 与
`"snapshot"`，其余落 `Unpriced`。新增 `"source_reported"` 分支，否则一次路径 reset
就会把桶状态降级（AC3.7）。实现时全仓搜索 `"snapshot" =>` 形式的解码点，确认没有
第二处遗漏。

## 决定 6：其他源不受影响

所有现有解析器把 `source_cost` 写 `None`（`Default` 即可），状态与成本值不变。
`src/commands/sync.rs:1253`、`src/remote/importer.rs:191`、`src/commands/remote.rs`
以及 `src/store/migrations.rs` 里构造 `UsageEvent` 的位置都要显式补字段或依赖
`..Default::default()`，以 `cargo check` 的报错清单为准。

## 兼容性

- 无 schema 变化，`schema_version` 不提升。
- 旧二进制读到 `pricing_status='source_reported'` 时会把它解码成 `Unpriced`
  （旧代码的 match 落 `_`），成本数值仍可读但状态显示错误。ADR 要写明这一点。
- 回滚：回退代码后跑 `sync --rebuild --source omp`，成本回到 `unpriced`。
