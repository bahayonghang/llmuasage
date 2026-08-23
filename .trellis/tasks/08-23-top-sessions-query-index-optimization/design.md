# Design：Top Sessions 精确查询与条件索引优化

## 1. Design intent

保持 `/api/sessions`、`TopSessionsQuery` 和 `TopSessionRow` 的公开契约不变，只替换内部
执行计划。设计采用递进门：D1 query-only 先解决重复工作；只有 D1 未达性能预算才实验
D2 expression index。持久化 rollup 不在本任务中。

```text
TopSessionsQuery + QueryFilter
  -> one exact event projection
  -> canonical identity + per-session accumulator
  -> bounded Top K for tokens | duration | cost
  -> unchanged TopSessionRow JSON
              |
              +-- D1 passes -> finish without migration
              +-- D1 misses -> isolated D2 index experiment
                                  -> accepted v24 | reject and re-plan
```

## 2. Baseline oracle and benchmark boundary

实现前保留 legacy loader 作为 test-only oracle，或在改写前由 fixture 固化完整 JSON。
oracle matrix 必须覆盖 identity 四级 fallback、全部 filter、三排序、并列、limit、空值、
零值和 active-gap 边界。不能只比较 session id 顺序；所有字段和浮点序列化都比较。

新增 Top Sessions 专用 benchmark，输出 sanitized JSON：只含 range/filter-shape 名称、
sort、status/support、wall/query time、payload bytes、schema/index/commit 元数据，不含
session/project/model/source 的真实值或响应 rows。

基线与候选都从同一个校验过的 online backup 派生；每组先 warm-up，再顺序执行五次，
避免并发干扰首轮决策。另做 concurrency-2 与快速 sort/range 切换，只验证监督与取消。

## 3. D1 query-only path

### 3.1 Event projection

构造一个使用 `QueryFilter::event_filter` 的只读 projection，返回计算聚合所需的最小列：

```text
canonical identity inputs
session/project/source labels
event_at
total_tokens
output_tokens + reasoning_output_tokens
cost_with_cache_usd
```

投影只执行一次。不得在得到 Top K 后按候选重新查询 event times，也不得为 duration 单独
加载全量时间表。

### 3.2 Session accumulator

每个 canonical identity 维护：

```text
label minima + homogeneous source
token/output/cost sums
event_count
first_event_at / last_event_at
ordered or sortable event times needed by ACTIVE_GAP_CAP_MINUTES=30
```

identity 计算必须与现有 `session_identity_sql` 同源。若 SQL 继续计算 identity，则 Rust 不
再复制 fallback；若改为 Rust 计算，则提取一个 typed helper，并用 SQL legacy oracle
逐分支验证。

成本累加顺序是风险点：D1 必须证明 serialized `cost_usd` 逐字节等价。不能用 tolerance
替代，也不能顺便引入 rounding。如果单次 projection 的 planner 顺序导致漂移，D1 被拒绝
或必须显式恢复 legacy 的确定性顺序。

### 3.3 Bounded selection

聚合完成后以 `limit <= 50` 的有界 Top K 选择器应用现有 comparator：primary 降序，
canonical id 升序 tie-break。先以完整排序实现 oracle，再证明 heap/selection 与它对随机
及并列 fixture 等价，避免为小常数引入错误复杂度。

## 4. D2 conditional expression index

D2 只在 D1 的代表性 all-range 任一排序 p95 超过 400 ms 时进入。候选索引逻辑为：

```text
usage_event(<full canonical identity expression>, event_at)
```

实施时必须先在副本动态创建同形索引，记录：

- normalized `sqlite_schema.sql`；
- pre/post `EXPLAIN QUERY PLAN` 和必要的 opcode/scan 计数；
- build wall time、page count/page size、DB/WAL bytes；
- D1+D2 HTTP matrix；
- 固定 synthetic `SyncRunWriter` 多轮交替中位数。

采用 D2 时新增 schema v24 `optimize_top_sessions_identity_order`。query 与 migration 的
identity expression 必须共享生成器，或由 parity test 比较 normalized SQL；查询计划测试
必须证明 all-range identity/time 路径使用目标索引且不再出现 identity 临时排序。

拒绝门：逐字节语义漂移、index > DB 15%、sync median regression >10%、目标 plan 未命中、
或 warm p95 仍 >400 ms。拒绝后不得提交 v24；持久化 rollup 需回到 Phase 1。

## 5. Migration, compatibility, and rollback

- D1-only：无 schema 变化，可单独 revert `top_sessions.rs` 与 tests。
- D2：v24 只增加一个真实索引，不添加列、表或 backfill；fresh 与 v23 upgrade 必须等价。
- migration 实验只在 verified online backup 上运行。活动 DB 不在本任务默认授权内。
- v24 会触发 newer-schema 边界；旧 binary 不能安全作为回滚。回滚是恢复 pre-v24
  verified backup 或继续用支持 v24 的 binary。
- index build 中断/失败必须由单 migration transaction 回滚，schema version 不前移。

## 6. Filter and planner matrix

每个策略至少验证：

| Shape | Required behavior |
| --- | --- |
| `all`, no filter | Primary failure reproduction and <=400 ms target |
| bounded range | Continue using effective event-time filtering; <=10% regression |
| source / host | Exact subset and canonical source prefix |
| model / project | Event-level filter before session aggregation |
| combined filters | Same SQL predicate conjunction and timezone boundaries |
| duration | Exact 30-minute gap; no rough-span candidate shortcut |
| limit 50 | No N+1 and bounded response size |

SQLite may choose different indexes for bounded and all-range shapes. Tests should assert semantic plan
properties rather than one universal index name where the current date/source index is cheaper.

## 7. API lifecycle and observability

`src/web/mod.rs` handler and 3-second Behavior deadline remain unchanged. Candidate work continues
through `load_behavior_api`, query permits, SQLite interrupt and `DashboardQuerySupervisor`.
Benchmark evidence correlates client timing with `section=sessions` query timing; timeout tests assert
background settlement and permit release. No new cache is introduced.

## 8. Validation and evidence

1. Focused equivalence/unit/integration tests.
2. D1 query-count/plan regression and sanitized benchmark.
3. Conditional D2 migration/plan/size/write benchmark.
4. Same-backup candidate HTTP matrix and rapid-switch browser check.
5. `python scripts/ci-rust.py`, `just ci`, `git diff --check`.
6. Independent `trellis-check` of exactness, migration safety, performance arithmetic and cleanup.

Cold/first-touch claims require an explicit reboot-cleared protocol. Without it, final evidence must say
`UNVERIFIED`; warm success must not be relabeled as cold success.

