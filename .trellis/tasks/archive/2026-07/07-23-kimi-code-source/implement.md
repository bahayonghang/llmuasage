# Implement — 新增 Kimi Code 数据源

## Preconditions And Review Gates

- [ ] 父任务规划已过审；本子任务 `prd.md` + `design.md` + 父任务 `research/` 已复核。
- [ ] `task.py start` 后才动产品代码。
- [ ] 动 Rust 前加载 `trellis-before-dev`，读 `llmusage/backend` 的 `source-sync-contracts.md`、`token-accounting-contracts.md` 与 `docs/agents/passive-parser-onboarding.md`。

## Ordered Workstreams

### 1. 来源模型接入

- 扩展 `SourceKind`（`KimiCode` / `kimi_code`）、稳定 id、aliases、descriptor、source label、status/monitor 投影、parser registry。

### 2. Kimi Code parser

- 根发现 + `wire.jsonl` 候选过滤（支持默认根与 `KIMI_CODE_HOME` 等价显式根）。
- turn-scoped usage 映射、模型保留、路径/序号事件身份、`FileCursor` append/reparse、原子 shard commit。

### 3. Fixture 与测试

- 合成脱敏 fixture：K3、未知模型后缀、零/非 turn、损坏行、重复 sync、追加、改写/截断、缺失根、source status。

## Validation Commands

```powershell
cargo fmt --check
cargo test kimi        # 用实现落地后的真实聚焦测试名
cargo test --all-features -- --test-threads=1
cargo clippy --all-targets --all-features -- -D warnings
```

## Risky Files And Rollback Points

- `src/domain/models.rs`、`src/domain/source_descriptor.rs`、`src/registry.rs`：稳定 id 与 rebuild 边界。若任何旧 source id 或 migration guard 回归，作为一个 registry 单元回滚。
- 新 parser 模块 + `src/parsers/source_files.rs`：增量状态与隐私边界。若 fixture 证据或 dedupe 不完整，**保持 parser 不注册**（只留 descriptor/status）。
- 无数据库 migration；任何 schema 需求退回设计评审。

## Cross-Task Coordination（重要）

- 与 `07-23-pi-omp-source` **共享** `models.rs` / `source_descriptor.rs` / `registry.rs`，以及（若未解耦）`sync_summary.rs` / `sync_progress.rs` 的 `source_label`。两者编辑相同文件的不同行——先落地者不冲突，后落地者 rebase 补自己的变体/臂。
- 与 `07-23-sync-table-output` 的关系：若该任务已把 `source_label` 改为从 descriptor 取显示名，本任务**无需**触碰展示文件；否则需在两处 match 各加 `KimiCode` 臂。
- 这些都是同文件不同行的合并协调，不是逻辑依赖；无强制先后顺序。

## Final Review Gate

提交前跑聚焦质量检查、`git diff --check`，确认无原始用户 artifact / prompt / 凭据 / 工作区路径进入 fixture。不在本任务内 commit/push（除非父任务集成阶段统一处理）。
