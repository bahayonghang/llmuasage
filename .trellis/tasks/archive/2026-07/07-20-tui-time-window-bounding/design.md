# TUI 时间窗与扫描范围设计

## 窗口语义

使用现有本地日历 `QueryFilter`：24h 档表示“今天”，7d/30d 表示含今天的最近 7/30 个本地日，All 不设边界。默认改为 All，保证启动展示与优化前一致。footer/nav/help 明示 `Today/7d/30d/All`。

受管辖：Models、Daily、Hourly、Cost、Stats source mix/context pressure、Behavior activity/tools/optimize/compare。例外：Overview lifetime、365d heatmap、sync center、zombie。窗口变更只失效受管辖缓存并递增 async generation。

## Blocks 等值收敛

展示窗口仍为 active + 3 天。以展示起点向前查找最近一次相邻事件间隔 >= 5h 的断档，从断档后首事件开始扫描；找不到断档则全量扫描。断档确保锚点链重置，因此结果与全量构块等值。记录 scanned row count/回退路径用于性能证据，不改 schema/索引。

## 兼容

所有 payload 不变；web 继续使用原 QueryFilter 契约。All 窗口作为数据回归基线。
