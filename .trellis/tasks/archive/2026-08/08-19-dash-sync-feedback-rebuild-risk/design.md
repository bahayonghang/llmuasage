# 技术设计：看板立即同步反馈与重建风险闭环

## 1. 边界与分层

| 层                 | 文件                                                                                                      | 改动性质                                                   |
| ------------------ | --------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| 应用层（同步作业） | `src/sync/job_registry.rs`、`src/sync/types.rs`                                                           | 受围栏保护的运行记录、共享 summary、严格终态交付           |
| 存储层             | `src/store/run_log.rs`、`src/store/mod.rs`                                                                | 统一 usage-import 恢复入口；取消态不计健康失败              |
| 查询层             | `src/query/mod.rs`                                                                                        | 共享命令中心中性语义；附加结构化重建保护事实               |
| TUI 展示           | `src/tui/panels/sync_status.rs`                                                                           | 标题、来源状态和重建事实采用共享中性语义                   |
| 看板渲染           | `src/web/assets/render/sync-command-center.js`、`src/web/assets/data/derive.js`、`src/web/assets/copy.js` | 完成态闭环与重建保护事实的中性展示                         |
| 文档与规范         | `.trellis/spec/llmusage/backend/*.md`、`docs/`、`README*`                                                 | 命令集合、取消边界、共享语义与完成态过渡的契约同步         |

ARCH-002 约束：`src/sync/` 不得引入 `commands::sync`。共享 summary 放在 `src/sync/types.rs`；运行记录恢复逻辑封装在 store 的 `RunLog` 视图中。`job_registry` 直接使用 `store.run_log()`，不依赖或复制 `commands::mod::run_tracked`。

## 2. D1：Job 路径的运行记录

### 现状

`run_job`（`src/sync/job_registry.rs:363`）流程：发 `Started` / `LockWaiting` → 取锁 → `fenced_store.bootstrap()` → `executor.run_once(...)` → 更新内存 `JobSnapshot` → `ctx.retire`。全程不触碰 `run_log`。

CLI 路径（`src/commands/sync.rs:246` 与 `:390`）在取锁与 bootstrap 之后调用 `recover_running_runs(["sync","hook-run"])`，再用 `run_tracked` 包裹一次执行。该列表遗漏了已经属于 `USAGE_IMPORT_COMMANDS` 的 `sync --rebuild`，因此本任务同时修复 CLI 与 Job 路径的恢复集合漂移。

### 目标结构

在 `RunLog` 中新增无参数的 `recover_running_usage_import_runs()`（名称可按现有风格微调），由它在 store 模块内部使用唯一的 `USAGE_IMPORT_COMMANDS = ["sync", "sync --rebuild", "hook-run"]`。CLI 的所有同步入口与 `JobRegistry` 都调用这一方法，不再各自维护字符串列表。

在 `run_job` 内，取锁并 bootstrap 成功之后、调用 executor 之前：

1. `fenced_store.run_log().recover_running_usage_import_runs()`；
2. `record_run_start(command_name)`，其中 `command_name` 由 `request.rebuild()` 决定为 `sync` 或 `sync --rebuild`；恢复或 start 失败时不运行 executor，JobSnapshot 进入 failed；
3. executor 返回后按终态收尾：

| 作业终态                                                 | `finish_run` 状态 | summary                                                            | error               |
| -------------------------------------------------------- | ----------------- | ------------------------------------------------------------------ | ------------------- |
| Completed                                                | `success`         | 与 CLI 相同的 `sources= seen= inserted_delta= stored_events=` 文本 | 无                  |
| Failed                                                   | `failed`          | 无                                                                 | `{err:#}`           |
| Cancelled（`cancel.is_cancelled()` 且 executor 返回 Ok） | `cancelled`       | 同 success 文本                                                    | `cancelled by user` |
| executor 返回 Err（即使同时观察到 cancel）              | `failed`          | 无                                                                 | `{err:#}`           |

summary 文本由 `job_registry::summary_text` 现有实现产出；需与 `src/commands/sync.rs` 的 `run_tracked` success_summary 格式对齐，避免 `logs` 输出出现两种格式。若两者当前不一致，以 CLI 格式为准，提取一个共享函数放在 `src/sync/types.rs`（`SyncSummary` 所在层），CLI 与 job 路径同时使用。

### 失败、取消与记账失败

