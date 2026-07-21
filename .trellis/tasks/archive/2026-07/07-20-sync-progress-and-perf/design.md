# 父任务设计说明

技术设计拆到各子任务；本文件只记录跨子任务共享的架构决策与契约。

## 1. 共享架构决策

- 渲染层归一化，不动事件契约：三个注册 parser 的进度单位不一致（OpenCode=行数、Codex/Claude=重放文件数、总量=inventory 文件数），渲染器按来源选择展示形态（见 lifecycle 设计 §2），不向 parser 回传任何新事件字段。
- 单一文案来源：`human_progress_line()`（src/commands/sync.rs:633-722）继续产出全部中文文案；TTY bar 的 message 与完成永久行复用它，非 TTY 逐行输出也复用它。
- 渲染器抽象 + 可注入 draw target：`LineRenderer`（非 TTY / `LLMUSAGE_PROGRESS=off`）与 `BarRenderer`（TTY indicatif）；构造时注入 `ProgressDrawTarget`，测试可注入 buffered/hidden target 断言清理行为。
- 生命周期归 CLI 命令层所有：渲染器与 RAII 清理守卫在 `run_with_human_events`（sync.rs:65-131）内创建，覆盖 bootstrap → 锁 → driver → 摘要全周期；reporter task 只消费事件，不拥有终端状态。
- 颜色仅经 `console::Style`，仅 TTY；`console` 与 `indicatif` 为仅有的两个新增直接依赖。

## 2. 跨子任务契约

- lifecycle 提供 `LLMUSAGE_PROGRESS=off`（非空即关）强制 LineRenderer，供 profiling 子任务做同 TTY 开/关对照，也作为用户 fallback。
- lifecycle 提供 Ctrl-C → `CancellationToken` 接线（`run_once_with_cancel`，sync.rs:306-315 已接受 token）；profiling 测量时不得依赖取消路径。
- summary 子任务只改 `print_summary`（sync.rs:223-244）及其下游纯函数，不触碰渲染器与事件流。
- profiling 子任务的计时布点走现有 tracing（debug 级），不加 CLI 旗标、不改 stats 结构。

## 3. 集成与回滚

- 每个子任务独立 commit/归档；回滚粒度为子任务。
- LineRenderer 全程保留为 fallback，回滚 lifecycle 即恢复现状行为。
- 无 schema/持久化格式变更。
