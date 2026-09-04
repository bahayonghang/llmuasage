# Desktop secondary UI — Design

沿用父 `design.md` 前端数据流第 3 步。`home_overview` 固定由本子任务在核心绘制后请求。

## 边界

- 次级并发 2。
- activity/tools/optimize/compare：使用 IPC 已施加的 3s 超时 degraded JSON。
- explorer 不套 3s 行为超时。
- heatmap 点击钻取；session 行只发跳转意图，logs 页由 ops-quota 承接。

## Change list

| 文件 | 动作 | 关键符号 |
|---|---|---|
| `desktop/src/app/secondary.ts` | 新建 | `SECONDARY_SECTIONS`、`runLoadersWithConcurrency(2)` |
| `desktop/src/features/overview/SummaryCards.tsx` | 新建 | 消费 `home_overview.summary` 六卡 |
| `desktop/src/features/heatmap/*` | 新建 | 日期钻取 toggling |
| `desktop/src/features/sessions/*` | 新建 | sort tokens/duration/cost；跳转意图 |
| `desktop/src/features/behavior/*` | 新建 | support 状态文案 |
| `desktop/src/features/explorer/*` | 新建 | `ExplorerDto` 全字段；unsupported 禁用 |

## Contract

```text
SECONDARY_SECTIONS = [
  home_overview, heatmap, trends_daily, top_sessions, hour_of_week,
  activity, tools, optimize, explorer, compare
]  // 与 src/web/assets/load-state.js 同一组 10 项

after core paint:
  runLoadersWithConcurrency(sections, 2)
  each invoke uses SecondaryRequest / ExplorerDto / TopSessionsDto + request_id
  generation mismatch → drop
  one section error → that section degraded; others continue

heatmap click date D:
  if current drill == D: restore previous rangePreset/filter
  else: save previous; rangePreset=custom; since=until=D; reload

include_non_tool false → ExplorerDto.include_non_tool=false
  shell maps to ExplorerFilters.is_tool = Some(true)
```

## Verification boundary

- 单测：并发 2、单块失败隔离、heatmap toggle、explorer 只重拉自己。
- `tauri dev`：AC1–AC6。
- 不修改 shell command 形状。

## 已考虑不做

- 核心阶段请求 `home_overview`（TPR-08：保持父设计，所有者为本子任务）。
- 把 logs 页做进本子任务。