- 锁等待期用户取消发生在 `record_run_start` 之前：快速结束为 cancelled JobSnapshot，不写 `run_log`，不在稍后取得锁；这是明确的未开始取消，不属于一次持久化运行。
- 取锁超时 / bootstrap 失败同样发生在 `record_run_start` 之前，不写 `run_log`；它们作为 JobSnapshot failed 交付。
- 创建运行记录之后，executor 的 success / failed / cancelled 都必须在持锁期间调用一次匹配的 `finish_run`。
- `record_run_start` 或 `finish_run` 失败不是旁路日志失败：JobSnapshot 不得进入 completed。发送 `SyncEvent::Failed`，将原始执行错误与记账错误组合为安全的可见错误，然后执行统一的 retire/通道排空。若失败使行仍为 `running`，下一次 `recover_running_usage_import_runs()` 将其恢复为 `aborted`。
- 不为 `finish_run` 增加无界重试；写围栏丢失时重试不能恢复合法写权限。正常 SQLite busy 行为继续由现有连接/事务策略处理。

### 写围栏

`record_run_start` / `finish_run` 都走 `store.write_transaction`。必须使用 `lock.fenced_store()` 返回的 store，且在 `drop(heartbeat)` / `drop(lock)` 之前完成，否则违反写围栏契约。实现上把「恢复 stale 行 → 创建记录 → 执行 → 分类 → 收尾」收拢在持锁作用域内，返回一个已完成记账的内部终态；外层只负责发送终态事件、更新 JobSnapshot、retire 和排空事件转发器。

## 3. D2：命令中心信号优先级

### 服务端

`sync_command_center_with_diagnostics`（`src/query/mod.rs:3401`）：

```
tone         = busy || last_run_failed ? "warn" : "good"
headline_key = busy        -> syncCenter.headline.busy
               last_failed -> syncCenter.headline.failed
               statuses 为空 -> syncCenter.headline.empty（沿用现有空态键）
               否则        -> syncCenter.headline.ready
reason_key   = last_failed && !busy -> syncCenter.reason.lastRunFailed
               statuses 为空        -> syncCenter.reason.empty
               否则                 -> syncCenter.reason.ready
```

`lossy_rebuild_risk` 与 `risk_sources` 继续计算并写入 `safety`，来源行的 `lossy_rebuild_risk` 字段保留；来源行 `status` 中的 `rebuild_risk` 取值移除，缺失文件的来源在无 `last_error` 时按 `ok` 归类。

为了避免同步状态行与 diagnostics 行缺失或筛选不一致时丢失计数，在 `SyncSafetyPayload` 增加结构化 `risk_details[]`：每项包含 `source`、`missing_file_count`、`protected_event_count`，直接由已按 QueryFilter source 筛选的 `diagnostics.by_source` 构建。`risk_sources` 保留兼容，且等于 `risk_details[].source` 的同序投影。

`syncCenter.headline.rebuildRisk` / `syncCenter.reason.rebuildRisk` 两个 key 在服务端不再被选中。`src/tui/panels/sync_status.rs` 的 key→英文映射和 copy.js 中两条文案保留，供旧快照或外部 payload 兼容；新运行时 payload 不再产生这些 key。

### TUI 共享语义

TUI 直接消费同一个 `SyncCommandCenterPayload`，因此不再声称其行为完全不变：

- 顶部 headline/reason 跟随新的共享优先级；仅有重建保护事实时显示 ready，而不是 rebuild risk。
- 来源 `status` 不再显示 `rebuild_risk`，来源行也不再仅因 `lossy_rebuild_risk` 整行使用 warning style。
- `rebuild-risk yes/no` 事实行保留，但改用 neutral/muted 样式；真正的 worker lock、最新导入失败、来源 last_error 与解析故障仍使用 warning。
- TUI 的同步动作、过滤、数据加载、取消与刷新机制不变。

### 看板渲染

- `safetyLine`（`sync-command-center.js:112`）：移除 `riskPrefix` 分支，仅保留空态与 `noRisk`；现有 `noRisk` 文案表达“普通同步保留历史”，不是“没有缺失文件”。
- `sourceCards` / `sourceSegmentedBar`：`tone` 不再因 `source.lossy_rebuild_risk` 变 warn；`copy.sourceStatus.rebuild_risk` 不再被命中。
- 同步详情内按 `safety.risk_details` 渲染中性事实（仅当非空）：`磁盘中已不存在的源文件：<来源> <n> 个；本地库保留 <m> 条事件。` 避免使用“已从磁盘删除”暗示删除者或删除动作已被证明。derive.js 规范化 `risk_details`，未知/旧 payload 缺少该字段时回退为空数组。
- `derive.js` 的 `lossy_rebuild` 洞察：`tone` 由 `warn` 改为 `neutral`，文案改为陈述事实，去掉 `action` 中的处置引导（保留「重建前需显式允许有损重建」这一句可选，措辞改为陈述）。

