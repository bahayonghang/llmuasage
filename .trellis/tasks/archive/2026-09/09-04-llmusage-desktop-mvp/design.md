# llmusage Desktop MVP — Design

DTO、转换、取消与启动契约的可执行真源在 `09-04-desktop-shell-ipc/design.md`。本节给出任务树共用冻结面。core / secondary / ops 只引用这些类型与错误码，不另造载荷。

## Architecture

```text
desktop/src (React 19 + Vite)
  runtime/          invoke 包装，唯一跨进程边界
  app/              壳、导航、筛选、主题、i18n、load-state
  features/*        各面板只渲染 façade JSON
        │  Tauri IPC (snake_case JSON)
desktop/src-tauri
  AppState { paths, store, jobs: JobRegistry, diagnostics_cache, query_supervisor }
        │  path dep
llmusage crate  (AppPaths, AppContext, Store, Dashboard, JobRegistry, subscription)
        │
~/.llmusage/llmusage.db + cache/subscription-usage.json + desktop.json
```

根 crate 保持单包。`desktop/src-tauri` 是独立 Cargo 包：`llmusage = { path = "../.." }`。不把 Desktop 加进根 workspace，避免改 MSRV/`--all-targets` 图。

## Change list（任务树文件归属）

| 路径 | 所有者 | 动作 |
|---|---|---|
| `desktop/**` 脚手架、`src-tauri` command/DTO/supervisor | shell-ipc | 新建 |
| `src/query/mod.rs` `Dashboard::interrupt_handle` 可见性 `pub(crate)`→`pub` | shell-ipc | 窄补丁 |
| `desktop/src/runtime`、`app`、核心 features、运行状态、筛选、sync 控件 | core-ui | 新建 |
| `desktop/src/features` 次级面板与 `home_overview` 六卡 | secondary-ui | 新建 |
| logs/CSV/prefs/quota features | ops-quota | 新建 |
| `justfile` recipes、`.gitignore` desktop 行、README/`docs/dashboard` Desktop 入口、`tauri.conf.json` nsis | windows-bundle | 修改/新建 |
| `src/commands/serve.rs` 监听契约、`src/web/**` 产品行为、`.github/workflows/ci.yml` 的 `CI gate` `name:` | 无人 | 禁止 |

## Contract

### 启动

```text
production:
  app = AppContext::discover()                          // llmusage::app::AppContext
  paths = app.paths                                     // 与 AppPaths::discover() 同一根
  store = Store::new(&paths)
  store.bootstrap()                                     // SchemaTooNew → 停，不写
  repair_legacy_token_accounting(&app, &store).await
  jobs = JobRegistry::default()
  jobs.register_terminal_hook(invalidate_diagnostics_cache)

tests:
  app = AppContext::with_cli_home(Some(temp_root))
  禁止 AppPaths::discover() / 真实 ~/.llmusage
```

`AppContext` 是启动期唯一额外允许的根类型，query command 签名不再接收它。

### 配额 FetchContext

```text
production:
  user_home  = llmusage::util::resolve_home_dir()     // 凭证根，不是 AppPaths.root_dir
  cache_path = paths.subscription_cache_path()        // {root}/cache/subscription-usage.json
  ctx = FetchContext { endpoints: UsageEndpoints::production(), user_home, cache_path: Some(cache_path), timeout: 8s }

tests:
  user_home  = 临时目录（放入夹具凭证）
  cache_path = Fixture root 下文件
  endpoints  = UsageEndpoints { 全部指向 127.0.0.1 本地监听 }
```

### 错误码

JSON `{ "code": "<snake>", "message": "...", "holder": optional }`:

| code | 来源 |
|---|---|
| `not_initialized` | bootstrap 前的空库；成功启动后不应再出现 |
| `schema_too_new` | `LlmusageError::SchemaTooNew` |
| `lock_busy` | `LlmusageError::LockBusy`；`holder` 原样 |
| `lock_lost` | `LlmusageError::LockLost` |
| `job_active` | `JobStartError::Active` |
| `invalid_request` | DTO 转换失败；`SyncRequestError` |
| `cancelled` | `LlmusageError::Cancelled`（interrupt 成功） |
| `timeout` | 命令硬截止后 interrupt，请求未来在截止点返回 |

行为四块 3s 截止返回 **degraded JSON**，`code` 不走错误通道。

