# PRD: 修复行为分析 Activity 首次触库超时（v3）

## 目标

先修复会污染冷/暖配对证据的 PERF-002 后台收尾缺口，再用可复核的重启后首次触库证据判定 Activity 超时是否由冷 I/O 主导；只有该因果门成立，才实施最小的 D1/D2 优化。最终仍须满足既有 Behavior 性能与精确性契约，不以放宽超时或近似结果换取通过。

## 已确认事实

- 用户观察到 Activity 卡片降级并显示 `dashboard query exceeded 3000 ms timeout`，同页 Tools 正常。
- 07-28 的 v18 实现使用全量 `usage_event(event_key, cost_with_cache_usd)` 投影、过滤后的 `usage_turn` 投影和 Rust 顺序归约；暖态结果满足既有预算，并保留精确浮点累加顺序。
- 07-29 早期只读复测在一份此前未预热的 v17 备份上测得首轮约 5.866 秒，但该次证据没有完整的 HTTP 生命周期、许可等待和重复样本矩阵。
- `research/baseline.md` 的 Step 1 代理冷态矩阵包含 40 轮、60 个请求：全部 HTTP 200/normalized，许可等待为 0；`all` 代理冷态为 1.14-1.42 秒，暖态为 1.12-1.56 秒。由于复制过程可能填充 Windows 页缓存，该矩阵既没有复现超时，也不能排除真实未缓存 I/O。
- 暖态直接 profile 中，全量 event 成本投影中位数约 574-598 毫秒，是 `1d` Activity 的主要固定成本；这只证明工作量集中处，不证明冷 I/O 是超时根因。
- 旧的 `legacy_activity_breakdown` JOIN 聚合已被否决：它更慢，并可能改变 SQL `SUM` 的浮点累加顺序。
- 现行 PERF-002 在 Web 查询超时后中断并 `drop` blocking `JoinHandle`，使请求能按硬截止线返回、permit 由后台任务继续持有；但没有 supervisor 或完成信号。同进程 warm 请求因此可能在上一条超时查询真正结束前启动，且现行性能 spec 仍错误写成请求路径会 await abandoned work。
- 用户已选择扩大本任务范围：先补齐 PERF-002 生命周期和契约，再继续首次触库取证；不得以恢复请求路径阻塞等待来实现。

## 范围内要求

### R0：PERF-002 硬截止与后台收尾前置门

- 保留配置 timeout 作为响应延迟硬边界：超时请求中断可用的 SQLite handle 后立即返回 structured timeout/degraded 结果，不在请求 future 中 await blocking task。
- blocking task 自持 query permit 直到真正结束；超时后由 `DashboardQuerySupervisor` 接管 `JoinHandle` 并在后台 await，不能裸 `drop`。
- 每条 Web dashboard query 获得进程内单调 `query_id`。超时日志与后台 settled 日志必须带同一 `query_id`、`section`；settled 日志记录 orphan duration 和 join outcome，不含用户维度。
- `/api/diagnostics` 在既有诊断字段之外追加实时 `dashboard_query_inflight`、`timed_out_tasks`、`orphaned_tasks`、`orphan_duration_ms`；不得把这些实时值冻结进 30 秒文件诊断缓存，也不改变 Dashboard 内嵌 archive payload。
- focused regression 必须证明：忽略 interrupt 的 blocking closure 仍在 timeout +100 ms 内返回；permit 在 closure 真正结束前不释放；supervisor 最终归零并记录 duration；正常快速查询不回归。
- 首触 harness 若观察到 first-touch timeout/cancel 请求，必须等待同一 `query_id` 的 settled 事件后才发配对 warm；缺失或超时等待应 hard-fail 并保留 consumed 语义。

### R1：重启清缓存的首次触库证据

