# home_overview 查询性能设计

## 1. Evidence Boundary

已确认的是“同机 current 与 clean HEAD 都约 95.7ms release，且标准 80ms 门失败”；尚未确认具体 SQL 根因。实现必须先建立逐阶段反馈循环，再按证据选择最小修复。

初始假设按优先级排序：

1. summary、by-platform、series 三次独立 `usage_event` 聚合重复扫描，并为 distinct/group/order 建临时 B-tree。
2. `dashboard.diagnostics()` 的多表统计或 source-file/archive 查询占主要固定成本。
3. session identity 表达式与 timezone `date()` 阻止既有索引提供有效顺序/覆盖。
4. 两条 run-log 查询、连接 PRAGMA、冷 page cache 或 Windows I/O 是次要固定成本。

每个假设必须用阶段计时和 SQL 结构信号证伪；不能因为第一项看起来合理就直接合并查询。

## 2. Feedback Loop

### 2.1 Synthetic Gate

- 保留现有 10k fixture 与 80ms 断言作为最终红/绿信号。
- 增加 test-only breakdown seam，独立调用 summary、by-platform、series、run state、diagnostics，并输出/返回每段 elapsed；生产 payload 不增加 timing 字段。
- 为主导 SQL 提供 test-only `EXPLAIN QUERY PLAN` helper；若 rusqlite 可在不新增运行时依赖的前提下读取 statement status，则同时记录 VM/full-scan/sort 指标，否则使用可重复的只读 SQLite harness。
- debug/release 分开记录；seed 不计入 query wall time。

### 2.2 Representative Database

- 使用 SQLite online backup 复制 `~/.llmusage/llmusage.db` 到忽略目录。
- 只在副本上执行 warm-up + 多轮 query，记录数据库规模、median/p95、各阶段比例和 plan；完成后清理副本。
- 不输出路径哈希、session/event key 或原始内容。

## 3. Decision Tree

### 3.1 Reuse Existing Projection

若 source/token/request/cost 等聚合可从 `usage_bucket_30m` 精确得到，则将可证明等价的字段下推到 bucket；session distinct、active-day/timezone 等不能近似，继续走事实表或单独精确查询。禁止把 bucket `event_count` 当 session 数。

### 3.2 Query Reshape

若重复 `usage_event` 扫描主导，优先考虑一次 materialized filtered working set、共享 session identity、减少重复 distinct/sort，或把兼容的聚合合并为一次 SQL/一次 row stream。任何合并必须覆盖 filter 参数顺序、NULL fallback 和跨日重复 session，且不得把相同 session 在多日重复相加成总 session。

### 3.3 Index Or Projection

只有 plan/VM 证据证明表达式或排序是主因时才新增索引。只有 existing bucket + 合理索引都无法满足精确语义时才新增 projection；projection 必须明确 writer、rebuild、migration、reprice 和 rollback 所有权，不能只服务测试 fixture。

### 3.4 Diagnostics And Run State

若 diagnostics 或 run state 主导，复用已有聚合/索引或合并重复读取，但保持 `HomeOverviewBootstrap`、archive diagnostics 和 `last_updated` 契约。不得延迟到返回之后或缓存上一次结果。

## 4. Compatibility

- 不改变 `HomeOverviewPayload` JSON shape、字段类型、默认平台集合或 public method signature。
- `QueryFilter` 的 source/model/project/since/until/timezone 行为逐字段保持。
- 若无 schema 变更，不增加 migration；若有，版本号必须在当前 v15 后追加真实 migration，并保持旧二进制不支持 downgrade 的既有约束。

## 5. Failure, Rollback And Scope

- 任何新 SQL 失败按现有 query error surface 返回，不以空 payload 降级。
- 优化以小步骤落地：feedback seam -> focused rewrite/index -> equivalence -> real backup。每步可单独回滚。
- 若 80ms 只能通过跨模块 projection 或硬件相关策略达到，停止实现并带证据回到任务设计，不修改预算。
