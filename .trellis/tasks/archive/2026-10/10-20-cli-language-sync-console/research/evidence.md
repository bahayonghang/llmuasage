# S implementation evidence

Implementation authorization is the 2026-10-20 user request recorded at the top of the task artifacts. This stage preserves the verified F startup/preferences/error work. No commit, installation, release, real-user import, or dependency/lockfile modification was performed.

## Interfaces and scope

- `common::i18n::logging::project(Language, &str, Option<&str>, &serde_json::Value, bool) -> String` is the pure presentation API for C's human log replay. The last argument enables detailed DEBUG/TRACE fields. Known events are selected only by `fields.event_code`; unregistered and historical logs have an explicit original-log marker and retain their original detail. Control characters are escaped. Default known-event output omits version/hash/path fields and redundant operation/phase facts; critical counts, source, host and error details remain visible.
- `runtime::logging::{init_cli_logging_for_paths, init_cli_stderr_logging}` capture an immutable language in the console formatter. Existing initialization APIs retain the default formatter for non-CLI callers. Console text still passes through the complete-event `StderrLogWriter` and existing scoped sink. File JSON, original message/target/level/fields, environment filters, dropped-event counters and rotating writer are preserved; optional stable event metadata is additive.
- `sync::notices::{RunNotice, NoticeSink}` and `engine::run_once_with_feedback` are crate-private. Public engine entry points delegate with no feedback. Notices use already-observed coverage, choice, retained stock, typed source issues, failures and remote outcomes. No extra per-file query or discovery scan was introduced. The cancellation notice belongs to the existing branch that withholds accounting certification after observing cancellation, rather than polling after successful return.
- `commands::sync_recovery` owns bilingual prompt I/O. The existing stream/flag predicate controls installation. A prompt suspends the same renderer that coordinates log warnings. Missing and still-present out-of-scope inputs remain distinct; unreadable/incomplete coverage cannot offer loss acceptance. k/K/r/R, empty input, EOF, read errors and the one-retry rule are preserved.
- `common::i18n::sync` contains pure paired progress copy. Human renderers capture language before spawning the reporter. Sync events and shards retain their original serialization.
- Summary rendering uses Unicode display widths and a closed ten-column table. Numeric cells, source ordering, stock totals and saturation remain independent of translated headings. Only source labels can shrink; minimum numeric widths may exceed a narrow requested terminal width. Diagnostics follow the bottom border, grouped by source. Typed absent sources are informational. Malformed/oversized faults remain separate from skipped/accounting information. Samples retain the cap of eight, total/omitted counts, basename-only locations, bounded reasons, OpenCode offset suppression and ZCode timestamp semantics.

## Production event coverage

`event-inventory.json` records the preflight inventory and classifies test-only sites. Its original line offsets describe the preflight source, not current post-format locations. `common::i18n::logging::tests::production_tracing_sites_have_unique_paired_catalog_entries` parses current Rust syntax, including imported logging macros, and verifies unique production IDs and exact catalog coverage.

There are 174 production macro sites in 44 files: 101 INFO, 39 DEBUG, 17 WARN and 17 ERROR. There are no production TRACE emitters at this snapshot; the formatter supports TRACE. Eight existing test-only emitters are excluded from user-surface coverage and retain their original contents. The preflight raw grep count of 183 was not used as evidence of complete coverage. `implementation-source-snapshot.json` records the implementer's source-stable snapshot for separate review.

## Validation record

