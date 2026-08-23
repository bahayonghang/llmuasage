# Baseline and database safety evidence

核对日期：2026-08-23。所有真实活动库访问均使用 SQLite `mode=ro` 与
`PRAGMA query_only=ON`；服务、migration 与候选索引只运行在
`target/trellis/08-23-top-sessions-query-index-optimization/` 下的任务副本。

## Fixed inputs

- 实现前 commit：`1bee90787dfd9ebcdd347665d240229d396e19c2`。
- legacy release binary：24,304,640 bytes，SHA-256
  `E274222A1622DE8C7030BC27B379A23DE3CB59BC5F5C8DD56BDCA5DE34938307`。
- 活动库：1,160,073,216 bytes，`mtime_ns=1787476925136392100`，schema v23，
  256,654 条 `usage_event`，`PRAGMA integrity_check=ok`。
- online backup：与活动库同字节数、同 schema/event count，
  `PRAGMA integrity_check=ok`；后续 baseline/candidate/final 均从该已验证副本派生。

最终验收结束后再次读取活动库，size、mtime、schema、event count 与上述值完全一致，
且不存在 `idx_usage_event_top_sessions_cover`。因此本任务没有迁移或写入活动库。

## Legacy HTTP baseline

协议：固定 legacy binary；一次 warm-up 后五次顺序样本；表中为 nearest-rank p95。

| Range | Tokens (ms) | Duration (ms) | Cost (ms) |
| --- | ---: | ---: | ---: |
| 1d | 67.70 | 23.29 | 55.79 |
| 7d | 214.86 | 53.06 | 156.60 |
| 30d | 2953.93 | 744.19 | 3015.91 |
| all | 3014.79 | 1945.29 | 3010.55 |

`all` 的 Token/Cost 已进入 3 秒降级边界，Duration 虽返回 supported，仍远高于
400 ms；30d Token/Cost 同样接近或达到 hard deadline。响应载荷不是瓶颈，最大值仍远低于
128 KiB。baseline server 停止后端口已释放，监督器最终为 0 inflight / 0 orphan。

