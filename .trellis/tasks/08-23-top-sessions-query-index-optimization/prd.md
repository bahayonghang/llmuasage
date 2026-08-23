# 优化全历史会话排行查询与索引

## Goal

让 `/api/sessions?range=all` 在代表性大库上稳定返回可用的 Top Sessions，而不是在
Token/成本排序时触发 3 秒降级；同时完整保留 canonical session identity、三种排序、
活跃时长、筛选、稳定 tie-break 和 JSON 字段语义。

本任务优化查询与必要索引，不通过延长超时、近似 Top N、裁剪历史或缓存旧结果掩盖
问题。

## Background and confirmed facts

- 上一任务在只读在线备份的 `1,160,073,216` 字节数据库上确认：默认 `1d` 的 Token、
  时长、成本排序 p95 分别为 `101.15 ms`、`41.60 ms`、`96.04 ms`，载荷均小于
  `4 KiB`；但无界 Token/成本排序达到既有 3 秒 section timeout，时长排序单次约
  `2.5 s`。证据位于归档任务
  `08-23-usage-overview-session-composition-redesign/evidence/performance.md`。
- `/api/sessions` 复用 Behavior 的 3 秒监督边界并在失败时返回局部 degraded payload
  （`src/web/mod.rs:1081-1113`）；不能把提高 timeout 当作查询优化。
- `Top Sessions` 当前先按计算出的 canonical identity 聚合全表。Token/成本只保留前
  `limit` 个候选，但随后为每个候选单独再次读取时间序列；时长排序先产生所有 session
  候选，再第二次读取全部事件时间（`src/query/top_sessions.rs:80-164,168-204`）。
- canonical identity 不是单纯的 `session_id`：空 session 会回退到
  `source_path_hash`，Codex/Claude 还可能从 `event_key` 提取 session，最后才回退到完整
  event key（`src/query/top_sessions.rs:239-255`）。现有
  `idx_usage_event_session(source, session_id, event_at)` 不能覆盖全部 identity 语义。
- 2026-08-23 对真实 schema v23 数据库执行只读 `EXPLAIN QUERY PLAN`：
  - Token 聚合：`SCAN e` + 临时 B-tree GROUP BY + 临时 B-tree ORDER BY；
  - 全量时长时间序列：`SCAN e` + 临时 B-tree ORDER BY；
  - 单候选时间序列：扫描 `idx_usage_event_event_at`，仍逐行计算 canonical identity。
- `QueryFilter` 对 source、host、model、project 和日期使用同一 event filter
  （`src/query/filter.rs:60-160`）；优化路径必须保留所有组合，而不仅优化无筛选演示。
- schema 当前为 v23；项目迁移只能通过 versioned runner 追加，真实大库首次升级前要
  创建并验证独立 SQLite online backup（`docs/adr/0004-schema-version-migration-runner.md`）。

## Requirements

### R1 — Exact semantics are the oracle

- 优化前建立 legacy oracle，对相同 fixture/filter/sort/limit 序列化完整
  `Vec<TopSessionRow>`；候选实现必须逐字节一致。
- 保持三种排序：Token 按 `total_tokens`、时长按 30 分钟 gap cap 计算的
  `active_minutes`、成本按 `cost_usd`；并继续以 canonical id 升序稳定打破并列。
- 保持 `session_label`/`project_label` 的当前选择、同一 canonical session 的 source
  语义、首末事件、span、event count、output 含 reasoning 的既有 Top Sessions 契约。
- 覆盖真实 `session_id`、空白 session、`source_path_hash`、Codex/Claude event-key
  fallback 和最终 event-key fallback；禁止把 identity 简化为非等价列。
- source/model/project/host/since/until/timezone、limit `0/1/10/50/>50`、空库、零值、
  NULL label 和成本并列都必须保持一致。

### R2 — Remove repeated full-history work first

- 第一候选必须是 query-only D1：一次读取当前 filter 的事件投影，在 Rust 中按同一
  canonical identity 聚合三种指标、首末时间和 active gap，只维护有界 Top K；删除
  Token/成本的候选 N+1 时间查询和时长排序的重复全量读取。
- D1 必须复用单一 identity 定义和单一聚合器，不为三种排序复制查询/归约逻辑。
- D1 若已经通过全部正确性和性能门，则停止，不创建 schema migration 或新索引。

### R3 — Add an index only behind evidence

- 只有 D1 在代表性备份上仍未达到性能门时，才在隔离副本实验 D2：匹配完整 canonical
  identity 表达式与 `event_at` 顺序的候选表达式索引，并记录 query plan、构建时长、
  数据库体积增量和写入回归。
- D2 被采用时，追加真实 schema v24 migration；fresh v24、v23→v24、已存在索引和
  漂移 v23 路径必须得到相同索引定义和 schema version。
- expression index 的 SQL 与查询 identity 必须由共享定义或显式 parity test 防漂移；
  `EXPLAIN QUERY PLAN` 必须断言目标 all-range 路径不再使用临时 identity 排序。
