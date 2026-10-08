# CLI command surface

## Question

How should `llmusage clean` be registered, and which existing commands already print tables or delete runtime files?

## Findings

- Entry is `llmusage::run()` in `src/lib.rs:99-107`. It parses `commands::Cli` and dispatches `commands::dispatch`.
- Subcommands live on `Commands` in `src/commands/mod.rs:54-217`. There is no `clean` variant. Dispatch is a match in the same file (doctor at `437-440`, logs at `450-455`, uninstall later in the match).
- Top-level help is a hand-written bilingual table, not clap's generated help. English rows are `ENGLISH_COMMANDS` in `src/commands/help.rs:320-366`; Chinese rows are the matching list further down. A new command must be added to both, or `llmusage --help` will omit it.
- Human tables for usage reports are built by private helpers in `src/tui/report_table.rs` (`render_table` at line 965, box-drawing borders). `doctor`, `logs`, and `catalog status` do not use that renderer; they print labeled lines or `--json`.
- Destructive precedents:
  - `remote remove --delete-usage` refuses without `--yes` (`src/commands/remote.rs:118`).
  - `update` asks for interactive confirmation (`src/commands/update.rs:157`, `365`).
  - `uninstall --purge` deletes the entire runtime root with no second prompt (`src/commands/uninstall.rs:33-36`).
  - `doctor` is read-only except `--refresh-pricing` (`src/commands/doctor.rs:19-24`, `48-57`).
- Global `--home` overrides the runtime root (`src/commands/mod.rs:44-46`). `clean` must honor it. `AppPaths::with_cli_home(None)` does **not** read `LLMUSAGE_HOME`; `AppPaths::discover()` does (`src/runtime/paths.rs:32-74`). The running CLI uses `with_cli_home`, so the env var is not a second override once `--home` is absent.

## Recommendation

Register `Clean` next to `doctor` / `logs`. Default to a read-only overview. Require an explicit `--yes` before any delete, matching `remote remove --delete-usage` rather than `uninstall --purge`. Render the overview with a small dedicated table; do not reuse the usage-report column model.

## Sources

- `src/lib.rs`
- `src/commands/mod.rs`
- `src/commands/help.rs`
- `src/commands/doctor.rs`
- `src/commands/uninstall.rs`
- `src/commands/remote.rs`
- `src/tui/report_table.rs`
