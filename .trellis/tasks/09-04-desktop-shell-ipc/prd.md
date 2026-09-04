# Desktop shell and IPC

父任务：`.trellis/tasks/09-04-llmusage-desktop-mvp`。架构与冻结 DTO 见父 `design.md` 与本任务 `design.md`。本子任务先于其它 Desktop 子任务。

## Goal

在 `desktop/` 落地 Tauri 2 进程与 façade command 层，使窗口能 bootstrap 本地库、按 Desktop DTO 查询 snapshot、真实取消进行中的查询、启动/取消 sync，并正确映射锁与 schema 错误。

## Requirements

- 继承父 R1–R6、R9–R12、R8.8 取消/超时、R8.9 载荷校验。允许调用集与父 AC2 相同。
- R1: 继承父任务独立 `desktop/` 窗口进程。
- R2: 继承父任务 path 依赖与非 workspace。
- R3: 继承父任务同一 `AppPaths` 库根。
- R4: 继承父任务 Desktop DTO 与 façade 边界。
- R5: 继承父任务 serve 契约不变。
- R6: 继承父任务不上传 session；额度测试注入本地。
- R8: 继承父任务取消、超时、sync 载荷校验。
- R9: 继承父任务进程内 bootstrap + repair。
- R10: 继承父任务锁与 SchemaTooNew 映射。
- R11: 继承父任务单实例。
- R12: 继承父任务 `runtime_info` 提供 `root_dir` 与锁摘要。
- R15: 继承父任务额度 command 注入面。
- R18: 继承父任务 CI 门禁名不变。
- C1. `desktop/src-tauri` 独立 Cargo 包，path 依赖根 crate。不改根 workspace，不启动 axum。
- C2. `AppState` 持有 `AppPaths`、`Store`、`JobRegistry`、`DesktopQuerySupervisor`。读 command 在 `spawn_blocking` 里 `Dashboard::open_with_busy_timeout(1500ms)`。
- C3. 实现父 design 的 command 表与 DTO。次级查询返回真实 façade JSON。`logs` 固定 `page_size=20`。行为四块 3s 后 degraded JSON。
- C4. 错误码：`not_initialized`、`schema_too_new`、`lock_busy`、`lock_lost`、`job_active`、`invalid_request`、`cancelled`、`timeout` 可测。
- C5. 单实例。测试用 `Fixture` / `AppContext::with_cli_home`，禁止真实 `~/.llmusage`。
- C6. `cancel_queries` + SQLite interrupt + supervisor 收束 JoinHandle。
- C7. `start_sync` 只接受 `SyncStartDto`；构造 `SyncOptions { rebuild: false, ... }`。
- C8. 启动调用 `repair_legacy_token_accounting(&AppContext, &Store)`。`fetch_quota` 生产 `user_home` 用 `resolve_home_dir()`，测试可注入。

## Out of scope

- React 面板实现（core-ui / secondary-ui）
- 额度拉取 UI（ops-quota 可先用 command）
- NSIS（windows-bundle）
- 改 `src/web` / `src/tui`
- 给根查询输入类型加 `Deserialize`

## Acceptance Criteria

- [ ] AC1（R1, R2, R3, R9, R4）：空临时 root 上启动路径会 `AppContext::with_cli_home` → bootstrap → `repair_legacy_token_accounting`，随后 `dashboard_interactive` 返回可反序列化 JSON（纯 Rust 包装即可）。独立 `desktop/src-tauri` path 依赖，不改根 workspace。
- [ ] AC2（R4）：`Fixture` 种子数据的 snapshot 字段与 `Dashboard::interactive_snapshot` 一致。DTO 转换覆盖：合法 IANA、非法 IANA→`invalid_request`、非法日期、`until < since`、未知 source、未知 explorer 枚举、`page_size≠20`。
- [ ] AC3（R10, R12）：另一持锁进程存在时 `start_sync` 得到 `lock_busy`（或 Job 失败快照含锁信息），库不被双写。`runtime_info` 含 `root_dir` 与当前锁摘要。
- [ ] AC4（R10）：`SchemaTooNew` 夹具返回 `schema_too_new`，无 migration 写入。
- [ ] AC5（R5, R18）：根 `src/web`、`src/commands/serve.rs` 监听契约与 `just ci` 作业名未被本子任务改坏。允许的 `src/` 改动仅 `Dashboard::interrupt_handle` 可见性。未碰其它根文件时可跳过满 `just ci`，只跑 Desktop cargo test。
- [ ] AC6（R8）：登记两个 `request_id` 后取消第一个：第一个返回 `cancelled` 或 `timeout`，supervisor `inflight` 收束到 0；第二个仍可完成。
- [ ] AC7（R8）：`SyncStartDto { recent_days: Some(7), source: Some("codex") }` 得到 `rebuild()==false`、`recent_days==7`。`recent_days: Some(0)` → `invalid_request`。即使测试夹具把 rebuild 字段塞进原始 JSON，构造结果仍为 false。
- [ ] AC8（R11）：单实例插件已接入 `tauri.conf.json`；本子任务用配置断言覆盖，焦点行为由父门禁人工确认。
- [ ] AC9（R15, R6）：`fetch_quota` 测试注入 `user_home`+本地 `UsageEndpoints`，不打公网；`cache_hit` 在有效缓存且 `bypass_cache=false` 时为 true。
