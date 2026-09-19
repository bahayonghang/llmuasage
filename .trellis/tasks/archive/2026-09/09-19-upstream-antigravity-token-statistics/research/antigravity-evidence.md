# Antigravity architecture evidence

The controlling new findings are in native-sqlite-findings.md. Current CLI support exists, but its token interpretation is disputed by ccusage's August31 adapter. Preserve history protections, not unverified old arithmetic.

| Local anchor | Contract / risk |
| --- | --- |
| src/registry.rs:22; src/domain/source_descriptor.rs:80 | Existing CLI parser/precise descriptor under antigravity |
| src/parsers/source_files.rs:270 | CLI-only DB discovery; IDE root absent |
| src/parsers/antigravity.rs:354,394,813 | Read-only SQLite generation blobs; no step/retry ingestion |
| src/parsers/antigravity.rs:452,466,532 | #1 input addition and #9/#10 mapping conflict; checksum insufficient proof |
| src/parsers/antigravity.rs:479,835 | Old typed timestamp then session fallback; modern steps needed |
| src/parsers/antigravity.rs:506,526 | Path-based event key; copied DBs do not logically dedupe |
| src/parsers/antigravity.rs:214,310,317 | bounded no cursor/reset; unreadable preserves; changed path resets whole old path |
| src/parsers/file_state.rs:49 | Main DB size/mtime trigger; real WAL sidecars now observed, regression required |
| src/sync/engine.rs:467; src/store/migrations.rs:1262 | Hook history absolute rebuild guard and current marker2; simple upgrade+rebuild insufficient |
| src/commands/source_status.rs:257; src/domain/platform_monitor.rs:229 | CLI ready does not establish IDE coverage; IDE still planned |

Existing tests at tests/sync/sources/antigravity.rs:62,202,298,365,565,651 cover idempotency/replacement/failure/history/bounded. Not run this turn. Old 1573-event reconciliation was a historical check against its decoder; it does not settle modern disputed field semantics.

## Upstream roles
ccusage: native DB roots, modern field interpretation, steps/retries and connected identities (parser.rs:648,567,598; loader.rs:34; paths.rs:9). Native SQLite path selected, with semantic proof gate.
Tokscale: useful typed steps association at core sessions/antigravity_cli.rs:450; runtime inference remains at :526 despite contradictory comments. Its token mapping shares the old premise and is not an independent oracle.

## Reviewed RPC alternative, outside scope
Tokscale CLI antigravity.rs:1057,1183,1381,2069,2118,3048 provides process/port discovery, Windows transport and retry JSON cache. README:667 macOS/Linux-only wording conflicts with Windows code. Its mapper does not establish cache/reasoning inclusion; HTTPS identity is weaker than plaintext (:1496 vs :1514); Core lib.rs:3335 concatenates IDE/CLI without cross-lane dedupe. These mechanics do not justify an active collector when native IDE DBs exist. No RPC credentials, requests or full trajectory bodies were accessed.

## Mandatory planning gates
E1: real sanitized normal/empty/error/retry metadata plus semantic oracle, per docs/agents/passive-parser-onboarding.md:5. E2: global identity vs path-reset and accounting transition vs protected history must have executable contracts before implementation. P1: separate CLI/IDE display/source choice belongs to user. Windows live import and other platforms stay UNVERIFIED; bounded structure reads are not release validation.
