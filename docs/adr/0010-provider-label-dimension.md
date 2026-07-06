# ADR 0010 — Provider label dimension for usage attribution

- Status: Accepted
- Date: 2026-07-02
- Related code: `src/store/migrations.rs`, `src/store/sync_writer.rs`, `src/domain/models.rs`, `src/domain/provider_map.rs`, `src/commands/sync.rs`
- Related terms: Source, Provider Label, Usage Event, Usage Bucket, SyncShard

## Context

`llmusage` currently stores usage by source, model, time bucket, and project. For
relay-based workflows, `source = claude` or `source = codex` is not enough to
answer which upstream provider actually served a request.

The raw Claude/Codex logs do not carry API `base_url` or relay provider. CCR
does know profile/provider activation and writes an append-only activation
timeline at `${CCR_ROOT:-~/.ccr}/analytics/provider_activation.jsonl`.

## Decision

Add `provider_label TEXT NOT NULL DEFAULT ''` to both `usage_event` and
`usage_bucket_30m`.

`usage_bucket_30m` changes its primary key from
`(source, model, hour_start, project_hash)` to
`(source, provider_label, model, hour_start, project_hash)`.

The empty string is the unattributed sentinel. Do not use NULL for this key: in
SQLite, NULL primary-key components do not participate in `ON CONFLICT` equality
the way this bucket rollup needs.

Sync loads provider activation data once per run:

- explicit `--provider-map <path>` overrides all defaults and errors if missing
  or unreadable;
- otherwise llmusage attempts `${CCR_ROOT:-~/.ccr}/analytics/provider_activation.jsonl`;
- missing or unreadable default map is non-fatal and leaves labels empty.

The provider label is stamped inside `SyncRunWriter::commit_shard` before event
chunks are inserted. That keeps `usage_event` and `usage_bucket_30m` consistent
without making parsers know about CCR.

## Rejected Alternatives

- CLI flag only: rejected because hook, TUI, library, and background sync are the
  common first import paths. `INSERT OR IGNORE` means later manual sync would not
  restamp already-imported events.
- Nullable provider: rejected because SQLite NULL conflict behavior would split
  unattributed bucket rows instead of deduplicating them.
- Parser-level stamping: rejected because parsers do not own external provider
  activation state, and ADR 0002 keeps write protocol decisions in
  `SyncRunWriter`.

## Consequences

- Existing rows migrate to `provider_label = ''`.
- Historical attribution requires `sync --rebuild` with an explicit or
  discovered provider map; ordinary incremental sync does not rewrite existing
  events.
- Older binaries should refuse the newer schema through the existing schema
  version guard. Downgrade recovery is rebuild from source logs, subject to the
  existing lossy-rebuild guard when original logs are missing.

## Verification

- Migration v14 unit test covers existing row backfill and bucket PK shape.
- Provider map unit tests cover malformed lines, clear events, pre-window events,
  and timestamp offset equivalence.
- Sync integration tests cover explicit provider map, default CCR discovery, and
  rebuild-derived labels.
