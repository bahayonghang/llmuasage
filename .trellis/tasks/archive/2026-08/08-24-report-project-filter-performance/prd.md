# 报表聚合复用与项目过滤性能优化

## Goal

把日/周/月、overall/source/host 报表收敛为一次 canonical period aggregate 管线；保留 `--project` 对 hash/label/ref 的大小写不敏感包含匹配，但让 token/model/cost totals 走 project-resolved bucket read model，仅用窄 SQL 计算无法由 bucket 表达的精确 conversation count。

## User Value

大历史库上的 `daily/weekly/monthly --project` 与 unified/focused 报表不再为每个事件构造完整 Rust 对象或重复扫描同一范围，输出仍与原命令逐字段一致。

## Confirmed Facts

- `src/query/reports.rs:548-1112` 分别实现 daily/monthly/weekly 与 by-source/by-host 映射，分支结构高度平行。
- `load_unified_report`（1266-1337）先调用 overall loader，再调用 by-source loader；无 project 时重复 bucket read，有 project 时重复 event read。
- `visit_events_filtered`（2047 起）只下推 source/host/date，project 在 `project_matches`（2288 起）中逐 event 对 hash/label/ref 小写化包含匹配。
- `usage_bucket_30m` 持有 project_hash/label/ref、token/model/cost/event_count；`project_dim` 由 writer 更新并在按源 reset 时保留。
- bucket 不持 session distinct，因此 project fast path 不能把 `conversation_count` 近似为 bucket/event count。
- 本轮 100k backlog semantic test 通过且 test body 4.23 s；该值包含 seed，不是隔离 query baseline。

## Requirements

- R1：新增 seed 与 query timing 分离的 100k/500k synthetic harness，记录 period、project/no-project、overall/unified/source/host 的 statement count、query plan、wall p50/p95 和 returned rows。
- R2：项目 selector 保留 trim 后大小写不敏感 `contains` 语义，匹配 project_hash/project_label/project_ref；覆盖空值、未知值、同标签多 hash、Unicode、远端/历史 bucket 和 stale `project_dim`。
- R3：selector 从 `project_dim` 与当前 bucket project fields 的 union 解析 hash，实际 totals 必须受 source/host/date/project hash 共同过滤；stale dimension 不得产生虚假 rows。
- R4：project-filtered token/model/cost/notes 使用 bucket aggregate；conversation count 使用 SQL 侧 `DISTINCT`/`GROUP BY` 的窄 event projection，完整 `EventRow` 不得进入 period totals 路径。
- R5：overall/source/host 从同一 aggregate bundle 派生；同一次 unified period load 对同一 read model 最多执行一次 totals aggregate和一次必要的 conversation query，不分别重扫 overall/by-source。
- R6：日/周/月 key、timezone/DST、order、breakdown、authoritative totals、visible totals、unpriced/reason notes、registered-source ordering 与 host label 语义不变。
- R7：保留所有 public report types/loaders、CLI JSON camelCase/key order、text表格、focused/unified `--sections/--by-agent/--no-cost` 契约。
- R8：只有 query plan + read/write benchmark 证明收益且迁移成本可接受时才新增 index；不得用缓存、进程预热或阈值放宽通过性能门。

## Acceptance Criteria

- [x] A1/R1：before/after harness 把 seed 排除在 timing 外，五次预热后样本保存 statement count、EQP、p50/p95 和 rows；100k 与 500k 两档可自动运行。
- [x] A2/R2,R3：table-driven fixture 证明所有 fuzzy/project/history/stale-dimension case 与旧 event matcher 返回完全相同的 project hash/period rows。
- [x] A3/R4：project totals 的 SQL 只读 bucket；conversation SQL 在 SQLite 内聚合并只返回 period/source/host/count，不 materialize token/model/project strings 为 Rust `EventRow`。
- [x] A4/R5：trace 证明每个 daily/weekly/monthly unified load 最多一条 bucket aggregate + 一条必要 conversation aggregate；overall/source totals 由同一 bundle 派生。
- [x] A5/R6,R7：现有 report query/CLI 逐字段、JSON key/order、text golden、timezone/DST、host/source/focused/unified tests 全绿。
- [x] A6/R8：100k project-filtered unified p95 相对 baseline 至少下降 50% 且 ≤400 ms；500k p95 ≤400 ms；无 project 路径 p95/statement count 不回归超过 10%。
- [x] A7/R8：如新增 index，fresh/upgrade/idempotent/rebuild、DB size、migration time、sync write p95 均记录，写入回归 ≤10%；不满足则撤销 index。
- [x] A8：代表性临时备份上的 daily/weekly/monthly project/no-project p95 均 ≤400 ms 且 payload 等价；没有明确授权/副本则保持 `UNVERIFIED`，不能用 synthetic 替代归档门。
- [x] A9：report/public API focused tests、fmt、严格 clippy、serial tests、rustdoc、docs 和 `just ci` 通过。

## Out of Scope

- 改变 fuzzy project 产品语义或把参数改成只接受 hash。
- session report/blocks 算法、Dashboard explorer、UI 重设计或全 query 模块拆分。
- 无证据新增 projection/index、缓存或后台预聚合。

## Key Decisions

- bucket 负责 totals，narrow event aggregate 负责 exact conversation count；二者不得互相近似。
- unified overall/source/host 是同一 bundle 的 projection，不是三套 loader 的重复计算。
- `project_dim` 是解析候选之一而非结果真值；最终 rows 必须由当前 filtered bucket/event 事实决定。
