# T UI implementation handoff

UI source frozen after a successful `cargo check --locked --offline --all-features --tests` on 2026-10-21. The check used the configured Rust 1.97 toolchain and `output/s-language-rust-target`; its only warnings were sandbox incremental-cache hard-link fallbacks. Executable tests and the complete Rust gate are run centrally by the coordinator. This document does not claim those pending results.

## Presentation coverage

- T01: Panel/short/sort/window/source labels, all footer width branches, email/auto-refresh/source feedback. Navigation painting and clicking share one label constructor and Unicode display width, including borders, separators and clipping.
- T02/T04/T05: Overview, Models, Daily, Hourly, Monthly and shared period headers, loading/error/empty/detail states. Data identifiers, sort keys, numeric formatters and visible-row construction are retained.
- T06: Stats graph/card/day details, month/weekday labels, empty/narrow branches, context and heat legends. Calendar data and click geometry remain unchanged.
- T07: Behavior categories, support levels/reasons and comparison metric labels use stable facts. Optimize keeps the original public DTO, SQL, detectors, thresholds, ranking, score and savings. Private same-call evidence retains original detector parameters and binds hints to the exact report fingerprint. Accepted loader results carry report and hints together under the existing generation/filter/window checks; nested frame scopes restore on unwind.
- T08/T09: Blocks and source/help/settings dialogs, monitor status/action explanations and associated controls.
- T10: Sync progress/completion/cancellation, typed safety refusal/pricing/domain/registry failures and remote skipped-host explanations. The observer captures neutral facts before canonical errors become strings, keys facts by job ID, and renders only on the UI thread. Each drain accepts at most the registry channel's 128-event batch; matching remote facts are consumed. Canonical events, snapshots and run logs remain unchanged.
- T03: Quota owner supplies report+hints updates and a fingerprint-checked frame scope; UI event-loop glue accepts the pair together. Public AppState gains no fields.

## Startup and boundaries

Direct public TUI entry points read preferences from the resolved Store root once before terminal setup. CLI dispatch uses the existing entrypoint snapshot through the private `run_terminal_with_language` adapter, preventing a second read/diagnostic. Both paths enter a synchronous unwind-safe scope on the UI thread. There is no menu, polling or runtime language switch.

Private loader failures retain typed cancellation/semaphore/task/domain facts until UI presentation. Unknown query/OS details carry an explicit original-detail marker. Domain workers do not read language or depend on the commands layer.

The shared catalog contains 209 paired static labels and 54 compile-time paired format templates, plus typed value/support/metric/Optimize/monitor projections. Unknown private static IDs fail explicitly; external values/details retain their separate fallback behavior.

## Test registration and added checks

`tests/tui/language.rs` is registered by the coordinator in the existing explicit TUI target. Its tests cover all nine panel shells/loading/data/empty branches, original error details, six navigation widths in both languages, dialogs/footer/details, unchanged hotkeys/sort/selection and Chinese NoColor across panels/dialogs.

Private tests additionally cover:

- `startup_language_is_one_root_snapshot_and_restores_on_unwind`
- `loader_owned_failures_are_typed_before_presentation_and_external_detail_is_marked`
- `sync_drain_yields_at_one_event_batch_and_keeps_the_remainder`
- `terminal_failure_uses_same_job_facts_and_ui_thread_language`
- `same_call_hints_preserve_public_payload_and_original_detector_parameters`
- `frame_hints_match_the_report_and_restore_after_nested_scope_and_unwind`

The coordinator's first TUI integration run reported 40 PASS / 1 FAIL. The failure was in the new test expecting arrow glyphs `↑/↓`; the existing renderer uses `▲/▼`. The assertion now checks those original glyphs. The original failed run log remains available; a rerun is required for the corrected test.

## Remaining verification

Coordinator: run focused private/TUI/query tests, strict clippy and the full required Rust gate after formatting. Reviewer: inspect the final owned diff and shared private interfaces. A owns docs/cross-surface validation and native terminal evidence. Native smoke is **NOT_RUN** here; TestBackend is not native acceptance.

No real usage sync/rebuild, public quota network calls, credential writes, dependency/lockfile updates, install, commit, push or release were performed by this UI owner.
