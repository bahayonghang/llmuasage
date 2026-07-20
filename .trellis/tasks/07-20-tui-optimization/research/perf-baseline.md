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

## 待补

- render 子任务：空闲无动画 10s draw 调用计数。
- 父任务最终验收：渲染线程最长连续阻塞时长汇总。

## 父任务集成验收（2026-07-20）

- `just ci`：通过（fmt、严格 clippy、365 library tests 中 2 个设计性 ignored、
  全部 integration/doc tests、node checks、VitePress build）。
- 隔离 TTY smoke：使用 Windows Terminal 与临时 `LLMUSAGE_HOME=%TEMP%\\llmusage-dash-smoke`。
  `dash` 成功启动；按 `x` 后进程在发送 Tab/9/j/o 后仍存活；发送 `q` 后
  `llmusage` 进程在 176ms 内退出，Windows Terminal 进程也恢复为 0。未触碰用户默认数据库。
- X7(b)：沿用异步任务 30d release 三次中位数，Stats 35.8%、Behavior 52.3%，
  均达到 A3 的 >=30% 门槛，见异步/时间窗子任务 evidence。
- X7(c)：render 子任务确定性证据：40 个 idle tick 产生 0 次 draw request，4 个
  active animation tick 产生 4 次；等值 buffer 与可见行窗口测试通过。
- X7(a)：**missing evidence**。现有资料只有查询线程迁移前的 wall-time 基线，
  没有按统一协议测量迁移后 Stats/Behavior/Blocks 渲染线程最长连续阻塞的三次样本；
  不将静态架构推断或 TTY smoke 的 176ms 退出耗时改写为该指标。
