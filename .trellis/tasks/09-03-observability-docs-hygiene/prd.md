# 命令日志与文档卫生

## Goal

默认报表命令进入 `run_tracked`；文档与真实远程行为一致；清掉已确认的死代码和 ADR 漂移。

## Background

- OBS-001 `run_tracked` 只包 init/sync/serve/export/uninstall。默认命令是 `daily`（`commands/mod.rs`）。catalog/logs/update/source-status 无 `error!`。
- OBS-002 `docs/architecture/index.md` 与 `docs/safety/index.md` 写“无远程 usage API”。TUI Usage 会用本地 token 拉供应商配额（`tui-subscription-contracts.md`）。这是文档错误，不是要删订阅功能。
- ARCH-003 `codex_tracer` 是独立产品岛：本任务只在架构页标明 sidecar，不合并。
- ARCH-004 ADR 0003 仍写 `TriggerStore`；代码是 HostStore + SourceFileStore。
- READ-001 `commands/tui.rs` 无调用方；`PricingCatalog::static_v1` 无调用方；`delete_for_source_in_tx` 无调用方。
- Cargo.toml `description` 仍写 hooks；当前发行不安装 hook。

## Requirements

- R1. daily/weekly/monthly/session/blocks/focused/catalog/logs/update/source-status/doctor/status/diagnostics/dash 失败时写 `run_log` 或至少 `tracing::error!`。优先复用 `run_tracked`，保持日志体积上限契约。
- R2. architecture + safety（中英）区分：无用量上传 / 无账号；TUI Usage 可只读拉取供应商配额。
- R3. architecture 页写明 `codex-tracer` 使用独立 `codex-tracer.db`，不经 `SyncShard`。
- R4. ADR 0003 与现状对齐（删除 TriggerStore 写 API；加上 HostStore/SourceFileStore）。
- R5. 删除或 `#[cfg(test)]` 化确认无调用的 `commands/tui.rs` 实现、`static_v1`、`delete_for_source_in_tx`（若 reset 子任务需要该函数，改为正式调用而不是 allow(dead_code)）。
- R6. Cargo.toml description 去掉“hooks”或改成“legacy hook cleanup”。

## Acceptance Criteria

- [ ] AC1. `llmusage daily` 失败会留下 `run_log` 行或 error 级 NDJSON（测试用注入失败）。
- [ ] AC2. 安全/架构文档出现配额拉取说明，且不再写绝对的“无远程 usage API”而不加限定。
- [ ] AC3. ADR 0003 不再把 `triggers()` 写成当前 API。
- [ ] AC4. `commands::tui::run` 若保留，必须被 dispatch 调用；否则模块删除或变为 `dash` 的别名文件并在文档说明。
- [ ] AC5. `cargo test` 相关切片通过；docs build 若改了 VitePress 页。

## Out of scope

- 查询 SQL 改造、分层重构、安全 header。
- 新建根目录 `CONTEXT.md`。
