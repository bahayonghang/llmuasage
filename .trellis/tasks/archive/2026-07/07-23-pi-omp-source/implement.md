# Implement — 新增 Pi / Oh My Pi 数据源

## Preconditions And Review Gates

- [ ] 父任务规划已过审；本子任务 `prd.md` + `design.md` + 父任务 `research/` 已复核。
- [ ] `task.py start` 后才动产品代码。
- [ ] 动 Rust 前加载 `trellis-before-dev`，读 `llmusage/backend` 的 `source-sync-contracts.md`、`token-accounting-contracts.md` 与 `docs/agents/passive-parser-onboarding.md`。

## Ordered Workstreams

### 1. 来源模型接入

- 扩展 `SourceKind`（`Pi` / `pi`，单 id 覆盖 Pi 与 OMP）、descriptor（两 roots）、source label、status/monitor 投影、parser registry。

### 2. Pi/OMP parser

- 两根发现 + canonical 路径去重（支持 `PI_AGENT_DIR` 等价显式根）。
- 解析 assistant usage，忽略 session/title/control/model_change 元数据。
- 保留 cache 通道、权威 total、reasoning 诊断、模型名、路径哈希。

### 3. Fixture 与测试

- 合成脱敏 fixture：两根、缺失 Pi 根、OMP 记录、重复发现去重、损坏 usage、重复 sync、追加、改写、query/report 可见性。
- 用合成 fixture 补齐 Pi-only 字段（本机无真实 Pi 样本）。

## Validation Commands

```powershell
cargo fmt --check
cargo test pi          # 用实现落地后的真实聚焦测试名
cargo test --all-features -- --test-threads=1
cargo clippy --all-targets --all-features -- -D warnings
```

## Risky Files And Rollback Points

- `src/domain/models.rs`、`src/domain/source_descriptor.rs`、`src/registry.rs`：稳定 id 与 rebuild 边界，作为一个 registry 单元回滚。
- 新 parser 模块 + `src/parsers/source_files.rs`：增量状态与隐私边界；证据/dedupe 不完整则保持 parser 不注册。
- 无数据库 migration；任何 schema 需求退回设计评审。

## Cross-Task Coordination（重要）

- 与 `07-23-kimi-code-source` **共享** `models.rs` / `source_descriptor.rs` / `registry.rs`，以及（若未解耦）`source_label`。编辑相同文件不同行——先落地者不冲突，后落地者 rebase。
- 与 `07-23-sync-table-output` 的关系：若该任务已把 `source_label` 改为从 descriptor 取显示名，本任务无需触碰展示文件；否则在两处 match 各加 `Pi` 臂。
- 均为同文件不同行的合并协调，非逻辑依赖；无强制先后顺序。

## Final Review Gate

提交前跑聚焦质量检查、`git diff --check`，确认无原始用户 artifact / prompt / 凭据 / 工作区路径进入 fixture。不在本任务内 commit/push（除非父任务集成阶段统一处理）。