- 候选索引若导致固定 `SyncRunWriter` 多轮中位数回归超过 `10%`，或索引体积超过代表
  数据库的 `15%`，不得进入产品 migration；回到设计评审，不静默放宽门槛。
- 如果 query-only + bounded expression index 仍不能达标，停止并提交证据；持久化
  session rollup、缓存或 parser/write-path 扩展需要新的规划批准，不属于本任务。

### R4 — Performance and lifecycle validation

- 新增可重复的 Top Sessions benchmark，固定为一次 warm-up 后每种排序至少 5 次，
  同时记录 wall time、HTTP status、support level、payload bytes 和服务端 query timing。
- 代表性矩阵至少包含 `1d/7d/30d/all × tokens/duration/cost`，以及 all-range 的
  source、model、project、host filter；不得只测最快排序。
- 所有 warm `all` 样本必须 HTTP 200、support 非 degraded、p95 `<=400 ms`、payload
  `<=128 KiB`；任何样本不得达到 3 秒 hard deadline。
- `1d/7d/30d` 不得相对当前基线回归超过 `10%`；无法稳定控制文件缓存时，只报告
  warm 协议，不声称 cold/first-touch 已验证。
- 快速排序切换和全局 range 切换继续遵守 AbortSignal、query permit、supervisor
  settlement 和 latest-wins；不得留下 orphan 或让旧结果覆盖新筛选。

### R5 — Safe migration and operational boundary

- 所有性能实验和候选索引构建只使用由真实库 `mode=ro` 在线备份得到的 task-owned
  副本；不得在规划或普通实现验证中迁移活动用户数据库。
- 若采用 v24，记录 backup integrity、pre/post schema、index SQL、build time、
  page/file delta、`PRAGMA integrity_check` 和旧二进制无法打开新版 schema 的回滚边界。
- 迁移活动用户数据库必须另获明确授权；回滚是恢复已验证备份，不是删除索引或手工
  下调 schema version。
- 临时 server 只监听 loopback；验收后停止进程、释放端口并只清理已验证的 task-owned
  精确路径。

## Acceptance Criteria

- [x] AC1：legacy 与候选实现对 identity fallback、三排序、并列、所有 filter、limit、
  空库和异常值 fixture 的完整 JSON 逐字节一致。
- [x] AC2：query-only D1 将同一请求的 event-table 主扫描收敛为一次，删除候选 N+1 和
  duration 的第二次全量时间读取；测试能阻止旧查询形态回归。
- [x] AC3：若 D1 达标，任务不产生 migration；若 D1 不达标，D2 证据明确记录进入
  索引候选的机械原因。
- [x] AC4：若采用 D2，fresh/v23-upgrade/drifted/idempotent migration tests 和
  query-plan assertion 全部通过，schema 仅推进到 v24。
- [x] AC5：若采用 D2，代表性索引体积增量 `<=15%`，固定同步写入基准中位数回归
  `<=10%`；否则候选被拒绝且不写入产品 migration。
- [x] AC6：代表性备份上 `all × tokens/duration/cost` 各 5 个 warm 样本均为 HTTP 200、
  非 degraded，p95 `<=400 ms`、payload `<=128 KiB`，且无 3 秒 timeout。
- [x] AC7：`1d/7d/30d` 三排序相对任务基线无超过 `10%` 的 p95 回归；all-range 的
  source/model/project/host filter 也满足同一响应预算。
- [x] AC8：快速 sort/range 切换保持 latest-wins；timeout/cancel regression 证明 permit
  和 supervisor 生命周期未被绕过。
- [x] AC9：若采用 migration，只在隔离 v23 备份上完成升级、integrity、plan、大小和
  rollback 证据；活动数据库保持 byte size/mtime/schema 不变。
- [x] AC10：更新 dashboard performance code-spec 与 ADR 0004（仅在 schema/query 决策
  落地时），双语 dashboard 文档只在可观察行为或约束变化时更新。
- [x] AC11：focused Rust tests、benchmark harness tests、`git diff --check`、
  `python scripts/ci-rust.py` 与 `just ci` 通过。
- [x] AC12：`trellis-check` 复核精确性、query plan、迁移安全、写放大、真实备份性能和
  清理证据；未取得的 cold/物理证据明确标记 `UNVERIFIED`。

## Out of Scope

- 延长 `WEB_BEHAVIOR_API_TIMEOUT`、增加并发 permit 或用前端缓存隐藏服务端慢查询。
- 近似 Top N、抽样、截断历史、用 rough span 代替 active duration，或改变 30 分钟 gap cap。
- 新增持久化 session rollup/bucket、后台物化任务、parser 字段或同步协议。
- 修改 Top Sessions UI、日志下钻、CSV、token/cost accounting 或数据库中的用户数据。
- 未经单独授权迁移活动用户数据库、重启系统、推送远程或创建 PR。
