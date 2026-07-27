# Technical Design

## Overview

把普通无界 sync 的 token accounting guard 从“发现 legacy 后直接报错”改为
“先形成安全 repair plan，再在同一 fenced sync run 内只重置 legacy 来源并执行
一次 parser fan-out”。这解决的是升级编排缺口，不改变 accounting 算法或 schema。

核心不变量：

1. 先检查全部自动目标，再发生任何 reset。
2. 自动路径永远无权接受 lossy rebuild。
3. reset、parser、marker 与 status 写入都在现有 worker lock / fenced Store 下。
4. parserless 来源永远不进入 repair plan。
5. bounded sync 不承载自动全历史重建。

## Existing Seams

- `src/commands/sync.rs:466-482` 已在同一位置得到本轮 parser/source 集合，并在
  driver 前执行 accounting guard 与显式 rebuild reset。
- `src/commands/sync.rs:583-598` 已保证 parser writer 完成后才推进 marker 和保存
  source status。
- `src/commands/sync.rs:688-723` 已拥有 lossy rebuild 风险检查，但当前 helper
  会读取 `options.allow_lossy_rebuild`，不能原样作为自动路径的授权判断。
- `src/commands/serve.rs:206-263` 已证明 per-source legacy 发现、风险查询、安全
  rebuild 和 blocked 报告的产品语义可行。
- `SyncEvent` 是 CLI human、NDJSON、TUI 与 Web JobRegistry 的共享 lifecycle，
  自动修复提示应走该边界，不能在 `run_once_locked` 中直接 `eprintln!`。

## Repair Planning

在 `commands::sync` 内把“事实收集”与“调用策略”拆开：

```rust
struct AutomaticTokenAccountingRepairPlan {
    sources: Vec<SourceKind>,
}

fn automatic_token_accounting_repair_plan(
    store: &Store,
    options: &SyncRunOptions,
    parser_sources: &[SourceKind],
) -> Result<Option<AutomaticTokenAccountingRepairPlan>>;

fn lossy_rebuild_risks(
    store: &Store,
    sources: &[SourceKind],
) -> Result<Vec<LossyRebuildRisk>>;
```

具体命名可按现有模块风格调整，但必须保留以下职责：

- `legacy_token_accounting_sources_for` 继续以调用方已筛选的 parser sources
  为范围和稳定顺序。
- `lossy_rebuild_risks` 只返回事实，不读取 `allow_lossy_rebuild`。
- 显式 rebuild policy 可以在调用方已明确传 flag 时跳过风险拒绝。
- 自动 repair policy 只要 risks 非空就拒绝，不存在 flag bypass。
- `recent_days.is_some()` 且存在 legacy 时返回专用可操作错误，且不 reset。

这样 `serve` 可以继续保留“safe 逐源修复、blocked 逐源跳过”的启动策略，
普通 sync 则采用“所选 legacy 集合全量预检、任一 blocked 时零 reset 失败”的
命令策略，两者共享事实定义但不强行共享不同的失败政策。

## Same-Run Data Flow

```text
validate request
  -> acquire worker lock
  -> fenced bootstrap
  -> select parsers in registry order
  -> explicit --rebuild?
       yes: existing explicit preflight/reset policy
       no:  discover selected legacy sources
              -> none: ordinary incremental sync
              -> recent_days: fail before mutation
              -> collect all lossy risks
                   -> any risk: fail before mutation
                   -> none:
                        emit repair_started(sources)
                        reset only legacy sources + clear their markers
  -> create one SyncRunWriter
  -> drive selected parsers once
  -> finish writer
  -> mark successfully returned parser sources current
  -> save statuses
  -> emit repair_finished(sources) after marker/status success
  -> return ordinary sync summary
```

普通无界 sync 的 legacy 来源因为 reset 了 cursor/source state，会完整重放；同轮
current 来源保留 cursor 并继续增量处理。无需递归调用 `run_with_options`，因此不会
二次申请 worker lock、嵌套 run-log 或对 repaired source 再扫描一次。

本轮 run-log 仍记录为一次 `sync`，summary 仍表示用户发起的这次同步；自动修复是
该 sync 的内部 upgrade phase，不伪装成用户显式执行的 `sync --rebuild`。

## Lifecycle Contract

在 `SyncEvent` 增加 additive variants：

```rust
TokenAccountingRepairStarted { sources: Vec<SourceKind> },
TokenAccountingRepairFinished { sources: Vec<SourceKind> },
```

- `Started` 文案必须明确这是 legacy accounting 安全自动重建，并明确不会自动启用
  `--allow-lossy-rebuild`。
