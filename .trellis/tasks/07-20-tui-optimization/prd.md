# TUI 界面样式/功能/性能综合优化（对标 tokscale）— 父任务

由用户请求「结合 ref/repo/tokscale 分析并优化 llmusage TUI 的界面样式、功能和性能（含异步/并行）」立项，对应 TODO.md 中「优化TUI界面的性能，采用异步的方式」。

证据底稿（实现前必读）：

- `research/llmusage-tui-analysis.md` — llmusage TUI 现状全量事实（架构/数据流/功能/样式/性能问题/死代码/可复用基建/测试）。
- `research/tokscale-patterns.md` — tokscale 参考实现与 18 条可移植模式清单，及与 llmusage 的适配判断。

## Goal

在不改变数据语义（token 口径、成本口径、同步契约）的前提下，让 `llmusage dash`：

1. 永不因数据加载/同步而冻结 UI（异步/并行化）；
2. 消除按键触发的崩溃风险（sync 嵌套 runtime）；
3. 查询范围可控（时间窗生效，告别恒全表扫描）；
4. 渲染开销与空闲功耗下降；
5. 样式统一（主题槽位全覆盖、文案语言一致、格式化去重）；
6. 交互对齐 tokscale 基线（行选中、排序、滚轮、加载指示）。

## Confirmed Facts（详见 research/，此处仅列立项依据）

- P0：TUI 按 `x` 触发 sync 在 tokio worker 线程上 `block_on` 新建 runtime（src/tui/mod.rs:179-207；入口链 main.rs `#[tokio::main]` → dash.rs:31 async → tui 同步循环），tokio 嵌套防护会 panic；即使不崩，全量导入期间 UI 冻结数十秒（冷跑基线约 30s，见 07-20-sync-full-profiling）。无测试覆盖该路径。
- P1：所有面板查询同步跑在渲染线程（src/tui/mod.rs:248-323），Stats=5 查询、Behavior=5 载荷（含文件系统扫描）、Blocks=全量 usage_event 流式扫描；"Loading…" 占位分支实际不可达。（勘误：research §5 P3 把 source_breakdown 的 per-source last_event_at 归为 N+1 缺陷系误判——那是 dashboard-performance-contracts.md「Source totals」条款要求的索引寻优形态，不得"修复"。）
- P1：`QueryFilter` 恒为默认值（无 since/until），除 hourly 外全部面板终身全表 GROUP BY；`TimeWindow`/`h`/`l` 按键是零效果死代码。
- P2：空闲时每 250ms 无条件全量重绘 + 每帧重建全部行字符串；theme 每次取色都拿 RwLock 读锁（每帧数百次）。
- 样式：多面板硬编码 `Color::*` 绕过主题槽位（切主题只变部分颜色）；中英文文案混杂；`format_number` 在约 11 个文件重复；footer 写 "1-8" 实为 9 面板；5 个面板的 row_highlight_style 因无 TableState 从不生效；spinner_frame、4 个面板模块、4 个缓存字段均为死代码。
- 可复用基建已存在：web/mod.rs:771-842 spawn_blocking + Semaphore + InterruptHandle（契约见 dashboard-performance-contracts.md）；sync/job_registry.rs 完整后台任务系统（tokio::spawn + 进度 mpsc + CancellationToken + 单任务准入）。
- tokscale 关键结论：其 TUI 也是同步事件循环，靠「后台线程加载 + try_recv 排水 + cache-first + loading 指示」保持不阻塞；rayon/simd-json 在解析核心层。llmusage 的对标点是把查询挪下渲染线程并复用自家 tokio 基建，而非引入新缓存层。

## 任务地图（children）