- 重启前从真实数据库创建五份隔离的 v18 SQLite 快照，保存在 gitignored 的 `target/tmp/` 下；真实数据库仅通过 SQLite online backup 读取，不 bootstrap、不迁移、不写入。
- 准备阶段记录快照大小、完整性、内容哈希、当前 boot identity 和被测二进制哈希。快照准备完成后，在重启前不再将其用于查询。
- 重启由用户在明确检查点手工执行；脚本不得调用系统重启，也不得使用特权的全局 standby/file-cache eviction。
- 重启后 runner 必须拒绝与准备阶段相同的 boot identity。首次请求前只允许读取 manifest、二进制和快照文件元数据，不得对快照执行哈希、`quick_check`、query plan 或其他内容读取。
- 每份快照启动一个全新 debug server，先执行一次 `range=all` Activity 请求作为 reboot-cleared first-touch 样本，再在同一进程中执行一次配对暖态请求；每份快照只承担一组冷/暖配对。
- 记录每组 wall time、HTTP/support、`semaphore_wait_ms`、`query_ms`、cancelled 和 degraded；不得持久化响应明细、模型、项目、路径、session 或 prompt 内容。

### R2：因果门

只有同时满足以下条件，才把“冷 I/O 主导”判为成立并进入 D1：

- 五个 first-touch `all` 样本的中位数大于 3 秒；
- 五个配对暖态 `all` 样本全部小于 3 秒；
- first-touch 超时样本的许可等待不是主要耗时来源，且没有独立的 SQLite busy/锁错误。

若任一条件不成立，停止 D1/D2，依据 timing/log 重新分类根因；除本任务已批准的 R0 生命周期修复外，不实施查询、schema 或缓存优化。

Step 1E 已证明硬截止会造成不完整暖化，因此用户批准以下替代性确认门。旧的
five-pair R2 结论保留为历史证据，但不再单独决定是否进入隔离 D1 实验：

- 从保留的 Step 1A 代表快照用 `robocopy /J` 创建三份全新、互相独立的副本；
  不读取或重用五份已经消费的 first-touch 快照。
- 每份副本由一个全新的当前 debug binary server 顺序发起最多五次
  `GET /api/activity?range=all`。每次 timeout/cancelled 后必须等待匹配
  `query_id` 的 orphan-settled 事件，之后才能继续。
- 单份副本只有在五次以内先出现至少一次初始 timeout，随后出现连续两次
  `<3000 ms`、normalized 且 non-degraded 的请求时才通过。
- 三份副本必须全部通过；所有 `semaphore_wait_ms` 必须为 0（因此不是主因），
  不得出现 SQLite busy/locked 信号，且 server/process/port/database-copy 清理必须成功。
- 持久化结果仅含 sanitized timing、状态、permit/lock/cleanup 证据；禁止响应行和
  model/project/path/session/prompt 等用户维度。

只有该确认门机械通过，才授权执行一次隔离 D1 实验；它不授权 production
migration、查询改写、cache 或 timeout 调整。

### R3：门成立后的最小优化

- 优先在隔离备份库验证 D1 紧凑覆盖索引；只有 D1 不足时才评估 D2 bounded 成本读取。
- Activity 序列化结果必须与现行 Rust reducer 逐字节一致；`legacy_activity_breakdown` 仅保留为测试参照，不恢复为生产路径。
- 不改变 Behavior 并发数、超时、降级 payload 或前端；许可与取消语义按 R0 明确为“请求硬截止、后台 supervised settle”。`/api/diagnostics` 只允许 R0 所列的 additive 字段。
- D1 只在新建的精确路径副本中创建
  `usage_event(event_key, cost_with_cache_usd)` 覆盖索引。记录 SQL、schema version
  未变、前后 query plan、database/page size 增量、build time、当前 binary 下顺序
  cold/warm HTTP timing，以及安全可行的代表性 insert/update 写放大证据。
- D1 必须保持现行 reducer 与输出语义，不创建 migration，也不修改 production
  source/query/cache/timeout。保留 sanitized 证据后只清理该精确 D1 副本，并停在
  下一次用户实现门。

隔离 D1 已通过且用户已批准 production D1。产品实现边界固定为：

- 追加 schema v19，唯一 schema 变化是创建
  `idx_usage_event_activity_cost`，精确列顺序为
  `usage_event(event_key, cost_with_cache_usd)`。