### Desktop 请求 DTO（冻结）

根查询输入类型保持 `Debug/Clone`，不加 `Deserialize`。Desktop 在 `desktop/src-tauri/src/dto.rs` 定义并转换。

```rust
// 字段均为 serde rename_all = "snake_case"
struct FilterDto {
    source: Option<String>,
    model: Option<String>,
    since: Option<String>,       // YYYY-MM-DD
    until: Option<String>,
    project_hash: Option<String>,
    host_id: Option<String>,
    timezone: Option<String>,    // IANA / "utc" / "local"；缺省 → 本机 IANA
}

struct InteractiveRequest {
    request_id: u64,
    filter: FilterDto,
    window: String,              // day | week | month | all
}

struct SecondaryRequest {
    request_id: u64,
    filter: FilterDto,
}

struct TopSessionsDto {
    request_id: u64,
    filter: FilterDto,
    sort: String,                // tokens | duration | cost
    limit: Option<u32>,          // 0/缺省 → 10；clamp 1..=50
}

struct ExplorerDto {
    request_id: u64,
    filter: FilterDto,
    granularity: String,
    metric: String,
    group_by: String,
    session_id: Option<String>,
    tool_name: Option<String>,
    tool_kind: Option<String>,
    token_type: Option<String>,
    include_other: Option<bool>,     // 缺省 true
    include_non_tool: Option<bool>,  // 缺省 true；false → filters.is_tool = Some(true)
    limit: Option<u32>,              // 缺省 8；clamp 1..=50
}

struct LogsDto {
    request_id: u64,
    filter: FilterDto,
    page_size: u32,              // Desktop 固定传 20；0 或 >500 → invalid_request
    cursor: Option<String>,
    include_total: Option<bool>,
    include_raw_json: Option<bool>,
    session: Option<String>,
    event_key: Option<String>,
}

struct SyncStartDto {
    source: Option<String>,
    recent_days: Option<u32>,    // 无 rebuild / since / until
}

struct CancelQueriesDto {
    request_ids: Vec<u64>,
}

struct QuotaResponse {
    cache_hit: bool,
    report: UsageFetchReport,    // 直接序列化 outputs + diagnostics
}

struct RuntimeInfoDto {
    version: String,
    root_dir: PathBuf,
    db_path: PathBuf,
    schema_version: u32,
    lock: Option<WorkerLockMeta>, // Store::current_worker_lock()
}

struct PrefsDto {
    theme: String,               // light | dark
    locale: String,              // zh | en
    auto_refresh_ms: u64,        // 0 | 30000 | 60000
    filter: FilterDto,
    window: String,
    range_preset: String,        // 1d | 7d | 30d | all | custom
}
```

### 受检转换

| 输入 | 成功 | `invalid_request` |
|---|---|---|
| `source` | `SourceKind::parse_id` | 非空且解析失败 |
| `since`/`until` | `%Y-%m-%d` | 非法日期；`until < since` |
| `timezone` 缺省 | 本机 IANA → `ReportTimezone::Iana`；取不到时 `Local` | — |
| `timezone` 有值 | `utc`/`Z`→Utc；`local`→Local；IANA `Tz::from_str` | 无法解析的名字（不静默变 Local） |
| `window` | `day\|week\|month\|all` | 其它 |
| Explorer 枚举 | 各 `::parse` | 未知 metric/group_by/granularity/token_type |
| `page_size` | 恰好 20 | 其它 |
| `recent_days` | `None` 或 `1..=3650` | 0 或 >3650 |
| `SyncStartDto` | `SyncOptions { rebuild: false, recent_days, source, parallelism: None }` | 未知 source |

查询 1d/7d/30d 的 `since`/`until` 由 **core** 按 `src/web/mod.rs` `apply_window_filter` 写入 FilterDto：`1d` = yesterday..=today，`7d` = today-6..=today，`30d` = today-29..=today，`all` 不传日期，`custom` 传控件日期。shell 不再二次套 range。

### Command 签名

读路径：登记 `request_id` → `spawn_blocking` → `Dashboard::open_with_busy_timeout(1500ms)` → 取出 `interrupt_handle()` 发给 supervisor → 方法 → JSON。禁止把 `Dashboard` 放进 Tauri managed state。

