# sync 全链路 profiling 执行计划

轻量研究任务，设计即 prd.md 的 R1-R6；无独立 design.md。

## Checklist

1. 布点：bootstrap/锁/driver/查询/摘要的 `Instant` 计时（tracing debug）；driver sink 丢弃计数器。
2. 夹具与快照：构造真实规模 tempfile home；写快照保存/恢复步骤（脚本或测试辅助函数，放入 `scripts/` 或 tests 公用模块），保证「恢复 → 运行」可重复。
3. 基线测量：同快照跑 3 次，记录各阶段耗时中位数 + min-max。
4. 渲染开销对照：同快照、同输出目标，`LLMUSAGE_PROGRESS=off` 开/关各 3 次。
5. 候选排查（a)-(e) 逐项记录结论；确认的问题按 R5 纪律就地修或另建子任务。
6. 撰写 `research/profiling.md`。
7. `cargo fmt --all -- --check`、严格 Clippy、`cargo test -- --test-threads=1`。

## 前置依赖

- `07-20-sync-progress-lifecycle` 完成（提供 `LLMUSAGE_PROGRESS=off` 与渲染器）。

## 风险点

- Windows 上计时受 Defender/索引影响：同机同负载连续测量，报告中注明环境干扰；相对对照优先于绝对值。