- 不修改 Activity reducer、查询/filter 语义、缓存、并发、前端、PERF-002 生命周期
  或 3 秒 Behavior deadline；D1 失败时也不自动进入 D2。
- v19 必须同时覆盖 schema-v18 升级和 fresh bootstrap；v18 migration 测试必须在
  `MIGRATIONS[..18]` 内隔离，不能因 latest schema 前进而把 v19 混入 v18 证据。
- Activity 在建索引前后必须逐字节序列化一致，并覆盖 missing event、NULL cost、
  `edit_turns=0`、category 排序并列和 source/model/project/date/no-data filter。
- 固定合成输入的端到端 sync 写入基准必须比较无索引与有索引两种 schema；若有索引
  的中位耗时回归超过 `10%`，production D1 阻断并回滚，不进入 D2。
- 自动化门通过后，用最终 binary 准备五份未消费的 v19 快照并停止；重启后样本不得在
  本轮提前读取或消费。

## 范围外

- 不自动或远程重启 Windows。
- 不使用 RAMMap/EmptyStandbyList 等特权全局缓存清理。
- 不在因果门前创建 schema v19、索引、查询重写或结果缓存。
- 不修改 Tools、Optimize、Compare；若其独立复现冷态问题，另行规划。
- 不把提高 `WEB_BEHAVIOR_API_TIMEOUT` / `WEB_API_TIMEOUT` 作为修复。
- 不把 PERF-002 改回请求路径中 await blocking task，也不新增持久化 supervisor 状态。

## 验收标准

| # | 标准 | 证据 |
|---|---|---|
| 0 | PERF-002 保留硬响应边界，orphan task 由 supervisor 收尾、permit 生命周期和实时指标正确；harness 在 settled 前不发 warm | focused Rust tests + harness tests + structured logs |
| 1 | 五份隔离快照在重启前完成完整性、大小和哈希验证，真实数据库大小与修改时间不变 | prepare manifest + 前后 stat |
| 2 | runner 在同一 boot identity 下拒绝运行，重启后在任何快照内容预读前执行五组 first-touch/暖态配对 | boot guard + runner 日志 |
| 3 | 每组记录 wall/query/wait、HTTP/support、cancelled/degraded，输出不含用户维度或响应明细 | sanitized results + privacy scan |
| 4 | 严格按 R2 判定，未通过时除已批准的 R0 生命周期与契约修复外，没有查询、migration 或 schema 改动 | `research/first-touch-validation.md` + git diff |
| 4a | 修订确认门的三份 `/J` 副本全部呈现“初始 timeout 后连续两次 normalized <3s”，permit/lock/cleanup 条件同时成立 | task-owned harness + sanitized confirmation results |
| 4b | 仅在 4a 通过后于独立副本完成 D1 index/plan/size/build/HTTP/write 观测；schema version 和 production code 不变 | sanitized D1 results + research validation |
| 4c | production v19 只追加精确 Activity covering index；v18 升级与 fresh schema 均得到相同索引 | migration tests + schema/query-plan assertions |
| 4d | Activity 指定边界在索引前后逐字节一致；合成端到端 sync 中位耗时回归不超过 10% | focused equivalence test + explicit single-thread benchmark |
| 5 | 若门通过，最终 `1d` 三样本中位数 <1 秒、`all` 每样本 <3 秒，且序列化逐字节一致 | 回归测试 + 最终冷/暖矩阵 |
| 6 | 若进入产品实现，`cargo test --all-features -- --test-threads=1` 与 `just ci` 通过 | 命令输出 |

## 风险与约束

- 五份快照临时占用约 5.8 GB；完成取证后再按精确路径清理。
- 杀毒软件或索引器可能在重启后提前读取快照。runner 会记录该限制，但不能从用户态完全证明第三方未触碰；因此快照应位于任务专用目录，并在重启后尽快运行。
- 五个独立文件共享同一次重启清缓存，而不是五次重启。它们具有不同文件身份和磁盘区段，强于复制后立即测量，但仍须在证据文档中如实描述。
- supervisor settled 信号依赖 server debug log；harness 必须按 `query_id` 匹配，不能用固定 sleep 或“看到 timeout 响应”代替真实完成。
