# 工程审查整改 — 执行（父任务）

父任务不实现。实现只在子任务 `task.py start` 之后。

## 建议顺序

1. `09-03-antigravity-fail-closed`（P0, S）
2. `09-03-query-sql-performance`（P0, M）
3. `09-03-ssh-target-hardening`（P1, S）与 `09-03-codex-tracer-http`（P1, S）可并行
4. `09-03-loopback-write-csrf`（P1, M）
5. `09-03-store-query-decouple`（P1, M）
6. `09-03-dashboard-report-facade`（P1, L）— 等 5 合入后再 start
7. `09-03-parser-sync-robustness` 与 `09-03-store-reset-rebuild-recovery`（P2）可并行
8. `09-03-remaining-security-hygiene`（P2）
9. `09-03-observability-docs-hygiene`（P3）
10. 父任务集成审查后归档

## 校验

- 子任务：各自 PRD 的 focused `cargo test` 切片，再 `python scripts/ci-rust.py` 若改了 Rust。
- 父任务归档前：对照 `research/engineering-audit-2026-09-03.md` 把每条 high/medium 标为 Fixed / Deferred。
- 不在父任务里跑全量 `just ci`，除非最后一个 child 的 2.2 还没跑过全量 Rust 测试。

## 回滚点

- 每个 child 一次合入。不要在父任务里 squash 跨 child 的产品提交。