- `data-equivalence.log`: PASS, actual Codex import plus Antigravity keep-history fixture with and without feedback. Exact sorted snapshots compare all event/bucket/turn/tool/cursor/source-file columns except explicitly named audit wall times, plus accounting meta. Codex imports one event in each run; preserved Antigravity history and version 2 remain unchanged. The first fixture failure was a non-discoverable filename (`feedback.jsonl`); changing it to `rollout-feedback.jsonl` fixed the fixture without weakening inserted-event or snapshot assertions.
- `streams.log`: PASS, four integration tests. The paired test uses temporary preference roots for en/zh and verifies human stdout/table versus stderr progress, NDJSON lifecycle events, protocol-v2 shard records, and escape-free output under NO_COLOR, LLMUSAGE_NO_COLOR and forced-line modes. Existing legacy-warning test verifies the original file message as well as a human warning outside machine stdout.
- `s-unit.log`: PASS, 74 focused unit tests (sync command/progress/recovery/summary, shared log projection and runtime logging). This includes exact data equivalence, the already-cancelled/late-cancel/early-error distinction, all recovery choices and safe defaults, reporter language across spawn, table numeric/privacy invariants, coordinated warnings, guard restoration and file rotation/retention/counter behavior. An earlier unit snapshot passed 73 and failed one old absent-source string assertion. The final assertion checks the typed informational projection while retaining bottom-border ordering and sample/privacy assertions.
- `domain-gate.log`: PASS, architecture_dependencies 12, remote 8, store 2, sync 151 (173 total). Command: `cargo test --locked --offline --all-features --test sync --test remote --test architecture_dependencies --test store -- --test-threads=1` with the command-scoped target and pricing settings below. This gate compiled the implementation snapshot before independent review edits.
- First `scripts/ci-rust.py` attempt: fmt passed; Clippy stopped at `needless_lifetimes` in `field_label` and `needless_borrow` in the adapted summary alignment assertion. No full-suite or rustdoc success is claimed for that attempt. Independent checker owns these mechanical fixes and any review regression/hardening changes. Final rerun results will be recorded separately from this snapshot.

## Toolchain notes

Direct Cargo is `C:\Users\lyh\.cargo\bin\cargo.exe`. All dependency-sensitive Cargo commands use `--locked`; focused reruns also use `--offline`. The repository `target` is a broken symbolic link to a disappeared `D:\mbx-cache\targets\v1\...` destination after the interrupted session. Both sandbox and escalated direct Cargo failed with OS error 5 before compiling. The safe command-scoped workaround is `CARGO_TARGET_DIR=$PWD/output/s-language-rust-target`, inside the workspace's already-ignored `output/` directory; no symlink, machine configuration or lockfile was changed. For `scripts/ci-rust.py`, command-scoped PATH also prepends `C:\Users\lyh\.cargo\bin;` so subprocess Cargo avoids the unavailable wrapper target. Test runs set the existing `LLMUSAGE_PRICING_REFRESH=off` switch; fixtures use temporary roots and local data.

Native terminal interaction is not yet independently exercised in this stage; headless injectable terminal tests verify rendering, prompt decisions, warning coordination and guard cleanup. Independent checker evidence is separate from this implementer snapshot.

## Acceptance mapping

| Acceptance | Concrete evidence |
| --- | --- |
| S-AC1 | Current-source AST catalog coverage; paired pricing/remote/legacy projector tests; dual console/file subscriber test; runtime logging rotation, filters, retention and counter tests; production event inventory above. |
| S-AC2 | Paired pricing percentage/count/version-display test, paired replay/commit copy, existing migration/pricing/source/remote/bar boundary tests, no permanent per-source completion sentence, spawned reporter language snapshot test. |
| S-AC3 | Private engine callback and notice enum; typed coverage/choice/legacy/retained/issue/failure/remote facts from existing decisions; observed cancellation versus late token change and early validation error regression. Public SyncSummary, SyncEvent and SyncRunOptions fields are unchanged. |
| S-AC4 | Paired recovery input/retry/read-error tests; existing full stream capability/flag matrix with NeverRead; sync accounting domain tests for accepted/kept recovery and write protection. |
| S-AC5 | Paired closed-table display widths, ten cells, usize/u64 saturation and absent placeholders; source-grouped fault/info fixture, eight-sample cap/omitted counts, basename/hash/path privacy and offset rules; bottom-border ordering tests. |
| S-AC6 | `optional_feedback_preserves_imports_cursors_accounting_and_kept_history`, exact persisted snapshots and retained marker assertions; accounting/rebuild rollback domain tests. |
| S-AC7 | Paired subprocess human/NDJSON/shards test under three plain modes; warning/progress interleaving and scoped sink restoration; terminal guard error/cancel cleanup; pure projector preserves historical raw logs with an explicit marker. |

These map implementer tests and source contracts; they do not substitute for the independent review verdict or a successful final combined gate.

## Independent review follow-up boundary

