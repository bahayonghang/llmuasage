# Sync Command Center 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在 llmusage Web Dashboard 顶部实现已确认的 TokenTracker 风格 Sync Command Center，用结构化后端数据先回答“现在能不能安全 sync”，再展示“最近/当前 sync 做了什么”。

**架构：** 新增一个后端 owned 的 `sync_command_center` 视图模型，挂入 `DashboardCoreSnapshot` / `/api/dashboard` / 静态 snapshot 导出；前端只透传并渲染该结构，不解析 `summary` 文本。页面顶部新增一个深色紧凑 command center，复用现有 `/api/jobs` 启动/取消/轮询流程，并在轮询中把 `JobSnapshot.last_event` 合并成临时 live 状态。

**技术栈：** Rust 2024、Axum、rusqlite、serde、原生 ES modules、CSS；测试使用现有 `cargo test -- --test-threads=1` 和 `src/web/mod.rs` 内的资产/API 集成测试。

---

## 规格来源与硬边界

- 已确认方案：`docs/spark/2026-05-29-sync-command-center-design.html` 中的 **方案 2：小型结构化 Sync 摘要契约**。
- 视觉方向：借鉴 `ref\TokenTracker\docs\screenshots` 的顶部强层级、大数字、分段条、紧凑 source cards；不要复制 TokenTracker 的 queue-file 数据模型。
- 产品优先级：第一回答 safety，第二回答 latest/current sync summary。
- 不修改 `SyncShard` / `SyncRunWriter` 写入协议；遵守 ADR 0002。
- 普通 sync safety 与 `sync --rebuild` 的 lossy rebuild 风险必须分开表达。
- 不声称 token/cost delta；第一版只展示可靠事实：`events_seen`、`events_inserted`、`stored_events`、source status、失败和锁状态。
- 前端不得解析人类可读 `JobSnapshot.summary` 或 `run_log.summary` 字符串。

## 文件结构

### 后端与 API

- 修改：`src/query/mod.rs`
  - 定义 `SyncCommandCenterPayload`、`SyncSafetyPayload`、`SyncLastRunPayload`、`SyncSourcePayload`、`SyncMetricPayload`、`SyncActionPayload` 等序列化类型。
  - 在 `DashboardCoreSnapshot` 和 `DashboardSnapshot` 中新增 `sync_command_center` 字段。
  - 新增 `Dashboard::sync_command_center(&self, filter: &QueryFilter) -> Result<SyncCommandCenterPayload>`。
  - 从同一 `Dashboard.conn` 读取：`overview`、`source_sync_status`、`run_log`、`current_worker_lock`、`diagnostics`、source breakdown，生成 safety 与 source cards。
- 修改：`src/web/mod.rs`
  - 在 `load_dashboard_snapshot_resilient()` JSON 中输出 `sync_command_center`。
  - 增加/更新 shell、asset、API 测试。
- 不改：`src/sync/job_registry.rs`
  - 当前 `JobSnapshot.last_event: Option<SyncEvent>` 已是结构化事件，足够前端合并运行态；不要新增会诱导前端解析 summary 的字段。

### 前端数据与渲染

- 修改：`src/web/shell.rs`
  - 在 topbar 后、现有 `#overview` hero 内第一位添加 `<div id="sync-command-center"></div>`，保持现有 `#btn-sync` 是真实按钮。
- 修改：`src/web/assets/data/fetch.js`
  - snapshot/live `/api/dashboard` 返回值和旧分段 fallback 都带上 `sync_command_center`；fallback 使用 `null`。
- 修改：`src/web/assets/data/derive.js`
  - `buildContext(...)` 接收 `sync_command_center`。
  - 新增 `normalizeSyncCommandCenter(payload)`，只做数值/数组/默认值规范化，不推导不可验证的 token/cost delta。
  - 输出 `context.syncCommandCenter`。
- 新建：`src/web/assets/render/sync-command-center.js`
  - 渲染 safety headline、latest/current summary、metrics、source segmented bar、source cards、primary action、secondary cancel/status。
  - 接收 `context` 与 `dashboardState`，从 `dashboardState.activeJobSnapshot?.last_event` 合并 running/current state。
  - 只调用或触发现有 `#btn-sync`，不直接重写 `/api/jobs` 流程。
- 修改：`src/web/assets/app.js`
  - import 并在 `renderDashboard` 中调用 `renderSyncCommandCenter(context, dashboardState)`，顺序在 `renderHero(context)` 前。
  - `pollJobUntilTerminal()` 每次更新 `state.activeJobSnapshot` 后调用 command center 局部刷新，避免等待终态才改变顶部卡片。
  - 点击 command center action 时复用 hidden/visible 的 `#btn-sync` click；实际启动/取消仍由 `setupSyncJob` 控制。
- 修改：`src/web/assets/mod.rs`
  - asset manifest 加入 `render/sync-command-center.js`，并更新 exact manifest 测试。
- 修改：`src/web/assets/copy.js`
  - 新增中英文 copy：标题、safety headline fallback、actions、metric labels、empty/running/failed/rebuild 风险文案。
- 修改：`src/web/assets/components.css`
  - 新增 `.sync-command-center*` 样式。
- 修改：`src/web/assets/layout.css`
  - 新增 command center 在 1100px / 720px 断点下的 grid 降级规则。

### 测试

- 修改：`src/web/mod.rs`
  - API fixture 测试 `sync_command_center` 结构和 rebuild 风险。
  - API fixture 测试最近成功/失败 run 的 structured status。
  - shell host 测试。
  - asset manifest exact list 测试。
  - app/fetch/derive/renderer asset wiring 测试。
  - guard：`render/sync-command-center.js` 和 `app.js` 不包含 `summary.match`、`split('inserted_delta')`、`split("inserted_delta")` 等 summary string parsing 形态。

---

## 任务 1：后端 view-model 类型与 API 契约测试

**文件：**
- 修改：`src/query/mod.rs`
- 修改：`src/web/mod.rs`

- [ ] **步骤 1：编写失败的 `/api/dashboard` 契约测试**

在 `src/web/mod.rs` 的 `#[cfg(test)] mod tests` 中，放在 `api_dashboard_embeds_archive_diagnostics_for_insights` 附近新增测试。该测试先手工插入 `source_sync_status`、`source_file`、`usage_event`、`run_log`，再断言 `/api/dashboard` 返回 `sync_command_center`。