| 子任务 | 优先级 | 主题 | 建议顺序 |
| --- | --- | --- | --- |
| `07-20-tui-sync-runtime-fix` | P0 | sync 崩溃修复 + 进程内 JobRegistry 后台化 + 进度展示 | 1（独立可先行） |
| `07-20-tui-async-panel-loading` | P1 | 面板查询异步基座（spawn_blocking/世代/loading 态/并行子查询） | 2（核心基座） |
| `07-20-tui-time-window-bounding` | P2 | TimeWindow 生效 + 扫描范围收敛（含 Behavior 载荷与 context_pressure） | 3（依赖 2；经 design 确认改动面不重叠时可与 4 并行） |
| `07-20-tui-style-unify` | P2 | 主题槽位全覆盖 + 文案统一 + 共享格式化 + 无色/受限色降级 | 4（须在 5 之前，先定 golden 基线） |
| `07-20-tui-render-efficiency` | P2 | 按需重绘 + 主题快照 + 行窗口化/世代 memo | 5（依赖 2 的数据世代与 4 的基线；与 4 串行——同触 theme.rs 与全部面板） |
| `07-20-tui-interaction-features` | P3 | 行选中/排序/滚轮/加载 spinner/死面板处置 | 6（依赖 2、4、5——复用窗口化状态与 selection 槽位） |

执行顺序为强建议（style 与 render 因共同修改 theme.rs/全部面板/golden 基线必须串行）；父子结构不是依赖系统，顺序约束同时写入各子任务 prd，子任务各自独立验收。六个子任务均按复杂任务对待：`task.py start` 前必须补齐 design.md 与 implement.md（workflow 启动门槛）。

## 跨子任务验收（父任务收口时检查）

- [x] X1：`llmusage dash` 中按 `x` 触发全量 sync：不 panic、UI 持续响应按键与重绘、有进度指示、可取消或至少可退出。
- [x] X2：首次进入 Behavior/Stats/Blocks 面板：先渲染 loading 态帧，数据就绪后填充；期间切换面板不产生过期结果覆盖（世代语义，对齐 dashboard-performance-contracts.md 的 range-click 语义）。
- [x] X3：`h`/`l` 切换时间窗对趋势/聚合面板有可见效果，且各面板查询带时间边界（除明确定义为 lifetime 的口径外）。
- [x] X4：切换主题后所有面板颜色一致跟随（无硬编码残留）；`NO_COLOR` 下可读。
- [x] X5：数据语义回归：默认启动态与 All 窗口下，TUI 展示的 token/成本数字与优化前逐面板一致（同一数据库快照）；非 All 窗口下与「全量查询 + 窗口过滤」等值；文案类有意变更以 style 任务的变更清单为准。不违反 token-accounting-contracts.md 与 source-sync-contracts.md。（前提：time-window 任务 R2 的默认窗口决策须保证启动态等价 All，否则回改本条。）
- [x] X6：`just ci` 全绿（fmt / clippy -D warnings / cargo test -- --test-threads=1 / node 检查 / docs build）。
- [ ] X7：性能证据（统一测量协议：代表性数据库快照 + release 构建 + 3 次取中位数，对齐 07-20-sync-full-profiling 协议）：(a) 首访 Stats/Behavior/Blocks 期间渲染线程最长连续阻塞时长前后对比；(b) Stats/Behavior 载荷 wall-time 达到 async 任务 A3 阈值；(c) 空闲无动画 10s 内 draw 调用计数达到 render 任务 A1 标准。基线与结果记录入父任务 research/perf-baseline.md。当前 (b)/(c) 已有 release/确定性证据；(a) 仍缺少三次 render-thread blocking 测量，不能将 X7 整体标记为通过。

## Out Of Scope

- 解析器/同步写入吞吐优化（已有 `07-20-sync-cold-import-write-throughput` 等任务线）。
- Web dashboard（axum/前端）改动；仅允许无行为变化的代码复用抽取。
- 新数据口径、新 SQL 语义、schema/migration 变更。
- tokscale 式磁盘快照缓存 / bincode 分片缓存（SQLite 已承担该层，见 research/tokscale-patterns.md §7）。
- 排行榜/上传类功能（tokscale submit 等）。
