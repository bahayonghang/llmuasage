# Design — 父任务（共享架构与跨子任务契约）

> 本文件只写**跨子任务共享**的架构、契约与协调。逐来源解析设计见各子任务 `design.md`；展示层设计见 `07-23-sync-table-output/design.md`。

## Shared Source Boundary（R2，所有来源子任务遵守）

复用既有来源与 parser 管线，不新增并行机制：

- `src/domain/models.rs`：只加稳定 parser-backed id（`kimi_code`；`pi` 一个 id 覆盖 Pi 与 Oh My Pi）。
- `src/domain/source_descriptor.rs` / `src/domain/platform_monitor.rs`：声明 display name、aliases/roots、`UsageQuality`、隐私类、monitor 状态，不制造假 usage rows。
- `src/parsers/source_files.rs`：枚举来源特定根并跨 Pi/OMP 根去重。
- `src/registry.rs`：只注册通过准入门槛的来源；注册即纳入 rebuild/token-accounting 边界。
- `src/parsers/source_parser.rs` / `src/store/mod.rs`（`FileCursor`）/ `src/parsers/file_state.rs`：新 reader 复用其 async parse/commit 与 append/reparse 状态机，不另造游标。
- 不在首版设计加 schema migration。现有事件键与 source-scoped dedupe 足以承载 Kimi/Pi 记录，不存原始 transcript。

## Token Accounting Contract（Kimi 与 Pi 共同遵守）

- 权威上游 total 优先；cache 通道保持独立；reasoning 保持诊断，除非来源证明它与 output 不相交。
- 逐来源字段映射见各子任务 `design.md` 的 Normalized Data Contract。

## Sync Wire Compatibility

- 保持 `SyncEvent` 与 `SourceSyncStats` wire 形状不变。展示层只改人类可读契约（`SourceFinished` 不落永久成功句、`format_summary_lines` 增 `TOTAL` 行），JSON/event 输出不变。
- 失败/取消行保留（诊断，非重复成功摘要）。

## Shared-File Coordination Map（合并协调，非逻辑依赖）

| 文件 | sync-table-output | kimi-code-source | pi-omp-source |
| --- | --- | --- | --- |
| `src/domain/models.rs`（`SourceKind`）| — | 加 `KimiCode` | 加 `Pi` |
| `src/domain/source_descriptor.rs` | — | 加 kimi descriptor | 加 pi descriptor |
| `src/registry.rs` | — | 注册 kimi parser | 注册 pi parser |
| `src/commands/sync_summary.rs` | TOTAL/宽度/（可选解耦 `source_label`）| 加 label 臂* | 加 label 臂* |
| `src/commands/sync_progress.rs` | 去完成句/清理 | 加 label 臂* | 加 label 臂* |

\* 若 `07-23-sync-table-output` 采纳"从 descriptor 取显示名"的可选解耦，则来源子任务**无需**触碰 `sync_summary.rs`/`sync_progress.rs`。这是消除来源子任务与展示子任务耦合的推荐做法。

- 上述均为同文件不同行的编辑：先落地者不冲突，后落地者 rebase。无强制先后。
- 建议（非强制）落地顺序：先 `sync-table-output`（并采纳 `source_label` 解耦）→ 再 Kimi / Pi（此后两者零展示层改动、互不冲突）。

## Compatibility And Rollback

- 现有 source ids、event schema、query DTOs、JSON events、SQLite migrations 保持兼容；新 id 只加行，不重解释历史 id。
- 失败/不完整的来源清点不得把现有文件扫成 `missing`；遵守 driver 契约。
- 若窄表适配过于侵入：非 TTY 保留完整表、TTY 用有文档的紧凑表头；绝不回退成重复成功句。

## Cross-Layer Verification Shape（父任务集成级）

- 每个 parser 的字段映射与 malformed/empty 记录单测（子任务内）。
- sync 集成 fixture：跑两次、追加、改写/截断、删除文件；断言事件计数与游标行为（子任务内）。
- CLI 子进程测试分别捕获 stdout/stderr（sync-table 子任务内）。
- **集成全量**：三子任务合并后跑 query/report 模型保留、source 状态/monitor 输出、文档构建与 `just ci`（父任务收尾）。
