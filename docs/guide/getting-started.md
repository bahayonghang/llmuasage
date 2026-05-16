# Getting Started

`llmusage` is a Rust CLI that keeps the entire analytics path local.

## Requirements

- Rust stable toolchain
- Node.js 20+
- npm 10+
- `just`

## Install dependencies

```powershell
just install
```

This does two things:

- installs the VitePress docs dependencies under `docs/`
- installs the CLI from the current checkout with `cargo install --path . --locked --force`

## Run the local flow

```powershell
llmusage init
llmusage sync
llmusage
llmusage serve
```

### What each step does

- `init` prepares `~/.llmusage/`, creates `llmusage.db`, generates hook wrappers, and installs Codex / Claude / OpenCode / Gemini integrations.
- `sync` parses local sources incrementally and upserts usage data into SQLite. It shows human progress on stderr; use `--json-events` when a script needs NDJSON lifecycle/progress events.
- `llmusage` without a subcommand prints the last 7 calendar days from the local DB, including today, in one aggregate ccusage-style daily table. The default table shows `Input`, `Output`, `Cache Create`, `Cache Read`, `Total Tokens`, and `Cost (USD)` with full comma-grouped token counts on wide terminals; `NO_COLOR=1` disables ANSI styling. `llmusage daily --json` keeps the stable aggregate snake_case JSON shape and includes `cache_creation_tokens`. Use `llmusage daily --all` for full history, `--source` / `--breakdown` for source/model detail, or `llmusage monthly`, `llmusage session`, and `llmusage blocks` for other report views.
- `serve` starts the browser dashboard on `127.0.0.1` and opens it in your default browser. The first screen uses one filtered dashboard snapshot for KPI, trend, ranking, health, and diagnostics panels; live mode can export the current view as JSON, run a sync job with cancel/progress state, or opt into 30s/60s auto-refresh.

Report commands are read-only and never upload data. They also do not auto-sync; run `llmusage sync` again when source data changes. Use `--source codex|claude|opencode|gemini` to restrict reports or sync. If you need to repopulate new session/source-file metadata after upgrading, run `llmusage sync --rebuild` while the original source files are still present. If a Codex/Claude/Gemini file was already cleaned up, normal `llmusage sync` keeps the imported history and diagnostics will show missing source files; `--rebuild` is refused unless you add `--allow-lossy-rebuild` to explicitly clear unrebuildable history. If you maintain a local pricing snapshot, run `llmusage doctor --refresh-pricing <file>`; llmusage stores it under `~/.llmusage/pricing/<catalog-version>.json`, recomputes event/bucket costs, and keeps later sync writes on that local catalog.

## Verify the repo

```powershell
just ci
```

`ci` runs format, clippy, tests, and a VitePress production build.

## Library API preview

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
    let _snapshot = dashboard.snapshot(&filter)?;
    let _overview = dashboard.overview(&filter)?;
    let _daily = dashboard.trends_daily(&filter)?;
    let _home = dashboard.home_overview(&filter)?;
    let _heatmap = dashboard.heatmap(&filter, 365)?;
    let _logs = dashboard.logs(&Default::default())?;
    Ok(())
}
```

Runtime root resolution is `--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`.
The 0.5.x ccr-ui surface includes `Dashboard::overview`, `trends_daily`, `home_overview`, `heatmap`, `logs`, archive diagnostics from the `source_file` state machine, persisted cost/pricing/cache fields, and in-process import jobs through `JobRegistry`. Runtime root resolution is shared by CLI and library entry points: `--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`.

Downstream adapters can enable fixture helpers for integration tests:

```toml
[dev-dependencies]
llmusage = { path = "../llmusage", features = ["testing"] }
```

```rust
let fixture = llmusage::testing::Fixture::new()?;
fixture.seed_dashboard(12)?;
let overview = llmusage::Dashboard::open(fixture.store())?.overview(&Default::default())?;
```
