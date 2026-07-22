# S3 实施后复测

测量时间：2026-07-22。本报告使用仓库内 `target/tmp/measure` 的代表数据库副本；原库未修改，报告不记录任何源文件路径或内容。原始数据见 `after-benchmark.json` 与最终复核 `after-benchmark-final.json`。

## 1. interactive 延迟与体积

`node scripts/benchmark-dashboard-range.mjs --url http://127.0.0.1:39103 --iterations 5`：

| range | baseline p95 | final p95 | 改善 | final bytes |
| --- | ---: | ---: | ---: | ---: |
| 1d | 96.78ms | 13.80ms | 85.7% | 11,304 |
| 7d | 117.85ms | 16.01ms | 86.4% | 13,614 |
| 30d | 125.40ms | 28.36ms | 77.4% | 24,833 |
| all | 145.14ms | 46.28ms | 68.1% | 55,801 |

全部范围满足 p95 <=400ms、JSON <=128KiB。浏览器点击反馈为 1.5–2.9ms；快速连切最终激活范围为 `all`，未出现陈旧结果覆盖。

## 2. 80ms cold 契约

显式清除 `CI` 宽限后，`home_overview_under_80ms_with_seeded_10k_events` 连续三次结果：

| profile | run 1 | run 2 | run 3 |
| --- | ---: | ---: | ---: |
| debug | 59.7148ms | 62.7767ms | 60.1822ms |
| release | 22.0076ms | 20.3775ms | 19.2181ms |

六次均低于 80ms，证明 diagnostics 缓存没有进入 query 层改变 cold read 语义。

## 3. diagnostics、compare 与 busy timeout

- WebState 缓存测试覆盖 TTL 命中、过期后外部删除、sync 终态失效、API/dashboard 共享、并发 single-flight，以及 invalidate 与在途计算之间的 generation fence。
- compare candidates 的 `usage_turn` 查询由最多 25 次降为固定 1 次；等价 oracle 覆盖空集、25 上界、无 turn 候选和多种过滤条件，并对候选及完整 payload 做字段等价断言。
- web 读连接测试断言 busy timeout 为 1500ms，Store 默认连接仍为 30s；锁占用测试进入既有超时/degraded 路径。

## 4. full 与双并发决策证据

代表库当前直调 section：core 152ms、activity 2.34s、tools 3.95s、optimize 3.63s、compare 638ms、explorer 13ms。顺序执行约 10.6s；即使三个慢 behavior 各按 1s 截断，仍约 3.65s，远超基线 full 约 1.19s 及 20% 退化门槛。

因此不实施单连接顺序 full，保留并行逐 section 降级。最终 HTTP 复测：

| 场景 | wall | activity/tools/optimize | compare | explorer |
| --- | --- | --- | --- | --- |
| 单 full，3 连 | 1.21–1.45s | degraded | degraded | normalized |
| 双并发 full，3 轮 | 1.24–1.52s | degraded | degraded | normalized |

请求均为 HTTP 200，未触发 5s 整体超时；但完整 `model_compare` 在并发重查询下仍可能超过其 1s section 预算。R3.4 消除的是 candidates N+1，不能据此声称整个 compare section 永不 degraded。这个 residual 接受为“不实施 R3.3”的显式结果，而不是验收遗漏。
