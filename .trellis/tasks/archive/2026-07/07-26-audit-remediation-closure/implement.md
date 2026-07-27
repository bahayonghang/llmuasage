# Implementation Plan: 审计整改二次闭环

## Ordered Checklist

- [x] 依次完善并审批九个子任务；只 `start` 当前要实施的子任务，不启动父任务。
- [x] 每个子任务先运行旧实现复现/负向测试，再实现最小完整修复。
- [x] 每个子任务完成 focused gate、`cargo fmt --check`、Clippy 和相关集测后独立提交归档。
- [x] 所有子任务完成后，父任务重新读取 live source，逐条复审 R1-R9。
- [x] 运行最终 `just ci`、`git diff --check`，记录 MSRV 和 subprocess 测试证据。
- [x] 只有所有验收项有证据时才归档父任务并写 journal；不 push。

## Review Gates

- Gate A：正确性子任务完成前，不开始架构收口。
- Gate B：任何“测试环境问题”必须有可重复证据，不允许直接降级为忽略。
- Gate C：父任务归档前必须由独立 review 检查代码，不只检查任务文档和 `task.py validate`。

## Validation

```powershell
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features -- --test-threads=1
cargo doc --locked --no-deps
node --test scripts/dashboard-fetch.test.mjs scripts/dashboard-bootstrap-watchdog.test.mjs scripts/dashboard-load-state.test.mjs scripts/dashboard-render-lifecycle.test.mjs
npm --prefix docs run docs:build
just ci
git diff --check
```

## Final Integration Evidence

九个 child 均位于 `.trellis/tasks/archive/2026-07/`，其 `task.json` 状态均为
`completed`。下表只引用真实工作提交，不引用 archive 或 journal 提交。

| Requirement | Live code and regression evidence | Work commit |
| --- | --- | --- |
| R1 写入互斥 | `src/store/lock.rs` 的不可伪造 `WritePermit` 和 generation 校验；`src/store/sync_writer.rs::stale_generation_cannot_commit_next_shard_transaction`、`stolen_lock_causes_old_owner_refresh_to_fail` 与并发 coordination upgrade 测试。前两项在共享 DB 上分别构造两个独立 `Store`，每次 DB 操作打开独立 connection，确定性模拟 lease expiry/steal；这是 SQLite fencing 边界证据，不是 OS subprocess 测试。 | `1d81fd91062f1becbbcb6fcc1ae3b80d6b28675e` |
| R2 外部配置安全 | `src/integrations/atomic.rs` 使用 Windows `ReplaceFileW` 且不先删除目标；`failpoints_leave_existing_target_complete`、`failpoints_leave_missing_target_absent` 在当前 Windows target 上覆盖 write/flush/replace 注入失败，record-failure tests 覆盖 existing/missing rollback，`windows_replace_covers_existing_and_missing_targets` 走真实 Windows replace/create 路径。 | `078006e990fb48bb5ba031ab4c9f565f55f5c82f` |
| R3 Job 契约 | `src/sync/types.rs` 统一 typed validation 与 recent cutoff；public API stable-code、Web invalid-input、recent-window cursor recovery 和 OpenCode SQL lower-bound 测试。 | `d23e94f8ea87071d9c688be0c35042effa1d3c2d` |
| R4 更新信任锚 | `src/commands/update.rs` 将 stable tag 解析为 immutable commit；stable planner/resolver、preview-confirm target drift 和 provider failure fail-closed 测试。 | `719845d773474b5ce8af08a7a7245548209afb0a` |
| R5 日志有界 | `src/runtime/logging.rs` 的 size/daily rotation、retention maintenance 和 dropped counter；完整 NDJSON rotation、file-count/age retention、queue-drop 与失败重试测试。 | `7db9b56457f1aa9b025cc83926bdf86b311fb8e4` |
| R6 JSONL 健壮性 | `src/parsers/file_state.rs::BoundedJsonlReader` 与四个 parser 共用读取路径；oversized bounded discard、malformed privacy、EOF cursor 和 cooperative cancellation tests。 | `d258d33aa2e43154d346cb099eafdb1732e81cbc` |
| R7 验证诚实性 | `Cargo.toml`、`.github/workflows/ci.yml` 同为 Rust 1.95，MSRV job 执行 locked all-features check。`json_events_subprocess_emits_ndjson_per_event` 的原 `os error 2` 根因是测试读取已失效的固定日志文件名；现改用 rolling-log reader，并保留 binary existence 与 spawn-context 断言。 | `6b67cfa926c99ace580ac93130f868408ebce186` |
| R8 分层约束 | `src/sync` 只持有 executor port，Web/TUI 显式注入 command adapter；`tests/architecture_dependencies.rs` 解析 use、全限定、alias、nested、crate-alias 和 relative fixtures 并报告文件/行/目标。 | `0f085ef8e78330214688cc9e2c01b82197b2d766` |
| R9 Public 读边界 | `src/web/mod.rs` 拆分 public/loopback router 与聚合 projection；真实 TCP sensitive-route absence、forbidden-field、project-filter 和 loopback compatibility tests。 | `6206f5cec193ff15929edcc79a9ee1e40ae828ef` |

### Cross-Cutting Evidence

- 每个 child 的归档 `implement.md` 记录 focused gate；对应工作提交均带正确
  `Agent-Task` trailer。
- `cargo +1.95.0 check --locked --all-features` 已在隔离 target 通过，Rust 1.94
  的依赖编译失败证据保存在 R7 child。
- `json_events_subprocess_emits_ndjson_per_event` 已在独立 M2 15/15 与完整
  serial Rust suite 中通过；测试通过公共 rolling-log reader 读取当前分片，且显式
  断言 binary 存在并附带 spawn context。
- 独立 Gate C reviewer 将 R1 的 heartbeat/write-fence tests 修正为两个独立
  `Store`/SQLite connection；外层 reviewer 进程在完成 Rust 验证后超时，主会话
  复核其仅测试层的 diff 并重跑两项 R1 tests，未把缺失的最终 prose 记作 pass。
- 父任务最终 `CI=1 just ci`、`git diff --check` 与 Trellis context validation
  均在归档前重新执行；结果支持勾选全部验收项。
