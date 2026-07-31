# Design: 重启清缓存的 Activity 首次触库门（v3）

## 边界

本阶段先修改 `src/web/mod.rs` 中 PERF-002 的后台收尾与 `/api/diagnostics` additive 观测字段，再更新对应性能契约和任务级 harness。仍不修改查询 reducer、schema、真实数据库、Behavior timeout/concurrency 或前端。D1/D2 设计保留为门通过后的候选，不在本阶段实现。

## PERF-002 前置生命周期

`WebState` 新增进程内 `DashboardQuerySupervisor`，不复用 sync `JobRegistry`，也不写 SQLite。每条 dashboard query 在获取 permit 后分配单调 `query_id` 并登记 inflight；permit 和 work guard 一起移入 `spawn_blocking` closure，只有 closure 真正返回时才释放并减少 inflight。

请求超时路径保持硬截止：

1. 设置 cancellation flag，并在 handle 已发布时调用 SQLite interrupt。
2. 记录带 `query_id`、`section`、wait/query/cancelled 的 timeout timing。
3. 将 blocking `JoinHandle` 移交 supervisor；请求立即返回，不 await。
4. supervisor 用独立 Tokio task await 该 handle。结束后减少 orphan count、记录 duration/join outcome，并输出带相同 `query_id` 的 `Dashboard query orphan settled` debug 事件。

supervisor snapshot 提供 `dashboard_query_inflight`、`timed_out_tasks`、`orphaned_tasks` 和最近一次 `orphan_duration_ms`。`/api/diagnostics` handler 在取得既有缓存 payload 后再附加 snapshot；这些实时值不得写进 `DiagnosticsCache`，Dashboard core/interactive 中的 archive payload 也保持原形。

正常完成路径仍在请求 future 中 await 原 JoinHandle，记录现有 completed timing，并由 work guard 收敛 inflight。进程关闭时状态随 `WebState` 消失，符合本地内存态运行模型。

## 两阶段协议

### A. Prepare（重启前）

新增任务级 harness `research/profile_activity_first_touch.py`，提供独立的 `prepare` 与 `run` 子命令。

`prepare`：

1. 以 `mode=ro` + `query_only=ON` 打开真实数据库，并用 SQLite online backup 创建第一份 v18 快照。
2. 从该一致性快照创建另外四份独立文件副本，目录固定为 `target/tmp/activity-first-touch-v3/sample-01..05/`。
3. 在重启前对五份文件执行 `quick_check`、schema/count 检查和 SHA-256；记录大小、mtime、哈希，不记录模型、路径或业务行。
4. 记录 Windows boot identity、Git HEAD、二进制版本和 SHA-256，写入 task-owned manifest。
5. 输出唯一的 post-reboot 命令。准备过程不启动 server，不修改真实数据库。

manifest 是阶段间契约。`run` 不接受任意扫描目录，只消费 manifest 中列出的五个精确文件。

### B. Run（重启后）

`run` 按以下顺序执行：

1. 读取 manifest，校验当前 boot identity 与 prepare 不同。
2. 校验二进制 SHA-256；对数据库只检查路径、大小和 mtime，不读取内容。
3. 对每个 sample 目录启动全新 server。server 的正常 bootstrap 可触碰 schema/meta 页，这是实际首次 Activity 请求前的产品生命周期，必须计入方法描述。
4. 发起一次 `/api/activity?range=all`，收集客户端 wall time与服务端 Activity timing。若该请求 timeout/cancelled，按 timing 中的 `query_id` 等待匹配的 supervisor settled 事件；只有 settled 后才在同一 server 发起暖态配对请求。
5. 停止 server 并确认其监听端口释放，再处理下一份快照。
6. 所有 first-touch 样本完成后，写 sanitized JSON；再生成 `research/first-touch-validation.md`，不得在首触前做数据库内容验证。

每份数据库经过 server bootstrap 后视为已消费，不得再声称是冷样本或用其补跑。

## Boot identity

Windows 下用 `GetTickCount64` 推导当前 boot epoch，并允许少量计时误差。prepare 写入该 identity；run 在同一 boot 范围内必须 hard-fail。该 guard 防止误把普通进程重启再次标成真实冷启动。

## 观测与隐私

- 客户端：wall time、HTTP status、support level、degraded/timeout 布尔值。
- 服务端：`section=activity` 对应的 `semaphore_wait_ms`、`query_ms`、`cancelled`。
- 生命周期：timeout timing 与 settled 事件以 `query_id` 关联；`query_id` 仅用于 harness 内部匹配，不写入最终业务维度结果。
- 产物禁止包含 response breakdown、模型、项目、路径、session、prompt 或完整真实数据库路径。
- server stdout/stderr 留在 gitignored 的 task temp 目录；任务文档只保存汇总与必要的安全元数据。

## Gate

`research/first-touch-validation.md` 必须机械计算 PRD R2：cold median >3s、五个 warm 全部 <3s、等待/锁不是主因。结论只有 `GO D1` 或 `NO-GO D1/D2`，不允许以单个异常样本越过门。

## Step 1E 后的修订确认门

