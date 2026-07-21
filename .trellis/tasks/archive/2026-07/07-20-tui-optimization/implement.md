# TUI 综合优化执行计划

- [x] 依次完成并归档 6 个子任务；每次开始前确认前序依赖已完成。
- [x] 维护父 PRD X1-X7 的证据矩阵，拒绝用局部单测替代跨任务验收。
- [x] 在 `research/perf-baseline.md` 记录统一数据快照、release 构建、3 次样本与中位数；X7(a) 的 render-thread blocking 样本已由 `07-21-tui-render-thread-benchmark` 补齐。
- [x] 手工烟测 `dash`：在隔离临时 `LLMUSAGE_HOME` 下验证 sync、快速切面板/键盘选择、帮助对话框与退出恢复；真实数据库未被触碰。
- [x] 运行 `just ci`，检查最终 diff 与未跟踪文件；父任务归档前将对新增基准再运行最终质量门禁。

回滚：每个子任务独立提交；集成失败时回退最近子任务并保留失败证据，不跨提交混合重写。
