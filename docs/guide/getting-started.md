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
- `llmusage` without a subcommand prints today's daily report from the local DB. Use `llmusage daily --all` for full history, or `llmusage monthly`, `llmusage session`, and `llmusage blocks` for other report views.
- `serve` starts the browser dashboard on `127.0.0.1` and opens it in your default browser.

Report commands are read-only and never upload data. They also do not auto-sync; run `llmusage sync` again when source data changes. Use `--source codex|claude|opencode|gemini` to restrict reports or sync. If you need to repopulate new session/source-file metadata after upgrading, run `llmusage sync --rebuild`.

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
    let _overview = dashboard.overview(&filter)?;
    let _home = dashboard.home_overview(&filter)?;
    let _heatmap = dashboard.heatmap(&filter, 365)?;
    let _logs = dashboard.logs(&Default::default())?;
    Ok(())
}
```

Runtime root resolution is `--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`.
The 0.5.0 ccr-ui surface includes `Dashboard::overview`, `home_overview`, `heatmap`, `logs`, archive diagnostics from the `source_file` state machine, and in-process import jobs through `JobRegistry`. Runtime root resolution is shared by CLI and library entry points: `--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`.

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
