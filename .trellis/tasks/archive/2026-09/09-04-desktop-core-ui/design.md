# Desktop core UI — Design

沿用父 `design.md`。本子任务只做 React 壳、核心 interactive 快照、运行状态与 sync 控件映射。IPC 类型以 shell DTO 为准。

## 边界

- `desktop/src/runtime` 唯一 `invoke`。
- 渲染：hero、筛选、overview/trends/models/sources/hosts/projects/costs、sync command center、运行状态。
- `home_overview` 六卡归 secondary-ui。本子任务可用 snapshot 内 overview 数字做 hero，六卡位置显示 loading，不调用 `home_overview`。
- 核心 2s 标慢、6s 失败；`cancel_queries`。
- 不实现 heatmap / explorer / logs / 额度数据。

## Change list

| 文件 | 动作 | 关键符号 |
|---|---|---|
| `desktop/src/runtime/invoke.ts` | 新建 | `invokeCommand`、request_id 分配、`cancel_queries` |
| `desktop/src/app/shell.tsx` | 新建 | 侧栏 10+额度占位、`root_dir`、锁摘要 |
| `desktop/src/app/filters.ts` | 新建 | `rangeToFilterDto`、`RANGE_TO_TREND_WINDOW`、`syncOptionsFromState` |
| `desktop/src/app/load-state.ts` | 新建 | generation、2s/6s、`loadDashboardProgressive` |
| `desktop/src/features/overview/*` 等核心面板 | 新建 | 只消费 interactive JSON |
| `desktop/src/features/status/StatusPanel.tsx` | 新建 | idle/running/failed/lock_busy/lock_lost |
| `desktop/src/styles/tokens.css` | 新建 | 从 `src/web/assets/base.css` 复制语义变量 |
| `desktop/src/styles/layout.css` | 新建 | 248px 侧栏；`@media (max-width: 720px)` 折叠 |

## Contract

```text
rangeToFilterDto(state) -> FilterDto
  1d: since=yesterday, until=today
  7d: since=today-6, until=today
  30d: since=today-29, until=today
  all: since/until omitted
  custom: since/until from inputs (YYYY-MM-DD)
  timezone: Intl.DateTimeFormat().resolvedOptions().timeZone  // 本机 IANA
  window: 1d→day, 7d→week, 30d→month, all→all; custom keeps last or day

syncOptionsFromState(state) -> SyncStartDto
  source: current filter source or omitted
  recent_days: 1d→1, 7d→7, 30d→30; all/custom omitted
  不包含 rebuild

loadDashboardProgressive(state):
  generation++
  cancel_queries(previousIds)
  core = dashboard_interactive({ request_id, filter, window })  // 2s slow / 6s fail
  paint core immediately (includes health/diagnostics)
  do not await home_overview
```

Hosts：`hosts.length <= 1` → 不渲染面板。

运行状态数据：`runtime_info` + 核心 JSON `health`/`diagnostics` + 最近 command `code`。

## 视觉

唯一来源 `src/web/assets/base.css`。不引用 `DESIGN.md` 陶土色或 Catppuccin 映射作为验收。布局尺寸来自 `layout.css`。

## Verification boundary

- 前端单测：`rangeToFilterDto`、`syncOptionsFromState` 五个 range 分支；hosts≤1 不渲染。
- `tauri dev`：AC1–AC8 人工/半自动。
- 不改 `src-tauri` command 形状。

## 已考虑不做

- 核心阶段 `invoke('home_overview')`（父设计次级列表；TPR-08 保持父设计）。
- 把运行状态放到 ops-quota（会推迟 R12 可见性）。
- 用暖色或 Catppuccin 覆盖 token。
