# C3：读取层 host 维度

父任务：`.trellis/tasks/08-20-ssh-remote-host-import`

## Goal

让 CLI 报表与 dashboard 能按主机过滤和分组，主机作为与 source 平行的独立维度。

## Scope

覆盖父任务 R4（全部）。不含 SSH 传输、`remote` 子命令、远端生命周期语义。

前置：C1 必须完成并通过 G1。不依赖 C2：C1 完成后本地事件已带 `host_id`，读取层可以先只对 `local` 生效并独立验证。

## Requirements

- R4.1 `QueryFilter` 与 CLI `ReportFilter` 都增加 `host_id: Option<String>`。dashboard / explorer 走 `QueryFilter`；报表命令走 `ReportFilter`。host 条件加在 `sql_filter_with_model_column`（`query/filter.rs:98-104`），覆盖 `usage_event`、`usage_bucket_30m`、`usage_turn`、`usage_tool_call`。
- R4.2 daily / weekly / monthly / session / blocks 与 focused 命令（`claude` / `codex` / `opencode` / `antigravity`）支持 `--host <LABEL>` 过滤，参数位置与既有 `--source` 平行（`commands/report_args.rs:62-64`）。解析在 `ReportCommonArgs::to_filter`（`commands/report_args.rs:67-81`），写入 `ReportFilter.host_id`；`push_bucket_filter` 与 `visit_filtered_events` 追加 host 条件。
- R4.3 提供可选每主机行，形状与既有 per-source 行同构（`query/reports.rs:597`），并进入 CLI JSON 报表。该组函数不替代 `--host` 过滤。
- R4.4 dashboard 增加独立主机分组 payload 字段与前端分组展示，不改造现有 source 分组。
- R4.5 `source-status` 与 `diagnostics` 输出按 host 区分。
- `--host` 接受 `label`，内部解析为 `host_id`；未注册的 label 报错并列出候选。解析函数名是 `to_filter`，不是 `into_filter`。

## Acceptance Criteria

- [ ] AC8 `--host` 过滤只返回该主机事件；不带 `--host` 时总量等于各主机之和。
- [ ] AC9 dashboard 主机分组的合计等于同条件下该主机 CLI 报表合计。
- [ ] AC8b `--host` 传入未注册 label 时报错，错误信息列出已注册 label。
- [ ] AC8c 每主机行与每 source 行在同一份 JSON 报表中共存且互不影响既有字段。
- [ ] AC8d behavior 与 Activity 视图（`usage_turn` / `usage_tool_call` 支撑的查询）在 `--host` 下返回一致结果。
- [ ] AC9b dashboard payload 新增字段不违反 `dashboard-performance-contracts.md` 的查询与负载预算。
- [ ] `cargo test --all-features -- --test-threads=1` 通过；`node --check` 与 `node --test` 通过（dashboard JS）。

## Out of Scope

- host 与 source 的两级组合视图（父任务已明确排除）。
- TUI（`dash`）的主机维度。若 TUI 因共享查询层出现编译或展示问题则修到不回归为止，不新增主机 UI。
- 远端主机状态展示（`unreachable` 等属 C4）。
