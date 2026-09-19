# Bounded Trellis implementation check — 2026-09-19

Scope: parent `09-19-upstream-antigravity-token-statistics` and the CLI/IDE
children. This check maps acceptance clauses to concrete test/code evidence
and checks contract consistency. It does not repeat the independent parser
audit or the primary agent's CI run. Product files and task acceptance state
are owned by the primary agent and were not changed by this reviewer.

## Acceptance evidence

| Contract | Concrete evidence | Assessment |
| --- | --- | --- |
| Native fields, attempts, mirrors | `src/parsers/antigravity/tests.rs:75` consumes the 13 sanitized samples and asserts event count, all six token channels, and source; `:186` separately exercises constructed positive cache-write; `:203` preserves independent attempts sharing a message ID | Descriptor/native evidence and constructed coverage are distinguished correctly |
| Identity, WAL, replay | `tests/sync/sources/antigravity.rs:4` exercises message-only identity strengthening, `:188` native product ownership across copied roots, `:249` committed WAL-only change, `:307` conflicting copied requests, `:459` second-sync idempotence, `:1050` old/new date windows; parser unit `:252` keeps idless rows at sparse indices separate | Direct coverage exists for these mechanisms |
| History and accounting | `tests/sync/accounting.rs:1021` preserves v2 ordinary/bounded history; `:1076` repairs parser rows while retaining hooks; `src/store/sync_writer.rs:1670` rolls back both sources and markers; subsequent writer tests cover membership transfer and bounded cursors | Direct coverage exists, including transaction failure |
| Remote/source consumers | `src/remote/importer.rs:941,992,1050,1091` covers v3 fresh/replay and v2 refusal; `src/commands/focused.rs:310` parses all five hosts; `tests/cli/reports.rs:472` verifies both Antigravity focused filters over all four periods | Covered within the intentional remote-history refusal boundary |
| Live native acceptance | Parent owns executed native importer/oracle evidence in `research/native-oracle-final.json`, `research/reconcile-native.py`, and `research/implementation-validation.md` | The reviewer did not repeat native reads or claim an independent second execution |

## Findings requiring primary follow-through

1. **Native busy/cancellation preservation lacks direct execution coverage.**
   CLI child `prd.md:15`, IDE child `prd.md:15`, and parent `prd.md:30`
   explicitly require these cases. The decoder has a 250 ms busy timeout
   (`src/parsers/antigravity/decode.rs:55`) and cancellation branches
   (`:70,114`); the family also checks cancellation before commit
   (`src/parsers/antigravity.rs:509`). Existing tests exercise corrupt/missing
   files and transaction rollback, but none holds a native SQLite exclusive
   lock or cancels a native import with pre-existing events/cursors/markers.
   The TUI cancellation test (`src/tui/sync_control.rs:327`) cancels while a
   worker lock is held, without usage fixtures or history assertions. Add
   focused native regressions or leave these exact acceptance clauses open.

2. **The explicit one-sided output clauses are not yet directly covered.**
   CLI child `prd.md:13` names thinking-only and visible-only cases. A numeric
   scan of all 13 compact fixture `oracle_usage_records` found neither a
   positive reasoning/zero visible case nor a positive visible/zero reasoning
   case. The constructed cache-write test supplies both outputs as positive.
   Add clearly labeled constructed one-sided tests, preserving the distinction
   between native schema evidence and observed native samples.

3. **Clarify total equality and two stale spec statements.**
   Parent AC5 and IDE AC4 must preserve the existing text projection contract:
   `.trellis/spec/llmusage/backend/report-cli-contracts.md:56-64` and
   `src/tui/report_table.rs:1083,1615` intentionally sum four visible channels
   for human tables; JSON/database totals include native reasoning. This is
   expected presentation behavior, not an arithmetic regression. Also, the
   source-sync empty-session sentence must not accept databases missing both
   usage tables (`decode.rs:61-63` rejects them), and report-cli's remaining
   “four source hosts” wording must become five or avoid a fixed count.
   Primary is updating specifications and task artifacts concurrently.

## Verification boundary

- This reviewer ran static source/artifact inspection and an independent
  numeric fixture-shape scan. No product mutation was made.
- Lint, TypeCheck, and Tests were not rerun here under the explicit bounded
  dispatch. Primary owns runtime gate results and remaining acceptance.
- Cache-write remains descriptor plus constructed-test evidence; non-Windows
  native operation remains live-UNVERIFIED. No commit, push, or archive.
- No additional demonstrated implementation defect was found in this scope.
  The findings above concern explicit acceptance coverage and contract text;
  they must not be represented as a full quality-gate PASS before resolution.

## Resolution review — follow-up

This section supersedes the open static findings above. Earlier line anchors
describe the initial review snapshot; the new tests shift subsequent anchors.
The primary agent owns test execution and the final runtime acceptance ledger.

| Finding | Follow-up evidence | Static resolution |
| --- | --- | --- |
| Native busy preservation | `tests/sync/sources/antigravity.rs:67`, `antigravity_busy_native_database_preserves_history_cursor_and_marker`, imports a positive sanitized native request, sets marker 2, proves an actual exclusive lock produces `DatabaseBusy`, invokes explicit rebuild, and compares full event/bucket/cursor/source-file rows plus the unchanged marker. Releasing the lock must restore a successful rebuild and marker 3. | Meaningful regression present; execution pending primary |
| Native cancellation preservation | `tests/sync/sources/antigravity.rs:113`, `antigravity_cancel_during_native_staging_preserves_history_and_inventory`, imports positive native history, cancels on the native family's `SourceStarted` event, and uses an exclusive read lock to prevent a successful parse racing past cancellation. It asserts cancellation, zero inserts, no repair-finished claim, zero busy/malformed diagnostics, unchanged full history/inventory rows, and marker 2. A deliberately stale inventory timestamp detects an unwanted sweep. | Meaningful regression present; execution pending primary |
| One-sided outputs | `src/parsers/antigravity/tests.rs:203`, `constructed_descriptor_fixture_preserves_one_sided_output_channels`, separately exercises `(visible, reasoning) = (0,37)` and `(37,0)`, asserting exact output channels, input 100, cache read 12, and total 149. The comment and CLI AC1 clearly label these as constructed descriptor-based cases. | Meaningful regression present; execution pending primary |
| Contract wording | Source-sync now rejects a database with neither usage table; report-cli says five hosts; parent AC5 and IDE AC4 explicitly distinguish authoritative JSON/database/dashboard totals from visible-channel human-table totals. | Resolved by inspected spec/PRD changes |

No remaining concrete static gap was found within this follow-up scope. The
cancellation test proves the native family exits during staging before a
commit; it does not claim to distinguish every individual cancellation branch
inside SQLite row iteration. The shared family/decoder boundary means these
CLI fixtures exercise the same busy/cancellation mechanism used for IDE.
This reviewer did not rerun lint, type-check, Cargo, or CI, and did not change
product code, specifications, task acceptance state, or memory checkpoints.
