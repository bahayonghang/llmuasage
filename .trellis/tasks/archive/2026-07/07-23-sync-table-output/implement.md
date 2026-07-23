# Implement — 表格化 sync 成功终态输出

## Preconditions And Review Gates

- [ ] 父任务规划已过审；本子任务 `prd.md` + `design.md` 已复核。
- [ ] `task.py start` 后才动产品代码；`task.py validate` 不等于实现批准。
- [ ] 动 Rust 前加载 `trellis-before-dev` 与 `llmusage/backend` 的 `source-sync-contracts.md` / `tui-presentation-contracts.md`。

## Ordered Workstreams

### 1. 进度清理
- 在 line renderer 与 TTY bar renderer 两处移除 `SourceFinished` 的永久成功句，保留终端清理与 `Failed`/`Cancelled` 诊断。
- 确认 `SourceFinished` 仍关闭/刷新活动进度，不残留半行。

### 2. TOTAL 行
- 在 `format_summary_lines` 增加由 source stats 聚合的 `TOTAL` 行，保留现有各列语义。
- 移除或折叠旧的独立 `- totals:` 行；保持 JSON/event 契约不变。

### 3. 宽度自适应
- 按 `design.md` 选定的 (A)/(B) 方案实现窄宽紧凑/截断：只截断展示标签、绝不截断数值、非 TTY 不出 ANSI。
- （可选）将 `source_label` 改为从 descriptor 取显示名以解耦来源子任务。

### 4. 测试
- 纯函数单测：流分离、无重复成功文本、非 TTY 无 ANSI、长来源名、窄宽、缺失来源、`TOTAL` 聚合。
- 更新 `sync_summary.rs::totals_line_keeps_legacy_shape` 到新 `TOTAL` 契约。

## Validation Commands

```powershell
cargo fmt --check
cargo test sync_summary
cargo test sync_progress
cargo clippy --all-targets --all-features -- -D warnings
```

CLI 输出：分别捕获 stdout / stderr，并以 `COLUMNS=60` / `COLUMNS=120` 等价方式在 Windows 测试环境验证窄/宽两档。

## Risky Files And Rollback Points

- `src/commands/sync_progress.rs`、`src/commands/sync_summary.rs`：用户可见流。若 parser 侧无关但终端快照失败，只回退展示改动。
- 不涉及数据库 migration；任何 schema 需求都要退回设计评审。

## Cross-Task Coordination（重要）

- 本任务与 `07-23-kimi-code-source`、`07-23-pi-omp-source` **共享** `sync_summary.rs` / `sync_progress.rs`。若保留硬编码 `source_label` match，则两个来源子任务会各加一个 match 臂——先落地者不冲突，后落地者按 `SourceKind` 变体补臂即可。
- 建议落地顺序：本任务（展示层，可独立验收）可**先合并**，为来源子任务提供已就绪的表格 + `TOTAL`；也可后合并再 rebase。无论顺序，冲突都是同文件不同行的合并问题，不是逻辑依赖。
- 若采纳"从 descriptor 取显示名"的解耦项，来源子任务将无需触碰这两个展示文件。

## Final Review Gate

提交前跑本子任务的聚焦质量检查、`git diff --check`，确认 stdout/stderr 分离且无重复成功句。不在本任务内 commit/push（除非父任务集成阶段统一处理）。
