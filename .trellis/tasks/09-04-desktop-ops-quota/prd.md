# Desktop ops and quota

父任务：`.trellis/tasks/09-04-llmusage-desktop-mvp`。依赖 `09-04-desktop-core-ui`。可与 `desktop-secondary-ui` 并行。IPC 只引用 shell DTO。

## Goal

补齐事件日志、CSV 保存、偏好持久化、自动刷新、订阅额度。

## Requirements

- 继承父任务 logs/CSV/刷新/额度范围。
- R6: 继承父任务额度测试不打公网、不上传 session。
- R8: 继承父任务 logs 分页与自动刷新。
- R13: 继承父任务 CSV 六块、BOM、公式中和、系统对话框。
- R14: 继承父任务 `desktop.json` 持久化。
- R15: 继承父任务额度缓存、刷新、diagnostics、隐藏邮箱。
- O1. logs 每页 20 行游标（`LogsDto.page_size=20`）；展开 raw；接收 session 过滤。
- O2. CSV 系统保存对话框；公式中和与 `csv-export.js` 一致。
- O3. `{root}/desktop.json` 存主题、语言、刷新间隔、筛选。
- O4. 额度导航：`fetch_quota`；缓存/刷新/隐藏邮箱；无凭证空态；失败不挡看板。展示 `cache_hit` 与 `diagnostics`。
- O5. 额度测试注入 `UsageEndpoints` + `user_home`，不打公网。

## Out of scope

- 新额度提供者、改凭证、托盘、forget 按钮、NSIS
- 运行状态主面板（core-ui 已承担）

## Acceptance Criteria

- [ ] AC1（R8）：从 session ranking 跳进 logs 时 `LogsDto.session` 等于该 session。每页 20 行；下一页 `cursor` 使首行 event_key 与上一页末行不同。
- [ ] AC2（R8）：未展开时 `include_raw_json` 为 false；展开一行时对该 `event_key` 请求 raw。
- [ ] AC3（R13）：系统保存对话框选定路径后，文件以 UTF-8 BOM（`EF BB BF`）开头，含 summary/daily/projects/models/sources/sessions 六块；单元格 `= + - @` 前缀被 `'` 中和。
- [ ] AC4（R14, R8）：重启后主题/语言/刷新/筛选恢复。自动刷新切到 30s 或 60s 后，下一次刷新按新间隔触发（不必重启）。
- [ ] AC5（R15）：无凭证：额度空态。有夹具凭证：outputs 可见。进入且缓存有效：`cache_hit=true`。刷新：`bypass_cache=true` 且 `cache_hit=false`。邮箱默认 `[hidden email]`。diagnostics 与 outputs 同时可展示。
- [ ] AC6（R15, R6）：额度测试不接触公网；凭证文件字节前后一致。额度 command 失败时 R8 核心/次级仍可操作。
