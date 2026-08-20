# C4：远端生命周期语义与文档

父任务：`.trellis/tasks/08-20-ssh-remote-host-import`

## Goal

让间歇可达的远端主机不破坏本地的文件状态机与维护操作，并把远端主机导入的决策与用法写入 ADR、文档和 spec 契约。

## Scope

覆盖父任务 R5 与 R6（全部）。

前置：C2 必须完成并通过 G2。

## Requirements

- R5.1 `missing` 扫描按 host_id 限定，只扫本轮实际联系成功的主机（`parsers/driver.rs:121-128`）。
- R5.2 `lossy_rebuild_risks` 排除 `transport='ssh'` 且本轮未联系成功的主机的 `missing` 行（`commands/sync.rs:768-819`）。
- R5.3 `source-status` 按主机输出只读三态：`idle`、`unreachable`、`never_contacted`。`live` 只出现在 sync 的 `SyncEvent` / `--json-events`。
- R5.4 `llmusage sync` 自动包含已注册远端；单台不可达时跳过、告警、退出码保持成功。
- R5.5 确认 C2 已实现的 `remote sync [--host <label>]` 只走 importer、不跑本地 driver；本子任务把同一 importer 接到 `run_once_locked` 的远端阶段，不重写该子命令。
- `SyncEvent` 新增 `RemoteHostStarted` / `RemoteHostFinished` / `RemoteHostSkipped`，供 `--json-events` 与 dashboard job 展示。
- R6.1 新增 ADR 记录远端主机导入决策，说明与 ADR 0011 的关系：passive parsing 仍是唯一解析机制，变化的是解析发生地；并登记 `docs/adr/index.md`。
- R6.2 更新 `README.md`、`README.zh-CN.md`、`docs/index.md`、`docs/safety/index.md`、`docs/reference/cli.md`、`docs/guide/getting-started.md` 及 `docs/zh/` 对应页。现有文档声明无远端数据通道（`docs/index.md:21`、`docs/safety/index.md:29`），必须同步修正。
- R6.3 更新 `.trellis/spec/llmusage/backend/` 五份契约：token-accounting-contracts、source-sync-contracts、write-fencing-contracts、report-cli-contracts、dashboard-performance-contracts。

## Acceptance Criteria

- [ ] AC7 远端不可达时 `llmusage sync` 完成本地同步、退出码为成功、输出含该主机告警，且该主机的 `source_file` 行不被扫为 `missing`。
- [ ] AC7b 远端不可达时 `sync --rebuild` 不因该主机的 `missing` 行被 `lossy_rebuild_risks` 拒绝。
- [ ] AC7c 远端不可达时自动 token-accounting 修复不被该主机的 `missing` 行阻断。
- [ ] AC7d `source-status` 对 `idle` / `unreachable` / `never_contacted` 各返回正确取值；该命令的输出不含 `live`。
- [ ] AC7e `sync --json-events` 输出包含远端主机的三个新事件，且不可达主机产出 `RemoteHostSkipped`。
- [ ] AC7f `remote sync --host <label>` 只同步指定主机，不触发本地 driver。
- [ ] AC12 `just ci` 通过。
- [ ] 文档：README 两份、`docs/` 与 `docs/zh/` 对应页、新 ADR 与 `docs/adr/index.md` 全部同步；`token-accounting-contracts.md` 记录 event_key 作用域变更。

## Out of Scope

- 并发同步多台远端。
- 远端二进制自动安装。
- TUI 的主机状态面板。
