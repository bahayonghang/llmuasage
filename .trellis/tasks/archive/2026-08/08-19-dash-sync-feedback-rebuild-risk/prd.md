# 看板立即同步反馈与重建风险闭环

## Goal

让 `llmusage serve` 看板点击「立即同步」后的呈现与实际发生的事情一致：同步详情跟随本次运行更新，主标题反映本次同步结果，不再把「重建风险」当成日常同步失败或需要立即处理的告警。Web 与 TUI 共享的命令中心采用同一套中性语义，重建保护事实仍可追溯。

## Background（现场证据）

截图现场（2026-08-19）：

- 横幅主标题：`检测到重建风险`，tone=warn，生成时间 `2026-08-19T12:39:53Z`。
- 同步详情：最近命令 `sync`、最近状态 `success`、完成时间 `2026-08-19T08:58:21Z`（比生成时间早 3.7 小时）。
- 指标卡：扫描 127 / 新增 84 / 已存 249,020，与左下角状态行一致，说明同步确实执行了。

本地核对：

- `llmusage logs --limit 40`：最新一条 `sync` 记录是 `#178 [success] sync 2026-08-19T08:58:20Z -> 08:58:21Z stored_events=248936`；12:39 的看板同步没有任何 `run_log` 行。
- `llmusage diagnostics`：所有来源 `recent_completed_at` 均为 `2026-08-19T12:39:53Z`，`claude` 的 `missing_file_count=728`、`protected_event_count=36495`、`lossy_rebuild_risk=true`。

即：来源同步状态更新了，运行记录没更新，重建风险是长期存在的既有状态。

## 三个独立缺陷

1. **看板与 TUI 触发的同步不写 `run_log`。**
   `JobRegistry::run_job`（`src/sync/job_registry.rs:404`）直接调用 `executor.run_once`，跳过了 CLI 侧的 `run_tracked`（`src/commands/mod.rs:222`）。只有 `commands::sync` 的 CLI 路径会写 `run_log`。因此看板同步不进入 `llmusage logs`、`doctor`、`diagnostics`、TUI 同步面板以及命令中心的 `last_run`。

2. **重建风险占据主标题，但日常同步不能改变该保护事实。**
   `sync_command_center_with_diagnostics`（`src/query/mod.rs:3463-3485`）把 `lossy_rebuild_risk` 排进 tone 与 headline 选择链。该风险来自 `source_file` 中路径已不在磁盘上的行（`src/store/source_file.rs:198`）；现有诊断能证明文件缺失，不能证明具体删除者。普通同步不会清除该状态，显式逐文件登记可通过 CLI `diagnostics --forget-file` 或本地 HTTP `POST /api/diagnostics/forget` 完成，但看板当前没有对应控件。看板 `syncOptionsFromState` 恒定发送 `rebuild:false`，`run_job` 恒定设置 `allow_lossy_rebuild:false`，所以「立即同步」不是处理重建保护的入口。

3. **同步成功后没有可见确认。**
   `centerWithJobOverlay`（`src/web/assets/render/sync-command-center.js:43`）覆盖 running / cancelling / failed / cancelled，没有 completed 分支；`app.js` 在 `finally` 中清空 `activeJobSnapshot` 后回落到服务端 payload，而该 payload 因缺陷 1 仍是旧的运行记录、因缺陷 2 仍是 warn 主标题。用户看到的横幅在点击前后完全一致。

## 参考实现对照（ref/repo/tokscale）

tokscale 把磁盘视为唯一事实源，本地存储只是派生缓存：文件消失时缓存条目直接删除（`crates/tokscale-core/src/message_cache.rs:1643` "A source that no longer exists can never be scanned again"，以及 `prune_missing_files`），不产生任何风险状态或告警。

llmusage 的 SQLite 是权威归档，会保留源文件消失后的事件（claude 728 个文件 / 36,495 条事件），所以 `sync --rebuild` 是有损的。**重建风险是 `sync --rebuild` 这条维护命令的前置守卫，不是日常同步面板的状态。** 本任务据此把它从看板主信号中移除。

## Requirements