The checker owns separate `check-evidence.md` and review fixes. It corrected the two Clippy mechanical findings, restored default visibility/Chinese labels for additional critical source/protection/pricing numeric fields, and added paired regression evidence. It confirmed that complete-line `safe_text` already protects both actual progress renderers, rejected an unnecessary second escaping edit, and added actual line/bar plus event-JSON preservation coverage instead. These edits are after `implementation-source-snapshot.json`.

The independent review also identified known sync error paths needing typed presentation context (lossy-rebuild refusal, OMP preflight, `--since` validation, fixed SQLite open-failure detail and inventory errors). The coordinator authorized a bounded private refinement, implemented as recorded below. The independent verdict remains checker-owned.

## Approved typed refinement and final rerun

- `sync::failure::{SyncFailure, FailureKind, RebuildRisk}` are private. The existing lossy/OMP/unsupported guard produces the exact previous canonical Display string with neutral facts, including host/source and missing/protected counts. Final CLI errors, private human failure rendering after reporter drain, and run-failed console logs project the same facts. Existing JSON Failed.error and file error/message remain canonical. Optional `failure_code/failure_facts` support C replay; unknown/malformed metadata remains visible rather than disappearing.
- `parsers::feedback` collects private facts only around the existing driver future. Public direct parser calls outside a collector continue unchanged. OpenCode/ZCode emit only in their existing failed-open branch; driver forwards private facts to engine notices without public trait/context/stats/event changes. Tests prove nested and concurrent-future isolation, no inherited collector in spawned tasks, and drop/error restoration. No new dependency was introduced.
- `SourceFileListing.inventory_facts` records typed provenance at original inventory failures without changing its canonical errors/error_summary. Only the six built-in inventory-error consumers call `error_summary_with_feedback(source)`. Merely consulting discovery/ownership/coverage emits no parser failure. Inventory samples retain the existing cap of three with explicit total/omitted; parse-issue samples retain eight. Unknown additional listing errors remain marked original detail; ownership-skip notes stay outside faults. Merged listings offset provenance indexes. WalkDir OS facts use `io_error().to_string()` so human samples avoid the full private path in the canonical WalkDir wrapper. Existing warning emitters add only optional neutral kind/detail fields; valid typed projection suppresses the own wrapper, while malformed/future metadata retains raw fallback.
- `refinement-red.log` is not evidence that both failures reached the CLI: its SQLite case reproduced the original user-visible gap, but its lossy fixture stopped at a missing required SQLite token column. The original actual lossy CLI reproduction is the independent review evidence in `check-evidence.md`. The positive-history fixtures were repaired by supplying required zero-valued token columns while keeping the 12-token and rollback assertions intact.
- `inventory-red.log` then passed the four repaired loss/OMP/since/SQLite/stream groups and reproduced the actual Codex regular-file-root gap. `refinement-green.log` finally passed all five paired actual subprocess groups: en/zh, human failure progress/final errors/console under RUST_LOG off and error, original wire/file errors, non-destructive history checks, canonical source stats/shards and original success/failure exits.
- `refinement-unit.log`: PASS 94 focused units including final typed guards, inventory provenance/cap/unknown/ownership/privacy, renderer boundaries, collector scopes, feedback equivalence and logging preservation. These passed before the final mechanical let-chain change below.
- `ci-rust-after-review-attempt1.log`: fmt passed, Clippy rejected the nested `if let` in the new private human failure adapter. The checker collapsed it to an equivalent let-chain; no decision or ordering changed. The failed source snapshot is `final-source-snapshot-attempt1.json`.
- `ci-rust-after-review-attempt2.log`: fmt and Clippy passed; the full library run passed 969, failed six profiling correctness fixtures and ignored 19. All six failures were OS error 183 from the existing test helper hardcoding `target/writer-benchmark`, bypassing CARGO_TARGET_DIR and traversing the broken MBX target symlink. Coordinator independently verified that lower-layer directory creation fails there while the ignored output target works. This is not a passed gate.
- The coordinator authorized the minimum test-only fix: `store/sync_writer/profiling/tests.rs::temp_root` now honors a nonempty CARGO_TARGET_DIR and otherwise uses the original manifest/target fallback, retaining writer-benchmark/generated suffixes and all benchmark data/assertions. No production or symlink/config change was made. `profiling-path-gate.log` records the targeted rerun; the final full gate also sets command-scoped CARGO_NET_OFFLINE=true.
- `final-source-snapshot.json` records all Rust src/test files plus Cargo.toml/Cargo.lock and the Rust gate script immediately before the new full rerun. The previous snapshot is `final-source-snapshot-attempt2.json`. `ci-rust-after-review.log` is the in-progress full gate after the test-target fix. No full-suite/rustdoc success is claimed until its completion is recorded.