- `Finished` 只在 writer、marker 与 status 均成功后发送。
- lossy / bounded / unexpected failure 继续通过现有 `Failed { error }` 收尾；错误
  文本包含具体风险或下一步，不发送虚假的 repair finished。
- `sync_progress::human_progress_line` 是 human copy 的唯一 owner。
- TTY renderer 把 repair started/finished 作为永久阶段边界，不覆盖 parser bar。
- TUI 的 `sync_progress_message` 为两个事件提供短英文状态。
- `--json-events` 只序列化事件到 stdout；不得夹入普通提示文本。
- Web JobRegistry 无需新增状态机分支，继续把 additive event 作为 last/progress
  event 转发。

新增 enum variant 后必须更新所有 exhaustive matches 与 serde/render tests。

## Reset And Marker Semantics

- 提取一个接受显式 `&[SourceKind]` 的 reset helper，让显式 full rebuild 与自动
  repair 都可复用“per-source `Store::reset_for_source` + clear marker”协议。
- 自动 helper 收到的只能是已经全量预检通过的 legacy parser sources。
- parserless Antigravity 不在 `parser_sources` 中，因此不会进入自动 reset。
- marker 仍由现有 post-driver 循环推进；reset 或 parser 中途失败时 marker 保持
  absent/legacy。
- 多 source reset 仍沿用现有逐源事务语义。若发生非预期的 lock/SQLite 故障，
  已重置的 safe 来源可能需要重试，但它们已通过 lossless 证明、marker 未推进，
  不会被误报为 current。

## Bounded Sync Boundary

`recent_days` 的契约是只导入时间窗口内的数据。若在同一请求中对 legacy 来源执行
全源 reset，再把同一个 cutoff 传给 driver，窗口外历史会被清除且 marker 仍可能被
推进，等价于未显式授权的数据丢失。

本任务采用最小安全策略：

- legacy + `recent_days`：在 reset 前拒绝，提示先运行无界 `llmusage sync` 完成
  自动 repair，然后重试 bounded sync。
- current + `recent_days`：现有行为完全不变。
- 不在本任务中引入 per-source cutoff 或两阶段 driver；那会扩大同步调度模型。

## Status And Documentation

`source-status` / diagnostics 的 legacy warning 从“必须手动逐源 rebuild”改为：

- 首选：运行无界 `llmusage sync`，它会在无损时自动修复。
- fallback：风险来源恢复文件后显式 `sync --rebuild --source <source>`。
- `--allow-lossy-rebuild` 仍只在用户明确接受清理不可重建历史时使用。

同步更新：

- `README.md` / `README.zh-CN.md`
- `docs/guide/first-sync.md` / `docs/zh/guide/first-sync.md`
- `docs/reference/cli.md` / `docs/zh/reference/cli.md`
- `docs/safety/index.md` / `docs/zh/safety/index.md`
- `.trellis/spec/llmusage/backend/token-accounting-contracts.md`
- 必要时补充 `source-sync-contracts.md` 的 lifecycle event 列表

## Compatibility And Rollback

- 行为变化仅针对普通 sync 遇到 safe legacy parser source 的场景：由失败改为
  提示并修复。
- 新库、空来源、全部 current、parserless-only 与显式 rebuild 行为不变。
- JSON lifecycle 新事件是 additive；现有 finished/failed/cancelled 终态不变。
- 不新增 schema 或持久字段，回滚代码不会逆转已成功重建的数据；已推进 marker 的
  数据仍符合当前 accounting 合约。
- 回滚后 safe legacy 来源重新恢复为需显式 rebuild，但不会产生数据库降级。

## Rejected Alternatives

1. **普通 sync 捕获报错后递归执行三条 CLI rebuild。** 会重复拿锁、产生多个
   run-log、重复 bootstrap/扫描，且容易让 JSON/TUI 生命周期断裂。
2. **直接复用 `options.allow_lossy_rebuild`。** 这会把异常库调用构造当成自动
   删除授权，违反“never automatically enabled”。
3. **把 repair 放进 schema v18。** rebuild 依赖外部文件、parser 和长任务，
   不属于单 SQLite migration 事务。
4. **bounded sync 自动 reset 后沿用 cutoff。** 会静默删除窗口外历史。
5. **一个 blocked source 时先修其他 source 再失败。** 普通 sync 保持 preflight
   all-or-nothing，避免用户看到失败却已经发生部分计划内 reset；`serve` 的
   best-effort 启动政策可继续独立存在。
