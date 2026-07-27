# GPT Pro 架构审计整改总控（快照 3def75c）

## Goal

承接 `llmuasage_code_architecture_audit_3def75c.md`（GPT Pro 对仓库 commit `3def75c` 的静态审计）全部 33 条编号发现的整改。父任务持有：发现清单与逐条核实结论、发现→子任务映射、跨子任务验收标准、执行顺序约束与最终集成审查。父任务本身不承载实现，实现全部下沉到 14 个子任务。

## 核实结论（2026-07-24，Claude 逐条对照当前 dev HEAD）

- 审计快照 `3def75c` 与当前 dev HEAD 的 `src/` 目录 **完全一致**（`git diff` 为空），报告行号可直接对照。
- 33 条编号发现（1×P0、7×P1、22×P2、3×P3）**全部证实**，无一被证伪、无一已修复。关键抽查证据：
  - SEC-001：`src/web/mod.rs:862-903` `reject_non_local_write` 仅校验客户端可控 Host header；Origin 缺失即放行；`src/commands/serve.rs:52-56` public 绑定 `0.0.0.0`，写路由与读路由同 Router（`src/web/mod.rs:313-316`）。
  - DATA-001：`src/parsers/claude.rs:405-418` 先 `offset += bytes_read` 再 parse，EOF 半行 parse 失败被 `continue`，cursor 越过半行。
  - CONC-001：`src/store/lock.rs:246-257` refresh 忽略受影响行数，无 fencing generation。
  - ARCH-001：`src/store/schema.rs:101` bootstrap 内调用 `upgrade_embedded_pricing_if_needed`。
  - DATA-002：`src/store/mod.rs:352,380-421` 每 5000 行独立事务提交，bucket 用内存 HashMap 汇总。
  - PERF-001：`src/query/explorer.rs:290-310` Rust 端全量收集后 `select_rows(limit)` 截断。
  - PERF-002：`src/web/mod.rs:1101,1117,1128` timeout 后仍 `task.await`。
  - RES-001/API-001：`src/sync/job_registry.rs:132-142,338-343` 任意 usize、unknown source→None→全量。
  - DATA-004：`src/store/migrations.rs:99-104,134-136` malformed version `unwrap_or(0)`，future version 静默跳过。
  - 其余（DATA-003/005/006、REL-001~006、SEC-002/003/004、CONTRACT-001、ARCH-002、MAINT-001、API-002、CI-001/002、SUPPLY-001/002、OBS-001、LEGAL-001、DOC-001、HYGIENE-001）均按报告引用位置逐一核对属实。
- 审计报告 §1.3 列出的 8 项"未证实高风险"（SQL injection、普遍 XSS 等）不纳入整改范围。

## 发现 → 子任务映射

| 子任务目录 | 覆盖发现 | 优先级 |
|---|---|---|
| 07-24-sec-public-boundary | SEC-001、SEC-003、SEC-004 | P0 |
| 07-24-jsonl-partial-tail | DATA-001 | P1 |
| 07-24-write-fencing-coordinator | CONC-001、ARCH-001 | P1 |
| 07-24-pricing-atomicity | DATA-002 | P1 |
| 07-24-explorer-sql-topn | PERF-001 | P1 |
| 07-24-web-hard-timeout | PERF-002 | P1 |
| 07-24-api-input-validation | RES-001、API-001、CONTRACT-001 | P1 |
| 07-24-store-robustness | DATA-004、DATA-005、REL-006 | P2 |
| 07-24-dst-timezone | DATA-003 | P2 |
| 07-24-integration-file-safety | SEC-002、REL-002、REL-003、REL-004 | P2 |
| 07-24-ci-supply-chain | CI-001、CI-002、SUPPLY-001、SUPPLY-002、LEGAL-001 | P2 |
| 07-24-log-registry-bounds | OBS-001、REL-001 | P2 |
| 07-24-arch-layering-api | ARCH-002、MAINT-001、API-002 | P2（长期） |
| 07-24-docs-hygiene | DOC-001、HYGIENE-001 | P3 |

补充映射说明：DATA-006（parser 静默跳过 malformed 行、单行无大小上限）与 REL-005（blocking parse 无协作取消）随 07-24-jsonl-partial-tail 的 BoundedJsonlReader 方向一并处理，写入该子任务 PRD。

## 执行顺序约束（来自审计 §5 依赖图）

1. 立即阻断项：sec-public-boundary、jsonl-partial-tail、api-input-validation（parallelism cap 部分）、ci-supply-chain（CI-001/LEGAL-001 部分）、store-robustness（DATA-004 部分）。
2. write-fencing-coordinator 是 pricing-atomicity 的**前置**；pricing 新实现不得在旧写入口仍可绕过时落地。
3. explorer-sql-topn 与 web-hard-timeout 可并行推进。
4. arch-layering-api（目录重构与 API 收口）必须在 correctness 类子任务（1-3）稳定后最后执行，避免制造巨大 diff。
5. docs-hygiene、dst-timezone、integration-file-safety、log-registry-bounds 无前置，可穿插执行。

## Requirements

- 每个子任务在启动前完成自身规划（复杂子任务需 design.md + implement.md）。
- 每个子任务的修复必须先有能在旧实现上稳定失败的 regression test（审计 §9 DoD 第 1 条）。
- 涉及数据 mutation 的子任务必须有 crash/failpoint test；涉及并发的必须有 two-process 或确定性交错 test。
- 安全类修复必须使用真实 TCP socket 验证，不得仅构造 HeaderMap。
- CLI 行为变化时同步更新 `README.md`、`README.zh-CN.md` 与 docs 对应页面（仓库规约）。

## Acceptance Criteria

- [ ] 14 个子任务全部归档，或残余项经 owner 显式接受风险并记录在案。
- [ ] 条件性 P0 归零：远程 peer + `Host: localhost` 无法触达任何 mutation 路由（真实 socket 集成测试）。
- [ ] P1 归零：四 parser partial-tail contract tests、fencing 双进程测试、pricing failpoint 矩阵、Explorer 物化行数 ≤ limit+1、timeout p99 ≤ 配置+100ms、parallelism 1..=32 校验全部通过。
- [ ] `just ci` 与 GitHub Actions gate 来源单一化且全绿。
- [ ] 集成审查：跨子任务回归（sync + serve + catalog 并发场景）通过后父任务方可关闭。

## Notes

- 审计报告原文：仓库根目录 `llmuasage_code_architecture_audit_3def75c.md`（§1.1 全量分级表、§1.2 深挖、§3 优化 Plan、§6 测试矩阵、§9 DoD）。
- 报告中工作量估算（1d~3w 不等）仅供排期参考，以各子任务实际规划为准。
