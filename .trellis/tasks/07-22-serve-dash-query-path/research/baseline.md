# S3 基线测量（实施前，commit 工作树现状）

测量时间：2026-07-22（本机，Windows，debug build）。测量代码：`src/testing/mod.rs::seed_stress_dashboard`（新增测试设施）+ 两个 `#[ignore]` 测量测试（`query::tests::measure_stress_diagnostics_and_full_sections`、`query::tests::measure_real_copy_diagnostics_and_full_sections`、`web::tests::measure_stress_double_full_concurrency`）。stat 计数由 `query/mod.rs` 内 `#[cfg(test)] DIAGNOSTICS_STAT_CALLS` 提供。

## 1. 数据集规模

| 数据集 | source_file | usage_event | usage_bucket_30m | usage_turn | distinct model |
| --- | --- | --- | --- | --- | --- |
| stress fixture | 5 000（4 000 真实文件 + 1 000 缺失路径） | 0 | 100 | 750 | 25 |
| 真实库只读副本（复制到 target/tmp/measure，原库未动） | 2 736 | 138 029 | 4 067 | 136 725 | 35 |

真实副本的 source_file 分布：antigravity 112 / claude 708（missing 72）/ codex 1 846 / opencode 0。按约定不记录任何文件路径内容。

## 2. diagnostics stat 风暴（R3.2 依据）

stress fixture（`measure_stress_diagnostics_and_full_sections`，3 连）：

| run | elapsed | stat_calls |
| --- | --- | --- |
| 0 | 119.9 ms | 5 000 |
| 1 | 121.0 ms | 5 000 |
| 2 | 113.9 ms | 5 000 |

真实副本（read-only，5 连，第二轮进程复测）：46.7–68.7 ms / 次，stat_calls=2 736（≈18–25 µs/stat）。**每次 dashboard 加载（core/interactive/full）与 `/api/diagnostics` 都全额支付该成本**——core_snapshot 在副本上 81–89 ms，其中 diagnostics 占 ~50 ms。

## 3. full scope 各 section 分解（真实副本，Dashboard 直调，两轮取第二轮）

| section | elapsed |
| --- | --- |
| core_snapshot | 81.3 ms（diagnostics 占 ~50 ms） |
| activity | 1.484 s |
| tools | 2.945 s |
| optimize | 2.676 s |
| compare | 484.8 ms（N+1：25 候选 × 单查） |
| explorer | 11.0 ms |

stress fixture 上同一组 section 全部 ≤1.2 ms（数据量太小，不具代表性，仅证明 fixture 不构成瓶颈）。

关键事实：**真实数据下 activity/tools/optimize 单查 1.5–2.9 s，远超 live 路径的 1 s behavior 超时（WEB_BEHAVIOR_API_TIMEOUT）**。即现状单请求 full 已固定降级这三个 section。

## 4. HTTP 层 full 行为（真实副本，`llmusage serve --home <copy>`，debug build）

单请求 full（4 次）：1.174 s / 1.183 s / 1.194 s / 1.211 s → 中位 ~1.19 s。support 级别：activity/tools/optimize = degraded（超时），compare = normalized（485 ms < 1 s），explorer 正常。

双并发 full（3 轮 × 2 请求）：A/B 均 1.17–1.20 s，与单请求持平（排队计入各自 1 s 预算，wall 不变）。但**降级面扩大**：两个请求的 compare 也变为 degraded（permit 排队吃掉其 1 s 预算）——级联效应体现为降级面而非 wall-clock。

| 场景 | wall | activity | tools | optimize | compare | explorer |
| --- | --- | --- | --- | --- | --- | --- |
| 单 full | ~1.19 s | degraded | degraded | degraded | normalized | ok |
| 双并发 full（每个） | ~1.18–1.20 s | degraded | degraded | degraded | degraded | ok |

## 5. interactive 范围延迟（benchmark-dashboard-range.mjs，副本，iterations=5）

| range | median | p95 | bytes |
| --- | --- | --- | --- |
| 1d | 84.2 ms | 96.8 ms | 11 583 |
| 7d | 96.7 ms | 117.9 ms | 13 614 |
| 30d | 121.5 ms | 125.4 ms | 24 833 |
| all | 125.0 ms | 145.1 ms | 55 801 |

均在 400 ms / 128 KiB 预算内；其中 ~50 ms 为 diagnostics stat 成本（占 1d 延迟约 60%）。浏览器端 secondary_complete 最长 2 253 ms（behavior 超时所致）。原始输出：`research/baseline-benchmark.json`。

## 6. 结论（对方案的影响）

- R3.2（diagnostics 缓存）收益明确：core/interactive/full 每次加载省 50–120 ms；`/api/diagnostics` 与 dashboard 共享缓存后 TTL 内第二次 stat=0。
- R3.3 风险量化：真实数据下 secondary 顺序总耗时 ≈ 1.48+2.94+2.68+0.48+0.01 ≈ **7.6 s**（即便按 1 s 超时截断也 ≈ 3.5 s），现状并行 wall ≈ **1.19 s**。顺序化 full p95 退化远超 20% 阈值；备选"permit 提升 + 自限并发"无法消除 compare 在并发下的排队降级（根本原因是 section 查询本身超过 1 s 预算，而非 permit 数量）。→ 倾向记录"不实施"，最终形态见 design.md 决策记录与 after.md 复测。
- R3.4（compare N+1）：真实库 25 候选 × 单查 ≈ 485 ms，接近 1 s 预算上限；改单条 GROUP BY 后预期降至一次扫描。
- R3.5：busy_timeout 30 s 对 web 读无意义（5 s 预算先到），降 1–2 s 让锁等待快速进入既有超时/降级。
