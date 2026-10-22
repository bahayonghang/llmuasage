# Implement — sync 日志结构化与 token/费用汇总

## Checklist

1. [x] 读 spec：`.trellis/spec/llmusage/backend/source-sync-contracts.md`、`token-accounting-contracts.md`、`pricing-catalog-contracts.md`、`cli-language-contracts.md`、`runtime-log-contracts.md`；ADR 0002、0016。
2. [x] 数据层：新增 `UsageDelta`（token 各项 + `cost_micro_usd` + `unpriced_events`），挂到 `ShardCommitStats` 与 `SourceSyncStats`（`#[serde(default)]`）；`commit_shard_inner` 在 `changed == 1` 时累加；`collect_sink` 分支同步。
   - 实际：类型名为 `InsertedUsage`（`src/domain/models.rs`）；`collect_sink`（仅 `--emit-shards`）不计价、保持为 0。
3. [x] 各 parser 改用 `SourceSyncStats::absorb_commit` 累加（codex、claude、grok、kimi_code、opencode、dsh、antigravity、omp/pi、zcode 等，grep `commit.events_inserted` 全覆盖）。
   - 实际：未加 `absorb_commit`，各 parser 在原累加处内联 `inserted_usage.absorb(commit.inserted_usage)`。
4. [x] 单测：sync_writer 重复插入同一事件时 delta 不重复计；费用与 `cost_with_cache_usd` 一致。
5. [x] 进度渲染：`LineRenderer`、`BarRenderer` 迁移摘要化 + 锁行条件化；`i18n::sync` 增加摘要文案（中英）。
6. [x] 汇总渲染：`SummaryDetail` 双模式；Usage 表、零新增合并行、Up to date、管线概要行、窄终端降级；`--verbose` 开关接入（含 help 与 i18n）。
7. [x] 诊断区重排（不改 `notice_lines` / `parse_issue_lines` 对外行为）。
8. [x] 测试：`sync_summary.rs` 内单测更新/新增（中英、color on/off、COLUMNS=60、空结果、rebuild）；`tests/sync/progress_io.rs`、`tests/sync/runtime/progress_io.rs` 适配；新增集成测试：新 home 同步 fixture 后，汇总 TOTAL 与 `daily --json` 一致。
   - 实际：未新增该集成测试；由 writer 单测 `commit_shard_reports_inserted_usage_once`（费用 = 持久化 `cost_with_cache_usd` 之和）覆盖，并在真实数据上手工核对 TOTAL 与 `daily --json` 完全一致。
9. [x] 文档：`README.md`、`README.zh-CN.md`、`docs/guide/first-sync.md`、`docs/zh/guide/first-sync.md`、`docs/reference/cli.md`、`docs/zh/reference/cli.md` 示例输出与 `--verbose` 说明。
10. [x] spec 更新：在 `source-sync-contracts.md` 记录 `inserted_usage` 语义与 sync 人类输出契约。

## Validation

```
cargo test --locked --all-features --test sync -- --test-threads=1
cargo test --locked --all-features --test cli -- --test-threads=1
python scripts/ci-rust.py
npm --prefix docs run docs:build
```

手工验证：`LLMUSAGE_HOME=<临时目录> cargo run -- sync`（首次 + 第二次）、`NO_COLOR=1`、`| cat`、`COLUMNS=60`、`cargo run -- lang zh-CN` 后再跑一次。

## Review gates / Rollback points

- Gate A（步骤 2–4 后）：数据累加正确，未改输出 → 可单独提交。
- Gate B（步骤 5–7 后）：截图对比新旧输出，确认后再改文档。
- 回滚：按提交 revert；无 schema 变更。
