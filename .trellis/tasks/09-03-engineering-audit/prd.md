# 工程审查整改

## Goal

把 2026-09-03 全库工程审查的已核实问题收成可独立完成的子任务。父任务持有发现分类、严重度与成本排序、子任务映射和最终集成审查。父任务不改产品代码。

用户价值：忙/损坏的源库不再悄悄丢掉已导入用量；大库查询不再把整表拉进内存；store 与 query 分层可测；loopback 写路径和 SSH 目标不再依赖“只有本机用户”这一条假设。

## Confirmed facts

- 审查层级：deep。范围：第一方 `src/`、`tests/`、`scripts/`。排除 `target/`、`ref/`、`docs/node_modules/`。
- 2026-07 审计闭环（public allowlist、write fencing、bounded JSONL、JobRegistry 校验）仍然成立。本轮是新基线，不是那次报告的 delta。
- `src/` 中无 TODO/FIXME。`TODO.md` 已全部勾完。
- 架构测试禁止 `query`→`commands/web/tui` 和 `sync/remote`→`commands`，不检查 `store`→`query`。
- 完整发现表与证据在 `research/engineering-audit-2026-09-03.md`。

## Requirements

- R1. 父任务只做规划、映射和集成审查，不改 `src/`。
- R2. 每个子任务必须能单独规划、实现、检查和归档；跨子任务依赖写在子任务 `prd.md` / `implement.md`，不靠目录位置暗示。
- R3. 子任务按「严重度优先、同级按成本从低到高」排序执行。P0 必须先于 P3。
- R4. 每个子任务验收必须有负向或回归测试；无法稳定复现旧行为时，PRD 写明替代证据。
- R5. 已记录为「看起来糟但实际没问题」和「产品已文档化的风险」不得再开成缺陷任务。
- R6. 任一未完成子任务都阻止父任务归档。

## Child task map

按严重度、再按成本排序。P 为 Trellis priority。

| 顺序 | 子任务 | 覆盖发现 | 严重度 | 成本 | P |
| --- | --- | --- | --- | --- | --- |
| 1 | `09-03-antigravity-fail-closed` | CORR-001 | high | S | P0 |
| 2 | `09-03-query-sql-performance` | PERF-001..008 | high | M | P0 |
| 3 | `09-03-ssh-target-hardening` | SEC-002 | medium | S | P1 |
| 4 | `09-03-codex-tracer-http` | SEC-003 | medium | S | P1 |
| 5 | `09-03-loopback-write-csrf` | SEC-001 | medium | M | P1 |
| 6 | `09-03-store-query-decouple` | ARCH-001 | high | M | P1 |
| 7 | `09-03-dashboard-report-facade` | ARCH-002 | high | L | P1 |
| 8 | `09-03-parser-sync-robustness` | CORR-002, CORR-006 | medium | M | P2 |
| 9 | `09-03-store-reset-rebuild-recovery` | CORR-003, CORR-004, CORR-005 | medium | M | P2 |
| 10 | `09-03-remaining-security-hygiene` | SEC-004..007 | low/medium | S | P2 |
| 11 | `09-03-observability-docs-hygiene` | OBS-001, OBS-002, ARCH-003(文档), ARCH-004, READ-001 | low | S | P3 |

执行顺序约束：

- `09-03-dashboard-report-facade` 若与 `09-03-store-query-decouple` 同时进行，后者先合入（都可能改 `src/query/`）。
- 其余子任务文件面不重叠，可并行。

## Acceptance Criteria

- [ ] AC1. `research/engineering-audit-2026-09-03.md` 中每条 high/medium 发现都映射到恰好一个子任务，无遗漏、无重复实现范围。
- [ ] AC2. 每个子任务 `prd.md` 含可观察验收标准和 `file:line` 锚点。
- [ ] AC3. 复杂子任务（query-sql-performance、store-query-decouple、dashboard-report-facade、loopback-write-csrf、parser-sync-robustness、store-reset-rebuild-recovery）在 `task.py start` 前有 `design.md` 和 `implement.md`。
- [ ] AC4. 全部子任务归档后，父任务对照研究清单做一次集成审查：high 发现为 Fixed；medium 为 Fixed 或显式延期。
- [ ] AC5. 父任务自身 `git diff` 不含 `src/**` 产品逻辑。

## Out of scope

- 本轮不改产品代码（审查当轮已满足；实现只在子任务进入 `in_progress` 之后）。
- 不重做 2026-07 已闭环项（public 路由 allowlist、write fencing、bounded JSONL、JobRegistry 入参校验）。
- 不把 `--public` 无认证、订阅配额拉取、静态导出含聚合标签改成缺陷。这些是已文档化的产品选择；文档与实现不一致时由 hygiene 子任务修文档。
- 不把 `codex_tracer` 并入主 Store（L 成本，超出本轮）。
- 不跑 `cargo audit` / 依赖 advisory 刷新（需网络，未授权）。
- 不恢复或新建根目录 `CONTEXT.md`，除非用户另开任务。

## Open questions

无阻塞项。loopback CSRF 的具体机制（Origin/Host 或本地 token）由 `09-03-loopback-write-csrf` 在其 design 中选定，推荐 Origin/Host 校验。
