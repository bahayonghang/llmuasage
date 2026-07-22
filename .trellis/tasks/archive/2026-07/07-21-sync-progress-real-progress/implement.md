# 实施计划

> 仅在用户评审通过规划并执行 `task.py start` 后实施。Inline 模式由主会话直接实现和检查，不派发 implement/check 子代理。

## 1. Planning Gate

- [x] 用户确认 PRD D1：采用 planned replay 文件数作为 denominator。
- [x] 用户确认 PRD D2：最大事件采样频率为 5Hz（200ms）。
- [x] 用户确认 PRD D3：TTY 满条后显示“重放完成，正在提交...”。
- [x] 根据答案定稿 `prd.md` / `design.md`，执行 PRD convergence pass。
- [x] 用户评审最终 `prd.md`、`design.md`、`implement.md`。
- [x] 运行 `python ./.trellis/scripts/task.py start 07-21-sync-progress-real-progress`。
- [x] Phase 2 开始时加载 `trellis-before-dev`；inline 模式跳过 JSONL context curation。

## 2. Red Tests First

- [x] 为共享进度采样 helper 添加失败测试：blocking worker 完成多个文件时，commit 前出现单调中间快照，重复 tick 不重复 emit，最终值不超过 total。
- [x] Claude 回归：一个 selected project 内多个 replay 文件，断言 `SourceStarted.files_total` 等于 selected project planned files，而非 inventory/trigger 数；断言最终 Progress 自然到 total。
- [x] Codex 回归：inventory 含 unchanged + changed 文件，断言 total 只含 planned replay files，append 仍只扫描追加范围。
- [x] 增补/调整 `sync_progress.rs` 文案与收敛测试，先证明旧分母会依赖 `complete_active` 强拉，并覆盖满条后的“正在提交”状态。

Focused commands（以实施后实际测试名为准）：

```powershell
cargo test parsers::file_progress -- --nocapture
cargo test --test sync_regression claude_ -- --test-threads=1
cargo test --test sync_regression codex_ -- --test-threads=1
cargo test commands::sync_progress::tests -- --nocapture
```

## 3. Progress Counter And Parser Integration

- [x] 在 parser 层新增最小共享 helper：relaxed atomic file counter + async interval sampler；不依赖 renderer/store。
- [x] Claude 构造 plans 后计算 planned total 并 emit SourceStarted；在 `parse_claude_shard` 每个成功文件末尾 increment；等待 batch 时采样；保留合并 commit。
- [x] Codex 构造 plans 后 emit planned total；在 `parse_codex_shard` 每个成功文件末尾 increment；等待 shard 时采样；保留逐 shard commit。
- [x] 每次 commit 前强制 emit 当前解析 count，使最终 batch 在写入期间进入 TTY 提交状态；commit 后再 emit 最新 records/count，成功 source 结束前验证 position 到 total。
- [x] 不改变 OpenCode、`SyncShard`、writer、cursor 或 cancellation 协议。

Rollback point：若必须改变 `SyncEvent` 形状、detach blocking worker 或拆 Claude batch commit 才能完成，停止实现并回到 planning。

## 4. Consumer Copy And Contract

- [x] 更新 Claude/Codex SourceStarted/Progress 人类文案，使其明确表示 replay 工作量；OpenCode 文案保持 row/spinner 语义。
- [x] TTY determinate bar 到达 length 时显示“重放完成，正在提交...”，SourceFinished 继续输出永久完成行。
- [x] 复核 TUI `sync_progress_message` 与 web sync command center，不让 planned total 被描述为 inventory total；只做必要改动。
- [x] 更新 `SyncEvent` 注释和 `.trellis/spec/llmusage/backend/source-sync-contracts.md` 的 determinate-bar contract。
- [x] 若代码实现未产生新的跨任务经验，不新增 ADR；ADR 0001/0002 既有边界保持成立。

## 5. Focused Verification

- [x] `cargo fmt --all -- --check`
- [x] 共享 helper、Claude/Codex parser/progress focused tests
- [x] `cargo test commands::sync_progress::tests -- --nocapture`
- [x] `cargo test --test m2_raw_archive_logs -- --test-threads=1`
- [x] `cargo test --test sync_regression -- --test-threads=1`
- [x] TUI sync progress focused tests
- [x] `git diff --check`

## 6. Performance Evidence

- [x] 在不写真实用户库的相同 fixture/snapshot 上记录旧基线与新实现；release 构建、parallelism、输出模式相同。
- [x] 每组至少 3 次，记录 wall time median/range、parse/write 分解、`RenderStats.calls/nanos`、`progress_dropped` 与 Progress event count。
- [x] 断言 wall-time median 回归 <=5%；9 次对照的 wall 中位数为 baseline 748ms、current 715ms（-4.41%），完整证据见 `research/performance-evidence.md`。

## 7. Broad Gate And Diff Review

- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo test -- --test-threads=1`
- [x] 仅在文档站内容发生变化时运行 `npm --prefix docs run docs:build`（本任务未改文档站，无需运行）。
- [x] 检查最终 diff 只包含本任务工件、parser/progress/tests/spec 的必要改动，并保留用户其他工作树状态。
