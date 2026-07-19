# 实施计划

## 1. Restore And Baseline

- [x] 用 `trellis-continue` 恢复本子任务，读取 PRD/design/implement、dashboard performance spec 和父任务 residual evidence。
- [x] 运行当前 debug/release 微基准与 detached clean-HEAD 对照，记录至少 3 次 query elapsed；不得把编译或 fixture seed 算入查询。
- [x] 在现有测试 seam 增加逐阶段 timing/work counters，使一个自动命令可稳定指出 summary/by-platform/series/run-state/diagnostics 主导项。
- [x] 为主导 SQL 捕获 `EXPLAIN QUERY PLAN` 与 VM/full-scan/sort 等结构证据，形成 3-5 个可证伪假设并按证据重排。

Focused commands:

```powershell
cargo test query::tests::home_overview_under_80ms_with_seeded_10k_events -- --exact --test-threads=1
cargo test --release query::tests::home_overview_under_80ms_with_seeded_10k_events -- --exact --test-threads=1
```

## 2. Correctness Lock

- [x] 增加 payload 等价测试：跨日重复 session、无 session fallback、未知/四平台键、token/cost/cache、bootstrap/archive/last_updated。
- [x] 增加 source/model/project/date/timezone filter matrix，特别验证 daily series 与总 session 不可通过逐日值求和近似。
- [x] 将 profiling seam 保持 test-only；生产 API 不暴露 timing/debug 字段。

## 3. Minimal Evidence-Driven Fix

- [x] 只优化测量确认的主导阶段；优先复用 bucket/既有索引，再考虑 query reshape，最后才是新 migration/projection。
- [x] 每次只改一个变量，重跑阶段 breakdown、原 80ms gate 与等价测试。
- [x] 若新增索引，增加 query-plan 回归并验证写入/迁移成本；若新增 projection，补全 writer/rebuild/migration/repricing 所有权测试。
- [x] 删除所有临时 debug 日志、profile 输出和 throwaway harness；可复用 benchmark 工具才保留在 `scripts/`。

## 4. Real Database Verification

- [x] 对真实数据库创建 online backup，不直接运行写操作于用户库。
- [x] 在副本上记录 event/bucket 数、各阶段 warm/median/p95、总 wall time、query plan 和结果摘要。
- [x] 对比 synthetic 与 real-data 主导项；若方向不一致，解释数据分布差异并以更保守方案为准。
- [x] 清理备份、临时 worktree 和诊断文件。

## 5. Quality Gates

- [x] 现有 80ms debug 微基准连续通过 3 次。
- [x] 现有 80ms release 微基准连续通过 3 次。
- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] focused query/home overview tests 串行通过。
- [x] `cargo test -- --test-threads=1`（不设置 `CI=1`）
- [x] `npm --prefix docs run docs:build`
- [x] `git diff --check`

## 6. Documentation And Closeout

- [x] 更新 `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`，记录 home overview 精确聚合与预算契约。
- [x] 未新增 schema/projection；本次 query reshape 与 missing-source bucket predicate 不需要 migration，记录 no-migration 决策。
- [x] 将最终基准、正确性证据和 residual risk 写回本任务，完成 `trellis-check`。

## 7. Evidence

- Baseline before the child: release isolated `95.69ms`, detached clean HEAD `95.77ms`; debug isolated runs `123-154ms`. A post-seam cold debug run reproduced `88.56ms` without changing the budget.
- Baseline profile on the 10k fixture: summary `19.30ms`, by-platform `11.95ms`, series `11.33ms`, run-state `0.14ms`, diagnostics `29.59ms`; plans showed three `usage_event` scans and temporary distinct/group B-trees.
- Final debug gate profiles: `42.42ms`, `43.27ms`, and an earlier post-fix `45.29ms` run; final plan is one shared `SCAN usage_event` (21 opcodes) for all three projections. Final release gate profiles: `18.04ms`, `16.47ms`, `16.06ms`.
- Representative online-backup copy: `132,279` events and `3,812` buckets. Before reshape, debug wall times were `1.04-1.19s`; after reshape, five read-only runs were `0.770-0.936s`, with event read `0.472-0.492s`, summary `0.556-0.582s`, by-platform `0.067-0.089s`, series `0.101-0.149s`, and diagnostics `0.044-0.115s`. The copy had 26 missing Claude files; diagnostics preserved protected-event behavior while restricting bucket aggregation to that source.
- No schema, projection, writer, rebuild, repricing, or migration ownership changed; ADR 0004 does not require an update.
- [ ] 子任务完成后回到父任务，重跑其标准完整串行门，再按 scope 分离 commit、归档子任务和父任务。