- R1：`JobRegistry` 在取得 worker lock、完成 bootstrap 并创建运行记录后，必须用 `run_log` 记录看板与 TUI 触发的同步；命令名沿用 `sync` / `sync --rebuild`，执行期终态覆盖 `success`、`failed`、`cancelled`。
- R2：CLI 与 Job 路径使用同一个 usage-import 恢复入口，恢复命令集合必须完整包含 `sync`、`sync --rebuild`、`hook-run`，避免任一导入命令永久停留在 `running`。
- R3：取消边界分为两类：取得锁并创建运行记录之前的取消只结束 JobSnapshot、不制造 `run_log`；创建记录之后的用户取消写入 `cancelled`。两类取消都不得被 `doctor` / `health` 计为失败。
- R4：共享的 `sync_command_center` tone / headline_key / reason_key 不再消费 `lossy_rebuild_risk`，优先级统一为：锁忙 → 最新 usage-import 失败 → 无数据 → 就绪。
- R5：`safety.lossy_rebuild_risk`、`safety.risk_sources`、来源行的 `lossy_rebuild_risk` 字段继续保留；CLI 重建守卫与 `diagnostics` 行为不变。Web 与 TUI 的命令中心标题、来源状态采用中性语义，但仍可显示重建保护事实。
- R6：看板不再把重建风险渲染为告警：`safetyLine` 不输出风险前缀，来源卡片和占比条不因该布尔值变成 warn 或 `重建风险`；同步详情以中性事实呈现来源、缺失文件数和本地库保留事件数。
- R7：`运行状态` 页的 `lossy_rebuild` 洞察由 warn 降为 neutral，措辞只陈述事实与重建守卫条件，不提供日常同步处置引导。
- R8：同步任务进入 completed 后，横幅立即呈现本次运行结果（tone good、就绪主标题），同步详情显式显示本次 `finished_at`；服务端 payload 重载和清空活动快照的整个过渡中不得闪回旧 warn 状态或旧完成时间。
- R9：`record_run_start` 或 `finish_run` 失败不得把 JobSnapshot 标成 completed。运行日志收尾失败必须通过失败事件与可见错误交付；持锁期间完成所有可完成的记账，再释放 heartbeat / lock。
- R10：更新 `.trellis/spec/llmusage/backend/source-sync-contracts.md`、`dashboard-performance-contracts.md` 及受影响的 TUI/Web 契约与中英文文档，记录命令集合、取消边界、共享中性语义和完成态过渡。

## Non-Goals

- 不新增批量或逐文件 forget / 清理缺失记录的接口或看板控件；现有本地 HTTP forget API 保持原样。
- 不改变 `sync --rebuild` 的有损守卫与 `--allow-lossy-rebuild` 语义。
- 不改变 llmusage 的权威归档定位（不改为 tokscale 式派生缓存）。
- 不改动同步作业的并发、worker lock 与实际取消/排空语义；只补充运行审计与展示契约。

## Acceptance Criteria

- [ ] AC1：`serve` 下点击「立即同步」完成后，`llmusage logs` 出现一条新的 `sync` 成功记录，`summary` 含 `sources= seen= inserted_delta= stored_events=`。
- [ ] AC2：同一次操作后，看板「同步详情」的最近状态与完成时间等于本次运行，不再停留在上一次 CLI 同步。
- [ ] AC3：存在重建风险且最近一次 usage-import 成功时，共享命令中心 tone 为 good、主标题为就绪文案；`safety.lossy_rebuild_risk` 仍为 `true`。
- [ ] AC4：最近一次 usage-import 失败时，横幅仍显示失败主标题与 `lastRunFailed` 原因；锁忙时仍显示忙碌主标题。
- [ ] AC5：Web 来源卡片与占比条对仅有重建风险的来源显示正常状态且不使用 warn；TUI 标题与来源状态同样不把该事实当成同步失败，但显式 rebuild-risk 事实仍可见。
- [ ] AC6：在执行期取消一次看板同步后，`run_log` 记录状态为 `cancelled`，`llmusage doctor` 与 health 不将其计入最近失败。
- [ ] AC7：在锁等待期取消时，任务在既有 SLA 内进入 cancelled，不新增 `run_log`，之后不会取得锁或继续写入，doctor/health 不新增失败。
- [ ] AC8：预置一条 `sync --rebuild` 的 stale running 记录后，下一次 CLI 或 Job usage-import 会把它恢复为 `aborted`；三个 usage-import 命令的恢复集合只有一个权威定义。
- [ ] AC9：注入运行日志收尾失败时，JobSnapshot 不得为 completed，前端收到失败事件/错误；正常成功、失败、取消路径仍在释放锁前完成 `finish_run`。
- [ ] AC10：completed overlay → 重载后的服务端 payload → 清空活动快照三阶段均保持 good/ready，详情始终显示本次完成时间，不出现旧状态闪回。
- [ ] AC11：`sync --rebuild` 在存在缺失文件时仍拒绝执行并提示 `--allow-lossy-rebuild`；`just ci` 全绿，包含 Rust、TUI 与 JS 回归测试。

## 已决策边界

- Web 与 TUI 共享命令中心语义，不为保持旧 TUI 告警而新增第二套查询投影。
- `cancelled` 是执行期用户取消的持久化状态，且不计为健康失败；锁前取消没有运行记录。
- `run_log` 是完成反馈的必需组成，不是可以静默失败的旁路日志。
