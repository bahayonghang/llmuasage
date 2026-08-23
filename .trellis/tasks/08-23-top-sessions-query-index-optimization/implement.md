# Implement：Top Sessions 查询与索引优化

## Preconditions

- [x] 用户审阅并明确批准最新 `prd.md`、`design.md` 和本清单；批准前不运行
  `task.py start`，不修改产品代码。
- [x] 运行 `trellis-before-dev`，读取 dashboard performance contract、cross-layer guide、
  ADR 0004 和本任务 `research/diagnosis.md`。
- [x] 检查工作树，保留无关改动；活动用户数据库只读，任何 index/migration 实验使用
  task-owned verified backup。

## 1. Freeze semantics and baseline

- [x] 在 `tests/web_sessions_endpoint.rs` 建立完整 serialized legacy oracle，覆盖三排序、
  tie-break、limit、所有 QueryFilter 字段、identity 四级 fallback、active gap、labels、空库。
- [x] 增加成本累计顺序敏感 fixture，逐字节比较 `cost_usd`，禁止 tolerance/rounding 漂移。
- [x] 新增 task-owned Top Sessions benchmark/harness tests；输出做 privacy allowlist。
- [x] 从活动库 `mode=ro` online backup 创建一个精确代表副本，记录 source size/mtime、
  backup integrity、schema/index/binary/commit，确认活动库未变化。
- [x] baseline 对 `1d/7d/30d/all × 3 sort` 各 warm-up + 5 samples；D1/final 再覆盖
  all filter matrix，并记录 wall/status/support/query/payload evidence 与 query plans。

**Gate A — baseline**

- [x] 复现 all Token/成本 degraded 或明显超预算；若当前基线已自然达标，停止并重新评估
  任务是否仍有价值，不为预期中的问题制造 migration。

## 2. Implement D1 single-projection aggregation

- [x] 抽取/复用 canonical identity 定义，保持四级 fallback，不创建相似但不等价 helper。
- [x] 将过滤后的 event projection 收敛为一次读取；一个 accumulator 同时生成 Token、成本、
  时长所需字段。
- [x] 删除 candidate `session_event_times` N+1 和 duration `all_session_event_times` 第二次
  全量读取；增加 test-only query/statement count assertion。
- [x] 保持现有 label/source/min/max/count/output/reasoning/active-gap/comparator 语义。
- [x] 实现并验证 bounded Top K；随机/并列测试与完整 sort oracle 等价。

**Gate B — D1 correctness**

- [x] legacy/candidate 全矩阵 serialized bytes 完全一致。
- [x] `rtk cargo test --test web_sessions_endpoint -- --test-threads=1` 及相关 query tests 通过。

**Gate C — D1 performance decision**

- [x] 在同一代表副本重新跑完整 benchmark。
- [x] 若 all 三排序 p95 均 `<=400 ms` 且其他范围回归 `<=10%`：记录 `D1 PASS`，跳过
  Section 3，不改 schema。（已评估：D1 未满足该条件，未进入此分支。）
- [x] 若任一 all 排序超预算：记录机械 `D1 MISS` 和 scan/plan 证据，才进入 Section 3。

## 3. Experiment and conditionally adopt D2 index

- [x] 只在 Gate C 为 `D1 MISS` 时，从同一 source backup 创建新的隔离候选副本。
- [x] 创建 canonical identity + event_at 候选 expression index；记录 SQL、build time、
  plan、page/file/WAL delta 和 integrity。
- [x] 运行 D1+D2 全矩阵；逐字节 oracle 必须再次通过，目标 all plan 不含 identity temp sort。
- [x] 运行固定 synthetic `SyncRunWriter` baseline/indexed 多轮交替单线程基准；中位数回归
  `<=10%`。
- [x] 计算 index/database bytes ratio；必须 `<=15%`。

**Gate D — production index decision**

- [x] 只有 semantics、p95、payload、plan、size、write regression 全部通过才接受 D2。
- [x] 任一门失败：不修改 migration，停止并回到 Phase 1 规划 rollup/其他方案。
  （已评估：D2 全部门通过，未触发此分支。）

## 4. Conditional schema v24 implementation

- [x] 仅在 Gate D 通过时追加 v24 `optimize_top_sessions_identity_order` migration。
- [x] query 与 migration identity SQL 使用共享定义或 normalized parity test。
- [x] fresh v24、v23→v24、漂移 v23、index already exists、migration failure rollback tests 通过。
- [x] query-plan test 证明目标 all path 使用 v24 index；bounded/source/host plans 不被强制到
  更差路径。
- [x] 更新 ADR 0004 和 dashboard performance contract 的 v24 签名、错误/回滚矩阵、
  tests 与 wrong/correct；D1-only 时只记录 query contract，不伪造 v24。

## 5. API lifecycle and compatibility

- [x] `/api/sessions` payload、support/degraded shape、public 404、snapshot/export 和 Logs
  canonical drilldown tests 不回归。
- [x] 强制慢/中断 query 测试验证 3 秒 deadline、InterruptHandle、permit ownership、
  supervisor settled 和 diagnostics 不变。
- [x] 快速 sort/range 测试证明 stale response 仍被拒绝；不新增前端 cache 或请求。
- [x] limit 50 的响应仍 `<=128 KiB`，且 query count 不随 limit 线性增长。

## 6. Final performance evidence

- [x] 使用最终 binary 和未被候选实验污染的新代表副本运行完整 warm matrix。
- [x] all 三排序各 5/5 HTTP 200、非 degraded，p95 `<=400 ms`、payload `<=128 KiB`。
- [x] all source/model/project/host filters 达到同一预算；1d/7d/30d p95 回归 `<=10%`。
- [x] 运行 concurrency-2 与快速 range/sort 切换，确认无 permit leak、orphan 残留、busy/locked。
- [x] 若未跨真实重启，明确把 cold/first-touch 标为 `UNVERIFIED`。
- [x] 停止 task-owned server、确认端口释放，精确清理已验证 task-owned DB/WAL/SHM；保留
  sanitized benchmark/plan/size evidence。

## 7. Quality and closeout

- [x] focused tests 与 harness tests。
- [x] `rtk python scripts/ci-rust.py`。
- [x] `rtk just ci`。
- [x] `rtk git diff --check`。
- [x] `task.py validate 08-23-top-sessions-query-index-optimization`。
- [x] 使用 `trellis-check` 独立复核 PRD、exactness、query plan、迁移安全、性能统计、写放大、
  活动数据库边界与 cleanup。
- [x] 使用 `trellis-update-spec` 判断并更新已落地的 query/migration 契约。
- [x] 最终 diff 只含本任务代码、测试、必要 spec/ADR/docs 与 task evidence。
- [ ] 按仓库规范本地中文 emoji Conventional Commit，随后 archive/journal；不 push、不建 PR。

## Rollback points

1. Oracle + benchmark：测试/任务证据，可独立 revert。
2. D1 query-only：恢复原 `top_sessions.rs` loader，不涉及 schema。
3. D2 实验：仅 task-owned backup，拒绝时不进入产品代码。
4. v24 migration：发布前 revert migration/query/spec；已迁移数据库只能恢复 verified
   pre-v24 backup，不能手工降 version。
