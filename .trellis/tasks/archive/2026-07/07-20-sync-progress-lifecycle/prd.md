# sync 进度条生命周期

父任务：`.trellis/tasks/07-20-sync-progress-and-perf`（R1/R2/R5-开关/R6/R7 归属本任务）

## Goal

用 `indicatif` 实现 sync human 模式的进度渲染：按来源选择正确工作单位的展示形态，RAII 保证所有退出路径终端干净，接通 CLI Ctrl-C 取消，并提供可注入 draw target 与 `LLMUSAGE_PROGRESS=off` 开关供测试与 profiling 使用。

## Confirmed Facts（含评审复核）

- 工作单位不统一：OpenCode `files_total=1`（src/parsers/opencode.rs:90-96）而 `Progress.files_scanned` 为消息/part 行数（opencode.rs:213-221, 261-269）；Codex/Claude `files_total`=inventory 全量（codex.rs:108-116、claude.rs:118-126）而 `files_scanned` 只累计重放文件（codex.rs:182-192、claude.rs:216-227），`current_file` 均恒为 `None`。
- CLI 无取消闭环：永不取消的 token（src/commands/sync.rs:267, 295-303）；reporter 在 bootstrap/锁之后才 spawn（sync.rs:87-93），sync.rs:75/78/86 的 `?` 提前返回无终端清理。
- `human_progress_line()`（sync.rs:633-722）为现有唯一文案来源；TTY 检测在 sync.rs:599。
- 事件通道 mpsc(128) + `try_send` 丢弃（driver.rs:80-83）；渲染在 reporter task。
- `--json-events` NDJSON 走独立路径（sync.rs:133-221），不经过本渲染器。

## Requirements

- R1 渲染器抽象：`LineRenderer`（非 TTY / `LLMUSAGE_PROGRESS=off`，现状逐行行为原样保留）与 `BarRenderer`（TTY，`indicatif::MultiProgress`）；构造注入 `ProgressDrawTarget`，测试可注入 hidden/buffered target。`human_progress_line()` 继续作为唯一文案来源。
- R2 分来源工作单位（不改 `SyncEvent` 契约）：
  - OpenCode：spinner/不确定进度，`{pos}` 显示已扫描行数，message 显示已导入条数与 DB 路径。
  - Codex/Claude：确定进度条 length=`files_total`，position=`files_scanned`（重放文件数），标签明确单位为「重放文件」；`SourceFinished` 时 `set_position(len)` 走满后 finish，并以 `MultiProgress::println` 落永久完成行（文案含跳过数，来自 `SourceSyncStats`）。中途不走满是真实工作量反映，完成行闭环。
- R3 阶段映射：bootstrap/migration/pricing（有 total 用确定条，否则 spinner）、锁等待 spinner、`LockAcquired`/`SourceFinished` 等边界落永久行；绘制节流 `ProgressDrawTarget::stderr_with_hz(10)`，steady tick 100ms 仅在活动阶段开启。
- R4 RAII 终端清理：`run_with_human_events` 在 bootstrap 之前创建清理守卫，所有 `?` 提前返回、失败、取消、正常完成路径都经 `Drop` 完成/放弃活动 bar、停止 tick、恢复终端。
- R5 Ctrl-C 取消：`tokio::signal::ctrl_c` 触发 `CancellationToken`，经 `run_once_with_cancel`（sync.rs:306-315）传入 driver；取消后发 `SyncEvent::Cancelled`，渲染器 abandon 当前 bar 并落永久行。
- R6 `LLMUSAGE_PROGRESS=off`（非空即生效）强制 LineRenderer，供 profiling 同 TTY 开/关对照与用户 fallback。
- R7 兼容：`SyncEvent` 不变；进度只写 stderr；非 TTY 输出无 ANSI；`--json-events` 路径不受影响；修正 `Progress` 变体过期注释（parsers/mod.rs:79-81）。
- R8 依赖：本任务引入 `indicatif` 唯一直接依赖（`console` 归 summary 子任务）。

## Acceptance Criteria

- [ ] A1/R2：构造三来源夹具的测试/手动验证：OpenCode 全程 spinner 无越界百分比；Codex/Claude 热同步（少量变化文件）进度条低位推进、完成时走满并落含跳过数的完成行。
- [ ] A2/R4,R5：注入 buffered/hidden draw target 的回归测试覆盖：失败路径、取消路径、bootstrap 阶段提前返回（模拟 bootstrap 错误），断言渲染器收尾被调用、无悬挂活动 bar。
- [ ] A3/R7：非 TTY 子进程测试：stderr 重定向到管道运行 sync，断言输出不含 `ESC`（0x1B）；`tests/m2_raw_archive_logs.rs:809` NDJSON 断言通过。
- [ ] A4/R5：CLI Ctrl-C 取消：测试或受控手动验证取消后 writer 已提交部分保持原子性（复用既有 `file_boundary_cancel_preserves_written_events` 语义），终端无残留。
- [ ] A5/R6：`LLMUSAGE_PROGRESS=off` 在 TTY 下输出与 LineRenderer 逐行一致。
- [ ] A6：`cargo fmt --all -- --check`、严格 Clippy、`cargo test -- --test-threads=1` 通过。

## Out Of Scope

- 摘要表格（归 `07-20-sync-summary-display`）；profiling 测量（归 `07-20-sync-full-profiling`）。
- web/TUI 进度展示；`SyncEvent` wire 变更。
