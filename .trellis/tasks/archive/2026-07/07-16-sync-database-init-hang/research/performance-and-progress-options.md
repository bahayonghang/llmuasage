# Performance And Progress Options

## Evidence Summary

- `recompute_costs_with_meta` already builds a complete
  `HashMap<BucketKey, PricingRollup>` while scanning events.
- Its final transaction nevertheless runs a correlated `NOT EXISTS` query from
  every persisted bucket back into `usage_event`.
- The live query plan scans buckets and can constrain event lookups only by
  `source`; it cannot seek by the complete bucket key.
- The diagnosed database has exactly 7,153 distinct event bucket keys and 7,153
  persisted buckets, with zero orphans. Building the distinct key set took
  1.211 seconds and an `EXCEPT` comparison took 0.936 seconds.
- Existing retry coverage uses 5,001 events in one bucket. Existing orphan
  coverage uses one live bucket and one orphan, so neither catches the
  bucket-count times source-event-count shape.

## Selected Performance Option

Reuse the recomputed bucket map as the authoritative live-key set:

1. Complete the existing paged event scan and pricing rollup.
2. Start the existing final transaction.
3. Read all persisted bucket primary keys once.
4. Update pricing for each recomputed bucket using the bucket primary key.
5. Delete only persisted keys absent from the recomputed map, also by primary
   key.
6. Apply activation metadata and commit.

This keeps reconciliation O(bucket_count), needs no schema migration, and keeps
the current atomic activation contract.

## Rejected Performance Options

- Add a permanent expression index over the event bucket key: it would fix the
  lookup but adds write/storage cost to every sync and requires schema v15 for a
  one-time maintenance query.
- Replace the query with `EXCEPT`: it is much faster on the live database but
  still rescans all events after the event pass, despite the live-key map
  already being in memory.
- Remove orphan cleanup: this weakens an existing consistency contract.
- Advance catalog meta before reconciliation: this violates fail-safe activation
  and can expose new catalog metadata with old bucket rollups.

## Selected Progress And Logging Option

- Add an internal `BootstrapProgressEvent` envelope that can carry existing
  migration events and new pricing-upgrade events.
- Preserve `bootstrap_with_migration_events`; add a compatible internal
  bootstrap-with-progress method for the sync command.
- Map bootstrap events through one helper into `SyncEvent`, removing the current
  duplicate human/JSON migration mapping.
- Emit pricing progress only at page commit boundaries. Throttle to one second
  or 25,000 events and always emit the final page.
- Use default human stderr progress for liveness, additive NDJSON events for
  integrations, and structured tracing for post-mortem diagnostics.
- Keep the warn-level default log filter; emit one warning only when elapsed
  time crosses 30 seconds. Routine start/finish records remain info-level.

## Relevant Contracts

- `.trellis/spec/llmusage/backend/pricing-catalog-contracts.md`
- `.trellis/spec/llmusage/backend/source-sync-contracts.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `docs/adr/0001-source-registry-and-parser-trait.md`
- `docs/adr/0004-schema-version-migration-runner.md`
