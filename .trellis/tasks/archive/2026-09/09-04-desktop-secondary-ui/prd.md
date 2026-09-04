# Desktop secondary UI

父任务：`.trellis/tasks/09-04-llmusage-desktop-mvp`。依赖 `09-04-desktop-core-ui`。IPC 只引用 shell DTO。

## Goal

补齐 live 次级面板（含 `home_overview` 六卡），并保持并发 2 与逐块降级。

## Requirements

- 继承父任务次级面板范围。
- R8: 继承父任务次级列表、`home_overview` 时序、explorer/行为状态。
- S1. heatmap、hour-of-week、top-sessions、trends-daily、**`home_overview` 六卡**（core 不调用该 command）。
- S2. activity、tools、optimize、compare。
- S3. explorer 全套控件。
- S4. 次级并发 2；degraded/no_data/unsupported 不装成 0。activity/tools/optimize/compare 使用 IPC 返回的 3s 超时 degraded 形状。
- S5. 点击 heatmap 日期钻取；再点还原。点击 session 行发出跳转 logs 的意图（logs 页由 ops-quota 承接）。
- S6. 次级 invoke 带 `request_id`；筛选变更时由 core 的 generation/`cancel_queries` 收束，本层丢弃旧 generation。

## Out of scope

- logs 实现、CSV、额度、打包
- 把 `home_overview` 并入核心请求

## Acceptance Criteria

- [ ] AC1（R8）：核心已画完后次级可逐块到位，单块失败不影响其它块。
- [ ] AC2（R8）：`home_overview` 在核心绘制之后作为次级列表一项请求。失败或超时：六卡 degraded/空态，核心块保持。
- [ ] AC3（R8）：explorer 改变 metric/group_by/granularity/limit/session_id/tool_name/tool_kind/token_type/include_other/include_non_tool 后只重拉 explorer。`unsupported` 时控件禁用并显示原因。
- [ ] AC4（R8）：compare 在不足两模型时展示 `insufficient_models`，不画 0。
- [ ] AC5（R8）：点击 heatmap 某一日期：筛选变为该日 custom（since=until=该日）并重拉；再点同一格还原到点击前的 range。
- [ ] AC6（R8）：点击 session 行发出 logs 跳转意图，payload 含该 `session` 过滤键。
