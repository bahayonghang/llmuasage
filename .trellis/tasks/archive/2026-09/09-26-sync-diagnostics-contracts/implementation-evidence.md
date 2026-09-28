# Implementation evidence

## Initial change boundary

The parser currently counts unavailable Antigravity members as malformed records. The record formatter suppresses nonzero offsets when a reason exists. The summary omits the number of samples that exceed the eight-sample limit. Source status loses source failures after restart.

The parser owns failure classification and file identity. A private diagnostics DTO will extend the existing JSON storage column. The driver and engine will transport source diagnostics without changing public struct fields or public function signatures. Store, query, doctor, source-status, and remote certification will retain the blocking state. CLI changes will cover sample rendering and the progress/warning boundary.

Expected product files: domain diagnostics/models, Antigravity/Grok/Codex parsers and driver, sync engine, sync status store, diagnostics query, CLI status/doctor/summary/progress, and remote adapter. Tests use temporary fixtures. No schema, token, cost, cursor, dependency, rebuild safety, or performance algorithm changes are authorized in this child task. Main owns docs and specs.

## Checks

Independent diagnostics review is recorded in `check-report.md`. The reviewer
completed private typed row-error classification, the `source_kind` derive fix,
and native-source attribution for row diagnostics. The historical failure-owner
set and all group commit guards remain unchanged. A nine-case synthetic matrix
checks classification, history retention, marker protection, and recovery.

Observed verification:

- `cargo check --locked --all-features`: exit 0.
- Full sync target: 143 passed, exit 0, before the final attribution correction.
- The new source-attribution case failed before its fix and passed afterward.
- Final focused lib suite: 150 passed, exit 0.
- Scoped rustfmt check: exit 0.

Logs: `review-sync.log`, `review-source-attribution-red.log`,
`review-source-attribution-green.log`, and `review-lib.log`.

Full Clippy, full CLI and cross-surface checks, and the formal
`cargo semver-checks --baseline-rev v1.2.0` gate remain parent-owned. The default
MBX Cargo wrapper produced two pre-test access violations; native Cargo with
the same test flags succeeded. The wrapper's internal cause remains unknown.
No task acceptance criterion is marked complete by these partial gate results.

## Architecture follow-up

The first full gate identified the newly added store-test dependency on query
(`ARCH-007`). Dashboard projection/privacy assertions were moved from
`src/store/sync_status.rs` to `src/query/diagnostics.rs`. All storage assertions
remain in the store test, and no architecture rule was weakened.

Direct native Cargo validation passed: architecture target 12/12; precise store
and query regressions 2/2; scoped rustfmt and diff checks exit 0. Evidence is in
`review-architecture.log` and `review-architecture-regressions-green.log`. The
intermediate missing-import compile failure is retained in
`review-architecture-regressions.log`.

## Parent final local gate (2026-09-28)

The parent ran the unchanged `just ci` command with a verified command-local
native Cargo path. Exit code was 0. Root lib tests passed 905 with 12 ignored;
all eight integration targets passed 246 tests, including the final sync target
143/143 and CLI 34/34. Format, Clippy, rustdoc, dashboard JS, desktop frontend
and Rust checks, and docs build passed. Four dependency lockfiles remained
byte-identical. See `ci-native.log`, `ci-native-result.json`, and
`ci-native-environment.log`.

D1, D2, D3, and D5 are verified. D4 remains open because the formal v1.2.0
SemVer failure needs the user's specific API-version decision. G1/G3 are done;
G2 is pending. The task remains in_progress and no subsequent optimization
task was started. No version, baseline, installed product, or real usage data
was changed.
