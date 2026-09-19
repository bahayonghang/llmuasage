# Implementation validation — completed within authorized scope

User authorized implementation with “请开始实施”. Parent and both children
were started on 2026-09-19. Planning NO-GO gates P1/E1/E2 are superseded by
the accepted source split, native descriptor/fixture evidence, and ADR 0017.

## Actual changes
- Shared CLI/IDE native SQLite decoding, corrected channels, attempt precedence,
  product ownership and grouped replay are implemented and under test.
- Fenced snapshot transaction preserves hook-era history; v21 remains literal2,
  current native sources use3. Engine avoids destructive pre-reset.
- Separate source/filter/overview/monitor/CLI/Web/Desktop consumers and docs
  are implemented. No production sync/rebuild, dependencies, commit or push.

## Checks so far
- Baseline old parser tests: 15 passed (not an oracle of new correctness).
- Task context validation: parent and both children passed. Updated check
  contexts point to accepted native semantics, ADR0017 and this execution ledger.
- Dashboard focused JS tests: 28 passed; Desktop filter tests: 8 passed.
- Build passed. Focused Antigravity library tests: 31 passed. Full sync
  suite after corrections: 139 passed. Full library first run: 886 passed,
  four Windows atomic file tests failed under sandbox restrictions; all eight
  atomic tests passed in the approved unrestricted temporary-test environment.
- Independent reviewer identified unreadable cross-root copy ownership,
  bounded inventory sweep and Windows historic-path canonicalization risks;
  corrections/regression checks landed. Final independent static review PASS:
  no unresolved actionable findings; the remote informational-marker finding
  was withdrawn after checking ParseIssues::total semantics and adding a test.
- Full just ci: first attempt found one test helper made unused by replacing
  the old parser; helper removed. Retry passed CI self-test/contract checks,
  format, strict clippy, 894 library tests (12 ignored), all eight integration
  targets (api3, architecture12, cli34, query9, remote8, store2, sync139, tui35),
  rustdoc and the dashboard JS gate. It stopped at an untouched Desktop shell
  test observing late mount-effect logs/fetch_quota calls. That entire file
  passed21 tests unchanged; a complete desktop-check rerun passed65 Vitest,
  TypeScript, Vite build, Tauri18 unit+9 acceptance+6 quota tests and doctests.
  VitePress docs build passed separately. All constituent cross-surface gates
  have passed; the aggregate invocation itself exited1 and is not reported as
  an uninterrupted just ci success.
- Final bounded Trellis check found missing direct busy/cancel/one-sided-output
  regressions and stale specification text. Three regressions are now added
  and passed: focused native sync slice20 tests, constructed one-sided test1.
  Busy holds a real exclusive lock, asserts SQLITE_BUSY and unchanged complete
  events/buckets/cursors/inventory/marker, then verifies recovery after unlock.
  Cancellation starts with positive native history, triggers from SourceStarted,
  preserves an inventory sentinel and marker2, emits no repair-finished event,
  and has zero malformed counts. Final static follow-up reports no remaining
  concrete gaps. Text/spec corrections and cargo fmt --check passed.
- The final documentation rebuild passed after correcting ordinary-sync and
  serve legacy-repair guidance for the new accounting contract. Final strict
  clippy on the newly added test code passed with no issues.

## Windows native isolated acceptance
- Runtime is task-owned `target/antigravity-native-validation-20260919`, not
  the user's production usage store. Native inputs were opened read-only.
- Revised native parser imported 110 CLI DBs / 2395 events / 98333012 tokens
  and 530 IDE DBs / 10063 events / 577564225 tokens, with zero parse issues.
- A second sync of each source changed zero files and inserted zero events.
- Independent dated oracle reconciliation: all110 CLI DBs and446 unchanged
  IDE DBs matched every channel and event count; zero mismatches. A new
  independent read-only comparison at 08:12:04–08:12:08 UTC verified all530 IDE
  DBs, including83 changed fingerprints and1new DB. All six channels and event
  counts match, with no concurrent changes, uncertain IDs or new omitted DBs.
  Evidence: native-oracle-final.json; the original fixture remains unchanged.
- First native attempt safely refused to commit because post-read fingerprints
  changed. Immediate pre-read sampling and absent/empty WAL equivalence passed
  the revised live run. Ten separate Python read-only probes found no drift;
  the original failure's precise cause was not isolated and is not asserted.

## Evidence boundaries
Native schema+metadata evidence: CLI1.2.5 and IDE2.5.5; 13 sanitized samples.
Full independent per-DB oracle: CLI2395 events/98333012 tokens;
IDE10043 events/576629526 tokens at dated collection (IDE continues growing).
Positive cache-write has descriptor and constructed-test evidence only;
thinking-only/visible-only positive examples also use labeled constructed
cases rather than native fixtures. Other platforms are not live-verified.
Existing remote v2 history continues to be refused; new remote restore is out
of scope, while fresh v3 import and host/version guards have regression tests.

Human CLI report tables intentionally sum the four visible channels under
report-cli-contracts.md. Database, JSON and dashboard authoritative totals
include native reasoning. AC comparisons preserve this existing projection
rather than requiring the human table to equal the authoritative total.

## Acceptance trace

| Parent / child clauses | Mechanism and executed evidence |
| --- | --- |
| Parent AC1 | upstream-checkpoint.md pins both verified remote heads, dates, missing old baseline and selected/deferred adoption |
| Parent AC2 / CLI AC1 / IDE AC1 | decode.rs, 13 native fixtures, descriptor oracle, constructed cache-write and one-sided cases; native CLI110/IDE530 complete integer reconciliation |
| Parent AC3 / CLI AC2–3 / IDE AC2–3 | native parser/sync suites cover typed time, sparse indices, attempts/mirrors/copies, identity strengthening, provenance, WAL-only commits, missing/corrupt/busy/cancelled input, full/bounded membership; live second sync0changed/0inserted |
| Parent AC4 / CLI AC4 / IDE AC3 | accounting.rs ordinary v2 skip and hook-preserving rebuild; sync_writer transaction rollback, ownership transfer, bounded cursor and host isolation; remote importer freshv3/replay and oldv2 refusal |
| Parent AC5 / IDE AC4 | focused report tests for five source hosts/four periods; query/home/registry/status, dashboard JS, Desktop filters/typecheck/build and Tauri gates; bilingual docs and ADR/spec changes |

The independent code audit and bounded acceptance review are separate from
runtime execution. Historical planning NO-GO and initial failed-gate evidence
remain as dated records; current conclusions are owned by this ledger.

## Delivery state

Implementation and required validation are complete. The product source did
not change after the full root/desktop/native gates; the final additions were
three focused tests and contract/documentation corrections, each rechecked at
its own boundary. No new dependencies or lockfile changes were introduced.
Trellis parent and both children retain formal in_progress status because
commit/push/archive were not requested. Production llmusage history remains
untouched; the native acceptance store is task-owned and ignored under target/.
