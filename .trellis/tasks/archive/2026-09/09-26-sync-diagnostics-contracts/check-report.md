# Diagnostics contracts independent check

Status: scoped review complete; the approved 2.0.0 version boundary, formal
SemVer gate, and new full local CI all pass. Scope is the diagnostics child
task, the timezone test repair review, and the approved gate/version work.
The independent version review is in `version-boundary-review.md`. No real usage database
was synced, rebuilt, reset, or modified by this review.

## Findings (fixed)

- `src/parsers/antigravity/decode.rs`, `src/parsers/antigravity.rs`: the initial
  source-failure conversion also removed real row-level malformed diagnostics.
  Private typed `RecordFailure` context now distinguishes generation, step, and
  trajectory row decoding from database access and SQL prepare failures. The
  parser does not classify errors by matching free-form messages. Missing typed
  request time and output-channel checksum faults retain stable malformed
  reasons. Both categories still block the complete product snapshot.
- The interrupted typed-error implementation used a `source: SourceKind` field.
  `thiserror` interpreted that field as the underlying `Error` source, causing
  E0599. Renaming the private field to `source_kind` restores compilation.
- `tests/sync/sources/antigravity.rs`: added a nine-case regression starting
  with positive imported history. Cases cover generation protobuf, usage wire
  type, step protobuf, trajectory protobuf, invalid timestamp nanos, missing
  timestamp, output checksum, changed native product ownership, and SQL prepare
  failure. Assertions cover exact
  classification, persisted diagnostics after Store reopen, unchanged
  event/bucket/cursor/inventory rows, retained accounting marker, absence of
  repair-finished events, recovery, and clearing the completed failure.
- The typed row failure was initially recorded for every historical owner. A
  source-limited regression reproduced a CLI malformed count of 1 after native
  ownership changed to IDE. Row diagnostics now use only the typed
  `source_kind`; every historical failure owner retains its source-level
  `incomplete_snapshot` blocker. The failure-owner set and commit guards are
  unchanged. The new case failed before this fix and passed afterward.
- Existing interrupted-checker fixes are present: Codex uses
  `oversized_non_usage_record`; ZCode renders `timestamp_ms=`; OpenCode rowids
  remain hidden; the legacy subprocess fixture supplies required token columns.
  The final focused tests and sync suite validate those behaviors.

## Cross-layer review

- The public record diagnostics and sync structs retain their fields. Private
  persisted diagnostics flatten the old payload and default absent source issues.
- Store compatibility writes preserve prior source issues. Current engine and
  remote importer writes use an explicit empty entry to clear a successful run.
- Query status, doctor, source-status, diagnostics export, remote trailer, and
  accounting certification inspect source failures. Zero record faults cannot
  certify a source with blocking source issues.
- The formatter retains reason and JSONL byte offset, emits at most eight
  samples, reports omitted diagnostic counts, and strips control characters and
  private path components from human sample output. Grok sample identity uses
  its sidecar path while event/session/reset identity remains unchanged.
- Warning routing has a scoped guard. Tracing buffers a complete event and
  passes the event through the renderer. The guard restores the previous sink.
  Progress and warnings remain stderr; JSON events remain stdout.

## Timezone repair review

Read-only inspection of `src/query/filter.rs`, `src/query/timezone.rs`, and
`src/web/mod.rs` found no additional defect in the repair. `Local` documentation
and expectations now match the existing IANA resolution and fixed-offset
fallback. Chicago fixtures assert explicit UTC bounds for 23-hour and 25-hour
dates. The two Web fixtures store and assert UTC calendar dates, so explicit
`timezone=UTC` removes an undeclared dependency on the machine's timezone. No
query runtime code or timezone parsing was changed by this reviewer.

## Findings (not fixed)

- The timezone repair was independently reviewed and passed in the final full
  local gate. No timezone production behavior changed.
- The previously observed intermittent CLI log assertion has no established
  cause. The final serial CLI suite passed all 34 tests. That successful run
  does not establish the cause of the earlier intermittent failure.
