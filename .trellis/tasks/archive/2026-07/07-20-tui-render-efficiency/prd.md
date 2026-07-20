# TUI 渲染效率优化（按需重绘/主题快照/行窗口化）

父任务：`.trellis/tasks/07-20-tui-optimization`。证据：父任务 `research/llmusage-tui-analysis.md` §1/§4/§5、`research/tokscale-patterns.md` §5/§6（模式 3/10/11）。建议顺序：在 07-20-tui-async-panel-loading（数据世代）与 07-20-tui-style-unify（golden 基线定稿）之后串行进行——与 style 同触 theme.rs 与全部面板 API，不并行。

## Goal

降低渲染路径的常驻开销：空闲时不再无条件全量重绘，每帧不再重建不变的字符串与重复拿主题锁；大表只构建可见行。

## Confirmed Facts

- src/tui/mod.rs:97 每次事件（含每 250ms Tick）无条件 `terminal.draw`；空闲 ~4fps 常驻全量重绘，无脏标记。
- 各表格面板每帧重建全部行 String（models.rs:61-98、daily.rs:57-100、hourly.rs:63-108、cost.rs:66-84 等）；行数据不随帧变化，仅数据世代变化时才需重建。
- theme.rs:135-137 `active_theme()` 每次取色拿 RwLock 读锁并复制整个 Theme；每帧数百次（每个 Span/Style 构造各一次）。
- event.rs:19 无界 std mpsc：主线程阻塞期间 Tick 积压，恢复后连发重绘。
- tokscale 对标：无循环级脏标记但靠 ratatui 双缓冲 diff 兜底 + 手动行窗口化（models.rs:144 只切可见区间构建行）+ 数据世代计数失效派生 memo（app.rs:538,2372-2445）+ 模型色表仅在数据更新时重建（app.rs:568）。
- 注意：spinner/进度类动画（async 任务与 sync 任务落地后）要求 loading 期间仍按 tick 重绘——脏标记须把「有动画活动」计为脏。

## Requirements

- R1 按需重绘：引入 dirty 标志——按键/resize/数据到达/状态行变化/dialog 开关/动画活动（loading spinner、sync 进度）置脏；空闲纯 Tick 不重绘。auto-refresh 与 status 过期检查逻辑不受影响。
- R2 主题快照：每帧开头取一次 Theme 快照并向下传引用（或等价方案：渲染期免锁），消除每 Span 级锁操作；主题切换后下一帧生效即可。API 迁移不得改变默认渲染输出。
- R3 行构建收敛：表格面板改为只构建可见窗口行（对标 tokscale 手动切片），并以数据世代为键 memo 已格式化行（世代由 07-20-tui-async-panel-loading 引入；若该任务未先行，则以「载荷替换」为失效点）。滚动只移动窗口不重格式化整表。
- R4 事件通道治理：Tick 合并（阻塞恢复后积压的多个 Tick 折叠为一次处理）或改有界通道；按键事件不得丢失。
- R5 行为保持：渲染结果与本任务开工时基线（style 任务完成后的输出）逐字节一致（TestBackend 对比：同数据同尺寸下 buffer 相同）；仅重绘时机与内部分配变化。
- R6 证据与测试：空闲 CPU/重绘次数前后对比（如 tick 计数器注入测试或手动 profiling 记录入 research/）；blocks.rs 现有 TestBackend 测试保留并扩展至少 2 个面板的 buffer 等值断言；Tick 折叠单测。

## Acceptance Criteria

- [ ] A1：空闲（无输入/无动画/无后台活动）10 秒窗口内 draw 调用计数为 0（注入计数器的测试或按父任务 X7 协议的手动证据）；有动画/加载时 tick 重绘保留。
- [ ] A2：TestBackend buffer 等值断言通过（渲染输出不变）。
- [ ] A3：主题锁获取次数降至每帧 O(1)（代码审查/测试证据）；`t` 切主题仍即时生效。
- [ ] A4：R6 证据入 research/；`cargo test -- --test-threads=1`、fmt、严格 clippy 全绿。

## Out Of Scope

- 查询/数据层改动（其余子任务）。
- 交互新增（选中/排序等，07-20-tui-interaction-features）。
- tick 周期调整为 100ms 等节奏变化（如需，随动画需求在 interaction 任务定）。

## Notes

- 复杂任务：`task.py start` 前必须补 `design.md`（脏标记触发面清单、主题快照传递方式、memo 失效键）与 `implement.md`。
