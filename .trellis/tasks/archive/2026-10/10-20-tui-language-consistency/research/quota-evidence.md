# T03 quota implementation evidence

Date: 2026-10-21. Scope: subscription producers/cache, private quota controller
and frame scope, Usage renderer and paired quota catalog. No real endpoints,
user usage, credentials, installation, commit or release were used.

## Implemented private contracts

- Public `fetch_all`, `UsageFetchOutcome`, report/output/metric/diagnostic DTOs
  retain their signatures, fields and canonical values. Provider fetchers
  produce private metric hints at the original window/quantity decisions;
  typed private errors retain the exact original Display before `maybe_fetch`
  erases them into the public diagnostic string.
- `subscription::fetch_all_with_presentation(ctx, bypass_cache)` returns
  `QuotaFetchOutcome { outcome, presentation }` from the same fetch/cache core.
  Hints contain neutral window/diagnostic IDs and original quantities. Kimi's
  original integers and Grok's original rounded quantities are retained without
  inferring them from percentages. Codex external feature prefixes stay data.
- Optional cache `presentation` is decoded independently from the public
  report. SHA-256 of the serialized report and closed shape checks bind hints
  to that report. Missing, malformed or mismatched hints are discarded without
  invalidating a valid fresh report or initiating endpoint calls. TTL remains
  300 seconds based on `fetched_at`; old readers ignore the optional field.
- `QuotaController::try_recv()` returns `QuotaUpdate { report, presentation }`.
  Its new private request tag rejects superseded/cancelled queued results;
  scheduling, cancellation and cache bypass policy retain their original role.
- `quota::with_presentation(report, hints, render)` checks the report fingerprint
  and installs an unwind-safe, nested, frame-local scope. It uses pointer
  identities only as lookup keys, never dereferences stored pointers, and
  stores no global latest-result buffer. The UI peer owns the session variable
  and the same-frame draw integration. Public AppState has no new fields.
- Usage copy follows the shared language scope. New own diagnostics/metric
  labels consume typed hints; unknown/old-cache details have an explicit
  localized original-detail marker. Unicode truncation counts display cells.

## Verification

| Check | Status | Evidence |
| --- | --- | --- |
| rustfmt of all eleven quota-owned Rust files | PASS | `rustfmt --edition 2024 --check` on subscription files, quota.rs, Usage, quota catalog |
| Quota-owned diff whitespace | PASS | `git diff --check -- src/subscription src/tui/quota.rs src/tui/panels/usage.rs src/common/i18n/quota.rs` |
| `cargo check --locked --offline --all-features --tests` | FAIL during peer integration | Intermediate failures were UI unit fixtures passing String to the new private LoadError; later centralized test builds succeeded. |
| `cargo test ... --lib subscription -- --test-threads=1` | PASS before final producer fix | 17 passed, 0 failed in `subscription-tests.log`. This predates the review fix for Codex's missing-credentials branch and fixture extension; rerun/final gate is pending. |
| `cargo test ... --lib tui:: -- --test-threads=1` | PASS | `tui-unit-tests.log`: 113 passed, 2 pre-existing ignored. Includes both quota scope/request-tag tests and all three Usage presentation tests below. |
| `tui::quota` owned regressions | PASS | Both nested/unwind/fingerprint scope and stale/cancelled queued-result tests passed in the TUI unit run. |
| `tui::panels::usage` owned regressions | PASS | All three layout/quantity/email, loading/empty/known-error/legacy-detail/Unicode and NoColor tests passed in the TUI unit run. |
| Existing `--test tui usage_panel` | NOT_RUN | Coordinator runs after source integration. |
| Full Rust gate / architecture / native terminal | NOT_RUN | Coordinator and final A acceptance own these; headless checks are not native evidence. |

All Cargo checks use the configured Rust 1.97 toolchain first on PATH,
`CARGO_TARGET_DIR=output/s-language-rust-target`,
`LLMUSAGE_PRICING_REFRESH=off`, locked/offline dependency resolution.
The repository's managed target link was not repaired.

Review caught the terminal `codex::read_auth` missing-credentials error as one
unannotated producer branch. It now carries `MissingCredentials` with the
unchanged canonical text. The existing no-HTTP credentials fixture now covers
invalid present Claude and Codex credential files and checks both unchanged
files and both hints. Owned-file formatting passed after this fix; the final
subscription rerun remains the coordinator's responsibility.

## Added test coverage

- Local listener: live/fresh cache preserve the exact report and hints, five
  provider requests, exact original absolute values, unchanged credential
  bytes, old reader/new metadata and old cache/new reader compatibility.
- Cache metadata: malformed scalar/null, unknown window, missing quantity
  field, incorrect dimensions, extra diagnostic field, wrong fingerprint and
  changed public report all keep a valid fresh cache hit without more requests.
- Known credential failure is captured before string erasure without HTTP;
  original credential bytes and canonical diagnostic stay exact.
- Kimi 101/100 remains 101/100 with a clamped 100% remaining. Grok fractional
  quantities and nested metric ordering retain their original values.
- Scope: nested/empty/mismatched/unwinding restores the previous hints; stale
  and cancelled queued updates cannot clear the current request's fetch state.
- TestBackend: narrow, medium and wide Usage layouts; paired copy; remaining
  quantities; hidden/revealed email; own versus original diagnostic detail;
  loading/empty/reset; Chinese display-width truncation; NoColor cell styles.
