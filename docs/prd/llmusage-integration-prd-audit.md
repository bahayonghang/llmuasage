# llmusage 集成 PRD 审计

> 审计对象：`llmusage-integration-prd.md`（v1）
> 审计基准：本仓 `main` HEAD（v0.4.1）
> 立场：站在 llmusage 维护者一侧，盘点 PRD 的合理性 + 实现盲点。

## 0. 一句话结论

字段映射层完成度 90%；**任务持久化、HomeOverview、migration、定价生命周期、历史数据迁移、async 改造影响面**这六处明显低估。建议把 PRD 升到 v1.1 再开工，否则 M0/M1 阶段返工概率很高。

---

## 1. 现状对账（PRD 声称 vs 仓库实测）

PRD §3 字段表的"llmusage 现状"列基本准确，下面只列**与 PRD 描述有出入**的项。

```text
┌────────────────────────┬──────────────────────────────────┬─────────────────────────────────┐
│ PRD 描述               │ 仓库实测                         │ 偏差影响                        │
├────────────────────────┼──────────────────────────────────┼─────────────────────────────────┤
│ Cargo.toml 显式声明    │ 当前只有 [package]，无 [lib]/    │ F2.1 工作量更小（默认 lib       │
│ [lib] + [[bin]]        │ [[bin]]，但默认推断生效         │ name=llmusage 已能被 import）   │
├────────────────────────┼──────────────────────────────────┼─────────────────────────────────┤
│ ReportTimezone 已有    │ 在 query::reports 内部，未公开   │ F4.1 需把它从 reports 提到      │
│ 直接搬过来作 stable    │ 导出；ReportFilter 仅 CLI 用     │ query 顶层并 re-export          │
├────────────────────────┼──────────────────────────────────┼─────────────────────────────────┤
│ Store::reset_usage_    │ 当前 reset_usage_data 删 5 张表  │ F3.4 拆分前要先确认             │
│ data 拆 reset_for_     │ 一刀切，无源参数                 │ project_dim 也要带源（当前      │
│ source                 │                                  │ 不带，需加列或拆表）            │
├────────────────────────┼──────────────────────────────────┼─────────────────────────────────┤
│ source_cursor 有       │ 现在没有 file_state；只有 cursor │ F5.1 要新增 source_file 整张表  │
│ "扫到哪了"无"在不在"   │ 表，且 cursor_key 与 file_path   │ 而不是扩展 source_cursor        │
│                        │ 是 1:N（一个文件可能多 cursor）  │                                 │
├────────────────────────┼──────────────────────────────────┼─────────────────────────────────┤
│ /api/trends 30 分钟桶  │ 实际返回的 TrendPoint 是按时间   │ F4.2 不是"加分项"，是"现在就    │
│                        │ 范围（day/week/month/all）聚合   │ 错"。新接口必须独立命名         │
└────────────────────────┴──────────────────────────────────┴─────────────────────────────────┘
```

---

## 2. PRD 关键缺口（必须 v1 补齐）

下面 12 项按重要性排序。前 6 项不补会直接卡住 M0/M1。

### 缺口 1：HomeUsageOverviewResponse 完全没覆盖

ccr-ui `src/types/usage.ts` 里的 `HomeUsageOverviewResponse`（首页面板）字段：

```text
┌──────────────────────────┬────────────────────────────────────────┐
│ 字段                     │ 来源建议                               │
├──────────────────────────┼────────────────────────────────────────┤
│ summary                  │ 复用 OverviewPayload，字段映射         │
│ by_platform              │ source_breakdown 的子集（sessions/     │
│                          │ requests/tokens 三视图）               │
│ series                   │ trends_daily 的折叠版                  │
│ bootstrap.is_warm        │ run_log 是否有 status=success          │
│ bootstrap.needs_initial_ │ usage_event count 是否 > 0             │
│   import                 │                                        │
│ bootstrap.needs_session_ │ 不在 llmusage 范围（ccr-ui 自查）      │
│   index                  │                                        │
│ empty_reason             │ 由 ccr-ui 适配层根据 bootstrap 判定    │
│ last_updated             │ run_log.MAX(finished_at)               │
└──────────────────────────┴────────────────────────────────────────┘
```

PRD §3 表只覆盖 `UsageDashboardResponse`，**完全漏掉 `HomeUsageOverviewResponse`**——这是 ccr-ui 的着陆页响应，比 dashboard 更早被用户看到。