| command | 入参 | 调用 | 硬截止 |
|---|---|---|---|
| `runtime_info` | — | `AppPaths` + schema + `Store::current_worker_lock` | — |
| `dashboard_interactive` | `InteractiveRequest` | `interactive_snapshot_with_diagnostics` | 6s → `timeout` |
| `home_overview` | `SecondaryRequest` | `Dashboard::home_overview` | 5s → 错误，UI 标 degraded |
| `heatmap` / `trends_daily` / `hour_of_week` | `SecondaryRequest` | 对应方法 | 5s |
| `top_sessions` | `TopSessionsDto` | `Dashboard::top_sessions` | 5s |
| `activity` / `tools` / `optimize` / `compare` | `SecondaryRequest` | 对应方法 | **3s → degraded JSON** |
| `explorer` | `ExplorerDto` | `Dashboard::explorer` | 5s；不套 3s 行为超时 |
| `logs` | `LogsDto` | `Dashboard::logs`，`page_size=20` | 5s |
| `diagnostics` | — | TTL≈30s 缓存；sync 终态 hook 失效 | — |
| `start_sync` | `SyncStartDto` | `JobRegistry::try_start(&store, SyncOptions)` `HolderKind::Library` | — |
| `job_snapshot` | `id: String` | `JobRegistry::snapshot` | — |
| `cancel_job` | `id: String` | `JobRegistry::cancel` | — |
| `cancel_queries` | `CancelQueriesDto` | supervisor.interrupt(ids) | — |
| `fetch_quota` | `bypass_cache: bool` | `subscription::fetch_all`；包装 `cache_hit` | 8s 上下文超时 |
| `load_prefs` / `save_prefs` | `PrefsDto` | `{root}/desktop.json` | — |

返回类型：façade 已 `Serialize` 的 payload 直接 JSON；`DashboardInteractiveSnapshot` 不必提升到根 `pub use`。

### 真实取消（TPR-02 / 1A）

对标 `src/web/mod.rs` `DashboardQuerySupervisor` + `src/web/assets/app.js` AbortController。

1. 每个读 command 携带 `request_id`（前端 generation 内单调递增）。
2. `DesktopQuerySupervisor`：permit=4；登记 `request_id → InterruptHandle`；`cancel_queries` 与硬截止都调用 `interrupt()`。
3. 截止到达时：interrupt、把 `JoinHandle` 交给 supervisor 后台收束，**请求未来在截止点返回**，不等待阻塞闭包退出。permit 由闭包 Drop 释放。
4. 前端：筛选变更 → `generation++` → `cancel_queries(旧 ids)` → 新 invoke；只接受当前 generation。
5. 观测：supervisor 快照 `inflight` / `timed_out_tasks` / `orphaned_tasks`；测试用可中断的慢查询证明旧工作停止后 `inflight==0`。

根 crate 窄补丁：`Dashboard::interrupt_handle` 改为 `pub`。

## 前端数据流

对标 `src/web/assets/load-state.js`：

1. 筛选变更 → 新 generation，`cancel_queries` + 新 invoke。
2. 先 `dashboard_interactive`（核心）。2s 标慢，6s 失败并取消。
3. 核心成功后**立即绘制** overview/trends/models/sources/hosts/projects/costs/sync/health/diagnostics。然后再跑次级列表，并发 2：`home_overview`、`heatmap`、`trends_daily`、`top_sessions`、`hour_of_week`、`activity`、`tools`、`optimize`、`explorer`、`compare`。`home_overview` 失败不回退核心绘制。
4. 只接受当前 generation 的结果。次级 degraded 只更新自己的块。
5. 自动刷新 30s/60s 走同一路径。sync 终态后刷新核心+次级。

`runtime/` 以外禁止直接 `invoke`。

额度是独立块：进入时 `fetch_quota(false)`，刷新 `fetch_quota(true)`。失败只填额度 diagnostics，不重载看板。

CSV 在前端由已加载 payload 生成（移植 `csv-export.js` 规则），再系统保存对话框写文件。

## 运行状态 UI 归属（TPR-03）

归 **core-ui**。数据：`runtime_info`（`root_dir`、锁）+ 核心快照的 `health`/`diagnostics` + command 错误码。

