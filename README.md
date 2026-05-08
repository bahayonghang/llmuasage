# llmusage

[简体中文](./README.zh-CN.md)

Local-first Rust CLI for AI coding usage analytics.

The goal is simple: use hooks and a local SQLite database to track multiple AI coding CLIs without upload, login, or any cloud API.

Thanks to [vibeusage](https://github.com/victorGPT/vibeusage) for the original idea. `llmusage` is a Rust rewrite and improvement built on that foundation, with a stricter local-first path.

Current 0.5.0 coverage:

- Codex `config.toml notify`
- Claude `Stop` / `SessionEnd` hooks
- OpenCode `session.updated` plugin event
- Gemini `SessionEnd` hooks and `~/.gemini/tmp/*/chats/session-*.json` parsing

Core sources of truth:

- Config directory: `~/.llmusage/`
- Database: `~/.llmusage/llmusage.db`
- Hook wrappers: `~/.llmusage/bin/llmusage-hook.cmd`, `~/.llmusage/bin/llmusage-hook.sh`

Commands:

Report-first commands (read local SQLite only; run `llmusage sync` first if data looks stale):

- `llmusage` / `llmusage daily`
- `llmusage monthly`
- `llmusage session`
- `llmusage blocks`
- `llmusage statusline`

`llmusage` / `llmusage daily` defaults to today's report in the selected timezone. Use `--all` for full daily history, or `--since YYYYMMDD` / `--until YYYYMMDD` for an explicit range.

Common report options include `--since YYYYMMDD`, `--until YYYYMMDD`, `--json`, `--breakdown`, `--order asc|desc`, `--timezone UTC|local|+08:00`, `--locale en-US|zh-CN|ja-JP`, `--compact`, and `--source codex|claude|opencode|gemini`.

Operational commands:

- `llmusage init`
- `llmusage sync` (`--rebuild` reparses local sources and rebuilds usage rows/buckets; progress is printed to stderr, while `--json-events` prints NDJSON lifecycle/progress events to stdout)
- `llmusage status`
- `llmusage diagnostics` (`--forget-file <PATH>` can mark a source file as intentionally ignored)
- `llmusage doctor` (`--refresh-pricing <file>` imports a local pricing snapshot and recomputes costs)
- `llmusage serve`
- `llmusage tui`
- `llmusage export html`
- `llmusage uninstall`

Web dashboard:

Below is the local browser dashboard served by `llmusage serve`.

![llmusage web dashboard overview](./docs/public/screenshots/web-dashboard-overview.png)

Development:

```powershell
cargo check
cargo test
cargo run -- init
cargo run -- sync
cargo run -- --json
cargo run -- serve
```

Notes:

- `serve` only binds to `127.0.0.1` and opens the dashboard in your default browser
- `export html` generates an offline static report
- report commands are read-only SQLite views and do not auto-sync
- `status` and `diagnostics` are read-only unless `diagnostics --forget-file` is used
- `doctor` is read-only unless `--refresh-pricing <file>` is used; pricing refresh only reads a local JSON file and writes local SQLite metadata/costs

## 0.5.0 highlights

- ccr-ui-facing read APIs: `Dashboard::overview`, `home_overview`, `heatmap`, `logs`, archive diagnostics, and source-file forget support.
- In-process import jobs through `JobRegistry` with progress snapshots and cancellation.
- Full schema migrations from v0/v1 through v10, including cache split, cost metadata, source-file state, raw archive, worker lock metadata, Gemini registration, and `pricing_catalog_version`.
- Stable snake_case JSON across CLI reports, HTTP API, and static exports.
- Public `LlmusageError` and `testing::Fixture` surfaces for downstream adapters.

## Library API (0.5.0)

The 0.5.0 line exposes a SemVer-stable library surface for desktop adapters such as ccr-ui:

```rust
use llmusage::{
    paths::AppPaths,
    query::{Dashboard, QueryFilter},
    store::Store,
    Result,
};

fn open_store(root: std::path::PathBuf) -> Result<Store> {
    let paths = AppPaths::with_root(root)?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    Ok(store)
}

fn load_ccr_ui(store: &Store) -> Result<()> {
    let filter = QueryFilter::default();
    let dashboard = Dashboard::open(store)?;
    let _overview = dashboard.overview(&filter)?;
    let _home = dashboard.home_overview(&filter)?;
    let _heatmap = dashboard.heatmap(&filter, 365)?;
    let _logs = dashboard.logs(&Default::default())?;
    Ok(())
}
```

Path resolution order is `--home <PATH>` first, then `LLMUSAGE_HOME`, then `~/.llmusage`.
The ccr-ui surface now includes filtered dashboard/home/heatmap/log queries, archive diagnostics from the `source_file` state machine, and `JobRegistry::start/get/cancel` for in-process import jobs. Use `Store::acquire_worker_lock_with` when embedding sync so CLI, library, and hook workers share one local lock.

For downstream integration tests, depend on the local crate with the testing
feature and use the isolated fixture helpers:

```toml
[dev-dependencies]
llmusage = { path = "../llmusage", features = ["testing"] }
```

```rust
let fixture = llmusage::testing::Fixture::new()?;
fixture.seed_dashboard(12)?;
let overview = llmusage::Dashboard::open(fixture.store())?.overview(&Default::default())?;
```
