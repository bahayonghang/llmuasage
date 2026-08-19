# Implement

1. Add `RunLog::recent_usage_import_runs_with_conn` filtering `sync` / `sync --rebuild` / `hook-run`, ordered by `id DESC`.
2. Switch command-center last-run, failed headline, `recent_failures` count, and `error_key` to that window and `status == "failed"`.
3. Pair `headline_key` with `reason_key` (failed with `lastRunFailed`; rebuild risk with `rebuildRisk`). Keep busy pairing as-is.
4. Add query tests for the screenshot mix, serve-noise window, and last-run `failed` + rebuild risk.
5. Assert headline/reason on the existing dashboard API contract test.
6. Document the command-center contract in `source-sync-contracts.md`.
7. Validate: focused `query::tests` + `web::tests` command-center cases, then `python scripts/ci-rust.py` if time allows a full slice.

## Validation

```
cargo test --lib query::tests -- --test-threads=1
cargo test --lib web::tests -- --test-threads=1 --exact
```

Use the new test names plus `api_dashboard_embeds_sync_command_center_contract` and `doctor_warns_on_recovered_aborted_runs`.

## Out of scope

Claude forget, Codex/Antigravity stale `missing` state, doctor aborted warn, TUI layout.
