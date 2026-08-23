# ADR 0015 — Oh My Pi source split

- Status: Accepted
- Date: 2026-08-23
- Related code: `src/domain/models.rs`, `src/parsers/pi.rs`, `src/parsers/source_files.rs`, `src/registry.rs`, `src/store/schema.rs`, `src/commands/sync.rs`, `src/remote/importer.rs`
- Related terms: Source, SourceParser, SourceDescriptor, FileCursor, token accounting version

## Context

Pi and Oh My Pi write the same session JSONL shape. Discovery previously merged
`~/.pi/agent/sessions` (or `PI_AGENT_DIR`) with `~/.omp/agent/sessions` into one
stable `pi` source. Events used `source = pi` and `event_key` prefix `pi:`.

That hid Oh My Pi as an independent source. Reports, source-status, and host
reset could not separate the two roots. A later cost/dimension/behavior
backfill needs a stable `omp` id.

## Decision

Register `SourceKind::Omp` (`omp`, display name `Oh My Pi`) beside `SourceKind::Pi`.

- Discovery: `pi` lists `PI_AGENT_DIR` or `~/.pi/agent/sessions`. `omp` lists
  `~/.omp/agent/sessions`. No new environment variable.
- Ownership is path-level and bidirectional. Canonicalize Pi roots, then skip
  an OMP candidate when `left == right || left.starts_with(right) ||
  right.starts_with(left)`. Pi wins. Skip only the conflicting files.
- One parse implementation: `PiFormatParser` holds `source` and `list_files`.
  Registry registers two instances. `event_key` is
  `{source.as_str()}:{hash}`.
- Token accounting: `Pi` expected version is `3`. `Omp` uses
  `TOKEN_ACCOUNTING_VERSION` (`2`).

### Three migration paths

1. Default unbounded `sync`: `has_legacy_token_accounting(Pi)` resets local
   `pi` rows, then replays `.pi` as `pi` and `.omp` as `omp`.
2. `sync --source omp` while legacy `pi` rows exist: refuse. The user must run
   `llmusage sync` with no `--source` and no `--recent-days` first.
3. Remote import: on the first `Omp` shard from host `H`, the same write
   transaction runs `reset_for_source(Pi, H)` and sets
   `meta['omp_split_migrated.<host_id>']`. The flag is idempotent.
   `remote remove --delete-usage` also resets `omp`.

Rejected alternatives: a variant column on `pi` (would change
`usage_bucket_30m` primary keys); SQL rewrite of `pi` rows to `omp` (path
hashes cannot recover deleted files); expanding `--source omp` to rebuild
`pi` (would silently widen `--source`).

## Consequences

- Schema structure is unchanged. `schema_version` does not increase.
- A first unbounded sync after upgrade rebuilds legacy `pi` rows. Identity
  coverage is `(source_path_hash, event_at, model, total_tokens)`, not totals.
- Later provider, project, cost, and behavior backfill uses
  `sync --rebuild --source omp`. Ordinary incremental sync does not rewrite
  unchanged files.
- Hardcoded source lists (help, diagnostics, dashboard filters, CSS tokens)
  must include `omp`. Compiler exhaustiveness does not catch those strings.

## Rollback

Code rollback does not delete already-written `omp` rows.

1. Revert the split code.
2. For each host, reset `omp` (`reset_for_source(Omp, host_id)` locally and
   per remote host). Delete `meta` keys `omp_split_migrated.*` and
   `token_accounting_version.omp`.
3. Delete `token_accounting_version.pi`, then run one unbounded
   `llmusage sync` with the pre-split binary so merged `pi` rows rebuild.
4. If `.omp` session files were deleted before step 2, restore
   `~/.llmusage/llmusage.db` from the pre-upgrade backup.

## Verification

- `parse_source_id("omp")` returns `Omp`.
- Listing tests cover disjoint roots and three overlap shapes.
- `event_key` prefixes `pi:` and `omp:` differ and stay idempotent.
- Integration tests cover identity-set replay after the Pi version bump,
  `--source omp` refusal before migration, and host-level remote reset
  idempotency.