Step 1E 将配对第二次请求重新分类为“前一次 hard timeout 后的部分暖态”。因此保留
旧门结果但不重解释原始样本；新增 task-owned confirmation harness，从保留的
`target/tmp/activity-baseline-step1-run2/snapshot/llmusage.db` 创建三份新副本。

每份副本必须通过精确的 `robocopy /J /R:0 /W:0` 文件复制生成，置于独立 runtime，
并启动一个全新当前 debug binary server。harness 顺序发出最多五次
`GET /api/activity?range=all`：

1. 收集 sanitized wall/status/support/degraded/timeout 与对应 Activity timing。
2. timeout/cancelled 时按 `query_id` 等待同一 server log 中匹配的
   `Dashboard query orphan settled`，不得用 sleep 替代。
3. 观察到至少一次初始 timeout 后，检测连续两次 `<3000 ms`、normalized、
   non-degraded 请求；满足后可提前停止该副本。
4. 停止 server、验证进程退出和端口释放，再仅删除该 runtime 内精确命名的
   `llmusage.db`、WAL、SHM。日志与 sanitized result 保留。

机械 gate 要求三个副本全部通过、所有 permit wait 为 0、无 SQLite busy/locked
日志、所有 cleanup 成功。harness 必须拒绝 source/runtime 边界异常，结果 privacy
scan 禁止响应 rows 和用户维度。五份 `activity-first-touch-v3` 样本不参与本门。

## 门通过后的候选

### D1：覆盖索引

- 只有修订确认门通过后，才从同一 Step 1A source 用 `/J` 创建另一个精确 D1
  runtime；候选 SQL 固定为
  `CREATE INDEX ... ON usage_event(event_key, cost_with_cache_usd)`。
- 索引前后记录 `PRAGMA user_version`、`EXPLAIN QUERY PLAN`、`page_count`、
  `page_size`、database/WAL size 和 index build wall time；`user_version` 必须不变。
- 索引构建完成后关闭写连接并启动全新当前 debug server，按与确认门相同的顺序
  Activity HTTP 方法采集最多五次 cold/warm timing，每次 timeout 后等待匹配 settled。
- 写放大用隔离副本上的 savepoint/transaction 观测代表性 insert/update 前后的
  page/freelist/WAL 或文件大小变化；测试数据只从既有行复制非用户维度结构且最终
  rollback，不持久化业务值。若无法形成可靠证据，必须明确标为未验证。
- 保留 sanitized SQL/plan/size/timing 证据后，仅删除精确 D1 database/WAL/SHM；
  不删除日志，不触碰 Step 1A source 或 first-touch 快照。
- `usage_turn` 候选索引必须经 query plan、页数和写放大 profile，避免与 v18 索引重复。
- 仅在隔离备份库实验；若采用则按 ADR 0004 追加真实 v19 migration。

### D2：bounded 成本读取

仅在 D1 不足时，对 bounded 请求先收集 filtered turn/event key，再分批读取成本；`all` 与 Rust 累加顺序保持现行语义。任何改变浮点累加顺序的 SQL 聚合都不允许。

## Production D1（已批准）

schema v19 追加单一 migration `optimize_activity_event_cost_projection`。migration 在
`usage_event` 存在时只执行：

```sql
CREATE INDEX IF NOT EXISTS idx_usage_event_activity_cost
    ON usage_event(event_key, cost_with_cache_usd);
```

不改 Activity SQL 文本或 reducer。v18 回归用 `MIGRATIONS[..18]` 构造并验证 v18；
v19 回归分别从 v18 形状升级及从空库执行全部 migrations，断言 schema version、索引
列顺序和 covering-index query plan 一致。

精确性测试使用最小 nullable-cost SQLite fixture，先在无 D1 索引状态序列化现行
Activity 与 legacy oracle，再创建与 v19 完全相同的索引重新序列化。对 default、
source/model/project/date 和 no-data filters 逐字节比较；fixture 显式包含 missing event、
NULL cost、`edit_turns=0` 及 category 排序并列。

同步写放大用 ignored、显式运行的单线程 Rust acceptance benchmark。它为 baseline 和
indexed 两侧建立独立临时 Store，使用相同固定 synthetic `SyncShard` 序列，计时同一
`SyncRunWriter` 端到端提交；多轮交替先后顺序并比较中位数。若
`indexed_median / baseline_median > 1.10`，测试 hard-fail。计时测试不进入并行默认 CI，
但必须在本任务验收中显式执行并保存命令和结果。

所有自动化门通过后，以最终 debug binary 把五份新隔离副本推进到 v19，验证版本、
完整性、大小和 binary hash，记录 manifest 后停止。该准备过程不得消费任何 Activity
请求；下一步仍是用户手工重启。

## 回滚与清理

- Prepare 前无新快照；Prepare 后可按 manifest 中五个精确目录清理，不能递归删除宽泛的 `target/tmp`。
- Run 失败时保留未消费样本；已启动过 server 的样本标为 consumed，不得重用。
- confirmation/D1 runtime 只允许按已解析且验证位于各自 task-owned work directory
  下的精确路径清理；保留 Step 1A source、五份 first-touch 快照和 sanitized logs。
- R0 回滚边界是 `DashboardQuerySupervisor`、Web diagnostics additive fields、对应 focused tests/spec 和 harness settled 等待；不得波及 query reducer、schema 或 timeout 常量。
