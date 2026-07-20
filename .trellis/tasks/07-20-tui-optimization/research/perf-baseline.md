# TUI 综合优化性能证据

统一协议：本机 1,160,073,216-byte 代表性数据库、release 构建、预热后 3 次取中位数。后续子任务在本文件追加各自指标。

## 异步加载与时间窗

30d 本地日历窗口（`2026-06-22..2026-07-21`）：

| Panel | 串行中位数 | 并行中位数 | 改善 |
| --- | ---: | ---: | ---: |
| Stats | 169.3 ms | 108.7 ms | 35.8% |
| Behavior | 3777.5 ms | 1800.4 ms | 52.3% |
| Blocks | 403.2 ms | 34.3 ms | 91.5% |

Blocks 主扫描由 135,803 行降至 8,163 行，反向锚点探测读取 1,536 行；有界结果与全量结果完全相等。详细样本与查询计划见 `07-20-tui-time-window-bounding/research/perf-results.md`。

## 首访渲染线程最长连续同步区段

命令：

```powershell
cargo test --release tui::tests::measure_local_render_thread_first_visit -- --ignored --nocapture --test-threads=1
```

固定 120x30 TestBackend；每个面板先预热一次，再运行 3 次冷状态首访。每次分别
计时请求分发、loading 帧绘制、匹配结果应用、populated 帧绘制，后台查询等待在
计时区段外。下表单位均为 ms：

| Panel | Sample | Dispatch | Loading draw | Result apply | Populated draw | Max section |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Stats | 1 | 0.013 | 0.117 | 0.001 | 0.190 | 0.190 |
| Stats | 2 | 0.014 | 0.110 | 0.001 | 0.210 | 0.210 |
| Stats | 3 | 0.012 | 0.118 | 0.001 | 0.203 | 0.203 |
| Behavior | 1 | 0.015 | 0.106 | 0.003 | 0.250 | 0.250 |
| Behavior | 2 | 0.017 | 0.114 | 0.002 | 0.235 | 0.235 |
| Behavior | 3 | 0.013 | 0.113 | 0.002 | 0.267 | 0.267 |
| Blocks | 1 | 0.015 | 0.170 | 0.001 | 0.186 | 0.186 |
| Blocks | 2 | 0.013 | 0.115 | 0.001 | 0.212 | 0.212 |
| Blocks | 3 | 0.013 | 0.105 | 0.001 | 0.213 | 0.213 |

| Panel | 优化前同步阻塞中位数 | 优化后最大同步区段中位数 | 改善 |
| --- | ---: | ---: | ---: |
| Stats | 169.3 ms | 0.203 ms | 99.9% |
| Behavior | 3777.5 ms | 0.250 ms | 100.0%（四舍五入到 1 位小数） |
| Blocks | 403.2 ms | 0.212 ms | 99.9% |

这里的“改善”只比较渲染线程连续阻塞，不表示后台查询 wall-time 降至同一数值。
原始输出与测量边界见 `07-21-tui-render-thread-benchmark/research/render-thread-results.md`。

## 父任务集成验收（2026-07-20）

- `just ci`：最终复跑通过（fmt、严格 clippy、366 library tests 中 3 个本地性能测试 ignored、
  全部 integration/doc tests、node checks、VitePress build）。
- 隔离 TTY smoke：使用 Windows Terminal 与临时 `LLMUSAGE_HOME=%TEMP%\\llmusage-dash-smoke`。
  `dash` 成功启动；按 `x` 后进程在发送 Tab/9/j/o 后仍存活；发送 `q` 后
  `llmusage` 进程在 176ms 内退出，Windows Terminal 进程也恢复为 0。未触碰用户默认数据库。
- X7(b)：沿用异步任务 30d release 三次中位数，Stats 35.8%、Behavior 52.3%，
  均达到 A3 的 >=30% 门槛，见异步/时间窗子任务 evidence。
- X7(c)：render 子任务确定性证据：40 个 idle tick 产生 0 次 draw request，4 个
  active animation tick 产生 4 次；等值 buffer 与可见行窗口测试通过。
- X7(a)：release 预热后三次证据通过。Stats/Behavior/Blocks 的最长连续渲染线程
  同步区段中位数分别为 0.203/0.250/0.212 ms；后台查询等待明确排除，不使用静态
  架构推断或 TTY smoke 的 176ms 退出耗时代替该指标。
