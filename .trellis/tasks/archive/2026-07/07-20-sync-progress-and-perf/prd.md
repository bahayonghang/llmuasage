# sync 扫描进度条与展示性能优化（父任务）

## Goal

为 `llmusage sync` 的扫描过程设计基于 `indicatif` 的进度条系统（仅 CLI human 模式）；对整体 sync 全链路再做一轮 profiling；把最终 `Sync finished` 统计块优化为对齐表格 + 颜色的展示形态。本任务为父任务：拥有需求集、任务映射与跨子任务验收，不直接作为实现目标。

## Task Map

| 子任务 | 交付物 |
|---|---|
| `07-20-sync-progress-lifecycle` | indicatif 渲染器（分来源工作单位、RAII 终端清理、Ctrl-C 取消、可注入 draw target、`LLMUSAGE_PROGRESS=off` 开关） |
| `07-20-sync-summary-display` | `Sync finished` 对齐表格 + TTY 颜色 + 人类可读字节/耗时 |
| `07-20-sync-full-profiling` | 快照恢复测量协议 + `research/profiling.md`；确认的问题小则就地修，大则另建本父任务下的新子任务 |

执行顺序：progress-lifecycle → summary-display → full-profiling（profiling 依赖 lifecycle 提供的 `LLMUSAGE_PROGRESS=off` 开关做渲染开/关对照）。profiling 发现的新问题另建子任务，不塞进任一既有子任务。

## Confirmed Facts（代码勘察，含评审复核）

- 渲染入口：`src/commands/sync.rs` 的 `HumanProgress`（sync.rs:590-631）+ `human_progress_line()`（sync.rs:633-722）。进度写 **stderr**，最终摘要 `print_summary()`（sync.rs:223-244）写 **stdout**。
- TTY 检测：`stderr.is_terminal()`（sync.rs:599），TTY 下 `\r`+空格覆写单行；非 TTY 逐行 `writeln!`。
- 仓库当前没有任何进度条/spinner 库（crossterm+ratatui 仅供 TUI）。
- **工作单位不统一（评审复核确认）**：
  - OpenCode：`SourceStarted{files_total: 1}`（opencode.rs:90-96），但 `Progress.files_scanned` 是消息行数、之后是消息+part 行数（opencode.rs:213-221, 261-269），可轻松破万 → 确定进度条会越界。
  - Codex：`files_total` = 全部 inventory 文件（codex.rs:108-116），`files_scanned` 只累计 `commit.files_seen`（codex.rs:182-192），即只含实际重放文件；热同步时进度长期停在低位。`current_file` 恒为 `None`。
  - Claude：同 Codex，`files_total` 全量（claude.rs:118-126），`files_scanned` 只累计提交批次（claude.rs:216-227），`current_file` 恒为 `None`。
- **CLI 无取消闭环（评审复核确认）**：human 路径每次新建永不取消的 `CancellationToken`（sync.rs:267, 295-303）；reporter 在 bootstrap 与锁获取之后才 spawn（sync.rs:87-93），bootstrap/锁阶段的 `?` 提前返回（sync.rs:75, 78, 86）无任何终端清理。
- 事件通道：mpsc 容量 128，`try_send` 满则静默丢弃（driver.rs:80-83）；渲染在独立 reporter task（sync.rs:87-93）。`Progress` 变体注释已过期（parsers/mod.rs:79-81）。
- `--json-events` NDJSON stdout（src/commands/mod.rs:80-82）不得被污染。
- `SourceSyncStats`（parsers/mod.rs:202-234）已有 bytes_scanned、parse_ms、write_ms、lock_wait_ms、absent、last_error；`SyncSummary`（sync.rs:21-27）未 Serialize。
- 测试锚点：sync.rs:733-792（pricing 文案单测）、tests/m2_raw_archive_logs.rs:809（NDJSON 子进程断言）、tests/sync_regression.rs:85/1236（摘要/统计 wire 契约）。`cancel_within_1500ms` 系列只覆盖 parser 取消，不覆盖终端清理。
- 前一任务 07-19-claude-sync-scan-performance（已归档）提供扫描性能基线。

## Decisions

- D1 进度条实现：引入 `indicatif`（用户授权新增运行时依赖）。
- D2 范围：仅 CLI human 模式；web dashboard 与 TUI 不动。
- D3 性能分析：全链路 profiling；确认的问题小修就地、大修另建子任务。
- D4 最终展示：对齐表格 + TTY 颜色 + 耗时 + 人类可读字节；非 TTY 纯文本。
- D5 依赖：允许的直接依赖为 `indicatif` + `console` 两个（console 已在 indicatif 依赖树中，无新增传递依赖）；不再引入其他依赖。（评审第 6 条修正）
- D6 CLI 实现 Ctrl-C 取消（tokio signal → CancellationToken），保留 R7 承诺而非删除。（评审第 2 条修正）

## Requirements（父级，归属见子任务 PRD）

- R1 进度条系统（lifecycle）：阶段 spinner + 分来源正确工作单位的进度展示；绘制节流。
- R2 兼容契约（lifecycle/summary）：`--json-events` NDJSON 不变；`SyncEvent` 与 `SourceSyncStats` wire 形状不变；进度→stderr、摘要→stdout；非 TTY 无 ANSI。
- R3 最终展示（summary）：对齐表格 + 颜色 + 耗时 + 人类可读字节。
- R4 全链路 profiling（profiling）：快照恢复协议，修复或立项所有确认问题。
- R5 进度开销（profiling 测量、lifecycle 提供开关）：同快照、同输出目标下渲染开/关对照，中位数差异 <2% 并报告波动范围。
- R6 依赖约束：仅 `indicatif` + `console` 两个直接依赖。
- R7 终端状态清理（lifecycle）：完成/失败/Ctrl-C 取消/bootstrap 或锁阶段提前返回，全部路径终端无残留。

## Acceptance Criteria（跨子任务集成验收）

- [ ] A1：三个子任务各自验收通过并归档。
- [ ] A2/R2：端到端回归：`tests/m2_raw_archive_logs.rs:809` NDJSON 断言、sync.rs pricing 文案单测、sync_regression wire 契约测试全部通过。
- [ ] A3/R1,R7：真实 TTY 手动冒烟一次（含 rebuild 触发 pricing 阶段、一次 Ctrl-C），终端无残留；管道运行无 ANSI。
- [ ] A4/R5：`research/profiling.md` 含渲染开/关对照数据（同快照同输出目标，3 次中位数 + 波动范围）。
- [ ] A5：`just ci` 全量通过。

## Out Of Scope

- web dashboard 与 TUI 的进度展示改造。
- `SyncEvent` / `SourceSyncStats` wire contract 变更（进度单位问题在渲染层归一化解决，不改事件）。
- 扫描/解析算法的无证据重构。
- 除 `indicatif` + `console` 外的新增运行时依赖。
