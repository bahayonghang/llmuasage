# Isolated Live-Scale Validation

Date: 2026-07-16

## Isolation

- Source database: `C:\Users\lyh\.llmusage\llmusage.db` (read only).
- Python 3.14 `sqlite3.Connection.backup` created a consistent temporary copy.
- The release candidate ran only with an isolated `--home` and isolated source
  environment. The live database was never opened by the candidate.

## Snapshot Before Upgrade

- Schema version: 14.
- Pricing catalog version: `static-v1`.
- Events: 539,146.
- 30-minute buckets: 7,153.

## Measured Upgrade

- Event repricing completed at 6,626 ms.
- Bucket reconciliation started at 6,627 ms and the final transaction finished
  at 6,764 ms, so bucket reconciliation plus activation took about 137 ms.
- Progress events were emitted every 25,000 committed events and for the final
  539,146-event page.
- Deleted orphan buckets: 0.
- The subsequent source-sync phase refused the snapshot's legacy Codex token
  accounting contract after pricing had committed. This expected, unrelated
  guard does not affect bootstrap performance or activation evidence.

## Postconditions

- `pricing_catalog_version = static-v2`.
- Event rows: 539,146; bucket rows: 7,153.
- Persisted bucket keys missing event keys: 0.
- Event keys missing persisted bucket keys: 0.
- Bucket `event_count` or summed cost mismatches: 0.
- A second bootstrap emitted zero pricing lifecycle events.

## Acceptance Result

The final bucket reconciliation budget was under two seconds on this machine
(about 137 ms), with ordered visible progress, atomic catalog activation, and
consistent event/bucket projections.
