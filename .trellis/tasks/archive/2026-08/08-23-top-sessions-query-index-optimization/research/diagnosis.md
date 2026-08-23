# Top Sessions 全历史查询诊断

核对日期：2026-08-23。所有数据库探测均以 SQLite `mode=ro` +
`PRAGMA query_only=ON` 执行，没有运行 migration 或写入用户数据库。

## 现象与边界

归档任务的 1.16 GB 在线备份实测显示：默认 `1d` 三种排序均在 102 ms 内，载荷小于
4 KiB；无界 Token/成本达到现有 3 秒 section timeout，时长单次约 2.5 秒。因此问题
不是 payload 或前端绘制，而是 `range=all` 的服务端查询形态。

`/api/sessions` 使用 `load_behavior_api`（`src/web/mod.rs:1081-1113`），故公开失败表现是
局部 degraded，不应通过提高 timeout 修复。

## 当前数据流

```text
tokens / cost
  full usage_event aggregate by computed identity
  -> ORDER BY aggregate + LIMIT K
  -> K separate event-time queries
  -> active-minute reducer + final sort

duration
  full usage_event aggregate by computed identity
  -> second full usage_event identity/time read + sort
  -> HashMap<identity, Vec<event_at>>
  -> active-minute reducer + final sort + LIMIT K
```

代码证据：`src/query/top_sessions.rs:80-204`。Token/成本是 `1 + K` 查询且每个候选条件
仍计算完整 identity；duration 是两次全历史读取。`limit` 最多 50，故 N+1 不能被视为
常数无害。

## 为什么现有 session index 不够

canonical identity 的顺序是：

1. 非空 `source + session_id`；
2. `source + source_path_hash`；
3. Codex/Claude 从 event key 提取；
4. 完整 event key。

定义见 `src/query/top_sessions.rs:239-255`。现有
`idx_usage_event_session(source, session_id, event_at)` 只覆盖第一分支，不能替换该
表达式。source_path/event_at/host 等现有索引同样不能直接提供完整 identity 顺序。

## 真实 v23 query plan

数据库信息：SQLite 3.50.4，schema v23。usage_event 已有 session、event_at、source、
host/source/event_at、Activity covering 与 Home compact covering 等索引。

只读 `EXPLAIN QUERY PLAN`：

```text
tokens_group
  SCAN e
  USE TEMP B-TREE FOR GROUP BY
  USE TEMP B-TREE FOR ORDER BY

duration_times
  SCAN e
  USE TEMP B-TREE FOR ORDER BY

candidate_times
  SCAN e USING INDEX idx_usage_event_event_at
```

结论：当前没有为完整 canonical identity 提供访问顺序的索引。candidate 查询虽然显示
使用 event_at index，但它仍扫描索引条目并逐行求 identity，重复 K 次。

## 方案分层

### D1 — query-only 单次投影（首选）

读取过滤后的事件投影一次，在 Rust 中按共享 identity 聚合 label、source、Token、成本、
event count、首末时间和 30 分钟 active gap，并用有界 Top K 选择器产生三种排序。

优点：先消除确定的 N+1、重复全表读取和 SQL temp grouping；不增加写放大、升级成本或
旧二进制兼容边界。风险：全历史仍需扫描事件；浮点成本的累加顺序必须通过 legacy
序列化 oracle，而不是假设等价。

### D2 — canonical identity + event_at 表达式索引（条件候选）

仅当 D1 未达标时，在隔离备份创建候选表达式索引，使 all-range identity/time 流不再
依赖临时排序。产品采用时才追加 v24 migration。

优点：不新增列或 backfill 表，identity fallback 可保持原样。风险：索引构建/体积、
sync 写放大、表达式重复漂移，以及索引顺序改变浮点累计结果。必须用 plan、逐字节 oracle、
体积和写入基准共同放行。

### 暂不采用 — 持久化 session rollup

rollup 能避免全历史读取，但 source/model/project/host/date 任意过滤与精确 30 分钟 gap
会把它扩展为新的写入投影和迁移/backfill 子系统。这超过已观察到的问题所需的最小
机制；D1+D2 不足时应重新规划，而不是在本任务中临时扩张。

## 验证原则

- legacy serialized payload 是精确性 oracle；不接受“数值近似相同”。
- warm benchmark 明确执行一次 warm-up + 五次样本；不把普通文件复制称为 cold。
- 真实数据库仅作为只读 backup source；候选 index/migration 只运行于 task-owned 副本。
- 若没有跨重启证据，first-touch 状态保持 `UNVERIFIED`。

