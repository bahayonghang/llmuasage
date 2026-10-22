# Shared sync presentation handoff

Date: 2026-10-21. Owning implementer source freeze; executable validation is delegated to the coordinator because sandbox MSVC linking previously failed. This document is not a whole-T acceptance claim.

## Files and behavior

- `src/sync/engine.rs` consumes the existing private `RemoteImporter::import_with_facts` at the original import call. Success unwraps the unchanged public outcome. Failure uses the original `failure.error.to_string()` for contact state, skipped-host map and `SyncEvent::RemoteHostSkipped.reason`; private facts accompany `RunNotice::RemoteSkipped` before that event. Canonical warning tracing and import/commit/deferred Pi/OMP reset/sweep/contact/watermark/accounting order remain unchanged.
- `src/sync/notices.rs` adds only private remote `host_id` and optional neutral importer facts. No public event/type/trait/wire fields were added.
- `src/commands/sync.rs` delivers the same private notice into its renderer before appending it to existing invocation notices. `sync_progress.rs` keeps facts by host in each line/bar renderer, renders the corresponding event with its captured language, then removes those host facts. No string classification, re-read, process-global last-fact buffer or new sync action exists.
- `src/common/i18n/sync.rs` adds `remote_detail` and `progress_with_remote`. Known importer copy uses the C catalog's closed facts; unknown transport detail remains explicit original detail. `sync_summary.rs` consumes the same facts for the post-table notice.
- The existing S notice content was moved without text/logic changes to pure shared `sync::notice_lines(notice, language)`. CLI color and the existing uncolored coverage/inventory branches remain in `sync_summary.rs`. The TUI uses `sync::notice`, which joins those same lines and adds the stable source id. Existing sample caps, counts, fault/information distinction, privacy and recovery actions are retained.
- `src/sync/job_registry.rs` adds a private optional borrowed observer. Ordinary `new`, `Default`, executor signatures and public snapshots/events stay unchanged. The private default constructor delegates the same locked engine core, with the same acquired fenced Store; it does not reacquire the worker lock or duplicate the sync algorithm.
- Job execution now retains the original typed anyhow error until terminal presentation; run-log and public event/snapshot strings still use the exact original alternate Display. Bootstrap's private pricing result carries facts beside its original `LlmusageError`, without a new canonical source wrapper.
- At the coordinator-approved terminal run-log failure branch, private `RegistryFailure` preserves the exact existing complete canonical Display plus neutral command/status, original typed finish error and optional original typed terminal error. Its Error source is deliberately empty because the canonical text already includes both details. Run-log failure policy and the public terminal string remain exact. Pure `sync::registry_failure` accepts already UI-projected details.

No quota/provider/DTO/business algorithm, CLI interaction policy, cancellation policy, public trait/API, settings, dependencies/lockfiles, real credentials/data, SSH/network, installation, commit, push or release change was made. `default.rs` did not need modification.

## Frozen private UI API

```rust
pub(crate) enum JobPresentation<'a> {
    Notice(&'a RunNotice),
    Failed {
        error: &'a anyhow::Error,
        pricing: Option<&'a crate::domain::pricing_catalog::diagnostic::DiagnosticFacts>,
    },
}
pub(crate) type JobPresentationObserver =
    Arc<dyn for<'a> Fn(&str, JobPresentation<'a>) + Send + Sync>;
JobRegistry::with_presentation_observer(observer) -> JobRegistry
```

Callbacks run synchronously at the original notice/failure point, before the corresponding public event. They clone only neutral facts into the UI's job-id-associated channel. Locale and rendering belong to the UI thread. `RegistryFailure` exposes crate-private `command`, `status`, `finish_error`, and `original_error`; capture the original recursively so a finish-write error cannot hide a known safety refusal. The UI owner was sent the exact interfaces before implementation and after freeze.

## Added focused regressions

