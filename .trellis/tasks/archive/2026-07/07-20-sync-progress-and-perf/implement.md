# 父任务执行计划

## 顺序

1. 启动并完成 `07-20-sync-progress-lifecycle`（其子任务 implement.md 为准）。
2. 启动并完成 `07-20-sync-summary-display`。
3. 启动并完成 `07-20-sync-full-profiling`；其确认的问题按该子任务 PRD 的规则就地修或另建子任务。
4. 父任务集成验收（见父 prd.md A1-A5）：真实 TTY 冒烟 + `just ci`。

## 评审门禁

- 每个子任务 `task.py start` 前过各自 planning 评审。
- profiling 子任务若提出语义层修复，先对照 `.trellis/spec/llmusage/backend/source-sync-contracts.md` §3 确认不越界。
