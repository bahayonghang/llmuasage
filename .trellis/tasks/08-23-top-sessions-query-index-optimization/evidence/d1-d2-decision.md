# D1 / D2 decision evidence

## D1: single event projection

D1 将同一过滤范围内的 `usage_event` 收敛为一个投影 statement，在 Rust 中使用单一
canonical identity accumulator 完成 label/source/token/cost/count/time 聚合，并用有界
Top K 选择器统一三种排序。Token/Cost 不再执行候选 N+1 时间查询，Duration 不再执行
第二次全量 identity/time 扫描；每个时间戳最多解析一次。

精确性测试先删除 v24 索引，在 pre-v24 访问路径上缓存 legacy 序列化 JSON；随后按精确
parity SQL 重建索引并运行候选。7 种 filter × 3 种 sort × 5 种 limit 的完整
`Vec<TopSessionRow>` 逐字节相等，另覆盖空库、identity fallback、并列、异常时间和 SQLite
补偿浮点求和。

D1-only 的最终热路径仍未达到性能门：

| Shape | Tokens (ms) | Duration (ms) | Cost (ms) |
| --- | ---: | ---: | ---: |
| 30d | 403.75 | 369.77 | 384.57 |
| all | 727.47 | 720.37 | 781.81 |
| source filter / all | 771.47 | 810.80 | 780.93 |
| host filter / all | 813.73 | 843.30 | 816.21 |

因此 Gate C 机械判定为 `D1 MISS`，按 PRD 进入 D2；没有提高 timeout、增加缓存、近似
Top N 或引入 rollup。

## D2 experiments

### Rejected two-column candidate

候选前缀为完整 canonical identity expression + `event_at`。它分配 4,946 pages，
即 20,258,816 bytes / 1.7463% 代表库，但 `EXPLAIN QUERY PLAN` 对 D1 投影仍为
`SCAN e`，没有命中目标访问路径，因此在 HTTP/migration 之前拒绝。

### Accepted covering candidate

接受的唯一新增索引是：

```text
idx_usage_event_top_sessions_cover(
  <exact canonical identity expression>, event_at,
  session_label, project_label, source,
  total_tokens, output_tokens, reasoning_output_tokens,
  cost_with_cache_usd, model, project_hash, host_id
)
```

- 独立副本构建耗时 1.293 s；最终 v23 -> v24 migration 耗时 1.380 s。
- 使用 freelist allocation 差值分配 16,521 pages，即 67,670,016 bytes / 5.8333%；
  主 DB 文件因复用 freelist 保持 1,160,073,216 bytes。
- `PRAGMA integrity_check=ok`，event count 不变。
- 无界查询显式 `INDEXED BY idx_usage_event_top_sessions_cover`；all/source/model/project/host
  均为 covering scan。任一 `since`/`until` 存在时不加 hint，并继续命中
  `idx_usage_event_event_at`。
- 固定 7 轮交替、每轮 4,000 events 的 `SyncRunWriter` 中位数：baseline
  185.836 ms，indexed 195.782 ms，ratio 1.053520（+5.35%），通过 +10% 门。

最初仅让 planner 自选 covering index 时，all/model/project 已小于 400 ms，但 source/host
仍选择旧非覆盖索引，p95 分别最高 519.70/616.81 ms。只对无界投影使用精确 index hint 后，
最终所有 shape 通过；bounded range 保留 planner 自由度。

## Production decision

D2 同时通过 serialized semantics、query plan、5.8333% size、5.35% write regression 与
最终 HTTP p95 门，故接受 schema v24 `optimize_top_sessions_identity_order`。migration 只
增加该索引，不增加列、表或 backfill。

