# Implement — 父任务（集成与排期）

> 父任务不逐行实现来源/展示代码——那在三个子任务里做。本文件是任务地图、排期、共享文件协调与最终集成评审。

## Preconditions And Review Gates

- [ ] 与用户复核父任务 `prd.md` / `design.md` / 全部 `research/` 证据，以及三个子任务的 `prd.md` / `design.md` / `implement.md`。
- [ ] 用户批准后，对**要开工的子任务**逐个 `task.py start`（`task.py validate`/`create` 不等于实现批准）。
- [ ] 保持当前 dirty-tree 边界：目前仅任务目录未跟踪。

## Task Map And Recommended Sequencing

1. `07-23-sync-table-output` — 展示层，独立，可先合并；建议同时采纳 `source_label` 从 descriptor 取显示名的解耦，使后续来源子任务零展示层改动。
2. `07-23-kimi-code-source` — Kimi 纵向切片；本机有真实样本，证据最强。
3. `07-23-pi-omp-source` — Pi/OMP 纵向切片；本机仅 OMP 样本，Pi-only 靠合成 fixture。

> 顺序是建议非强制：三者无逻辑依赖，可并行推进，冲突仅为同文件不同行的合并。

## Shared-File Coordination

- 见 `design.md` 的 Shared-File Coordination Map。三子任务共同触碰 `models.rs` / `source_descriptor.rs` / `registry.rs`，以及（若未解耦）两个展示文件。
- 每个子任务的 `implement.md` 已写明各自的协调条款；后落地者负责 rebase 补自己的变体/臂。

## Integration Validation（父任务收尾）

三子任务合并到集成分支后统一跑：

```powershell
cargo fmt --check
cargo test --all-features -- --test-threads=1
cargo clippy --all-targets --all-features -- -D warnings
npm --prefix docs run docs:build
just ci
```

重点核对：现有来源回归、Kimi/Pi 聚焦测试、query/report 模型保留、source 状态/monitor、sync 表格（含 `TOTAL`、非 TTY 无 ANSI、窄宽）。

## Documentation（R9，父任务统筹）

- Kimi/Pi 的来源路径、质量标签、游标/幂等限制在各子任务落地时写各自文档片段；
- 父任务收尾统一校对 README / README.zh-CN / VitePress 的一致性，补 Reasonix 已知缺口与新 sync 表格契约。

## Final Integration Review Gate

归档整棵任务树前：跑全量质量检查、`git diff --check`、确认无原始用户 artifact / prompt / 凭据 / 工作区路径进入 fixture、重读最终 PRD 与实现的收敛。不在规划延续里 commit/push。

## Rollback

- 无数据库 migration。任何 schema 需求把该子任务退回设计评审。
- 单个子任务可独立回滚而不影响其他子任务（各为独立可验收切片）。