```rust
#[tokio::test]
async fn api_dashboard_embeds_sync_command_center_contract() -> anyhow::Result<()> {
    let (_temp, store) = make_store()?;
    let conn = store.open_connection()?;
    conn.execute(
        r#"
        INSERT INTO source_sync_status(
            source, files_processed, changed_files, bytes_scanned,
            events_seen, events_replayed, events_inserted, stored_events,
            parse_ms, write_ms, lock_wait_ms, updated_at
        ) VALUES
            ('codex', 2, 1, 2048, 7, 0, 5, 42, 10, 4, 0, '2026-05-29T00:01:00Z'),
            ('claude', 1, 0, 1024, 0, 0, 0, 11, 3, 1, 0, '2026-05-29T00:02:00Z')
        "#,
        [],
    )?;
    conn.execute(
        r#"
        INSERT INTO source_file(source, file_path, state, last_seen_at, last_state_change_at)
        VALUES ('codex', ?1, 'missing', NULL, '2026-05-29T00:00:00Z')
        "#,
        [r"D:\missing\codex.jsonl"],
    )?;
    conn.execute(
        r#"
        INSERT INTO usage_event(
            event_key, source, model, event_at, hour_start,
            input_tokens, cache_creation_tokens, cache_read_tokens,
            output_tokens, reasoning_output_tokens, total_tokens,
            project_hash, project_label, project_ref, path_hash,
            session_id, session_label, source_path_hash, created_at,
            cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate
        ) VALUES ('codex:event:sync-center', 'codex', 'gpt-5',
            '2026-05-29T00:00:00Z', '2026-05-29T00:00:00Z',
            10, 0, 0, 0, 0, 10,
            'project-a', 'Project A', NULL, 'path-a',
            NULL, NULL, NULL, '2026-05-29T00:00:00Z',
            0.1, 0.1, 'static', 'static-v1', NULL)
        "#,
        [],
    )?;
    conn.execute(
        r#"
        INSERT INTO run_log(command, status, summary, error, started_at, finished_at, duration_ms)
        VALUES ('sync', 'success', 'human summary that must not be parsed', NULL,
                '2026-05-29T00:00:00Z', '2026-05-29T00:03:00Z', 180000)
        "#,
        [],
    )?;
    drop(conn);

    let addr = serve(store, Some(0)).await?;
    let (status, payload) = route_json(addr, "GET", "/api/dashboard?source=codex&timezone=UTC", None).await?;
    assert_eq!(status, StatusCode::OK);

    let center = &payload["sync_command_center"];
    assert_eq!(center["mode"], "live");
    assert_eq!(center["tone"], "warn");
    assert_eq!(center["safety"]["ordinary_sync_safe"], true);
    assert_eq!(center["safety"]["lossy_rebuild_risk"], true);
    assert_eq!(center["safety"]["risk_sources"][0], "codex");
    assert_eq!(center["last_run"]["status"], "success");
    assert_eq!(center["last_run"]["finished_at"], "2026-05-29T00:03:00Z");
    assert_eq!(center["metrics"]["inserted_delta"], 5);
    assert_eq!(center["metrics"]["stored_events"], 42);
    assert_eq!(center["sources"][0]["source"], "codex");
    assert_eq!(center["sources"][0]["events_inserted"], 5);
    assert!(center["sources"][0]["share"].as_f64().unwrap() > 0.0);
    Ok(())
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```powershell
cargo test api_dashboard_embeds_sync_command_center_contract -- --test-threads=1
```

预期：FAIL，失败点是 `center["mode"]` / `center["safety"]` 为空，因为后端还没有输出 `sync_command_center`。

- [ ] **步骤 3：在 `src/query/mod.rs` 增加序列化类型**

在 `DiagnosticsPayload` 之后、`DashboardSnapshot` 之前插入类型定义。字段名必须与测试一致。

```rust
/// Top dashboard sync command-center payload. It answers ordinary sync safety
/// separately from lossy rebuild risk and exposes only structured facts.
#[derive(Debug, Clone, Serialize)]
pub struct SyncCommandCenterPayload {
    pub mode: String,
    pub tone: String,
    pub headline_key: String,
    pub reason_key: String,
    pub generated_at: String,
    pub current_job: Option<SyncCurrentJobPayload>,
    pub last_run: Option<SyncLastRunPayload>,
    pub safety: SyncSafetyPayload,
    pub metrics: SyncMetricsPayload,
    pub sources: Vec<SyncSourcePayload>,
    pub actions: Vec<SyncActionPayload>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncCurrentJobPayload {
    pub job_id: String,
    pub status: String,
    pub last_event: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncLastRunPayload {
    pub status: String,
    pub command: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSafetyPayload {
    pub ordinary_sync_safe: bool,
    pub worker_lock: String,
    pub worker_lock_holder: Option<String>,
    pub lossy_rebuild_risk: bool,
    pub risk_sources: Vec<String>,
    pub recent_failures: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncMetricsPayload {
    pub events_seen: i64,
    pub inserted_delta: i64,
    pub stored_events: i64,
    pub sources_ready: i64,
    pub sources_total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSourcePayload {
    pub source: String,
    pub status: String,
    pub tone: String,
    pub events_seen: i64,
    pub events_inserted: i64,
    pub stored_events: i64,
    pub updated_at: Option<String>,
    pub share: f64,
    pub last_error: Option<String>,
    pub lossy_rebuild_risk: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncActionPayload {
    pub id: String,
    pub label_key: String,
    pub primary: bool,
    pub disabled: bool,
    pub reason_key: Option<String>,
}
```

然后在两个 snapshot struct 中加入字段：

```rust
/// Structured top-of-page sync safety and latest-run summary.
pub sync_command_center: SyncCommandCenterPayload,
```

- [ ] **步骤 4：实现 `Dashboard::sync_command_center` 和 helper**

在 `impl Dashboard` 中 `health()` 后新增方法；在 `core_snapshot()` 中填充它。实现只依赖现有表，不解析 summary。

```rust
    /// Builds the top-of-dashboard sync command center payload.
    pub fn sync_command_center(&self, _filter: &QueryFilter) -> Result<SyncCommandCenterPayload> {
        let diagnostics = self.diagnostics()?;
        let statuses = load_sync_statuses_with_conn(&self.conn)?;
        let recent_runs = self.store.run_log().recent_runs_with_conn(&self.conn, 10)?;
        let current_lock = self.store.current_worker_lock()?;
        let recent_failures = recent_runs
            .iter()
            .filter(|run| matches!(run.command.as_str(), "sync" | "sync --rebuild" | "hook-run"))
            .filter(|run| RunRecord::counts_as_failure(run))
            .count();
        let risk_sources = diagnostics
            .by_source
            .iter()
            .filter(|source| source.lossy_rebuild_risk)
            .map(|source| source.source.clone())
            .collect::<Vec<_>>();
        let risk_set = risk_sources.iter().cloned().collect::<BTreeSet<_>>();
        let inserted_total = statuses.iter().map(|row| row.events_inserted).sum::<i64>();
        let seen_total = statuses.iter().map(|row| row.events_seen).sum::<i64>();
        let stored_total = statuses.iter().map(|row| row.stored_events).sum::<i64>();
        let ready_total = statuses
            .iter()
            .filter(|row| row.last_error.is_none() && row.stored_events > 0)
            .count() as i64;
        let sources_total = statuses.len() as i64;
        let max_stored = statuses
            .iter()
            .map(|row| row.stored_events)
            .max()
            .unwrap_or_default()
            .max(1);
        let last_run = recent_runs
            .iter()
            .find(|run| matches!(run.command.as_str(), "sync" | "sync --rebuild" | "hook-run"))
            .map(|run| SyncLastRunPayload {
                status: run.status.clone(),
                command: run.command.clone(),
                started_at: run.started_at.clone(),
                finished_at: run.finished_at.clone(),
                error: run.error.clone(),
            });
        let worker_lock = if current_lock.is_some() { "busy" } else { "available" }.to_string();
        let worker_lock_holder = current_lock.as_ref().map(|lock| lock.holder_identity());
        let lossy_rebuild_risk = !risk_sources.is_empty();
        let tone = if worker_lock == "busy" || recent_failures > 0 || lossy_rebuild_risk {
            "warn"
        } else {
            "good"
        };
        let headline_key = if worker_lock == "busy" {
            "syncCenter.headline.busy"
        } else if recent_failures > 0 {
            "syncCenter.headline.failed"
        } else if lossy_rebuild_risk {
            "syncCenter.headline.rebuildRisk"
        } else {
            "syncCenter.headline.ready"
        };
        let reason_key = if lossy_rebuild_risk {
            "syncCenter.reason.rebuildRisk"
        } else if statuses.is_empty() {
            "syncCenter.reason.empty"
        } else {
            "syncCenter.reason.ready"
        };

        Ok(SyncCommandCenterPayload {
            mode: "live".to_string(),
            tone: tone.to_string(),
            headline_key: headline_key.to_string(),
            reason_key: reason_key.to_string(),
            generated_at: now_utc(),
            current_job: None,
            last_run,
            safety: SyncSafetyPayload {
                ordinary_sync_safe: worker_lock != "busy",
                worker_lock,
                worker_lock_holder,
                lossy_rebuild_risk,
                risk_sources,
                recent_failures,
            },
            metrics: SyncMetricsPayload {
                events_seen: seen_total,
                inserted_delta: inserted_total,
                stored_events: stored_total,
                sources_ready: ready_total,
                sources_total,
            },
            sources: statuses
                .into_iter()
                .map(|row| {
                    let source_risk = risk_set.contains(&row.source);
                    let status = if row.last_error.is_some() {
                        "error"
                    } else if source_risk {
                        "rebuild_risk"
                    } else if row.stored_events > 0 || row.events_seen > 0 {
                        "ok"
                    } else {
                        "idle"
                    };
                    let tone = match status {
                        "error" | "rebuild_risk" => "warn",
                        "ok" => "good",
                        _ => "neutral",
                    };
                    SyncSourcePayload {
                        source: row.source,
                        status: status.to_string(),
                        tone: tone.to_string(),
                        events_seen: row.events_seen,
                        events_inserted: row.events_inserted,
                        stored_events: row.stored_events,
                        updated_at: Some(row.updated_at),
                        share: (row.stored_events as f64 / max_stored as f64).clamp(0.0, 1.0),
                        last_error: row.last_error,
                        lossy_rebuild_risk: source_risk,
                    }
                })
                .collect(),
            actions: vec![SyncActionPayload {
                id: "sync".to_string(),
                label_key: "syncCenter.action.sync".to_string(),
                primary: true,
                disabled: worker_lock == "busy",
                reason_key: if worker_lock == "busy" {
                    Some("syncCenter.action.busy".to_string())
                } else {
                    None
                },
            }],
        })
    }
```

如果 Rust 报 `BTreeSet` 未导入，在文件顶部把 `use std::collections::BTreeMap;` 改为：

```rust
use std::collections::{BTreeMap, BTreeSet};
```

在 `DashboardCoreSnapshot` 构造里新增：

```rust
sync_command_center: self.sync_command_center(filter)?,
```

在 `Dashboard::snapshot()` 中从 core 移入：

```rust
sync_command_center: core.sync_command_center,
```

- [ ] **步骤 5：新增 `load_sync_statuses_with_conn` helper**

在 `load_source_diagnostics` 附近新增私有 struct 与 helper，避免 `store.sync_status().load_source_sync_statuses()` 再开连接，并支持 `last_error: None`。

```rust
#[derive(Debug)]
struct SyncStatusRow {
    source: String,
    events_seen: i64,
    events_inserted: i64,
    stored_events: i64,
    updated_at: String,
    last_error: Option<String>,
}

fn load_sync_statuses_with_conn(conn: &Connection) -> Result<Vec<SyncStatusRow>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT source, events_seen, events_inserted, stored_events, updated_at
        FROM source_sync_status
        ORDER BY stored_events DESC, source ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SyncStatusRow {
            source: row.get(0)?,
            events_seen: row.get(1)?,
            events_inserted: row.get(2)?,
            stored_events: row.get(3)?,
            updated_at: row.get(4)?,
            last_error: None,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
```

- [ ] **步骤 6：把 API JSON 输出接上新字段**

在 `src/web/mod.rs::load_dashboard_snapshot_resilient()` 的 `json!` 里加入：

```rust
"sync_command_center": core.sync_command_center,
```

位置放在 `"overview": core.overview,` 之后，便于测试和浏览器 payload 扫读。

- [ ] **步骤 7：运行后端契约测试验证通过**

运行：

```powershell
cargo test api_dashboard_embeds_sync_command_center_contract -- --test-threads=1
```

预期：PASS。

- [ ] **步骤 8：Commit 后端契约切片**

```powershell
git add src/query/mod.rs src/web/mod.rs
git diff --staged --check
git commit -m "feat(sync): add structured command center snapshot" -m "Constraint: preserve SyncShard and SyncRunWriter commit protocol" -m "Rejected: frontend-only summary parsing | backend-owned contract is safer" -m "Confidence: medium" -m "Scope-risk: moderate" -m "Directive: do not add token or cost deltas until run logs store before-after facts" -m "Tested: cargo test api_dashboard_embeds_sync_command_center_contract -- --test-threads=1" -m "Not-tested: full dashboard asset wiring pending"
```

---

## 任务 2：静态 snapshot、fallback 与数据上下文透传

**文件：**
- 修改：`src/web/assets/data/fetch.js`
- 修改：`src/web/assets/data/derive.js`
- 修改：`src/web/mod.rs`

- [ ] **步骤 1：编写资产测试覆盖透传**

在 `src/web/mod.rs` 的资产测试区域新增测试：

```rust
#[test]
fn dashboard_data_layers_pass_through_sync_command_center() {
    let fetch_js = asset_manifest()
        .iter()
        .find(|asset| asset.path == "data/fetch.js")
        .expect("fetch.js asset")
        .body;
    let derive_js = asset_manifest()
        .iter()
        .find(|asset| asset.path == "data/derive.js")
        .expect("derive.js asset")
        .body;

    assert!(fetch_js.contains("sync_command_center: snapshot?.sync_command_center"));
    assert!(fetch_js.contains("sync_command_center: null"));
    assert!(derive_js.contains("sync_command_center"));
    assert!(derive_js.contains("function normalizeSyncCommandCenter"));
    assert!(derive_js.contains("syncCommandCenter: normalizeSyncCommandCenter(sync_command_center)"));
}
```

- [ ] **步骤 2：运行测试验证失败**

```powershell
cargo test dashboard_data_layers_pass_through_sync_command_center -- --test-threads=1
```

预期：FAIL，`fetch.js` / `derive.js` 尚未包含新字段。

- [ ] **步骤 3：修改 `src/web/assets/data/fetch.js` 的 snapshot/live 返回值**

在 `state.mode === 'snapshot'` 返回对象中加入：

```js
sync_command_center: snapshot?.sync_command_center,
```

在 `/api/dashboard` 成功返回对象中加入同一行。

在旧分段 API fallback 中，不新增额外 endpoint；解构数组不变，return 改为：

```js
return { overview, trends, models, sources, projects, costs, activity, tools, optimize, explorer, compare, health, diagnostics, sync_command_center: null };
```

- [ ] **步骤 4：修改 `src/web/assets/data/derive.js` 接收参数和 normalize helper**

把函数签名改为：

```js
export function buildContext({ overview, trends, models, sources, projects, costs, activity, tools, optimize, compare, explorer, health, diagnostics, sync_command_center }) {
```

在 `normalizeExplorer` 后新增：

```js
function normalizeSyncCommandCenter(payload) {
  const metrics = payload?.metrics || {};
  const safety = payload?.safety || {};
  return {
    mode: payload?.mode || 'live',
    tone: payload?.tone || 'neutral',
    headline_key: payload?.headline_key || 'syncCenter.headline.empty',
    reason_key: payload?.reason_key || 'syncCenter.reason.empty',
    generated_at: payload?.generated_at || '',
    current_job: payload?.current_job || null,
    last_run: payload?.last_run || null,
    safety: {
      ordinary_sync_safe: safety?.ordinary_sync_safe !== false,
      worker_lock: safety?.worker_lock || 'unknown',
      worker_lock_holder: safety?.worker_lock_holder || null,
      lossy_rebuild_risk: Boolean(safety?.lossy_rebuild_risk),
      risk_sources: normalizeRows(safety?.risk_sources),
      recent_failures: Number(safety?.recent_failures || 0),
    },
    metrics: {
      events_seen: Number(metrics?.events_seen || 0),
      inserted_delta: Number(metrics?.inserted_delta || 0),
      stored_events: Number(metrics?.stored_events || 0),
      sources_ready: Number(metrics?.sources_ready || 0),
      sources_total: Number(metrics?.sources_total || 0),
    },
    sources: normalizeRows(payload?.sources).map((row) => ({
      source: row?.source || '--',
      status: row?.status || 'idle',
      tone: row?.tone || 'neutral',
      events_seen: Number(row?.events_seen || 0),
      events_inserted: Number(row?.events_inserted || 0),
      stored_events: Number(row?.stored_events || 0),
      updated_at: row?.updated_at || '',
      share: Math.max(0, Math.min(1, Number(row?.share || 0))),
      last_error: row?.last_error || null,
      lossy_rebuild_risk: Boolean(row?.lossy_rebuild_risk),
    })),
    actions: normalizeRows(payload?.actions),
  };
}
```

在 `const context = { ... }` 中加入顶层字段：

```js
syncCommandCenter: normalizeSyncCommandCenter(sync_command_center),
```

建议放在 `ledgerSummary` 后、`leaders` 前。

- [ ] **步骤 5：运行透传测试验证通过**

```powershell
cargo test dashboard_data_layers_pass_through_sync_command_center -- --test-threads=1
```

预期：PASS。

- [ ] **步骤 6：Commit 数据透传切片**

```powershell
git add src/web/assets/data/fetch.js src/web/assets/data/derive.js src/web/mod.rs
git diff --staged --check
git commit -m "feat(web): pass sync command center into dashboard context" -m "Constraint: keep legacy segmented API fallback working without a new endpoint" -m "Confidence: high" -m "Scope-risk: narrow" -m "Tested: cargo test dashboard_data_layers_pass_through_sync_command_center -- --test-threads=1" -m "Not-tested: renderer wiring pending"
```

---

## 任务 3：Shell host、renderer 与 i18n copy

**文件：**
- 修改：`src/web/shell.rs`
- 新建：`src/web/assets/render/sync-command-center.js`
- 修改：`src/web/assets/app.js`
- 修改：`src/web/assets/mod.rs`
- 修改：`src/web/assets/copy.js`
- 修改：`src/web/mod.rs`

- [ ] **步骤 1：编写 shell/asset wiring 测试**

在 `src/web/mod.rs` 资产测试区域新增：

```rust
#[test]
fn dashboard_shell_and_assets_wire_sync_command_center() {
    let html = live_index_html();
    assert!(html.contains("id=\"sync-command-center\""));
    assert!(html.contains("data-i18n=\"shell.syncCenter.eyebrow\""));

    let app_js = asset_manifest()
        .iter()
        .find(|asset| asset.path == "app.js")
        .expect("app.js asset")
        .body;
    assert!(app_js.contains("import { renderSyncCommandCenter } from './render/sync-command-center.js';"));
    assert!(app_js.contains("renderSyncCommandCenter(context, dashboardState)"));
    assert!(app_js.contains("refreshSyncCommandCenter(state)"));

    let renderer = asset_manifest()
        .iter()
        .find(|asset| asset.path == "render/sync-command-center.js")
        .expect("sync command center renderer asset")
        .body;
    assert!(renderer.contains("export function renderSyncCommandCenter"));
    assert!(renderer.contains("activeJobSnapshot?.last_event"));
    assert!(renderer.contains("document.getElementById('btn-sync')?.click()"));

    let copy_js = asset_manifest()
        .iter()
        .find(|asset| asset.path == "copy.js")
        .expect("copy.js asset")
        .body;
    assert!(copy_js.contains("syncCenter.headline.ready"));
    assert!(copy_js.contains("insertedDelta"));
}
```

更新既有 `asset_manifest_contains_required_files` 的 vec，在 `"render/hero.js"` 后插入：

```rust
"render/sync-command-center.js",
```

- [ ] **步骤 2：运行测试验证失败**

```powershell
cargo test dashboard_shell_and_assets_wire_sync_command_center asset_manifest_contains_required_files -- --test-threads=1
```

预期：FAIL，因为文件/host/import 都未接入。

- [ ] **步骤 3：修改 `src/web/shell.rs` 添加 host**

在 `<!-- Hero + Status -->` 下、`<section id="overview" class="block">` 内、现有 `<div class="hero">` 之前插入：

```html
      <div class="sync-command-center" id="sync-command-center" aria-live="polite">
        <div class="sync-command-center-empty">
          <div class="section-eyebrow" data-i18n="shell.syncCenter.eyebrow">SYNC</div>
          <div data-i18n="shell.syncCenter.loading">正在读取同步状态…</div>
        </div>
      </div>
```

- [ ] **步骤 4：新增 renderer 文件**

创建 `src/web/assets/render/sync-command-center.js`：

```js
import { UI_COPY, getShellCopy } from '../copy.js';
import { escapeHtml, formatNumber } from '../data.js';

const logger = window.console;

function eventName(event) {
  return event?.event || event?.type || null;
}

function runningOverlay(center, snapshot) {
  if (!snapshot || snapshot.status !== 'running') return center;
  const event = snapshot.last_event || null;
  const next = {
    ...center,
    tone: 'neutral',
    headline_key: 'syncCenter.headline.running',
    reason_key: 'syncCenter.reason.running',
    current_job: {
      job_id: snapshot.job_id,
      status: snapshot.status,
      last_event: eventName(event),
      started_at: snapshot.started_at,
      finished_at: snapshot.finished_at || null,
      error: snapshot.error || null,
    },
  };
  if (event?.event === 'finished' && event?.summary) {
    next.metrics = {
      ...center.metrics,
      events_seen: Number(event.summary.total_seen || center.metrics.events_seen || 0),
      inserted_delta: Number(event.summary.total_inserted || center.metrics.inserted_delta || 0),
      stored_events: Number(event.summary.stored_events || center.metrics.stored_events || 0),
      sources_total: Number(event.summary.sources || center.metrics.sources_total || 0),
    };
  }
  if (event?.event === 'source_finished' && event?.stats) {
    next.reason_key = 'syncCenter.reason.sourceFinished';
  }
  return next;
}

function metricCard(label, value, foot) {
  return `
    <article class="sync-command-center-metric">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value)}</strong>
      <small>${escapeHtml(foot || '')}</small>
    </article>
  `;
}

function renderSegments(sources) {
  const visible = sources.filter((row) => row.share > 0).slice(0, 8);
  if (!visible.length) {
    return '<div class="sync-command-center-segments" aria-hidden="true"><i style="width: 100%" data-tone="neutral"></i></div>';
  }
  const total = visible.reduce((sum, row) => sum + row.share, 0) || 1;
  return `
    <div class="sync-command-center-segments" aria-hidden="true">
      ${visible.map((row) => `<i style="width: ${Math.max(5, (row.share / total) * 100).toFixed(2)}%" data-tone="${escapeHtml(row.tone)}"></i>`).join('')}
    </div>
  `;
}

function sourceCards(sources, copy) {
  if (!sources.length) {
    return `<div class="sync-command-center-source empty">${escapeHtml(copy.sourcesEmpty)}</div>`;
  }
  return sources.slice(0, 4).map((row) => `
    <article class="sync-command-center-source" data-tone="${escapeHtml(row.tone)}">
      <div>
        <strong>${escapeHtml(row.source)}</strong>
        <span>${escapeHtml(copy.sourceStatus[row.status] || row.status)}</span>
      </div>
      <code>${escapeHtml(formatNumber(row.events_inserted))} Δ · ${escapeHtml(formatNumber(row.stored_events))}</code>
    </article>
  `).join('');
}

function actionLabel(center, state, copy) {
  if (state?.mode === 'snapshot') return getShellCopy('shell.sync.snapshotDisabled');
  if (state?.activeJobSnapshot?.status === 'running') return getShellCopy('shell.sync.cancel');
  const action = center.actions?.find((item) => item.id === 'sync') || null;
  return action?.label_key ? getShellCopy(action.label_key) : copy.actions.sync;
}

export function renderSyncCommandCenter(context, state) {
  logger.info('开始渲染 Sync Command Center');
  const host = document.getElementById('sync-command-center');
  if (!host) return;

  const base = context?.syncCommandCenter || {};
  const center = runningOverlay(base, state?.activeJobSnapshot);
  const copy = UI_COPY.sections.syncCenter;
  const metrics = center.metrics || {};
  const safety = center.safety || {};
  const headline = getShellCopy(center.headline_key || 'syncCenter.headline.empty');
  const reason = getShellCopy(center.reason_key || 'syncCenter.reason.empty');
  const actionText = actionLabel(center, state, copy);
  const actionDisabled = state?.mode === 'snapshot' || (center.actions || []).some((item) => item.id === 'sync' && item.disabled);
  const riskText = safety.lossy_rebuild_risk
    ? `${copy.riskPrefix} ${escapeHtml((safety.risk_sources || []).join(' / ') || '--')}`
    : copy.noRisk;

  host.dataset.tone = center.tone || 'neutral';
  host.innerHTML = `
    <div class="sync-command-center-head">
      <div>
        <div class="section-eyebrow">${escapeHtml(copy.eyebrow)}</div>
        <h2>${escapeHtml(headline)}</h2>
        <p>${escapeHtml(reason)}</p>
      </div>
      <button class="btn btn-primary sync-command-center-action" type="button" ${actionDisabled ? 'disabled' : ''}>
        ${escapeHtml(actionText)}
      </button>
    </div>
    <div class="sync-command-center-meta">
      <span>${escapeHtml(copy.generatedAt)} <code>${escapeHtml(center.generated_at || '--')}</code></span>
      <span>${escapeHtml(copy.workerLock)} <code>${escapeHtml(copy.workerLockState[safety.worker_lock] || safety.worker_lock || '--')}</code></span>
      <span>${riskText}</span>
    </div>
    <div class="sync-command-center-grid">
      ${metricCard(copy.metrics.seen, formatNumber(metrics.events_seen), copy.metrics.seenFoot)}
      ${metricCard(copy.metrics.insertedDelta, formatNumber(metrics.inserted_delta), copy.metrics.insertedFoot)}
      ${metricCard(copy.metrics.storedEvents, formatNumber(metrics.stored_events), copy.metrics.storedFoot)}
      ${metricCard(copy.metrics.sourcesReady, `${formatNumber(metrics.sources_ready)} / ${formatNumber(metrics.sources_total)}`, copy.metrics.sourcesFoot)}
    </div>
    ${renderSegments(center.sources || [])}
    <div class="sync-command-center-sources">
      ${sourceCards(center.sources || [], copy)}
    </div>
  `;

  host.querySelector('.sync-command-center-action')?.addEventListener('click', () => {
    document.getElementById('btn-sync')?.click();
  });
  logger.info('完成 Sync Command Center 渲染');
}
```

- [ ] **步骤 5：修改 `src/web/assets/app.js` 接入 renderer 与轮询刷新**

顶部 import 区加入：

```js
import { renderSyncCommandCenter } from './render/sync-command-center.js';
```

在 `renderDashboard(rawData)` 中，`renderHero(context);` 前加入：

```js
renderSyncCommandCenter(context, dashboardState);
```

在 `renderDashboard` 后新增局部刷新 helper：

```js
function refreshSyncCommandCenter(state = dashboardState) {
  if (!state?.rawData) return;
  const context = buildContext(state.rawData);
  renderSyncCommandCenter(context, state);
}
```

在 `pollJobUntilTerminal` 循环内 `updateSyncButton(state, snapshot);` 后加入：

```js
refreshSyncCommandCenter(state);
```

在 `setupSyncJob` 中 `state.activeJobSnapshot = payload.snapshot; updateSyncButton(...)` 后加入：

```js
refreshSyncCommandCenter(state);
```

在 cancel 分支 `updateSyncButton(...)` 后加入：

```js
refreshSyncCommandCenter(state);
```

finally 中 `updateSyncButton(state);` 后加入：

```js
refreshSyncCommandCenter(state);
```

- [ ] **步骤 6：修改 asset manifest**

在 `src/web/assets/mod.rs` 的 `render/hero.js` 后加入：

```rust
    WebAsset {
        path: "render/sync-command-center.js",
        content_type: "application/javascript; charset=utf-8",
        body: include_str!("render/sync-command-center.js"),
    },
```

- [ ] **步骤 7：新增中英文 copy**

在 `UI_COPY_ZH.sections` 中新增：

```js
syncCenter: Object.freeze({
  eyebrow: 'SYNC COMMAND',
  generatedAt: '生成',
  workerLock: 'worker',
  riskPrefix: 'rebuild 风险来源',
  noRisk: '未检测到 rebuild 风险',
  sourcesEmpty: '暂无 source sync 状态。首次同步后会显示每个来源的结果。',
  actions: Object.freeze({ sync: '运行同步' }),
  metrics: Object.freeze({
    seen: 'Seen events',
    seenFoot: '本次/最近来源统计',
    insertedDelta: 'Inserted delta',
    insertedFoot: 'SQLite 去重后的新增事件',
    storedEvents: 'Stored events',
    storedFoot: '当前已保存事件',
    sourcesReady: 'Sources ready',
    sourcesFoot: '有可用记录的来源',
  }),
  workerLockState: Object.freeze({ available: '可用', busy: '占用中', unknown: '未知' }),
  sourceStatus: Object.freeze({ ok: 'ok', idle: 'idle', error: 'error', rebuild_risk: 'rebuild risk' }),
}),
```

在 `UI_COPY_EN.sections` 中新增英文对应：

```js
syncCenter: Object.freeze({
  eyebrow: 'SYNC COMMAND',
  generatedAt: 'Generated',
  workerLock: 'worker',
  riskPrefix: 'Rebuild risk sources',
  noRisk: 'No rebuild risk detected',
  sourcesEmpty: 'No source sync status yet. Source results appear after the first sync.',
  actions: Object.freeze({ sync: 'Run sync' }),
  metrics: Object.freeze({
    seen: 'Seen events',
    seenFoot: 'Current/latest source stats',
    insertedDelta: 'Inserted delta',
    insertedFoot: 'New events after SQLite dedupe',
    storedEvents: 'Stored events',
    storedFoot: 'Events currently stored',
    sourcesReady: 'Sources ready',
    sourcesFoot: 'Sources with usable records',
  }),
  workerLockState: Object.freeze({ available: 'Available', busy: 'Busy', unknown: 'Unknown' }),
  sourceStatus: Object.freeze({ ok: 'ok', idle: 'idle', error: 'error', rebuild_risk: 'rebuild risk' }),
}),
```

在 `SHELL_COPY_ZH` 加入：

```js
'shell.syncCenter.eyebrow': 'SYNC',
'shell.syncCenter.loading': '正在读取同步状态…',
'syncCenter.headline.ready': 'Sync ready',
'syncCenter.headline.rebuildRisk': '普通 sync 安全，rebuild 需注意',
'syncCenter.headline.failed': '最近同步需要检查',
'syncCenter.headline.busy': '另一个 sync 正在运行',
'syncCenter.headline.running': 'Sync running',
'syncCenter.headline.empty': '尚未同步',
'syncCenter.reason.ready': '普通 sync 会增量导入并保留已导入历史。',
'syncCenter.reason.rebuildRisk': '普通 sync 安全；缺失源文件只会让 sync --rebuild 触发保护。',
'syncCenter.reason.empty': '首次同步后这里会显示来源、增量和安全状态。',
'syncCenter.reason.running': '正在读取结构化 progress event；完成后 Dashboard 会刷新。',
'syncCenter.reason.sourceFinished': '已有来源完成，等待最终汇总。',
'syncCenter.action.sync': '运行同步',
'syncCenter.action.busy': 'worker lock busy',
```

在 `SHELL_COPY_EN` 加入英文对应：

```js
'shell.syncCenter.eyebrow': 'SYNC',
'shell.syncCenter.loading': 'Reading sync status…',
'syncCenter.headline.ready': 'Sync ready',
'syncCenter.headline.rebuildRisk': 'Regular sync is safe; rebuild needs attention',
'syncCenter.headline.failed': 'Recent sync needs review',
'syncCenter.headline.busy': 'Another sync is running',
'syncCenter.headline.running': 'Sync running',
'syncCenter.headline.empty': 'Not synced yet',
'syncCenter.reason.ready': 'Regular sync imports incrementally and keeps imported history.',
'syncCenter.reason.rebuildRisk': 'Regular sync is safe; missing source files only gate sync --rebuild.',
'syncCenter.reason.empty': 'After the first sync, source results, deltas and safety appear here.',
'syncCenter.reason.running': 'Reading structured progress events; the dashboard refreshes when complete.',
'syncCenter.reason.sourceFinished': 'One source has finished; waiting for the final summary.',
'syncCenter.action.sync': 'Run sync',
'syncCenter.action.busy': 'worker lock busy',
```

- [ ] **步骤 8：运行 wiring 测试验证通过**

```powershell
cargo test dashboard_shell_and_assets_wire_sync_command_center asset_manifest_contains_required_files -- --test-threads=1
```

预期：PASS。

- [ ] **步骤 9：Commit renderer wiring 切片**

```powershell
git add src/web/shell.rs src/web/assets/render/sync-command-center.js src/web/assets/app.js src/web/assets/mod.rs src/web/assets/copy.js src/web/mod.rs
git diff --staged --check
git commit -m "feat(web): render sync command center" -m "Constraint: reuse existing /api/jobs lifecycle instead of adding a second sync flow" -m "Rejected: parsing human sync summaries | renderer consumes structured fields and last_event only" -m "Confidence: medium" -m "Scope-risk: moderate" -m "Tested: cargo test dashboard_shell_and_assets_wire_sync_command_center asset_manifest_contains_required_files -- --test-threads=1" -m "Not-tested: visual browser pass pending"
```

---

## 任务 4：CSS 视觉实现与响应式优化

**文件：**
- 修改：`src/web/assets/components.css`
- 修改：`src/web/assets/layout.css`
- 修改：`src/web/mod.rs`

- [ ] **步骤 1：编写 CSS presence 测试**

在 `src/web/mod.rs` 资产测试区域新增：

```rust
#[test]
fn dashboard_assets_style_sync_command_center_responsively() {
    let components_css = asset_manifest()
        .iter()
        .find(|asset| asset.path == "components.css")
        .expect("components.css asset")
        .body;
    let layout_css = asset_manifest()
        .iter()
        .find(|asset| asset.path == "layout.css")
        .expect("layout.css asset")
        .body;

    assert!(components_css.contains(".sync-command-center"));
    assert!(components_css.contains(".sync-command-center-segments"));
    assert!(components_css.contains(".sync-command-center-source[data-tone='warn']"));
    assert!(layout_css.contains(".sync-command-center-grid"));
    assert!(layout_css.contains("@media (max-width: 720px)"));
}
```

- [ ] **步骤 2：运行测试验证失败**

```powershell
cargo test dashboard_assets_style_sync_command_center_responsively -- --test-threads=1
```

预期：FAIL，CSS class 尚未存在。

- [ ] **步骤 3：添加 `components.css` 样式**

在 `/* Status panel */` 之前插入：

```css
/* Sync command center */
.sync-command-center {
  position: relative;
  overflow: hidden;
  margin-bottom: 18px;
  padding: 18px;
  border: 1px solid var(--dark-line);
  border-radius: 16px;
  background:
    radial-gradient(circle at 82% 0%, rgba(200, 85, 61, 0.22), transparent 42%),
    linear-gradient(135deg, var(--dark), #231d18);
  color: var(--dark-text);
}

.sync-command-center::before {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, rgba(255,255,255,0.04), transparent 45%);
  pointer-events: none;
}

.sync-command-center > * { position: relative; }

.sync-command-center[data-tone='warn'] { border-color: rgba(192, 138, 59, 0.5); }

.sync-command-center-head {
  display: flex;
  justify-content: space-between;
  gap: 18px;
  align-items: flex-start;
  margin-bottom: 12px;
}

.sync-command-center-head h2 {
  margin: 3px 0 6px;
  font-size: clamp(24px, 4vw, 42px);
  line-height: 0.98;
  letter-spacing: -0.045em;
}

.sync-command-center-head p {
  margin: 0;
  max-width: 680px;
  color: var(--dark-muted);
  font-size: 13px;
  line-height: 1.5;
}

.sync-command-center-action {
  background: var(--dark-text);
  color: var(--dark);
  border-color: var(--dark-text);
  white-space: nowrap;
}

.sync-command-center-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 14px;
  color: var(--dark-muted);
  font-size: 11.5px;
}

.sync-command-center-meta span {
  padding: 5px 8px;
  border: 1px solid var(--dark-line);
  border-radius: 999px;
  background: rgba(255,255,255,0.035);
}

.sync-command-center-meta code {
  color: var(--dark-text);
  background: transparent;
  padding: 0;
}

.sync-command-center-metric,
.sync-command-center-source,
.sync-command-center-empty {
  border: 1px solid var(--dark-line);
  background: rgba(255,255,255,0.045);
  border-radius: 12px;
}

.sync-command-center-metric { padding: 12px; }

.sync-command-center-metric span,
.sync-command-center-source span,
.sync-command-center-metric small {
  color: var(--dark-muted);
  font-size: 11px;
}

.sync-command-center-metric strong {
  display: block;
  margin-top: 3px;
  font-family: var(--font-mono);
  font-size: clamp(20px, 3vw, 30px);
  line-height: 1;
  letter-spacing: -0.04em;
}

.sync-command-center-segments {
  display: flex;
  height: 8px;
  margin: 14px 0;
  overflow: hidden;
  border-radius: 999px;
  background: rgba(255,255,255,0.08);
}

.sync-command-center-segments i[data-tone='good'] { background: #34d399; }
.sync-command-center-segments i[data-tone='warn'] { background: #fbbf24; }
.sync-command-center-segments i[data-tone='neutral'] { background: #60a5fa; }
.sync-command-center-segments i[data-tone='error'] { background: #f87171; }

.sync-command-center-source {
  display: flex;
  justify-content: space-between;
  gap: 10px;
  align-items: center;
  min-width: 0;
  padding: 10px 12px;
}

.sync-command-center-source[data-tone='warn'] { border-color: rgba(192, 138, 59, 0.55); }
.sync-command-center-source[data-tone='good'] { border-color: rgba(79, 122, 92, 0.45); }

.sync-command-center-source strong {
  display: block;
  font-family: var(--font-mono);
  font-size: 12px;
}

.sync-command-center-source code {
  flex: 0 0 auto;
  color: var(--dark-text);
  background: rgba(0,0,0,0.18);
}

.sync-command-center-source.empty,
.sync-command-center-empty {
  padding: 14px;
  color: var(--dark-muted);
}
```

- [ ] **步骤 4：添加 `layout.css` grid 与响应式**

在 `.kpi-grid` 后加入：

```css
.sync-command-center-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}

.sync-command-center-sources {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 10px;
}
```

在 `@media (max-width: 1100px)` 内加入：

```css
  .sync-command-center-grid,
  .sync-command-center-sources {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
```

在 `@media (max-width: 720px)` 内加入：

```css
  .sync-command-center-head {
    flex-direction: column;
  }

  .sync-command-center-grid,
  .sync-command-center-sources {
    grid-template-columns: 1fr;
  }
```

- [ ] **步骤 5：运行 CSS 测试验证通过**

```powershell
cargo test dashboard_assets_style_sync_command_center_responsively -- --test-threads=1
```

预期：PASS。

- [ ] **步骤 6：Commit CSS 切片**

```powershell
git add src/web/assets/components.css src/web/assets/layout.css src/web/mod.rs
git diff --staged --check
git commit -m "style(web): polish sync command center layout" -m "Constraint: match existing dashboard theme variables and responsive breakpoints" -m "Confidence: high" -m "Scope-risk: narrow" -m "Tested: cargo test dashboard_assets_style_sync_command_center_responsively -- --test-threads=1" -m "Not-tested: browser visual pass pending"
```

---

## 任务 5：前端 safety guard、job overlay 和 summary parsing 禁止线

**文件：**
- 修改：`src/web/mod.rs`
- 修改：`src/web/assets/render/sync-command-center.js`
- 修改：`src/web/assets/app.js`
- 修改：`src/web/assets/copy.js`

- [ ] **步骤 1：编写 summary parsing 禁止测试**

在 `src/web/mod.rs` 资产测试区域新增：

```rust
#[test]
fn sync_command_center_does_not_parse_human_summary_strings() {
    let app_js = asset_manifest()
        .iter()
        .find(|asset| asset.path == "app.js")
        .expect("app.js asset")
        .body;
    let renderer = asset_manifest()
        .iter()
        .find(|asset| asset.path == "render/sync-command-center.js")
        .expect("sync command center renderer asset")
        .body;
    let combined = format!("{app_js}\n{renderer}");

    for forbidden in [
        "summary.match",
        "summary.split",
        "split('inserted_delta')",
        "split(\"inserted_delta\")",
        "inserted_delta=",
        "stored_events=",
    ] {
        assert!(!combined.contains(forbidden), "forbidden summary parsing marker: {forbidden}");
    }
    assert!(renderer.contains("event.summary.total_inserted"));
    assert!(renderer.contains("event.summary.stored_events"));
}
```

- [ ] **步骤 2：运行测试**

```powershell
cargo test sync_command_center_does_not_parse_human_summary_strings -- --test-threads=1
```

预期：如果任务 3 renderer 按计划实现，应 PASS；如果失败，删除任何 summary string parsing，只读取 `last_event.summary` 的结构化字段。

- [ ] **步骤 3：补强 renderer 的 failed/cancelled overlay**

如果 command center 在失败/取消时只显示旧 base 状态，在 `runningOverlay` 下新增 `jobOverlay` 并替换调用。

```js
function jobOverlay(center, snapshot) {
  if (!snapshot) return center;
  if (snapshot.status === 'running') return runningOverlay(center, snapshot);
  if (snapshot.status === 'failed') {
    return {
      ...center,
      tone: 'warn',
      headline_key: 'syncCenter.headline.failed',
      reason_key: 'syncCenter.reason.failedJob',
      current_job: {
        job_id: snapshot.job_id,
        status: snapshot.status,
        last_event: eventName(snapshot.last_event),
        started_at: snapshot.started_at,
        finished_at: snapshot.finished_at || null,
        error: snapshot.error || null,
      },
    };
  }
  if (snapshot.status === 'cancelled') {
    return {
      ...center,
      tone: 'neutral',
      headline_key: 'syncCenter.headline.cancelled',
      reason_key: 'syncCenter.reason.cancelled',
    };
  }
  return center;
}
```

把 `const center = runningOverlay(base, state?.activeJobSnapshot);` 改为：

```js
const center = jobOverlay(base, state?.activeJobSnapshot);
```

并补 copy。中文：

```js
'syncCenter.headline.cancelled': 'Sync 已取消',
'syncCenter.reason.failedJob': '最近一次前台 sync job 失败；请查看错误后重试。',
'syncCenter.reason.cancelled': '同步已取消，已导入数据保持不变。',
```

英文：

```js
'syncCenter.headline.cancelled': 'Sync cancelled',
'syncCenter.reason.failedJob': 'The latest foreground sync job failed; review the error before retrying.',
'syncCenter.reason.cancelled': 'Sync was cancelled; imported data remains unchanged.',
```

- [ ] **步骤 4：重新运行禁止测试和 wiring 测试**

```powershell
cargo test sync_command_center_does_not_parse_human_summary_strings dashboard_shell_and_assets_wire_sync_command_center -- --test-threads=1
```

预期：PASS。

- [ ] **步骤 5：Commit guard 切片**

```powershell
git add src/web/mod.rs src/web/assets/render/sync-command-center.js src/web/assets/app.js src/web/assets/copy.js
git diff --staged --check
git commit -m "test(web): guard sync command center truthfulness" -m "Constraint: human summaries remain display-only and are not data contracts" -m "Rejected: regex extraction from run summaries | structured SyncEvent fields already exist" -m "Confidence: high" -m "Scope-risk: narrow" -m "Tested: cargo test sync_command_center_does_not_parse_human_summary_strings dashboard_shell_and_assets_wire_sync_command_center -- --test-threads=1" -m "Not-tested: full cargo suite pending"
```

---

## 任务 6：端到端回归与本地浏览器核验

**文件：**
- 可能修改：前面任务暴露的小问题对应文件
- 不修改：`ref/`、`.superpowers/`、`target/`、`docs/node_modules/`、`docs/.vitepress/cache/`、`docs/.vitepress/dist/`

- [ ] **步骤 1：运行相关测试集合**

```powershell
cargo test api_dashboard_embeds_sync_command_center_contract dashboard_data_layers_pass_through_sync_command_center dashboard_shell_and_assets_wire_sync_command_center dashboard_assets_style_sync_command_center_responsively sync_command_center_does_not_parse_human_summary_strings -- --test-threads=1
```

预期：全部 PASS。

- [ ] **步骤 2：运行 web 模块更宽测试**

```powershell
cargo test web::tests:: -- --test-threads=1
```

预期：全部 PASS。若命令过滤不被 Cargo 接受，改用：

```powershell
cargo test --lib web::tests -- --test-threads=1
```

- [ ] **步骤 3：运行全量 Rust 测试**

```powershell
cargo test -- --test-threads=1
```

预期：全部 PASS。

- [ ] **步骤 4：运行格式与 clippy**

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

预期：全部 PASS。

- [ ] **步骤 5：启动本地 dashboard 做浏览器核验**

启动服务：

```powershell
cargo run -- serve
```

打开输出的 localhost 地址，检查：

- 顶部出现 `Sync Command Center`，在 hero/kpi 之前。
- 空库时显示“尚未同步/Not synced yet”，没有假 token/cost delta。
- 普通库有 source cards、inserted delta、stored events、worker lock 状态。
- 有 `source_file.state='missing'` 且受保护事件时，文案明确为普通 sync 安全、`sync --rebuild` 有风险。
- 点击 command center 主按钮会触发现有 `#btn-sync` 行为；running 状态显示 Cancel sync。
- 720px 宽度无横向溢出，source cards 单列。

如果当前环境不能使用浏览器工具，至少运行：

```powershell
cargo run -- serve
```

并用浏览器/HTTP 手动请求 `/api/dashboard` 确认 JSON 字段存在；在最终报告中明确“浏览器视觉核验未运行”。

- [ ] **步骤 6：最终 diff 检查**

```powershell
git diff --check
git status --short
```

预期：无 whitespace 错误；只剩本功能相关文件。保留既有未跟踪 `.superpowers/`，不要提交。

- [ ] **步骤 7：最终功能 commit 或确认已有切片 commit**

如果前面已经按任务分片 commit，这一步只记录状态：

```powershell
git log --oneline -5
```

如果实施者选择不分片 commit，则一次性提交：

```powershell
git add src/query/mod.rs src/web/mod.rs src/web/shell.rs src/web/assets/data/fetch.js src/web/assets/data/derive.js src/web/assets/render/sync-command-center.js src/web/assets/app.js src/web/assets/mod.rs src/web/assets/copy.js src/web/assets/components.css src/web/assets/layout.css
git diff --staged --check
git commit -m "feat(web): add sync command center" -m "Constraint: preserve SyncShard protocol and expose only structured sync facts" -m "Rejected: TokenTracker queue model | llmusage remains SQLite and SyncShard based" -m "Rejected: frontend summary parsing | backend view-model owns the contract" -m "Confidence: medium" -m "Scope-risk: moderate" -m "Directive: do not claim token or cost delta until run logs persist before-after totals" -m "Tested: cargo test -- --test-threads=1; cargo fmt --check; cargo clippy --all-targets --all-features -- -D warnings" -m "Not-tested: docs build unless docs assets changed"
```

---

## 自检结果

### 规格覆盖度

- Safety first：任务 1 的 `SyncSafetyPayload`、任务 3/4 的顶部渲染覆盖。
- Latest/current sync summary second：任务 1 的 `last_run` / `metrics`、任务 3 的 renderer、任务 5 的 job overlay 覆盖。
- 结构化契约：任务 1/2 后端与数据透传覆盖。
- 禁止 fake delta / summary parsing：任务 5 guard 覆盖。
- TokenTracker 风格但不复制数据模型：任务 3/4 视觉实现覆盖；任务边界明确不改 `SyncShard`。
- 普通 sync 与 rebuild 风险区分：任务 1 safety、任务 3 copy、任务 6 浏览器核验覆盖。
- 现有 job lifecycle：任务 3 通过 `#btn-sync` 复用；任务 5 guard 覆盖。

### 占位符扫描

已扫描并移除技能禁止的占位式表述。每个代码变更步骤包含目标路径、代码或精确插入内容、命令和预期结果。

### 类型一致性

- 后端字段名统一为 snake_case JSON：`sync_command_center`、`inserted_delta`、`stored_events`、`lossy_rebuild_risk`。
- 前端 context 字段统一为 camelCase：`syncCommandCenter`。
- renderer 使用 `center.headline_key` / `center.reason_key` 调用 `getShellCopy`，copy keys 在 `SHELL_COPY_ZH/EN` 中定义。
- job overlay 只读取 `activeJobSnapshot.last_event.summary.total_inserted` 等结构化字段，不读取 `activeJobSnapshot.summary`。

### 验证策略

最小验收为任务 6 步骤 1-4。视觉任务还需要任务 6 步骤 5 的浏览器核验；如果无法运行，最终报告必须列出该验证缺口。