## Pre-existing dashboard fixture boundary found by the full gate

`ci-rust-after-review-attempt3.log` passed fmt/Clippy and 974 library tests, with one failure and 19 ignored: `api_dashboard_interactive_scope_is_lean_and_selected_window_only` saw 0 rather than 10 tokens. Three exact-filter probes reproduced the failure. The earlier complete-library attempt had passed that same test before UTC midnight.

Source tracing and fixed-clock SQLite probes establish a Local-calendar versus requested-UTC window difference, **not** the initial same-second comparison hypothesis. At Windows Local Oct 20 evening / UTC Oct 21 early, existing `apply_window_filter` selects until Oct 20 from Local time, then QueryFilter converts the requested UTC bound to `hour_start < Oct 21T00:00:00Z`. The fixture's raw current UTC seed is outside; merely flooring it to the current half-hour still yields 0. A previous UTC day's canonical half-hour bucket yields the expected 10. See the independent `check-dashboard-clock-proof.json` and implementer `web-window-bound-proof.json` (Windows .NET clock input, not the Python wrapper's different local timezone).

Coordinator authorized only that response-shape/scope fixture to use `crate::util::bucket_start(Utc::now() - ChronoDuration::days(1))`. Its 10 tokens, 500 cursors, selected-window/lean-shape and 128 KiB assertions remain unchanged. `web-window-fixture-gate.log` and the independent exact-filter rerun pass. No Web production window predicate or timezone behavior changed. This failed source snapshot is `final-source-snapshot-attempt3.json`; the final snapshot was refreshed afterward.

**OUT_OF_SCOPE, unresolved:** existing relative Web windows derive dates from Local even when a different query timezone is requested. The sibling day/range behavior fixture uses a fresh 2-hour-offset UTC seed and can encounter the same broader date-policy mismatch later in the cross-date interval; it tests filtering semantics rather than response shape and was not changed. The one-pass raw-now sweep found only the single response-shape fixture above with the identical immediate-now seed. This issue must not be reported as fixed by S.

## Final verified gate

`ci-rust-after-review.log` completed with exit code **0** on `final-source-snapshot.json`. Every source/test/Cargo/script hash still matches that snapshot after completion. `git diff --check` also passed.

| Gate | Actual result |
| --- | --- |
| cargo fmt --check | PASS |
| cargo clippy --locked --all-targets --all-features -- -D warnings | PASS |
| cargo test --locked --all-features -- --test-threads=1 | PASS: 1253 passed, 0 failed, 19 existing ignored |
| Library | 975 passed, 19 ignored |
| API / architecture_dependencies / CLI | 3 / 12 / 54 passed |
| query / remote / store | 9 / 8 / 2 passed |
| sync / TUI | 155 / 35 passed |
| Binary / doc-tests | 0 / 0 tests, successful |
| RUSTDOCFLAGS=-D warnings cargo doc --locked --no-deps | PASS |

Final command environment was scoped to this invocation: direct Cargo PATH prefix, `CARGO_TARGET_DIR=<repo>/output/s-language-rust-target`, `CARGO_NET_OFFLINE=true`, and `LLMUSAGE_PRICING_REFRESH=off`. The official Rust gate script and manifests/lockfiles were unchanged. `profiling-path-gate.log` separately passed all seven active profiling correctness tests (six existing release benchmarks ignored), and `web-window-fixture-gate.log` passed the one approved Web shape fixture. Stopped attempts remain separately recorded and are not counted as successful gates.

S-AC1 through S-AC7 now have the implementation and real test evidence mapped above; independent acceptance is recorded only in checker-owned `check-evidence.md`. Native terminal smoke and final cross-surface `just ci` remain A-owned and are not claimed by this Rust stage. The pre-existing Web timezone issue remains unresolved outside S. No commit, push, installation, release, real-user import or real-user rebuild was performed.