**建议**：新增 F4.5 `Dashboard::home_overview(filter) -> HomeOverviewPayload`，映射上述字段；`empty_reason` 由 ccr-ui 适配层组装，llmusage 只暴露 booleans。

### 缺口 2：Sync 任务持久化 / job_id 机制

PRD §F3 用 `tokio::mpsc<SyncEvent>` 作进度通道，但 ccr-ui 期望的是 `StartUsageImportJobResponse { job_id, snapshot }` + 独立的 `get_usage_import_job(job_id)` 轮询。

mpsc 是 in-process push，**`job_id` 要求跨调用持久化**。两套语义不能直接桥接。

```text
┌──────────────────┬──────────────────────────────────────────────┐
│ ccr-ui 期望      │ PRD 提供                                     │
├──────────────────┼──────────────────────────────────────────────┤
│ 启动→拿 job_id   │ 启动→直接拿 receiver                         │
│ 任意时刻轮询     │ 必须持有 receiver 才能读                     │
│ Tauri 重启续接   │ 进程退出 receiver 即丢                       │
│ 取消按 job_id    │ 取消按 CancellationToken                     │
└──────────────────┴──────────────────────────────────────────────┘
```

**建议**：在 PRD 加 F3.5 `JobRegistry`：

- `pub struct JobRegistry`，内置 DashMap<job_id, JobState>。
- `start_job(opts) -> JobHandle { id, snapshot_rx, cancel }`。
- `snapshot(job_id) -> Option<JobSnapshot>`（轮询友好）。
- `cancel(job_id) -> bool`。
- 进程内存活，重启即清空（与 ccr-ui 语义一致）。

mpsc 和 JobRegistry 不冲突，前者是后者的内部实现。

### 缺口 3：schema migration 框架（被 §9b 推迟，但 v1 就要拍）

F1.2 / F1.3 / F1.5 / F5.1 都要改 schema：加列、加表、改约束。

当前 `Store::bootstrap` 用的是「`CREATE TABLE IF NOT EXISTS` + `ensure_column` 探测式 ALTER」，**没有版本号**。一旦 raw_archive 表上线后用户回退老版本，老版本无法识别新表，pragma_table_info 探测路径也无法处理"列已存在但语义换了"的情况。

PRD §9b 把这条放进"仍待确认"是错误的——它是 F1 之前必须落定的依赖项。

**建议**：F0 加一项「引入 `meta(key='schema_version', value='N')` 表 + 顺序 migration 函数数组」。`refinery` 太重，自家 versioned migration 30 行能写完，不需要外部 crate。

### 缺口 4：定价快照刷新后的成本重算策略

§F1.3 把 `cost_with_cache_usd` 落到每条 `usage_event`。`doctor --refresh-pricing` 后：

```text
┌────────────────────────┬──────────────────────────────────────┐
│ 选项                   │ 取舍                                 │
├────────────────────────┼──────────────────────────────────────┤
│ 历史不重算（snapshot   │ 历史 cost 永远反映"那一天的价目"，   │
│ semantics）            │ UI 上和"现在的价目"对不上            │
├────────────────────────┼──────────────────────────────────────┤
│ 历史全量重算           │ 简单但需要重新跑一遍 sum；千万级行   │
│                        │ 时阻塞 sync                          │
├────────────────────────┼──────────────────────────────────────┤
│ 历史标 stale，按需重算 │ 实现复杂；Dashboard 查询要支持双价   │
│                        │ fallback                             │
└────────────────────────┴──────────────────────────────────────┘
```

PRD 没说选哪条。建议默认走"全量重算"，并在 F1.3 加 `Store::recompute_costs() -> Result<()>`，绑到 `doctor --refresh-pricing` 流程末尾。

### 缺口 5：ccr-db 历史数据迁移

M3 验收说 "ccr-db 用量管线代码可以删除"。**但用户在 ccr-db 里已有数月历史数据**，删了就丢了。

PRD 没有 import-from-ccr-db 通道。两个走法：

- 让 ccr-ui 在初次启用 llmusage 时，调一个一次性 `llmusage import-ccr-db --path <ccr.db>`，把 ccr-db 的 usage_repo 表翻译成 `usage_event` 写入。
- 不迁移，让用户重跑 `llmusage sync` 从原始 JSONL/SQLite 重建——但这要求**所有源文件还在**。Codex/Claude/OpenCode 多数情况下还在；老用户清理过的会丢。

