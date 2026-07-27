# Implementation Plan

## 1. Lock The Behavior With Focused Regressions

- [x] 将 `tests/token_accounting_parity.rs` 中
  `legacy_source_requires_guarded_explicit_rebuild_before_new_writes` 改为普通
  sync 自动修复合约：Codex marker `2 -> 3`、事件顺序、最终数据与显式 rebuild
  等价。
- [x] 增加多个 safe legacy 来源的一次性修复测试，断言 registry 顺序、每个 parser
  仅运行一次、current 来源未 reset、Antigravity 行/桶/行为/cursor/source-file
  状态未改变。
- [x] 增加 mixed-risk all-or-nothing 测试：一个 safe、一个 missing-files legacy
  source 时，命令在任何 reset 前失败，两个来源的行数与 marker 均保持原状。
- [x] 增加 targeted `--source`、already-current、empty-source、parserless-only
  no-op 覆盖。
- [x] 增加 `recent_days + legacy` 零 reset 拒绝测试，并保留 current bounded sync
  的 `RecentReady` 回归。
- [x] 增加 parser/Store failure 与 cancellation 测试，断言不推进 marker、不发
  repair-finished。
- [x] 先运行 focused tests，记录旧实现失败证据。

## 2. Separate Repair Facts From Policies

- [x] 在 `src/commands/sync.rs` 提取纯风险收集 helper；返回逐源
  `LossyRebuildRisk`，不读取 `allow_lossy_rebuild`。
- [x] 保留显式 rebuild 的 opt-in policy，确保现有错误文本和 parserless
  preservation 不回归。
- [x] 为普通 sync 构建 selected legacy repair plan：source filter、registry
  order、bounded guard、全量风险预检。
- [x] 让 `serve::repair_legacy_token_accounting` 复用可复用的发现/风险事实 helper，
  但保留其 safe-repair / blocked-continue 产品策略与公开 report 形状。

## 3. Execute Safe Repair Inside One Sync Run

- [x] 用显式 source slice 的内部 helper 执行 per-source reset + marker clear。
- [x] 在 `run_once_locked` 的 parser 选择之后、writer 创建之前应用自动 repair
  plan；所有 mutation 使用传入的 fenced Store。
- [x] 不递归调用 `run_with_options`，不二次拿锁，不创建额外 run-log。
- [x] reset 后让原选中 parser 集合只经过一次 driver；current source 保留增量路径。
- [x] 仅在 writer finish、marker 与 status 写入成功后宣布 repair 完成。
- [x] 为自动开始、blocked、完成和失败增加结构化 tracing 字段（source 列表、风险
  计数），避免记录完整路径或源内容。

## 4. Add Cross-Surface Lifecycle Feedback

- [x] 在 `src/parsers/mod.rs` 增加
  `TokenAccountingRepairStarted/Finished` additive `SyncEvent` variants。
- [x] 更新 `src/commands/sync_progress.rs` 的唯一 human copy source、plain/TTY
  renderer 与单测，使警告成为永久阶段边界。
- [x] 更新 `src/tui/sync_control.rs` 的英文进度投影和 exhaustive matches。
- [x] 覆盖 serde round-trip / NDJSON 顺序，证明 `--json-events` stdout 无人类文本。
- [x] 检查 Web JobRegistry event forwarding、snapshot last-event 与完成/失败状态
  不受新中间事件影响。

## 5. Update Guidance And Contracts

- [x] 更新 `src/commands/source_status.rs` 的建议：首选无界普通 sync 自动 repair，
  风险场景保留显式 source rebuild 指引。
- [x] 更新中英文 README、first-sync、CLI reference 与 safety 页面。
- [x] 更新 `.trellis/spec/llmusage/backend/token-accounting-contracts.md` 的
  signature、normal-sync matrix、good/bad cases 和 required tests。
- [x] 若 lifecycle 列表发生变化，同步
  `.trellis/spec/llmusage/backend/source-sync-contracts.md`。
- [x] 检查文档明确区分 schema migration、token accounting rebuild 与
  `--allow-lossy-rebuild` 用户授权。

## 6. Validation

- [x] `cargo test --test token_accounting_parity -- --test-threads=1`
- [x] `cargo test commands::sync_progress:: -- --test-threads=1`
- [x] `cargo test sync::job_registry:: -- --test-threads=1`
- [x] `cargo test --test m2_raw_archive_logs -- --test-threads=1`
- [ ] `python scripts/ci-rust.py`
- [ ] `just ci`
- [x] 检查最终 diff 只包含 sync repair、反馈、测试、文档与对应 Trellis spec。
- [x] 只用临时 home/SQLite fixture 验证 destructive rebuild；除非用户另行授权，
  不在开发验证中对 `~/.llmusage/` 真实数据库执行自动 repair。

### Validation Notes

- `python3 scripts/ci-rust.py` 的 format/clippy 已通过；串行全量测试仅有两个未修改
  的 Grok Windows 路径归一化断言稳定失败。跳过这两个基线断言后为
  `686 passed, 6 ignored, 2 filtered out`。
- `just ci` 在本机进入检查前因 recipe 调用不存在的 `python` 失败；按相同 recipe
  使用 `python3` 执行 Rust 门禁，并单独完成 rustdoc、dashboard JS checks 与
  中英文 VitePress build。

## Risk And Rollback Points

- 自动 policy 不得通过任何 options 路径读取或推断 lossy opt-in。
- 全部 legacy 目标的风险检查必须先于第一个 reset。
- `recent_days` 检查必须先于 reset，不能靠文档约束。
- 新事件必须更新所有 exhaustive match；终态仍只有 finished/failed/cancelled。
- 若同轮 current 来源出现 reset 或 repaired 来源被解析两次，停止并调整执行 seam，
  不以性能回归换取表面自动化。
- 若 marker/status 写入失败，不得补发完成事件或在错误处理里强行推进 marker。
