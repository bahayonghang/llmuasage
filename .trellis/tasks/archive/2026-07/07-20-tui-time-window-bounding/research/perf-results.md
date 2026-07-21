# TUI 时间窗与 Blocks 扫描收敛性能证据

## 测量协议

- 日期：2026-07-20
- 数据库：本机 `~/.llmusage/llmusage.db`，1,160,073,216 bytes
- 构建：release
- 每条路径预热后测 3 次，取中位数
- 时间窗：30d，本次本地日历边界为 `2026-06-22..2026-07-21`

## Stats / Behavior

命令：

`cargo test --release tui::data_loader::tests::measure_local_parallel_panel_loading -- --ignored --nocapture --test-threads=1`

| Panel | 串行样本 (ms) | 并行样本 (ms) | 中位数改善 |
| --- | --- | --- | ---: |
| Stats | 141.9 / 169.3 / 172.8 | 101.1 / 108.7 / 111.4 | 35.8% |
| Behavior | 3225.6 / 3777.5 / 4004.5 | 1739.7 / 1800.4 / 1834.6 | 52.3% |

Stats 保留 lifetime Overview 与 365d heatmap，只对 source mix/context pressure 应用 30d 窗口。`context_pressure` 单次全源组成测量为 118.8 ms；并行路径按注册 source 拆为等值索引范围查询，再按事件数加权合并平均值。测试同时验证全源结果与逐 source 合并等值，查询计划使用 `idx_usage_event_source_event_at`。

## Blocks

命令：

`cargo test --release query::reports::tests::measure_local_blocks_bounding -- --ignored --nocapture --test-threads=1`

| 路径 | 样本 (ms) | 中位数 | 主扫描事件数 |
| --- | --- | ---: | ---: |
| 全量 | 397.6 / 403.2 / 429.3 | 403.2 ms | 135,803 |
| 断档锚定 | 33.6 / 34.3 / 35.6 | 34.3 ms | 8,163 |

- wall-time 改善：91.5%
- 主扫描行数下降：94.0%
- 反向锚点探测读取：1,536 行
- 扫描起点：`2026-07-17T00:51:17.788Z`
- fallback：false
- 锚点计划：`SEARCH usage_event USING INDEX idx_usage_event_source_event_at (source=? AND event_at<?)`
- 主扫描计划：`SEARCH usage_event USING INDEX idx_usage_event_source_event_at (source=? AND event_at>?)`

同一 benchmark 每次断言有界路径与全量构块结果完全相等。常规测试另覆盖跨展示边界块、断档重锚、active block 与连续无断档回退。
