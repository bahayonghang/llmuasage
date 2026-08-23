# Implementation Plan

1. Baseline and contracts
   - [x] 固化 old/new parser equivalence corpus 与 100k performance harness。
   - [x] 为当前全量 Vec 路径记录 wall/RSS/rows，seed 时间排除。
2. Shared decoder
   - [x] 从主 Codex parser 提取最低 envelope record contract，继续使用 bounded reader。
   - [x] 让主 parser 通过新 decoder，先证明 accounting/cursor fixture 零差异。
3. Tracer persistent state
   - [x] 添加 additive/idempotent migration 与 file-state Store API。
   - [x] 覆盖 fresh、旧库升级、replace、failure rollback。
4. Streaming ingestion
   - [x] 实现 batch sink、同事务 cursor、取消 drain 和跨 batch thread linkage。
   - [x] 切换 `run` 与 `/api/refresh`，旧 public collector 仅保留兼容 wrapper。
5. Validation
   - [x] 运行 focused tests 与 100k before/after，检查 RSS 与 wall budgets。
   - [x] 检查 API/UI payload、CLI help/docs、database rebuild。
   - [x] 运行 architecture、fmt、clippy、serial tests、docs 和 `just ci`。

Risky rollback points: tracer schema migration、file replace deletion、thread linkage finalization。每一项单独提交或保留可精确回退的 patch 边界。