## 4. D3：完成态呈现

`centerWithJobOverlay` 增加 `completed` 分支：tone `good`、headline `syncCenter.headline.ready`、reason `syncCenter.reason.ready`，并保留 `current_job`。

仅保留 `current_job` 不足以显示完成时间，因此同步修改两个渲染点：

- `summaryRows` 为终态 current job 增加 `finished` 行，值来自 `current.finished_at`；copy.js 增加中英文 `statusLabels.finished`。
- `secondaryStatus` 增加 completed 分支，以 good/完成文案展示当前任务最近事件与 `finished_at`，不回退到旧 `last_run`。

`app.js` 的 `setupSyncJob` 顺序保持不变（terminal snapshot 先渲染，reload 在 `finally` 之前完成）。`finally` 清空快照后横幅回落到重载后的服务端 payload，此时 D1 已使 `last_run` 指向本次运行，D2 已使主标题为就绪。

JS 生命周期测试模拟三次连续渲染：旧 server payload + completed overlay、重载后的 ready payload + completed overlay、清空 overlay 后的 ready payload。三阶段均断言 good/ready 且完成时间为本次值，从可观察 DOM 证明没有旧 warn/旧时间闪回。

## 5. D4：`cancelled` 与失败判定

`RunRecord::counts_as_failure`（`src/store/mod.rs:173`）现为 `status != "success" && status != "running"`。改为同时排除 `cancelled`。

消费方核对：

- `Dashboard::health_summary`（`src/query/mod.rs:3380`）→ doctor / health 展示，期望排除用户取消。
- `commands/doctor.rs`、`commands/diagnostics.rs`、`commands/logs.rs` 直接读 `recent_runs`，按状态串展示，不依赖该谓词的具体取值。
- 命令中心 `last_run_failed` 与 `safety.recent_failures` 用的是 `run.status == "failed"` 字面比较，不受影响。
- TUI `last_run_spans` 改为 success=positive、cancelled=neutral、其他终态=warning，避免把用户取消渲染成同步故障。

锁前取消与执行期取消必须分别测试：前者只有 cancelled JobSnapshot 且没有 run row；后者持久化 `cancelled`。需在 `source-sync-contracts.md` 中把 `counts_as_failure` 的描述从「仍包含 aborted 恢复」扩写为「包含 aborted 恢复，排除用户取消」，并记录这两个审计边界。

## 6. 兼容性

- `run_log` 新增 `cancelled` 状态串。历史数据不含该值；`logs` 输出按原样打印，doctor/health 明确排除，无需迁移。
- payload 增加 `safety.risk_details[]`，不删除 `lossy_rebuild_risk` / `risk_sources` / 来源布尔字段。旧 payload 缺少该数组时前端按空数组处理。
- `sources[].status` 不再产生 `rebuild_risk`，属于运行时取值收窄；文案 key 与 TUI key 映射暂不删除以兼容旧快照。仓库内 Web/TUI 消费方同步更新。
- TUI 顶部与来源行的重建风险视觉语义有意从 warning 改为 neutral；这是共享 payload 语义修正，不新增第二套投影。
- 无数据库 schema 变更，无迁移。

## 7. 回滚

回滚按契约组进行：

- D1 回滚：同时撤销 JobRegistry 记账、共享恢复入口和 summary 共享函数；不能只回滚 JobRegistry 而留下两套恢复命令集合。
- D2 回滚：同时恢复 query 优先级、TUI 与 Web 的 warn 分支，并移除 `risk_details`；不能让共享 payload 与任一消费者语义分裂。
- D3 回滚：恢复 completed overlay 前必须确认 D1 的服务端 last_run 仍能立即刷新，否则会重新暴露原始旧状态闪回。
- D4 回滚：恢复 `counts_as_failure` 前必须同时停止写入 `cancelled`，避免用户取消重新进入 doctor/health 失败列表。
