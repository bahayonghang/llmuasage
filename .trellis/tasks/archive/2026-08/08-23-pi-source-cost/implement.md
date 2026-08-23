# 执行计划：接入 Pi 源上报成本

## 前置

- 依赖 `08-23-omp-source-split` 已归档。
- 读 `.trellis/spec/llmusage/backend/pricing-catalog-contracts.md`、
  `token-accounting-contracts.md`、`write-fencing-contracts.md`。
- 读父任务 `design.md` D2（回填口径）。
- 记录基线：`source='omp'` 成本合计 0.00、`unpriced` 100%。
- 跑一次真源扫描存档，取 `cost_positive` 与 `cost_total` 作为本轮期望值：
  `python .trellis/tasks/08-23-pi-omp-usage-accounting/research/scan_pi_source.py`

## 步骤

1. [x] `src/query/pricing.rs`：加 `PricingStatus::SourceReported` 与 `as_str`
       映射 `"source_reported"`；补状态字符串单测。
2. [x] `src/domain/models.rs`：加 `SourceCost` 结构与 `UsageEvent::source_cost`
       （`#[serde(default)]`）。`cargo check` 收集所有构造点并补字段。
       补旧版 shard JSON 反序列化单测（AC3.9）。
3. [x] `src/query/pricing.rs`：新增源上报成本折算函数，实现推导与回落两条路径，
       写 `pricing_source` 与 `pricing_rate`（AC3.4）。
4. [x] `src/store/sync_writer.rs`：成本选择点改为「源上报优先，否则目录」。
       单测覆盖 `total>0`、`total==0`、`cost` 缺失、`cost` 非对象，
       外加一例 overlay 提供 omp 行时 `total==0` 的记录被标 `static`/`snapshot`（AC3.3）。
5. [x] `src/parsers/pi.rs`：读 `message.usage.cost`，容忍缺失与非对象值，
       映射为 `SourceCost`。
6. [x] 桶状态解码补 `"source_reported"` 分支（`src/store/sync_writer.rs:1143`），
       并全仓搜索 `"snapshot" =>` 形式确认无第二处遗漏（AC3.7）。
7. [x] 重算改造（`src/store/mod.rs`）：阶段一读取语句增加已持久化的成本与状态列；
       `source_reported` 行跳过 UPDATE 但**用持久化值**入 `buckets`。
       单测：事件金额不变（AC3.5）；纯 `source_reported` 桶与混合桶的成本、状态、
       存在性都正确（AC3.6）。
8. [x] 确认 `unpriced_events` 统计不把 `source_reported` 计入未定价（AC3.10）。
9. [x] 集成测试：临时 home 下构造带 `cost` 的 `.omp` 会话，断言事件成本、状态、
       `pricing_source` 落库，桶成本一致，且重算不改值。
10. [x] 文档（AC3.11）：新增或增补 ADR 记录成本来源优先级、`total==0` 的处理、
        旧二进制会把新状态解码成 `unpriced` 的兼容性说明；
        更新 `pricing-catalog-contracts.md`、`token-accounting-contracts.md`；
        如 README 描述过 pi 成本为 unpriced，同步修正。

## 验证命令

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features -- --test-threads=1
cargo run -- sync --rebuild --source omp     # 历史回填（父任务 R8）
cargo run -- daily --source omp
just ci
```

本机数据校验（只读查询，对照当次扫描输出）：

```sql
SELECT pricing_status, COUNT(*), ROUND(SUM(cost_with_cache_usd),6)
FROM usage_event WHERE source='omp' GROUP BY 1;
-- 期望：source_reported 行数 = 扫描的 cost_positive；金额合计与 cost_total 差 < 1e-6；
--       其余行为 unpriced（默认目录下不应出现 static/snapshot）

SELECT ROUND(SUM(cost_with_cache_usd),6) FROM usage_bucket_30m WHERE source='omp';
-- 期望：与事件侧合计差 < 1e-6（AC3.8）

SELECT pricing_status, COUNT(*) FROM usage_bucket_30m WHERE source='omp' GROUP BY 1;
-- 期望：出现 source_reported（或与 unpriced 混合时的 mixed），不出现全 unpriced
```

执行 `llmusage catalog` 触发重算后，重复上面三条查询，结果必须完全一致。

## 评审门

- 步骤 4 完成后先跑步骤 7 的重算与桶测试，确认成本不会被目录重算清零或删桶，再继续。
- 步骤 6、7 是同一风险面（状态降级 + 桶被删），两者都通过后才执行本机回填。
- 步骤 9 通过后再改文档。

## 回滚点

- 步骤 1–9 为纯代码改动。
- 若本机 `omp` 行已写入 `source_reported`：回退代码后跑
  `sync --rebuild --source omp`，成本回到 `unpriced`。
