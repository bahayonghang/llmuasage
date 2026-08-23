# Implementation Plan

1. Baseline
   - [x] 建立 100k/500k seed-outside-timing harness、trace 与 EQP。
   - [x] 固化 old matcher/table/JSON/timezone equivalence oracle。
2. Consolidate period projections
   - [x] 引入 `PeriodSpec` 和 aggregate bundle。
   - [x] 先切 no-project daily/weekly/monthly overall/source/host/unified，证明 statement reduction 与 payload parity。
3. Resolve projects
   - [x] 实现 dimension + current bucket union selector 与 exact hash filter。
   - [x] 覆盖 stale/missing dimension、multi-hash、Unicode、empty/unknown。
4. Split totals and conversations
   - [x] totals/model/cost/notes 走 bucket aggregate。
   - [x] exact conversation count 走 SQL-side narrow distinct aggregate。
5. Evidence-driven index decision
   - [x] 捕获 EQP/read p95；必要时临时实验 candidate index。
   - [x] 只在 read/migration/size/write 四门全过时保留并添加 migration tests。
6. Validation
   - [x] 运行 report query、CLI integration、public API 与 sync-write non-regression。
   - [x] 运行 synthetic/representative performance gates。
   - [x] 运行 fmt、clippy、serial tests、rustdoc、docs 和 `just ci`。

Rollback points: bundle projection、project selector、conversation query、optional migration；不得把性能失败通过放宽 400 ms 或删除语义断言解决。
