# P1 异步面板加载性能证据

## 测量协议

- 日期：2026-07-20
- 数据库：本机 `~/.llmusage/llmusage.db`，1,160,073,216 bytes
- 构建：release
- 命令：`cargo test --release tui::data_loader::tests::measure_local_parallel_panel_loading -- --ignored --nocapture --test-threads=1`
- 每条路径预热后测 3 次，取中位数；串行基线为优化前单连接顺序调用，异步路径为独立连接 + `spawn_blocking`，最终许可数 5。

## 最终 P1 实现样本

| Panel | 串行样本 (ms) | 异步/并行样本 (ms) | 中位数改善 |
| --- | --- | --- | --- |
| Stats | 310.2 / 319.2 / 334.3 | 287.1 / 295.8 / 303.9 | 7.3% |
| Behavior | 6121.5 / 6715.4 / 6854.4 | 3447.8 / 3474.1 / 3801.1 | 48.3% |

4-permit 对照中 Stats 改善 13.2%、Behavior 改善 51.6%；增加第五许可没有解决 Stats，排除“第五任务单纯排队”为主因。

## Stats 瓶颈分解

同一数据库的一次顺序组成测量：

| 子载荷 | 耗时 (ms) |
| --- | ---: |
| overview | 5.1 |
| heatmap | 2.2 |
| source breakdown | 1.1 |
| health | 7.0 |
| context pressure | 362.5 |

`context_pressure` 约占该样本 Stats 查询时间的 96%。并行其余四项无法带来 30% 总体改善，多连接并发扫描同一 SQLite 文件还会产生 I/O 争用。

## 验收状态

- Behavior 达到 A3 的 >=30% 门槛。
- Stats 只达到 7.3%，A3 **未完成**。
- 不降低门槛、不把 UI 非阻塞等同于查询加速。下一步按已规划的 `07-20-tui-time-window-bounding` 给 `context_pressure` 接入时间范围，再用相同命令与数据库复测；P1 在该证据达标前保持 `in_progress`。