**建议**：在 PRD 加 F9 "迁移路径"，明确二选一并把命令落到 §F2.2。

### 缺口 6：async 改造对现有同步代码的连锁影响

PRD §F3.2 写 `pub async fn run_with_progress(&Store, ...) -> Result<...>`。当前：

- `commands/sync.rs` 是 sync 函数。
- `parsers/driver.rs` 是 sync 解析。
- `store::acquire_worker_lock` sync。
- `SyncRunWriter::commit_shard` sync。

把入口改 async，整条链要么全程 spawn_blocking 包一层，要么对解析路径动手。两种代价都不小：

- spawn_blocking 包一层：实现快，但 CancellationToken 的颗粒度只能在 file 边界生效，不能在 event 边界——与 PRD §F3.2 "下一个 event 边界中断"冲突。
- 改解析路径：要把 walkdir 换 tokio::fs，把 rusqlite 换 sqlx 或保持 rusqlite + spawn_blocking。前者动 schema 层，后者颗粒度妥协。

**建议**：F3.2 显式声明颗粒度——"file 边界级取消"是足够的，"event 边界级取消"目前不在 v1 范围。这样不用动解析栈。

---

## 3. 次要缺口（可放 v1.1，但 PRD 应该提到）

### 缺口 7：WorkerLock 跨进程崩溃恢复

`§9.4` 说"扩展 holder_pid + acquired_at"。当前 `WORKER_LEASE_MINUTES`（lease）已经能处理，但没说"读端被锁了怎么办"——答案是：读端走 WAL 不读 lease 表，永远不会被锁住。这点要在 §F2.2 + 非功能需求里写死，避免下游误以为读端会受影响。

### 缺口 8：OpenCode 的 raw_json 语义

§F1.5 说 driver 写 raw_json。但 OpenCode 是 SQLite，**没有原始 JSON 行**——要么把 OpenCode 的 row 序列化成 JSON 落 raw 表，要么在 `usage_event_raw` 引入 `raw_format ENUM('jsonl','sqlite_row')`。PRD 含糊。

### 缺口 9：Tauri 命令清单缺口

PRD §6 + §附录 A 只示范了 `get_usage_dashboard_v2`。ccr-ui 还有 `start_usage_import_job / get_usage_import_job / cancel_usage_import_job / getUsageLogs / getUsageHeatmap / getHomeOverview / get_usage_archive_diagnostics` 至少 7 个。**M0 验收"能渲染 summary / model / project"过于松散**——用户感知的页面里这些命令都得能调。

**建议**：在 §7 阶段表里把 M0/M1/M2/M3 各阶段对应的"已通"Tauri 命令显式列出，避免只看 summary 就觉得 M0 完成了。

### 缺口 10：Gemini hook 的 Windows wrapper

§F1.1 的 install 步骤里只写"合并写入 hooks.SessionEnd"，没说 `HookTarget::shell_command(gemini, SessionEnd)` 在 Windows 下要走 `.cmd` wrapper（与现有 Claude/Codex 一致）。属于实现细节，但 §F1.1 既然展开了完整 spec 就应该写完整。

### 缺口 11：deleted_by_user 的入口

§F5.1 三态 `live / missing / deleted_by_user`，但全文找不到"用户怎么把文件标 deleted"——是 CLI 命令？是 web UI 按钮？还是某个 settings 文件？没有入口的状态等于死字段。

**建议**：要么砍掉 deleted_by_user，只保留 live/missing；要么在 F5 加一个 `Store::mark_source_file_deleted(path)` API + `llmusage diagnostics --forget-file <path>` CLI。

### 缺口 12：测试 Fixture 公开化

§6 提"`llmusage::testing::Fixture`"作为下游 e2e 入口，但 §F2.2 没把它列进公开表面。要么在 F2.2 补，要么从 §6 删——不能只在示例里出现。

---

## 4. 字段口径风险

