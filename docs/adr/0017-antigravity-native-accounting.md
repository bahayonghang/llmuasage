# ADR 0017: Antigravity CLI and IDE native accounting

- Status: Accepted for implementation, 2026-09-19
- Supersedes: the token-field mapping and blanket rebuild refusal in ADR 0012

## Context

Antigravity now writes native SQLite conversation databases for both CLI and
IDE. The installed CLI 1.2.5 and IDE 2.5.5 protobuf descriptors agree with the
modern field map. Earlier llmusage fixtures incorrectly treated the model enum
as input tokens and reversed thinking and visible output. A checksum showing
that two channels sum to output did not establish their semantic names.

Native multi-attempt records also show that direct generation usage can be the
sum of all retries while sharing the first attempt's identity. Adding both
direct usage and retry rows would overcount. The upstream adapter is useful
evidence, but cannot replace independent native reconciliation.

## Decision

Keep `antigravity` for CLI and add `antigravity_ide`. Both use one passive
SQLite decoder, existing `GEMINI_CLI_HOME`, and native product directories.
Read only metadata needed for tokens, model, identity, time and attribution;
do not read transcript columns or use an RPC collector. Old/backup roots and
`.pb` files remain outside supported discovery.

ModelUsageStats maps field 2 to fresh input, 4 to cache creation, 5 to cache
read, 9 to reasoning and 10 to visible output. Field 1 is a model enum and 3
is total output. Total tokens are the sum of the five disjoint channels.
Prefer individual attempt usage when retries exist; remove generation/step
mirrors by stable identities. Preserve distinct attempts. Use typed timestamps,
never file modification times or arbitrary context-window bytes.

Resolve native product metadata before source filtering: trajectory source 17
is CLI and 1 is IDE. A copied database retains product ownership. Canonical
paths handle aliases; request identities handle copies. Replay complete product
groups, observing SQLite WAL changes. Stage reads before atomically resetting
and replacing selected groups. Incomplete input and cancellation preserve old
events and full-history cursors. A source-limited run cannot transfer an event
from another product; it requests a full family run when needed.

The corrected parser accounting version is 3 for both products. Historical
migration v21 stays pinned to literal 2. Ordinary sync preserves older usage
and requires explicit rebuild. Rebuild replaces only attributed parser rows,
retains hook-era rows, and certifies parser accounting in the same transaction.
Retained hooks remain in totals with a visible historical-semantics warning.
They are not presented as recalculated data. Bounded sync has no group reset
or full-cursor advancement.

## Consequences and limits

Product-group replay is more conservative than per-file reset, but avoids a
new persistent membership schema and makes copied-database deduplication safe.
Legacy remote host history is still refused by existing version guards; remote
full restoration is not implemented in this change. Fresh v3 remote streams
keep source and host isolation.

Positive native cache-write examples were absent from the observed sample;
the field is descriptor-proven and covered by constructed fixtures. Native
normal, empty, error, steps-only and real multi-attempt cases supply independent
oracles. Windows native acceptance and automated checks are recorded in the
Trellis task; other operating systems must not be called live-verified.

## Evidence

- `.trellis/tasks/09-19-upstream-antigravity-token-statistics/research/upstream-checkpoint.md`
- `.trellis/tasks/09-19-upstream-antigravity-token-statistics/research/native-semantic-validation.md`
- `.trellis/tasks/09-19-upstream-antigravity-token-statistics/research/sanitized-native-usage.json`
- `tests/sync/sources/antigravity.rs` and the shared parser's unit tests
- Store transaction rollback and remote accounting regression tests
