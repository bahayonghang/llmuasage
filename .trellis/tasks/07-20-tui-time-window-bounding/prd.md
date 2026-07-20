# TUI 时间窗生效与查询扫描范围收敛

父任务：`.trellis/tasks/07-20-tui-optimization`。证据：父任务 `research/llmusage-tui-analysis.md` §2/§3/§5（含勘误节）。建议在 07-20-tui-async-panel-loading 落地后进行（复用其失效/世代语义）；经 design 确认改动面不重叠时可与 07-20-tui-style-unify 并行。

## Goal

让 `h`/`l`（TimeWindow 24h/7d/30d/全部）真正作用于面板查询，并把恒全表扫描收敛为按需范围扫描：TUI 常用视图的查询成本随时间窗而非终身历史增长，且父任务「告别恒全表扫描」目标在本任务闭环。

## Confirmed Facts

- `TimeWindow`（src/tui/app.rs:126-174，默认 Week7d，src/tui/app.rs:270）当前是死代码：NextWindow/PrevWindow（src/tui/mod.rs:150-161）只清了从未被读的 `trends` 缓存字段，`h`/`l`/`←`/`→` 零可见效果。
- `QueryFilter.since/until` 是 `Option<NaiveDate>` 本地日历日（src/query/filter.rs:29-42，按 timezone 转 UTC 边界），**无法表达滚动 24h**；现有 `Dashboard::trends("day")` 用的是精确 `now - 24h`（src/query/mod.rs:847-850）——两种边界语义并存，接线时必须显式选择。
- 全表扫描清单：overview/model_breakdown/cost_breakdown/trends_daily/source_breakdown 全表 GROUP BY usage_bucket_30m；context_pressure（query/mod.rs:1000）全表 GROUP BY 原始 usage_event；Behavior 载荷 activity/tools/optimize/model_compare 全表 join usage_event/usage_turn/usage_tool_call。已验证这些查询全部接受 `&QueryFilter` 并经 `SqlFilter` 生成 WHERE（tool_attribution_rows 用 event_filter/tool_filter，query/mod.rs:1305-1324；activity 用 turn_filter；context_pressure 用 event_filter）——纳入时间窗管辖是机械改动。zombie_report 是文件系统扫描，不在此列。
- Blocks 锚点链事实（2026-07-20 评审确认）：`load_blocks_report`（src/query/reports.rs:764-787）的块边界**不是固定时钟窗**——首块锚定于首个扫描事件的 `floor_to_hour`，其后每个事件按「是否 ≥ 前块 end」链式归属；连续使用（相邻事件间隔 < 5h）时锚点链可上溯至全史首事件。因此「从展示窗口前回看 5h 开始扫描」**不保证**与全量扫描逐块一致。关键性质：相邻事件间隔 ≥ session_length（5h）的断档处，后一事件必然 ≥ 前块 end（前块 end ≤ 前事件 + 5h ≤ 后事件），锚点链在断档后重置——这给出一个可证等值的有界扫描起点。
- 展示语义现状：Blocks 面板只展示 active + 近 3 天（reports.rs:800-810），但扫描量为全史。
- 口径注意：Overview 的 lifetime 汇总、Stats 的 365 天 heatmap 属于「定义即全量/固定窗」的口径，不应被时间窗破坏。

## Requirements

- R1 时间窗管辖集合（须闭环父任务目标）：受管辖 = Models/Daily/Cost/Hourly、Stats 的 source mix 与 context_pressure、Behavior 的 activity/tools/optimize/compare。不受管辖 = Overview lifetime 卡、heatmap（固定 365 天）、Blocks（独立收敛见 R3）、zombie（文件系统）、sync command center（非用量查询）。任何调整须在 design.md 给出清单+理由；最终每个重载荷查询要么受窗管辖、要么有明确 lifetime-by-design 理由，不允许「无人认领的全表扫描」。`h`/`l` 变更后：失效受管辖面板缓存（走 async 任务的世代语义）、带边界重查、nav/footer 显示当前窗口标签，help 同步说明管辖范围。
- R2 边界语义与默认值（两项 design 决策，须与父任务 X5 联动）：
  - 边界语义二选一：(a) 窗口=本地日历范围（7d/30d = 含今日的近 N 个本地日；24h 档降级为「今天」或沿用 trends("day") 的精确滚动路径并在 UI 标注差异）——改动面小，QueryFilter 现状即可表达；(b) 扩展 QueryFilter 支持滚动时刻边界——表达力强但波及 web 契约（dashboard-performance-contracts.md §2 QueryFilter 字段契约），须评估。默认推荐 (a)。
  - 默认窗口二选一：(i) 把 TUI 默认 `TimeWindow` 从 Week7d 改为 All——启动态数字与优化前一致，直接满足父任务 X5（推荐）；(ii) 保留 Week7d 默认——则父任务 X5 须限定为 All 窗口比对，且发布说明须标注默认视图口径变化。选定后同步父任务 prd。
- R3 Blocks 收敛（锚点链必须显式处理，三选一，design 阶段定）：
  - (a) 等值保持（推荐）：扫描起点 = 展示窗口起点之前最近一次「相邻事件间隔 ≥ session_length」断档之后的首个事件（可用 `(event_at)` 索引倒序探测；无断档时回退全量扫描并记录日志/计数）。等值性可证：断档处锚点链重置。
  - (b) 语义变更：改为固定时钟对齐窗口（如 UTC 整点对齐的 5h 网格），扫描可任意截断；须更新展示/文档/测试并在发布说明标注。
  - (c) 放弃收敛：Blocks 保持全量扫描，仅依赖 async 任务使其不阻塞 UI；在 design 记录放弃理由。
  - 选 (a)/(b) 时：等值/语义测试须覆盖跨窗口边界块、断档重锚、active 块判定、连续使用无断档回退。
- R4 语义与性能证据：受管辖面板在窗口内数字与「全量查询 + 窗口过滤」等值；按父任务 X7 统一协议（代表性快照、release、3 次中位数）记录收敛前后查询耗时与扫描行数对比，证据入任务 research/。
- R5 契约红线：不改查询输出语义与 payload 形状；不动 schema/索引（若测量表明需要索引，另立任务）；dashboard-performance-contracts.md 的 web 契约零影响（若选 R2(b) 则须先单独走契约修订）。
- R6 测试：时间窗→filter 换算单测（含时区边界）；每个受管辖面板的窗口等值测试；R3 所选方案的 Blocks 测试组；`h`/`l` 状态机测试更新（现有 proptest 保留）。

## Acceptance Criteria

- [ ] A1：`h`/`l` 在受管辖面板产生可见数据变化，当前窗口在 UI 有标识；不受管辖面板口径不变；管辖清单与 design.md 一致且无「无人认领的全表扫描」残留。
- [ ] A2：R3 所选方案的测试组通过；选 (a) 时含断档重锚与无断档回退用例；扫描行数收敛有量化证据。
- [ ] A3：R2 两项决策已定且父任务 X5 已同步；R4 性能对比数据入 research/；R6 测试 + `cargo test -- --test-threads=1`、fmt、严格 clippy 全绿。

## Out Of Scope

- 异步加载基座（07-20-tui-async-panel-loading）。
- 新增日期选择器等交互（07-20-tui-interaction-features 或后续任务）。
- 索引/schema 变更；web 契约修订（若 R2 选 (b)，契约修订单独立项先行）。

## Notes

- 复杂任务：`task.py start` 前必须补 `design.md`（R1 清单、R2 两项决策、R3 方案选择及等值论证）与 `implement.md`。