```text
┌────────────────────────────┬───────────────────────────────────────┐
│ 风险点                     │ 取舍                                  │
├────────────────────────────┼───────────────────────────────────────┤
│ Codex/OpenCode 没有 cache_ │ PRD 已说设 0；但 cache_efficiency =   │
│ creation 维度              │ cache_read / (input + cache_read)     │
│                            │ 跨源对比误导。建议 efficiency 字段    │
│                            │ 在非 Anthropic 源上返回 None 而非 0   │
├────────────────────────────┼───────────────────────────────────────┤
│ output_tokens 是否合并     │ PRD §3 说"建议合并"；但 §F1.2 拆字段  │
│ reasoning                  │ 时仍然分开。两处不一致。建议 store    │
│                            │ 分开存，dashboard 默认合并、提供 flag │
├────────────────────────────┼───────────────────────────────────────┤
│ pricing_source 命名带日期  │ "litellm-snapshot-2026-04" 每月一变。 │
│                            │ UI 上要做版本提示需切串。建议拆       │
│                            │ pricing_catalog="litellm-snapshot" +  │
│                            │ pricing_version="2026-04"             │
└────────────────────────────┴───────────────────────────────────────┘
```

---

## 5. 阶段划分修订建议

PRD §7 阶段表前置依赖不全。重排：

```text
┌──────┬─────────────────────────────────────────────────────────┐
│ 阶段 │ 内容                                                    │
├──────┼─────────────────────────────────────────────────────────┤
│ M0-  │ schema_version + migration runner（缺口 3）             │
│      │ AppPaths::with_root + LLMUSAGE_HOME（F7）               │
│      │ Cargo lib/bin 显式声明（F2.1）                          │
├──────┼─────────────────────────────────────────────────────────┤
│ M0   │ QueryFilter 提级（F4.1）                                │
│      │ JobRegistry 雏形（缺口 2）                              │
│      │ 公开 API 表面整理（F2.2，含 testing::Fixture）          │
├──────┼─────────────────────────────────────────────────────────┤
│ M1   │ cache 拆分 + 双价 + event_count（F1.2/1.3/1.4）         │
│      │ 价目重算策略 recompute_costs（缺口 4）                  │
│      │ daily trends + heatmap（F4.2/4.3）                      │
│      │ home_overview（缺口 1）                                 │
├──────┼─────────────────────────────────────────────────────────┤
│ M2   │ raw_archive opt-in（F1.5）                              │
│      │ logs 分页（F4.4）                                       │
│      │ source_file 状态机（F5.1，含 deleted_by_user 入口）     │
│      │ ccr-db 历史迁移命令（缺口 5）                           │
│      │ JobRegistry + RecentReady/进度节流（F3）                │
│      │ --rebuild --source 精确 reset（F3.4）                   │
├──────┼─────────────────────────────────────────────────────────┤
│ M3   │ Gemini 完整源（F1.1，含 Windows wrapper）               │
│      │ 序列化口径统一（F8）                                    │
│      │ thiserror 化（F2.3）                                    │
│      │ 0.5.0 SemVer 切线                                       │
└──────┴─────────────────────────────────────────────────────────┘
```

新增 M0-（M0 之前的基建步骤），不能跳。

---

## 6. 决议建议（PRD §9b 待定项）

```text
┌─────────────────────────┬──────────────────────────────────────┐
│ 待定项                  │ 建议拍板                             │
├─────────────────────────┼──────────────────────────────────────┤
│ migration 框架          │ 自家 versioned migration（缺口 3）   │
│ pricing 快照来源        │ LiteLLM model_prices_and_context_    │
│                         │ window.json（已是社区事实标准）      │
│ OTel 引入               │ v2，v1 不做                          │
└─────────────────────────┴──────────────────────────────────────┘
```

---

## 7. 风险提示

- **现有 0.4.x 用户的 schema** 没有版本号。引入 schema_version 时要 detect "无版本号 == 0"，不能默认从 1 起。
- `query::reports::ReportTimezone` 提级到 `query::QueryFilter` 是 breaking change（即便仅是路径），建议 0.5.0 一次性切。
- mpsc + JobRegistry 双轨期间，`run_with_progress` 文档里要说清"直连 receiver"和"通过 job_id 轮询"两条调用方式适用场景。

---

## 8. 给本仓 maintainer 的最小动作

如果本周就要进 M0：

1. 决定 migration 方案（最多 2 小时）。
2. 把 `ReportTimezone` 与 `QueryFilter` 雏形从 reports.rs 提到 query/mod.rs（半天）。
3. 加 `AppPaths::with_root` + `LLMUSAGE_HOME` 优先级（1 小时）。
4. 写 `JobRegistry` 骨架（1 天）。

这些做完，ccr-ui 就能开 M0 适配。其他 F 项可串行展开。
