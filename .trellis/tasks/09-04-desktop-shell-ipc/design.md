# Desktop shell and IPC — Design

沿用父任务 `design.md` 的冻结 DTO、错误码、启动序与取消语义。本子任务只落实进程与 command 层。

## 本子任务边界

- 新建 `desktop/` 与 `desktop/src-tauri`。
- React 可放最小占位页（调用 `runtime_info`），完整壳留给 core-ui。
- 次级 command 返回真实 façade JSON，即使 UI 尚未消费。

## Change list

| 文件 | 动作 | 关键符号 |
|---|---|---|
| `desktop/package.json`、`desktop/src-tauri/tauri.conf.json`、`desktop/src-tauri/Cargo.toml` | 新建 | identifier `com.bahayonghang.llmusage`；`llmusage = { path = "../..", features = ["testing"] }`（dev） |
| `desktop/src-tauri/src/main.rs` | 新建 | `tauri::Builder`、command 注册、single-instance |
| `desktop/src-tauri/src/state.rs` | 新建 | `AppState`、`startup(app: AppContext)` |
| `desktop/src-tauri/src/dto.rs` | 新建 | `FilterDto`、`convert_filter`、`convert_explorer`、`convert_logs`、`convert_sync` |
| `desktop/src-tauri/src/error.rs` | 新建 | `map_llmusage_error` → 稳定 `code` |
| `desktop/src-tauri/src/supervisor.rs` | 新建 | `DesktopQuerySupervisor` |
| `desktop/src-tauri/src/commands/*.rs` | 新建 | 父 command 表 |
| `src/query/mod.rs` | 修改 | `Dashboard::interrupt_handle`：`pub(crate)` → `pub` |
| `desktop/src-tauri/tests/*.rs` 或 `src` 内 `#[cfg(test)]` | 新建 | DTO/bootstrap/lock/cancel/sync |

## Contract

父 `design.md` Contract 整节对本子任务有效。补充实现要点：

```text
startup(root: Option<PathBuf>) -> Result<AppState>
  app = match root { Some(r) => AppContext::with_cli_home(Some(r)), None => AppContext::discover() }
  store = Store::new(&app.paths)?
  store.bootstrap()?                          // SchemaTooNew 映射 schema_too_new，随后不 repair
  repair_legacy_token_accounting(&app, &store).await?
  jobs = JobRegistry::default()
  jobs.register_terminal_hook(|| diagnostics_cache.invalidate())

convert_filter(dto: FilterDto) -> Result<QueryFilter, DesktopError>
convert_sync(dto: SyncStartDto) -> Result<SyncOptions, DesktopError>
  SyncOptions { rebuild: false, recent_days: dto.recent_days, source: dto.source, parallelism: None }

run_query(request_id, timeout, f) -> Result<T>
  permit (max 4) → spawn_blocking:
    dash = Dashboard::open_with_busy_timeout(&store, 1500ms)
    handle = dash.interrupt_handle()
    supervisor.attach(request_id, handle)
    if cancelled: return Cancelled
    f(&dash)
  on timeout/cancel_queries: handle.interrupt(); supervisor.supervise(JoinHandle)

fetch_quota(bypass_cache) -> QuotaResponse
  ctx.user_home = injected.or_else(resolve_home_dir)
  ctx.cache_path = Some(paths.subscription_cache_path())
  cache_hit = !bypass_cache && cache_file_age <= 300s
  report = fetch_all(&ctx, bypass_cache)
```

`runtime_info.lock` = `store.current_worker_lock()?`。

## 取消

与 web `DashboardQuerySupervisor` 同形：截止点返回、后台收束、permit 留在阻塞闭包直到退出。前端 generation 由 core 实现；本层提供 `request_id` 与 `cancel_queries`。

## Verification boundary

- `cargo test --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1`
- 改了 `src/query/mod.rs` 时加跑 `python scripts/ci-rust.py`
- 不跑 `tauri build`、不满跑 `just ci`（除非误改根 CI 文件）
- 单实例焦点：配置存在即可；人工焦点归父门禁

## 已考虑不做

- 给 `QueryFilter` 等加 `Deserialize`
- 只做代际抑制（父任务已选真实取消）
- 把 `AppContext` 放进每个 command 入参
- 用 `AppPaths.root_dir` 当额度 `user_home`
- 把 `Dashboard` 放进 Tauri managed state（`Connection` 非 `Sync`）
