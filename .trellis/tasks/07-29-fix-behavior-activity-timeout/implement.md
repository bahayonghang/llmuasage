# Implement: Activity 首次触库因果门（v3）

## 已完成

- [x] Step 1A copy-backed-cold 矩阵：40 轮/60 请求，未复现超时；产出 `research/baseline.md` 和 `profile_activity_baseline.py`。
- [x] 用户选择推荐路线：不越过证据门，改为重启清缓存的首次触库验证。
- [x] Step 1B 独立检查发现 PERF-002 detached query 可与 warm 重叠；用户选择先修产品生命周期与契约，再继续取证。

## Step 1B：实现两阶段 harness `[gate]`

- [x] 实现 `research/profile_activity_first_touch.py` 的 `prepare` / `run` 子命令及 boot identity guard。
- [x] 添加 focused tests：同 boot 拒绝、manifest tamper/缺文件拒绝、结果 gate 计算、隐私字段约束、server 清理。
- [x] 编写 `research/first-touch-runbook.md`，只提供精确路径命令，不自动重启。
- [x] `python -B ... --help`、focused tests、`git diff --check` 通过。

## Step 1B.5：PERF-002 supervisor 前置门 `[gate]`

- [x] 先添加 red-capable Rust regression：忽略 interrupt 的 blocking closure 在 timeout +100 ms 内返回，但 permit/inflight 保持到 closure 真正结束；supervisor 最终 settled。
- [x] 在 `src/web/mod.rs` 实现 query ID、inflight work guard 与 `DashboardQuerySupervisor`；三条超时分支不得裸 `drop(task)`，也不得在请求路径 await。
- [x] `/api/diagnostics` additive 暴露实时 `dashboard_query_inflight`、`timed_out_tasks`、`orphaned_tasks`、`orphan_duration_ms`，保持文件诊断 cache 和 Dashboard archive payload 不变。
- [x] 更新首触 harness：timeout/cancelled first-touch 必须等待匹配 `query_id` 的 settled 日志后才发 warm；增加缺失、错 ID、等待超时与正常快速路径测试。
- [x] 运行 focused Rust/Python tests、`ruff`、`pyright`、`cargo fmt --check`、`cargo clippy`；完成 2.2 对抗检查并修复发现。
- [x] 用 `trellis-update-spec` 把 dashboard performance contract 修正为硬响应截止 + supervised background settle，删除“请求路径 await abandoned work”的漂移表述。

## Step 1C：重启前准备 `[user checkpoint]`

- [x] 确认没有 task-owned `llmusage` server/sync 进程。
- [x] 运行 `prepare`：在 `target/tmp/activity-first-touch-v3/` 创建五份隔离快照和 manifest。
- [x] 验证五份快照 quick_check、schema/count、大小与 SHA-256一致；记录真实数据库前后 size/mtime 不变。
- [x] 报告约 5.8 GB 临时占用、manifest 与 post-reboot 命令；停在人工重启检查点。

## Step 1D：重启后 first-touch `[gate]`

- [x] 在不读取快照内容的前提下运行 manifest 指定命令；boot guard 必须证明已跨重启。
- [x] 对五份快照各采一组 first-touch `all` + 同进程 warm `all`。
- [x] 确认每个 task-owned server 已停止、端口已释放。
- [x] 机械计算 PRD R2，写 `research/first-touch-validation.md`：first-touch 中位数 `3023.90 ms`，但五个 warm 均不低于 `3000 ms`，结论为 `NO-GO D1/D2`。
- [x] 用户确认 `NO-GO D1/D2`；回诊断，不进入 D1/D2。

## Step 1E：配对 warm 超时回诊断 `[gate]`

- [x] 从十个 query-ID settled 日志确认 orphan 在 interrupt 后 13-37 ms 内退出；排除配对请求与旧查询重叠。
- [x] 在 Step 1A 代表副本上只读分段：首次完整 event 投影 4982.40 ms，首次完整 all-range turn 投影 2134.97 ms；完整读取后均降至亚秒级。
- [x] 两份 `/J` 无缓冲隔离副本均复现“前两次 Activity `all` 超时、第三次 normalized”，其后稳定低于 1.21 秒。
- [x] 写 `research/warm-timeout-diagnosis.md`：根因分类为 hard-timeout-induced incomplete warming；配对第二次请求不是完整暖态。
- [x] 保持机械结论 `NO-GO D1/D2`；未改生产代码、索引、migration、cache 或 query/reducer。
- [x] 用户批准推荐的修订验收协议与方案设计：三份独立 `/J` 副本必须在五次内
  从至少一次初始 timeout 过渡到连续两次 normalized/non-degraded `<3000 ms`。

## Step 1F：三副本修订确认门 `[gate]`

- [x] 把修订协议写入 PRD/design/implement，并实现 task-owned mechanical harness/tests。
- [x] 从保留的 Step 1A 代表快照创建三份新 `/J` 副本；不读取或重用五份
  first-touch 快照。