- `sync::engine::presentation_tests::remote_skip_facts_preserve_events_contacts_partial_commits_and_watermarks`: local MemoryShardSource fixtures cover no header, wrong first record, missing trailer after an actual committed event and successful import. Feedback enabled/disabled compares exact public events, canonical contact/skipped values, event counts, contact-set and watermark. Failure cannot certify accounting, partial commits remain, and success keeps its event-derived watermark.
- `commands::sync_progress::tests::paired_remote_facts_reach_line_bar_and_summary_without_changing_events`: English/Chinese line and actual indicatif bar targets plus post-table text use known protocol facts, keep numeric arguments and canonical event JSON, consume per-host facts and retain explicit unknown detail.
- `sync::job_registry::tests::presentation_observer_sees_typed_failure_before_unchanged_events_and_run_log`: fake executor returns nested typed refusal. Observer sees original kind and exact job id; events/snapshot/run-log remain canonical and enabled/disabled results match. Only generated job id and measured lock-wait milliseconds are normalized in event comparison.
- `sync::job_registry::tests::presentation_observer_keeps_bootstrap_pricing_facts_beside_original_error`: a temporary invalid pricing marker yields typed same-call pricing facts with the exact public canonical error once; no run-log row is created before bootstrap failure.
- `sync::job_registry::tests::presentation_observer_keeps_terminal_write_and_original_typed_failure`: a temporary SQLite trigger rejects terminal run-log UPDATE after the fake typed refusal. Observer sees both typed inputs, paired presentation retains the refusal and external detail, and canonical event/snapshot do not duplicate a source chain. The failed UPDATE leaves the original running row intact.
- `common::i18n::sync::notice_tests::neutral_notice_projection_pairs_sources_counts_actions_and_original_details`: paired own notice branches, all eight stable source-issue codes, source labels, recovery protection, capped inventory 3/14 with 11 omitted and unknown original detail.

All tests are unit modules under the existing library target; no new integration target/registration is required. Existing recovery, summary diagnostic privacy, cancellation/drain, worker-fencing, remote accounting/Pi-OMP and sync JSON regressions remain required in the coordinator gate.

## Actual validation

Environment: real toolchain `C:/Users/lyh/.rustup/toolchains/1.97.0-x86_64-pc-windows-msvc/bin` before `.cargo/bin`; `CARGO_TARGET_DIR=output/s-language-rust-target`; `LLMUSAGE_PRICING_REFRESH=off`; locked/offline/all-features. The broken root target symlink was left untouched.

| Check | Actual result |
| --- | --- |
| `rustfmt --check --edition 2024` on all seven owned source files | PASS at freeze |
| `git diff --check --` the seven owned sources | PASS at freeze |
| `cargo check --locked --offline --all-features --tests` | PASS, zero errors; only filesystem incremental-cache hardlink/copy warnings. Earlier attempts sampled peer files while their types were changing and failed; one notice-wrapper Style move found during extraction was fixed with cloning before this passing check. |
| Executable focused/library/sync/remote/architecture tests | NOT_RUN by this worker; coordinator owns escalated execution after stable-source handoff. No repeated sandbox linker attempt. |
| Native CLI/TUI smoke | NOT_RUN; headless/type-check evidence is not native acceptance. |
| Full Rust gate / just ci | NOT_RUN by this worker; coordinator/A ownership. |

Source SHA256 values are saved in `sync-presentation-source-snapshot.json`. Readiness is source/API freeze for executable review, not independent T completion. Any subsequent fix requires refreshing the affected hashes and evidence.


## Full-gate regression follow-up

The coordinator's `ci-rust.log` records formatting and strict Clippy PASS, then
1016 library tests PASS / 19 existing ignored / 1 FAIL. The sole failure was
`dynamic_progress_detail_is_escaped_for_lines_and_bars_without_changing_events`:
its expected value still came from the old raw remote progress helper while
both renderers intentionally add the approved localized original-detail marker.

The narrow test fix uses independent explicit escaped English/Chinese expected
remote-skip strings, including the marker and exact escaped host/reason. The
existing failure-event expectation, no-control-character checks, line/bar exact
comparisons and unchanged canonical event JSON assertions remain. Product code
was unchanged. Owned-file rustfmt and git diff checks PASS after this fix; the
coordinator owns the focused executable rerun and full gate rerun. The progress
source hash was refreshed in the snapshot.
