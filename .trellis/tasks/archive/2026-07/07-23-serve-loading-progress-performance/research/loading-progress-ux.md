# 加载动效与进度语义

## Problem Restatement

用户需要知道本地看板是否仍在工作、已经完成多少、哪里变慢，以及失败后如何恢复。动画不是目的；它只应证明请求仍未决和状态确实发生了变化。

## Chosen Direction

加载反馈分成独立但连续的两层：HTML shell 自己负责“应用是否启动”，module 接管后由现有同步命令中心 instrument card 负责“数据加载到哪一步”。这样入口模块本身失败时也不会永远保留静态文案。

shell 层：

1. HTML parse 后启动 3 秒 bootstrap watchdog；`app.js` 只有在完整 module graph 成功解析并执行时才 claim，并在 core-pending/error handling 建立后 ready。claim 后/ready 前异常仍由 shell 收敛。
2. 未 claim 时对轻量根路由做 1.5 秒有界探测。根路由不可达显示“本地服务已停止”，可达则显示“页面资源未能启动”；snapshot 显示离线资源错误。
3. shell error 使用静态 ZH/EN 文案和 Reload，不依赖 `copy.js`，也不显示动画假装仍在工作。

module/data 层在同步命令中心内增加一条稳定尺寸的 load rail：

1. `core_pending`：一段窄条沿轨道平移，文案“正在读取本地汇总”。无百分比。
2. `slow`：轨道继续，tone 切为 warn，文案“本地查询耗时较长”，显示经过时间但不推算剩余时间。
3. `secondary_loading`：轨道变为 5 个固定 segment，按 activity/tools/optimize/explorer/compare 的 settle 事实更新 `n/5`。
4. `complete`：停止动画，真实同步命令中心完整可用；加载状态以短暂 opacity transition 结束。
5. `error/timeout`：停止动画，显示具体错误与 Retry；导航、筛选和 local-only 标识仍可用。

## Why This Is Honest

- Core SQLite query没有流式工作量，indeterminate 是唯一诚实表达。
- Secondary loader 数量固定且每个独立 settle，`n/5` 是可验证进度。
- Degraded 是完成态，不让进度卡在 4/5；对应 section 继续解释超时原因。
- 不根据已用时间制造 20%、60%、90% 等伪进度。
- 当前 sync job snapshot 不保留 overall denominator，因此不把文件扫描计数冒充整体同步百分比。
- “服务已停止”只来自有界根路由探测或 core fetch network failure；查询慢只进入 slow/timeout，不混用服务退出文案。

## Motion Contract

- Core scan: 1.2-1.6s linear transform loop, only while unresolved.
- Segment fill/state: 160-200ms opacity/transform transition.
- No animated width/height/position offsets; no bounce, glow, blur or shadow pulse.
- `prefers-reduced-motion` makes the scan segment static and transitions near-instant.
- Animation stops when tab state reaches complete/error and does not run below the fold after completion.

## Layout And Accessibility

- Rail and status row have reserved dimensions; longest ZH/EN status wraps inside the card instead of changing outer width.
- `role=status` plus `aria-live=polite`; do not announce every animation frame or elapsed-second tick. Announce phase changes and final error only.
- Warning/error use icon/text plus semantic color. Segment `aria-label` names the section and state.
- Retry is a normal button with the existing refresh icon, focus ring and 44px mobile target.
- The sync action button remains distinct from Retry; retry only reloads dashboard data and never starts sync.
- Shell watchdog 的 Reload 在 module 不可用时由 inline handler 工作；module 接管后的 Retry 走 generation/AbortController，不要求整页重载。

## Deferred Alternative

A true overall sync job percentage would require a backend `JobProgressSnapshot` that retains source/file denominators across events. That is a separate API design, not a CSS follow-up, and is intentionally excluded from this task.