| 状态 | 可观察条件 |
|---|---|
| idle | 无 running job，无 lock，无失败 |
| running | `JobSnapshot.status == Running` 或 lock busy 且为本进程 job |
| failed | 最近 job Failed，或 last_run failed |
| lock_busy | `lock_busy` / `worker_lock=busy`，展示 `holder` |
| lock_lost | command 返回 `lock_lost`，告警，停止写 |
| diagnostics 可用 | `diagnostics` 对象非空可渲染 |
| diagnostics 不可用 | 命令失败；块内明确空态，不装成 0 |

## 同步映射归属（TPR-04）

- core：控件 → `SyncStartDto`（R8.9 表）。
- shell：`SyncStartDto` → `SyncOptions { rebuild: false, ... }` → `ValidatedSyncRequest`。

## 视觉（TPR-09 / 2A）

Token 从 `src/web/assets/base.css` 复制语义变量（`--bg-primary`、`--bg-surface`、`--text-primary`、`--accent` / `--accent-blue`、`--good`/`--warn`/`--danger` 及 dark 覆盖）。侧栏 248px、≤720px 折叠抄 `layout.css`。空 hosts：行数 ≤ 1 时不渲染 hosts 面板（对齐 `render/hosts.js`）。

## 锁与生命周期

- 启动序见 Contract。`SchemaTooNew` 不写库。
- `JobRegistry` 放在进程内 managed state。与 CLI 争的是 SQLite `worker_lock`。
- 单实例插件：第二进程唤醒主窗口后退出。
- 无托盘：关窗口即退出；退出前 `cancel_queries(all)` 与 `cancel_job`（尽最大努力，锁租约仍靠既有 expiry）。

## 打包

- `tauri.conf.json` identifier：`com.bahayonghang.llmusage`
- Windows bundle：`nsis`。不设 updater pubkey/endpoints。
- `.gitignore`：`desktop/node_modules/`、`desktop/dist/`、`desktop/src-tauri/target/`
- `just desktop-dev` / `just desktop-test` / `just desktop-build`
- 根 `just ci` 与 `.github/workflows/ci.yml` 的 `CI gate` 作业名不动。本任务不把 `tauri build` 加进该门禁。
- 最终 NSIS 在 shell+core+secondary+ops 均完成后由 windows-bundle 产出。

## Compatibility

- CLI `serve` / `dash` / `sync` 行为不变。
- Desktop 与 CLI 共用库文件；双写由既有 fencing 拒绝。
- `subscription` 缓存路径仍是 `AppPaths::subscription_cache_path()`，与 TUI 共用。

## Verification boundary

- 自动化：Desktop `cargo test`（Fixture、DTO 转换、取消/interrupt、lock、schema_too_new、rebuild=false、配额本地 endpoints）；前端单测映射表与 generation。
- 人工：最终 `tauri dev` 或安装包走 AC1–AC27。
- 不验证：macOS/Linux 编译；签名；SmartScreen 解除；公网额度主机。

## 已考虑不做

- 给根查询输入加 `Deserialize`：会污染 crate serde 边界。
- 仅代际抑制、不 interrupt：已否决（TPR-02 / 1A）。
- WebView 包 serve：违背路径 B。
- 把 Desktop 加进根 workspace / 把 `tauri build` 加进 `just ci`。
- 用 `DESIGN.md` 暖色或 Catppuccin 草案验收（TPR-09 / 2A）。
- 把 `home_overview` 放进核心请求：会破坏 AC6/AC26。
- 用 `AppPaths.root_dir` 当额度 `user_home`：凭证在 OS home。

## Trade-offs

- 重写 React 看板而不是 WebView 包 serve：满足路径 B，代价是面板对等要按子任务铺。
- 前端编排次级并发，而不是一个巨大 command：保留逐块降级；代价是 IPC 次数与 serve 的 HTTP 次数同量级。
- 独立 src-tauri 包而不是根 workspace：保护现有 CI 图；代价是两份 Cargo.lock。
- 未签名 NSIS：能本机安装；Windows SmartScreen 可能告警，本任务接受。
- 真实取消：对齐 live serve，代价是 supervisor 与根 crate 一处 `pub` 补丁。

## Rollback

删除 `desktop/`、还原 `justfile` / `.gitignore` / 文档、还原 `Dashboard::interrupt_handle` 可见性。根 crate 无强制 schema 变更。若误改了 `src/web` 或 lock 契约，按那些文件的 git 历史回退。
