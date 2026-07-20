# TUI 首访渲染线程阻塞基准

## Goal

补齐父任务 X7(a) 的事实证据：按统一性能协议测量 Stats、Behavior、Blocks
首次访问期间，渲染线程各连续同步区段的最长耗时，并与优化前同步查询中位数对比。

## Requirements

- 使用代表性本地数据库、release 构建、预热后 3 次样本并取中位数。
- 每次访问分别计时请求分发、loading 帧绘制、结果接收后的状态应用、数据帧绘制；
  后台查询等待必须排除在计时区段之外。
- 每次访问报告四个同步区段和其中最大值；每个面板报告 3 次最大值的中位数。
- 基准必须是 `#[ignore]` 的本地证据测试，不进入常规测试耗时，不写数据库或改变
  生产 TUI 行为。
- 优化前基线沿用父任务已记录的同步查询中位数：Stats 169.3 ms、Behavior
  3777.5 ms、Blocks 403.2 ms；不得用查询 wall-time、静态架构或退出 smoke 代替
  优化后渲染线程测量。

## Acceptance Criteria

- [x] `cargo test --release tui::tests::measure_local_render_thread_first_visit -- --ignored --nocapture --test-threads=1`
  输出三个面板各 3 次同步区段样本、最大值和中位数。
- [x] 父任务 `research/perf-baseline.md` 与集成矩阵记录精确结果，X7(a) 只在证据
  满足协议后标记通过。
- [x] focused test、fmt、严格 clippy、serial tests 与 `just ci` 全绿。
- [x] 最终 diff 不包含用户的未跟踪 `TODO.md`。

## Notes

- 父任务：`.trellis/tasks/07-20-tui-optimization`。
