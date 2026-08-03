# 会话分析性能实测

日期：2026-08-03。Windows debug build，使用仓库脱敏
`docs_dashboard_serve` fixture；每个端点先热身一次，再连续请求三次，记录最大墙钟时间与
未压缩 UTF-8 JSON bytes。该 fixture 用于稳定回归，不等同于用户生产库规模。

| 请求 | 三次最大耗时 | JSON bytes | 预算 |
| --- | ---: | ---: | --- |
| `/api/sessions?timezone=UTC&sort=tokens&limit=10` | 12.34 ms | 2,126 | <= 400 ms / <= 128 KiB |
| `/api/sessions?timezone=UTC&sort=duration&limit=10` | 10.58 ms | 2,116 | duration <= 100 ms；端点 <= 400 ms |
| `/api/hour_of_week?timezone=UTC` | 8.71 ms | 8,847 | <= 400 ms / <= 128 KiB |
| `/api/logs?timezone=UTC&page_size=50` | 8.99 ms | 9,965 | < 30 ms / page |

所有 measured fixture 路径均在预算内。代表性真实用户库仍应由 check/发布前基准复测；
本次没有放宽任何既有阈值。

## Check 修复后复测

`trellis-check` 将 duration 排序从不正确的固定 `3×N` span 候选改为全候选批量精算后，
使用同一脱敏 fixture 的 5,000 行种子逐端点热身一次并复测：

| 请求 | 耗时 | JSON bytes | 结果 |
| --- | ---: | ---: | --- |
| `/api/sessions?timezone=UTC&sort=duration&limit=10` | 81.12 ms | 2,138 | `<100ms` 且 `<128KiB` |
| `/api/hour_of_week?timezone=UTC` | 20.84 ms | 9,339 | `<400ms` 且 `<128KiB` |
| `/api/logs?timezone=UTC&page_size=50` | 7.35 ms | 42,029 | `<30ms` 且 `<128KiB` |
