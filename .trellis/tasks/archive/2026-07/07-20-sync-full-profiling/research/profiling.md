# sync 全链路 profiling 记录

日期：2026-07-20 · 构建：release（`cargo build --release`）· 机器：Windows（Git Bash），测量期间同机同负载连续运行。

## 1. 测量协议

- 数据：真实源数据（Codex 1813 文件 / 2.3 GB，Claude 698 文件 / 854 MB，OpenCode 1 DB / 41 MB），源目录只读；llmusage 目标 DB 用 `--home` 指向临时目录。
- 快照恢复：热跑前从全量导入后的快照整目录复原目标 DB（`cp -r snap home`），保证每次面对同一增量状态；冷跑每次全新 home。
- 同输出目标：stdout/stderr 全部重定向到文件。
- 每种模式 3 次，报告中位数与 min-max。LLMUSAGE_LOG=debug，指标取自 `<home>/logs/llmusage.ndjson` 的布点字段。
- 布点（本任务新增，tracing debug）：bootstrap_ms、driver_ms、stored_query_ms/stored_queries、pipeline_ms、per-source parse_wall_ms、progress_dropped（try_send 丢弃计数）、render_calls/render_ms（渲染器累计耗时）。

## 2. 数据规模与总览

冷跑（全量导入，134,075 事件入库）：

| 运行 | wall | pipeline | codex parse_wall | claude | opencode | bootstrap | stored_query(4次) | render |
|---|---|---|---|---|---|---|---|---|
| cold1 | 32,464ms | 32,283 | 27,547 | 4,167 | 440 | 35 | 42 | 118 calls / 3ms |
| cold2 | 32,361 | 32,175 | 27,245 | 4,121 | 642 | 35 | 64 | 118 / 3ms |
| cold3 | 31,466 | 31,276 | 26,618 | 4,099 | 428 | 36 | 40 | 118 / 3ms |
| 中位数 | 32,361（31,466–32,464，±3%） | | | | | | | |

热跑（恢复快照，增量，仅 1 个 Codex 文件变化 / 10.6 KB）：

| 运行 | wall | pipeline | driver | bootstrap | stored_query | render |
|---|---|---|---|---|---|---|
| hot1 | 472ms | 182 | 95 | 141 | 40 | 8 calls / 0ms |
| hot2 | 493 | 187 | 97 | 154 | 43 | 8 / 0ms |
| hot3 | 515 | 184 | 96 | 194 | 42 | 8 / 0ms |
| 中位数 | 493（472–515，±8%） | | | | | |

冷跑摘要（cold1）：codex parse 2.2s / write 25.3s；claude 1.4s / 2.8s；opencode 243ms / 194ms。
热跑摘要（hot1）：codex 53ms / 0ms（skipped=1812），claude 30ms / 0ms，opencode 6ms / 0ms。

## 3. 候选排查结论（prd R4）

- (a) `stored_events_for_source` 逐来源查询：4 次共 40–64ms。冷跑占比 0.1–0.2%，**不是问题**；热跑占比约 8%（40/472ms）但绝对值小，单次 COUNT 约 10ms，记录不处理。
- (b) 通道 128 容量丢弃率：**progress_dropped=0**（冷热全部来源、含 118 事件尖峰），未复现背压问题。
- (c) 非 TTY 每事件 write!+flush：render 118 次共 3ms（冷）、8 次 0ms（热），**不是问题**。
- (d) 锁心跳间隔：租约 30 分钟、间隔 = lease/3 = 10 分钟一次单 UPDATE（src/store/mod.rs:44、src/store/lock.rs:57-60），可忽略，**不是问题**。
- (e) 渲染线程事件处理速率：118 事件/32s，render 累计 3ms，**不是问题**。

## 4. 渲染开销对照（父 R5 / 子 A4）

- 直接证据：渲染器 `render()` 累计耗时 —— 冷跑 3ms / 32.4s ≈ **0.009%**，热跑 0ms。远低于 2% 阈值。
- 本环境无 TTY，管道下两种配置（默认与 `LLMUSAGE_PROGRESS=off`）都选 LineRenderer，墙钟差异无意义故未列表；Bar 路径成本由 `stderr_with_hz(10)` 绘制节流 + 独立 reporter task（不阻塞 parser）+ try_send 丢弃背压三重上限约束。TTY 实机对照列入父任务手动验收 A3。
- 结论：进度渲染开销可忽略，有测量证据。

## 5. 确认的问题与处置

- **P1（立项）**：冷跑 Codex write 25.3s（约 4.1k events/s，104,110 事件），占冷跑 78%。全量导入为一次性路径，但写入吞吐有明确优化空间（批大小/索引/pragma 待测量）。超出本任务「小修就地」范围（涉 writer 提交协议），按 R5 纪律另建子任务 `07-20-sync-cold-import-write-throughput`（PRD-only backlog，P3），修复需先对照 source-sync-contracts.md §3 单写者/原子提交契约。
- **P2（记录不处理）**：热跑 bootstrap 141–194ms（冷跑仅 35ms），含定价目录版本检查；绝对值小，暂不处理。
- **P3（记录不处理）**：热跑 stored_events 4 次 COUNT 共 40ms（见 3a）。

## 6. 复现步骤

```bash
cargo build --release
bash /tmp/bench_sync.sh ./target/release/llmusage.exe /tmp/sync_bench
# 脚本协议：3 冷跑（全新 --home）→ 全量导入做快照 → 3 热跑（cp -r 恢复快照）；
# 指标从 <home>/logs/llmusage.ndjson 提取（LLMUSAGE_LOG=debug）。
```

脚本全文见本任务 implement.md 附录（会话临时文件 /tmp/bench_sync.sh，未入库；协议如上足以复现）。
