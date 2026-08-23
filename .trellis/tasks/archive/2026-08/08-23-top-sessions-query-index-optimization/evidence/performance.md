# Final representative performance evidence

## Protocol

- Binary：包含最终 query、migration、`Server-Timing` 的 release SHA-256
  `1C13EE218B67153C3C1C8A0D9ED2D99595E8A6DD13B493B867504DFF81EF0C24`；source
  metadata 为 `1bee90787dfd-dirty`，提交后由 commit trailer 绑定任务。
- Database：从 verified v23 baseline 派生的全新任务副本，由 final binary 真实升级到 v24。
- Server：`127.0.0.1:39062`，非 public；验收后已停止并确认端口释放。
- 每个 shape 一次 warm-up，随后五次顺序 HTTP 样本；p95 使用 nearest-rank（五次中的最大值）。
- filter 值从副本内选择，只保留 source/model/project/host 形状，不记录实际标识。
- 可重复命令由 `scripts/benchmark-top-sessions.mjs` 固化；原始 allowlisted 输出为
  `final-sessions-harness.json`，逐样本保留 status/support/wall/query/payload，不保留 URL、
  response rows 或 filter 值。

## Range matrix

全部样本为 HTTP 200 / `supported`。

| Range | Tokens p95 (ms) | Duration p95 (ms) | Cost p95 (ms) |
| --- | ---: | ---: | ---: |
| 1d | 11.17 | 13.27 | 10.92 |
| 7d | 21.47 | 21.73 | 23.30 |
| 30d | 233.91 | 244.11 | 233.99 |
| all | 210.99 | 207.93 | 207.74 |

1d/7d/30d 的九个 p95 均低于任务 legacy baseline，因此没有超过 10% 的 bounded-range
回归。all 三排序均低于 400 ms，且没有样本触发 3 秒 deadline。

## All-range filter matrix

| Filter shape | Tokens p95 (ms) | Duration p95 (ms) | Cost p95 (ms) |
| --- | ---: | ---: | ---: |
| source | 192.57 | 191.52 | 194.05 |
| model | 143.31 | 147.19 | 149.01 |
| project | 126.72 | 121.21 | 125.89 |
| host | 212.85 | 219.08 | 209.11 |

矩阵共 24 shapes / 120 warm samples：0 non-200、0 degraded、0 p95 >400 ms、
0 payload >128 KiB；最大响应 21,202 bytes。`/api/sessions` 的
`Server-Timing: sessions-query;dur=<ms>` 逐样本证据完整，最大 case p95 为 242.43 ms。

## Server query p95 matrix

| Shape | Tokens query (ms) | Duration query (ms) | Cost query (ms) |
| --- | ---: | ---: | ---: |
| 1d | 9.19 | 11.39 | 9.48 |
| 7d | 20.06 | 20.37 | 21.69 |
| 30d | 232.14 | 242.43 | 232.06 |
| all | 208.81 | 205.72 | 205.92 |
| source | 190.49 | 189.42 | 192.00 |
| model | 141.34 | 145.41 | 147.04 |
| project | 125.23 | 119.67 | 124.43 |
| host | 210.51 | 217.17 | 207.19 |

## Lifecycle checks

- 独立 lifecycle server `127.0.0.1:39061` 上，concurrency-2：2/2 HTTP 200 +
  supported，最大客户端 wall 376.22 ms；服务停止后端口已释放。
- 快速交替 `1d/all/7d/all × tokens/duration/cost`：12/12 HTTP 200 + supported。
- 最终 `/api/diagnostics`：0 inflight、0 timed-out、0 orphan，orphan duration 为空。
- Rust timeout/interrupt tests 与 Node AbortSignal/latest-wins tests 通过。
- 未跨真实系统重启或控制操作系统文件缓存；cold/first-touch：**UNVERIFIED**。