- The MBX Cargo wrapper produced a sync test executable that exited twice with
  `0xc0000005` / `STATUS_ACCESS_VIOLATION`. That executable also failed with
  `--list` before printing test names. Native Cargo rebuilt the same sources
  and arguments into a working test executable. The MBX internal cause remains
  unknown. No Cargo, linker, global configuration, or repository gate settings
  were changed during these checks.

## Verification

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo check --locked --all-features` | PASS, exit 0 | 10.94 seconds after the `source_kind` correction |
| Native Cargo, `test --locked --all-features --test sync -- --test-threads=1` | PASS, exit 0; 143 passed | `review-sync.log`, 47.73 seconds; before the final attribution correction |
| Native Cargo, `test --locked --all-features --test sync antigravity_record_faults_and_source_failures_preserve_history -- --test-threads=1` | FAIL before attribution fix, exit 101; PASS after fix, exit 0 | `review-source-attribution-red.log` and `review-source-attribution-green.log`; final matrix has nine cases; final run 3.96 seconds |
| Native Cargo, focused lib command below | PASS, exit 0; 150 passed, 766 filtered out | `review-lib.log`, 5.47 seconds after the final code change |
| Scoped `rustfmt --edition 2024 --check --config skip_children=true` | PASS, exit 0 | All six owned product/test files |
| Clippy | PASS in the new 2.0.0 `just ci`, exit 0; `-D warnings` unchanged | `ci-v2.log` |
| Full local gate | PASS, exit 0; 276.111 seconds; four locks unchanged | `ci-v2.log` and `ci-v2-result.json` |
| Formal SemVer | PASS for approved major boundary; 0 checks, 254 skipped | `semver-2.0.log` and `semver-2.0.exit`; unchanged v1.2.0 baseline |

The first timezone-test attempt stopped at the typed-error compile failure. It
did not execute those tests and is not evidence of a failing timezone test.

The first eight-case matrix attempt failed because its synthetic database did
not supply native product metadata. The expected extra root-attribution
accounting diagnostic was valid. The fixture now explicitly sets source 17;
the implementation was not changed to silence that diagnostic.

Native Cargo path used for the passing test runs:
`C:\Users\lyh\.cargo\bin\cargo.exe`. The default executable resolved to
`C:\Users\lyh\AppData\Local\mbx\bin\cargo.exe`. The failing wrapper artifact
was `sync-1d5d62225673a70a.exe`; the passing native artifact was
`sync-c7acfcc23bc77bb7.exe`. The two wrapper commands were the same matrix
command without and with `--nocapture`. The direct startup probe was
`target/debug/deps/sync-1d5d62225673a70a.exe --list` (exit 1, no output).

Focused lib command (native Cargo; ordinary test parameters):

```text
cargo test --locked --all-features --lib -- --test-threads=1 parsers::antigravity::tests parsers::codex::tests parsers::grok::tests parsers::file_state::tests commands::sync_summary::tests commands::sync_progress::tests commands::source_status::tests commands::doctor::tests domain::models::tests domain::source_diagnostics::tests store::sync_status::tests remote::importer::tests remote::protocol::tests
```

The full sync suite covered the repaired legacy subprocess fixture, source-status
and doctor after missing-member persistence, ZCode timestamp locators, native
busy/cancel/copy/WAL/history guards, and source recovery. The final focused lib
run covered sample privacy/counts, Codex bounded reads, Grok identities and
incomplete usage, progress-warning ordering, store round-trip, and remote marker
certification. The parent subsequently completed `just ci`, including the
complete CLI suite. The later 2.0.0 gate and migration review close the formal
SemVer requirement for D4 without claiming v1.2.0 source compatibility.

## ARCH-007 follow-up

The parent's first full `just ci` run detected a dependency violation in the
new store round-trip test: `src/store/sync_status.rs` called
`crate::query::Dashboard::open`. The existing `store_does_not_depend_on_query`
test correctly rejected that dependency. Original evidence remains in
`ci-expanded.log`; no architecture rule was changed or disabled.

The Dashboard status, warning tone, and private-diagnostics exclusion assertions
now live in `src/query/diagnostics.rs` under
`source_failure_projects_warning_without_private_diagnostics`. The fixture has
zero malformed/oversized counters, so the warning proves source-level failure
projection independently of record faults. The store test retains all storage
round-trip, reopen, compatibility-write preservation, completion-time update,
successful clearing, and invalid-JSON rejection assertions. Product behavior
and public interfaces are unchanged by this test relocation.

Verification used `C:/Users/lyh/.cargo/bin/cargo.exe` directly:

| Command | Result | Evidence |
| --- | --- | --- |
| `test --locked --all-features --test architecture_dependencies -- --test-threads=1` | PASS, exit 0; 12 passed, 1.93 seconds | `review-architecture.log` |
| `test --locked --all-features --lib -- --test-threads=1 store::sync_status::tests::parse_issue_diagnostics_round_trip_and_reject_invalid_json query::diagnostics::tests::source_failure_projects_warning_without_private_diagnostics` | PASS, exit 0; 2 passed, 915 filtered out, 0.37 seconds | `review-architecture-regressions-green.log` |
| Scoped rustfmt check and `git diff --check` for the two files | PASS, exit 0 | No other product file changed in this follow-up |

The first precise unit-test build reported two missing imports in the relocated
test. Both imports were corrected; that failed build remains recorded in
`review-architecture-regressions.log`. The reviewer's build slot is released.

## Parent 1.4.0 local gate (historical, 2026-09-28)

`just ci` passed with exit 0 after the final test relocation. The command used
the installed native Cargo through a command-local PATH; the executable path
and toolchain versions are recorded in `ci-native-environment.log`. The command
and repository gate arguments were unchanged.

- Root lib: 905 passed, 0 failed, 12 ignored.
- Eight integration targets: 246 passed, including architecture 12, CLI 34,
  sync 143, and query 9.
- Format, Clippy, rustdoc, CI gate contract checks, dashboard JS (66 tests),
  desktop frontend/build/Rust checks, and docs build passed.
- Four lockfiles were byte-identical before and after the gate.

Evidence: `ci-native.log`, `ci-native-result.json`, and
`ci-native-locks-before.json`. The prior architecture failure remains in
`ci-expanded.log`. The earlier intermittent CLI log failure and wrapper access
violations retain unknown causes; the final full CLI suite passed.

At that earlier checkpoint the crate was 1.4.0 and the original v1.2.0 SemVer
failure still needed a version decision. The user subsequently approved the
2.0.0 boundary. No baseline, lint policy, schema, accounting version, or product
installation was changed.

## 2.0.0 version boundary review (2026-09-28)

The independent review verified 13 files containing 15 product version fields,
all set to 2.0.0. Four lockfiles retain their complete third-party dependency
structures versus HEAD. Both migration pages match the actual additions to
14 structs, parameter order for 16 methods, enum changes, removed entry
points, and the `subscription::fetch_all` result change. The exact bilingual
Rust example compiles with native rustc and the current llmusage rlib.

The reviewer corrected three rustfmt differences in
`desktop/src-tauri/tests/quota.rs` using edition 2024. This correction changes
only formatting. The desktop manifest format check passed. No additional
migration-document or API implementation correction was necessary.

The new original `just ci` used native Cargo and completed with exit 0 in
276.111 seconds. Root lib: 905 passed, 12 ignored. Eight integration targets:
246 passed, including sync 143 and CLI 34. Desktop frontend: 65 passed;
desktop Rust: 18 lib, 9 acceptance, and 6 quota tests passed. Format, Clippy,
rustdoc, CI contract checks, dashboard JS, frontend build/type-check, and the
docs build passed. All four lockfiles remained byte-identical during CI.

Formal `cargo semver-checks --baseline-rev v1.2.0` returned 0 after the
approved major change. The tool skipped all 254 compatibility checks. This
pass verifies the declared version boundary; the pass does not establish
v1.2.0 source compatibility. The historical minor-version failure and the
first online TLS failure remain recorded.

Evidence: `version-boundary-review.md`, `version-boundary-independent.json`,
`version-boundary-api-review.json`, `version-boundary-example-result.json`,
`ci-v2.log`, `ci-v2-result.json`, `ci-v2-env.json`,
`ci-v2-locks-before.json`, and `ci-v2-locks-after.json`. The Cargo slot is
released. No gate failure remains for this review scope.