- [x] 每份副本最多五次顺序 Activity `all`；每次 timeout/cancelled 后等待匹配
  query-ID settled，机械判定初始 timeout 后连续两次 normalized `<3000 ms`。
- [x] 验证三个副本全部通过、permit wait 全为 0、无 SQLite busy/locked、server/port/
  精确 database-copy cleanup 全部成功，并保存 sanitized evidence。

结果：三份副本均为两次 timeout 后连续两次 normalized；第三次为
`1595.34-1662.35 ms`，第四次为 `940.09-977.63 ms`。所有 wait 为 0，无
busy/locked，cleanup 与 privacy scan 通过，机械结论 `GO D1`。

## Step 2：D1（仅 `GO D1` 后）

- [x] 仅在 Step 1F 通过后，从 Step 1A source 新建精确 D1 `/J` 副本并创建
  `usage_event(event_key, cost_with_cache_usd)` candidate covering index。
- [x] 记录 index SQL、schema version 不变、前后 plan、database/page size delta、build
  time 和当前 binary 下顺序 cold/warm Activity HTTP timings。
- [x] 用隔离副本上的可回滚代表性 insert/update 证据评估 sync 写放大；保留 sanitized
  evidence 后精确清理 D1 database-copy，停止在 production implementation 用户门。
- [x] D1 结果：build `3993.71 ms`；plan 变为 covering index；复用 freelist
  `4256` 页/`17,432,576` active bytes；rollback WAL ratio `1.075`；fresh-server
  Activity 首次 `2127.34 ms`、其后 `657.64-737.83 ms`，全部 normalized、wait=0、
  无 busy/locked，schema/user version 与 production files 未变。
- [ ] 若 D1 不足，回到设计评审决定是否进入 D2。

Production 门已由用户批准：采用 schema v19 的单一 covering index，sync throughput
回归阈值为 `10%`；不修改 reducer/query/cache/concurrency/frontend/timeout，不进入 D2。

## Step 3：产品实现与精确性（条件执行）

- [x] 更新 PRD/design/implement，记录 production D1 批准、单一 v19 索引边界和
  `10%` sync throughput 阈值。
- [x] 在 `src/store/migrations.rs` 追加 v19
  `idx_usage_event_activity_cost(event_key, cost_with_cache_usd)`；保持 v18 测试隔离在
  `MIGRATIONS[..18]`，增加 v18->v19 与 fresh-schema 回归。
- [x] 保留现行 reducer/legacy oracle，增加建索引前后逐字节序列化对比测试。
- [x] 覆盖无匹配 event、NULL 成本、`edit_turns=0`、category 并列，以及
  source/model/project/date/no-data filters。
- [x] 增加固定 synthetic `SyncShard` 的显式单线程端到端吞吐比较；多轮中位数回归
  超过 `10%` 必须 hard-fail。
- [x] 先运行 focused migrations/Activity/sync benchmark，再运行
  `python scripts/ci-rust.py` 与 `just ci`。

## Step 4：验收与收尾（条件执行）

- [x] 独立 `trellis-check` 调度不可用后执行等价 inline 全范围检查；更新 dashboard
  performance contract 与 ADR 0004 的 schema v19 决策/验证条款，并复跑受影响门。
- [x] 构建并固定最终 debug binary；准备五份未消费 v19 快照，记录 manifest、binary
  hash、磁盘占用与 post-reboot 精确命令后停止等待用户手工重启。
- [x] 重启后完成最终 1d/all 冷暖矩阵和看板首个 Activity 请求验证。
  五个 reboot-cleared `all` 首触样本均为 normalized、无降级且 `<3000 ms`，中位数
  `2572.02 ms`；代表矩阵的 `1d` solo 中位数为 `29.53 ms`，`all` solo 中位数为
  `640.45 ms`，全部 HTTP 200 且无 timeout。真实 Chromium 首屏 Activity settled 为
  合法 `no_data`，DOM 无 loading/timeout 文本，server/port/process cleanup 通过。
- [ ] 最终验证后检查 diff、提交、archive、journal；不 push，除非用户另行要求。

当前检查点：最终 v19 验收已通过；`research/v19-final-validation.md` 汇总重启首触、
代表矩阵、浏览器与 cleanup 证据，下一步执行最终 diff/check、提交、archive、journal。

## 审查门

- v3 规划：用户确认后才实现 Step 1B。
- PERF-002：用户已选择扩大范围；Step 1B.5 全部通过前不得运行 `prepare`。
- 重启：只由用户在 Step 1C 后手工执行。
- 因果门：Step 1D 结论经用户确认后才进入 D1/D2。
- 回诊断门：Step 1E 本身不解锁 D1/D2；用户随后批准修订协议，Step 1F 已满足该门。
- Production 门：Step 1F 与隔离 D1 完成后仍不得直接创建 migration 或修改查询；
  必须先由用户审阅 D1 体积、写放大、缓存限制与语义测试缺口并批准下一步；该批准
  已取得，当前可执行 Step 3，但 v19 冷样本仍受下一次人工重启检查点约束。
