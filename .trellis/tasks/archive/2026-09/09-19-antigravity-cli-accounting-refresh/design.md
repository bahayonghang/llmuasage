# CLI design — accepted implementation

## D1 ModelUsageStats (R1)
Installed CLI1.2.5 and IDE2.5.5 descriptors independently prove #1 model enum, #2 fresh input, #3 total output, #4 cache creation, #5 cache read, #9 reasoning and #10 visible output. Total is the sum of the five disjoint token channels. Parent research/native-semantic-validation.md records 13 sanitized native fixtures and a full integer oracle. Positive cache-write is descriptor-proven with a labeled constructed arithmetic case, not an observed native sample.

## D2 Attempts, identity and time (R2)
Direct usage can aggregate all retries or represent only the final attempt. Prefer usable retry attempt records, then remove gen/steps mirrors by response/provider-message/message identity. Distinct strong identities cannot be merged through a shared message; token amounts and times are never identity. Conflicting counters fail the affected snapshot, never component-maxima merge. No-ID rows remain file+idx scoped with a diagnostic.
Use typed generation and step timestamps; never mtime, current clock or context-window bytes. Preserve raw model evidence and leave unknown model pricing unpriced.

## D3 SQLite and replay (R3)
Read both native roots before source filtering. Installed trajectory product metadata (17 CLI, 1 IDE) owns product attribution and outranks root fallback; conflicting explicit product metadata fails. Canonical paths remove aliases, stable request identities remove copies. One complete replay group per product owns events, while cursor/source_file retain every physical member. Historical raw paths are canonicalized before matching.
Stage consistent read-only DB snapshots, with DB+WAL fingerprints. Missing, unreadable, malformed or cancelled members preserve prior events/cursors. Membership includes forgotten source_file rows and first-bounded imports, not only cursors. Bounded runs keep membership but never reset groups or advance full cursors; identity changes that would duplicate history require full sync. Cross-product ownership transfer needs both sources in an unbounded atomic transaction.

## D4 Accounting transition (R4)
Both native sources use marker3; migration v21 stays literal2. Ordinary sync skips v2 and keeps history. Explicit rebuild bypasses engine pre-reset, stages input, then atomically replaces only selected host/source attributed parser rows and related buckets/cursors/inventory. NULL/empty-path hook events and facts remain queryable and included in totals with a historical-semantics warning. The marker commits with parser rows, never after a failed parse.
Missing files require explicit allow-lossy; unreadable databases are not exempted. No production auto-backfill. Fresh remote v3 streams and host guards are tested; existing remote v2 history remains refused, with full remote restoration outside this change.
