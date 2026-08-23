# 同步应用核心与 CLI 适配器边界收敛

## Goal

让 `sync` 应用层成为同步执行、重建/自动修复、远端导入和 `JobRegistry` 默认 executor 的唯一所有者，使 CLI/Web/TUI 只负责 transport、取消信号和呈现，同时保持所有公开入口与同步语义兼容。

## User Value

同步可作为库能力可靠嵌入 Web/TUI/下游应用；修改 CLI 文案或进度呈现不再触及数据重建与远端生命周期，修改同步策略也不要求依赖命令模块。

## Confirmed Facts

- `src/lib.rs:87-93` 将 `JobRegistry` 作为 root facade 导出。
- `src/commands/sync.rs:535-855` 拥有 `CommandSyncExecutor`、`JobRegistry::default` 和三阶段同步主流程。
- `src/web/mod.rs:294`、`src/tui/sync_control.rs:32` 与 runtime tests 直接构造 `commands::sync::CommandSyncExecutor`。
- `src/sync/executor.rs` 已定义依赖反转 trait；`src/sync/job_registry.rs:83-101` 明确期望 adapter 注入 executor。
- `tests/architecture/main.rs:308-337` 只禁止 sync/remote 依赖 commands，无法禁止 commands 拥有 sync 类型的 impl；当前 3/3 绿色仍允许上述所有权反转。

## Requirements

- R1：`sync` 层拥有默认 concrete executor 和三阶段同步 engine，包括 typed validation 之后的 parser selection、rebuild/repair、remote import/sweep、status persistence 与 summary。
- R2：`commands::sync` 只拥有 CLI bootstrap/lock timing、human/NDJSON reporter、Ctrl-C、summary formatting 和 public compatibility wrapper。
- R3：Web/TUI/public `JobRegistry::default()` 不再引用 `commands`; 依赖方向必须为 adapters → sync → parsers/store/remote。
- R4：保留 `commands::sync::{run_once,run_once_with_options,run_once_with_cancel,run_store_once_with_options,CommandSyncExecutor}` 的可编译路径；兼容项只能 delegate/re-export。
- R5：不得改变 source ordering、single writer、SyncShard commit、write fencing、recent-window、OMP split、remote host、automatic accounting repair、lossy rebuild guard 或 cancellation 语义。
- R6：human stdout/stderr、NDJSON event 顺序/shape、run_log command/status、Dashboard/TUI job lifecycle 与 public validation error code 必须等价。
- R7：架构测试必须表达允许层图，并能用负 fixture 捕获 sync/remote → commands、非 adapter 对 commands concrete executor 的依赖、commands 对 sync type 提供核心 impl 三类回归。
- R8：结构迁移不得造成性能退化；synthetic sync wall/statement/event count 相等，代表性 hot/cold p95 若可验证不得回归超过 10%。

## Acceptance Criteria

- [x] A1/R1-R3：核心 engine/default executor 位于 `src/sync/`，Web/TUI/JobRegistry 构造路径不含 `crate::commands`，commands 中无同步数据库/重建/远端主流程。
- [x] A2/R4：现有 root facade 和 `commands::sync` compatibility 编译 fixture 全绿，旧 wrapper 与新 engine 对同一 fixture 返回逐字段等价 `SyncSummary`/events。
- [x] A3/R5：full/recent/rebuild/auto-repair/OMP/remote/cancel/lock-loss focused tests 全绿，Store 写入与 cursor/bucket/behavior facts 等价。
- [x] A4/R6：human 与 NDJSON golden/subprocess tests、Web/TUI JobRegistry lifecycle、invalid request matrix 无差异。
- [x] A5/R7：architecture positive tests 与至少三类 negative fixtures 通过；对新 engine 人为加入 commands dependency 时测试可失败。
- [x] A6/R8：固定 synthetic baseline 的 event/statement counts 完全相等且 wall p95 不回归超过 10%；representative copy 不可用时明确标记 `UNVERIFIED`。
- [x] A7：source-sync、write-fencing、token-accounting focused gates，fmt、clippy、serial tests、docs 和 `just ci` 通过。

## Out of Scope

- 改写 parser 算法、Store/SyncShard 协议、remote wire format 或新增同步功能。
- 改变 `SyncExecutor` async 签名或下游 public API；删除兼容路径需要另行 semver 决策。
- 为了“解耦”引入 service container、全局单例或新依赖。

## Key Decisions

- 迁移 concrete ownership，不重新设计同步行为。
- `DefaultSyncExecutor` 是 canonical type；`CommandSyncExecutor` 保留为兼容 re-export/type alias。
- `AppContext` 参数即使当前 executor 未使用也先保留，避免无关 public contract 变化。
