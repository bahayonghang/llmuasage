# 接入 Pi 源上报成本

## Goal

Pi / Oh My Pi 事件在真源自带成本可用时写入非零成本，成本来源可与目录定价区分，
并且事件行与聚合桶都不会被目录重算清零。

## 背景与证据

- 成本计算只有一条通路：`src/store/sync_writer.rs:364` 调
  `pricing::compute_cost_with(catalog, source, model, tokens)`，
  而 `PricingCatalog::find` 按 `(source, model)` 匹配（`src/query/pricing_catalog.rs:377`）。
- 内置 `pricing/static-v2.json` 只有 13 行，`sources` 覆盖 `codex`、`claude`、`opencode`，
  没有 pi/omp 行。因此本机 423 条 `pi` 事件 100% 为 `unpriced`，成本合计 0.00。
- 目录不止内置一份：`PricingCatalog::load_snapshot`（`src/query/pricing_catalog.rs:302`）
  与 `load_overlay`/`merge_overlay`（`:315`、`:325`）接受用户提供的文档，
  其 `sources` 可以是任意值，因此用户自备目录**可以**包含 `omp` 行。
- 真源每条 usage 记录都带 `cost{input,output,cacheRead,cacheWrite,total}`
  （2026-08-23 扫描：1198/1198 条带 `cost`，其中 118 条 `total>0`，合计 $0.441788）。
  按 provider：openai-codex 6/8 = $0.3756；deepseek 112/115 = $0.0661；
  xai-oauth 0/86；openrouter 0/989。
- 真源模型名包含 `stealth/ox-alpha`、`deepseek-v4-flash`、`grok-4.6` 等路由模型，
  逐条补目录定价不可收敛，因此走源上报成本。
- 重算不只改事件行：`Store::recompute_costs_with_meta_and_progress` 对**每条**事件
  计算目录成本并累加进 `buckets`（`src/store/mod.rs:491`、`:545`、`:565`），
  第二阶段 `reconcile_pricing_buckets` 用该 rollup 覆盖桶并删除不在 rollup 中的桶
  （`src/store/mod.rs:598`、`:641`、`:665`）。
- 桶状态解码只识别 `static` / `snapshot`，其他值一律变成 `Unpriced`
  （`src/store/sync_writer.rs:1143` 的路径级 reset 解码）。
- ccusage 的做法是把 `usage.cost.total` 当 display cost，在 `auto` 模式优先使用它，
  `calculate` 模式回落目录定价。

## Requirements

- **R3.1** 新增成本来源状态 `source_reported`，与 `static` / `snapshot` / `unpriced` 并列。
- **R3.2** 判定优先级：`usage.cost.total > 0` 时写入源上报成本并标 `source_reported`；
  否则（`total == 0`、`cost` 缺失、`cost` 非对象）回落到既有目录定价通路。
  内置目录没有 omp 行，因此默认配置下的结果就是 `unpriced`；用户自备 snapshot/overlay
  含 omp 行时按目录结果标 `static`/`snapshot`，这是允许的。
- **R3.3** `cost_without_cache_usd` 在可推导时按输入单价推导，不可推导时等于
  `cost_with_cache_usd`，并把回落事实记入 `pricing_rate`。
- **R3.4** 目录重算不得改变 `source_reported` 行的金额与状态，**且**不得让这些行在
  `usage_bucket_30m` 中被清零、错标或删除。
- **R3.5** 桶状态解码必须认识 `source_reported`，路径级 reset 与重算两条路径都要一致。
- **R3.6** 远端导入路径与既有 `UsageEvent` 构造点保持可编译且语义明确：
  没有源成本的源写 `None`。跨版本传输保持兼容。
- **R3.7** 报表与看板的 `unpriced` 统计口径不变：`source_reported` 计入已定价。

## 非目标

- 不给内置 `pricing/static-v2.json` 增加 pi/omp 的模型定价行。
- 不把 `cost == 0` 解释为「免费」。xai-oauth 与 openrouter 记录恒为 0，
  无法区分「订阅内零边际成本」与「真源未上报」，因此让它走目录通路，
  默认结果为 `unpriced`，由既有 `unpriced_events` 指标暴露覆盖率。
- 不改 `--no-cost` 等既有成本展示开关语义。
- 不给 Grok 的 `costUsdTicks` 接同一通路（后续任务）。

## 依赖

依赖 `08-23-omp-source-split` 完成。历史回填按父任务 R8 用
`sync --rebuild --source omp`。

## Acceptance Criteria

- [ ] **AC3.1**（R3.1/R3.2）本机 `sync --rebuild --source omp` 后 `source='omp'` 的
      `SUM(cost_with_cache_usd)` 与当次真源扫描的 `cost_total` 差值绝对值 < 1e-6。
- [ ] **AC3.2**（R3.2）默认目录下 `source='omp'` 行的 `pricing_status` 只出现
      `source_reported` 与 `unpriced`；`source_reported` 行数等于当次扫描的
      `cost_positive`。
- [ ] **AC3.3**（R3.2）单测：`total>0` 写 `source_reported`；`total==0`、`cost` 缺失、
      `cost` 为非对象三种情况都走目录通路且不丢事件；再加一例「overlay 提供 omp 行」，
      断言此时 `total==0` 的记录被标为 `static`/`snapshot` 而不是 `unpriced`。
- [ ] **AC3.4**（R3.3）单测：`cost_without_cache_usd` 的推导与回落各一例，
      回落时 `pricing_rate` 含回落标记。
- [ ] **AC3.5**（R3.4）单测：`recompute_costs_with` 前后 `source_reported` 事件行的
      金额、状态、`pricing_source` 完全不变。
- [ ] **AC3.6**（R3.4）单测：重算后 `usage_bucket_30m` 中纯 `source_reported` 桶的
      成本合计不变、状态不被改成 `unpriced`、桶不被删除；另有一例覆盖
      `source_reported` 与 `unpriced` 混合的桶。
- [ ] **AC3.7**（R3.5）单测：路径级 reset 后重放，桶的 `pricing_status` 仍是
      `source_reported`（不是被解码成 `unpriced`）。
- [ ] **AC3.8**（R3.4）本机验证：`usage_bucket_30m` 中 `source='omp'` 的成本合计与
      事件侧差值绝对值 < 1e-6，重算前后都成立。
- [ ] **AC3.9**（R3.6）单测：缺少 `source_cost` 字段的旧版 shard JSON 可反序列化
      （`#[serde(default)]`）。
- [ ] **AC3.10**（R3.7）单测或查询验证：`unpriced_events` 指标不把 `source_reported`
      计入未定价。
- [ ] **AC3.11**（R3.1）ADR 记录成本来源优先级与 `total==0` 的处理；
      `.trellis/spec/llmusage/backend/pricing-catalog-contracts.md` 与
      `token-accounting-contracts.md` 同步更新。
