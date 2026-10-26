# Implement: Cindy passive source

Do not start until the planning summary is explicitly approved. Read `.trellis/spec/llmusage/backend/source-sync-contracts.md` and `token-accounting-contracts.md` before editing.

## Checklist

1. Add `SourceKind::Cindy` (`cindy`) and descriptor metadata: parser, passive probe, precise, local artifacts. Accounting version `2`. No schema migration.
2. Add `list_cindy_session_files` / `cindy_session_roots`. Official config-dir profiles, env replacement, `sessions/shared` exclusion, and overlap skip against Pi and Oh My Pi roots.
3. Add `PiFormatParser::cindy()`, register it, and pass Cindy discovery roots into project resolution.
4. Add the platform monitor descriptor and the remote host reset entry. Add a dashboard color token and a project-authored `cindy.svg`. Do not add a CLI subcommand or copy the Cindy trademark.
5. Extend parser listing tests and `tests/sync/sources/pi_omp.rs` (or a sibling module) with a sanitized fixture: non-zero usage, zero usage skipped, shared transcript skipped, overlap skipped, sync twice, env override does not read the real profile.
6. Update passive-source candidates, source-sync and token-accounting specs, `README.md`, `README.zh-CN.md`, `docs/index.md`, `docs/zh/index.md`, `docs/guide/install-and-init.md`, `docs/zh/guide/install-and-init.md`, `docs/guide/first-sync.md`, `docs/zh/guide/first-sync.md`, and both architecture indexes. Add a short ADR for the source split. Do not copy user transcripts into fixtures or docs.
7. Run the validation commands below. Do not bump the crate version.

## Validation

- `cargo test --locked --all-features --test sync -- --test-threads=1`
- `python scripts/ci-rust.py`
- `npm --prefix docs run docs:build`
- If dashboard CSS changes are non-trivial, also `node scripts/ci-js.mjs`. A color token alone still belongs in the Rust/docs gate; run the JS gate if a JS test snapshots source colors.

## Rollback

Revert the registration commit. `cindy` rows already written need `reset_for_source` only if a partial database must be cleaned. Pi and Oh My Pi cursors must stay untouched.

## Review gate

- Overlap skip does not set `last_error`.
- `PI_AGENT_DIR` still does not add Cindy roots.
- No SQLite aggregate import and no `codex-home` discovery.
- Fixtures contain usage fields only, not prompt or response text.
