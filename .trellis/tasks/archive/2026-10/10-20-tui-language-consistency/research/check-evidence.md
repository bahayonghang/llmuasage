# Independent T source review

Date: 2026-10-21. Reviewer: `tui_check`. Task authorization is the leading
2026-10-20 authorization note; historical planning-only paragraphs do not
override it. Review preserves concurrent owners' changes. No real data,
credentials, network, install, commit, push or release was used.

## Scope and source conclusions

- Read the saved check-hook output, T PRD/design/implement/check context,
  parent contracts, C-to-T source handoff and owning-worker evidence. Reviewed
  shared sync/registry/CLI consumers, all subscription producers/cache/private
  metadata, quota controller and Usage renderer, then current UI/query changes.
- Shared sync uses the same locked engine core. The observer retains original
  typed failures until presentation; canonical alternate Display still feeds
  public events/snapshots/run logs. Remote importer facts accompany the exact
  original event/contact/skipped reason. No import/reset/sweep/watermark or
  public executor interface change was found.
- Quota private hints are emitted alongside original provider decisions and
  quantities. Public DTO fields/values and request/credential reading order
  stay intact. Kimi/Grok quantity provenance is retained without deriving
  absolute amounts from percentages. Cache metadata parses independently;
  report fingerprint/shape validation rejects bad hints while preserving a
  valid fresh report. TTL remains `fetched_at` / 300 seconds.
- Quota request tags reject replaced/cancelled queued outcomes before changing
  fetch state. The nested/unwind-safe frame scope validates the report and
  uses pointer identities only for lookup during its borrowed render frame.
- UI uses a synchronous language scope; workers retain neutral typed data.
  Optimize captures existing detector locals with unchanged SQL/math/sorting
  and public DTOs; accepted generation/filter/window plus report fingerprint
  associates private presentation. Navigation shares localized label building
  and Unicode display width. Literal `tr()` IDs were statically checked against
  the paired catalog with no missing ID.

## Findings and coordinated fixes

1. `src/subscription/codex.rs::read_auth` still emitted an untyped own error
   when a credential file exists but provides no usable login. This reached
   Chinese Usage as English original detail. Quota owner changed it to the
   existing private `MissingCredentials` fact with exact original Display and
   extended the no-HTTP malformed-credentials fixture to include Codex. Reviewer
   confirmed the producer and exact canonical/no-HTTP fixture fixes in source;
   final centralized test rerun pending.
2. The new `run_dashboard` preference read would follow the CLI startup read
   and duplicate its corrupt-settings warning, potentially changing language
   between startup and rendering. Reported to coordinator/UI owner; approved
   fix is a private selected-language entry for CLI and the unchanged public
   root-reading entry for direct callers. Confirmed final source:
   `commands::dash::run` calls `run_terminal_with_language(current())`, while
   public `run_dashboard`/`run_terminal` retain their original signatures and
   use one `with_startup_language` read. The startup scope regression covers
   missing-root no-create, profile settings, no polling, reopen and unwind.
3. `SyncController.remote` retained consumed host facts for the whole job.
   Reported to UI owner to remove facts with the matching skipped-host event.
   Confirmed final source consumes `remote.remove(host_id)` at the matching
   `RemoteHostSkipped` event and clears remaining facts on terminal cleanup.
4. Presentation `std::mpsc::channel` is unbounded. While a paused UI is limited
   by the existing two 128-event queues, the old receive-until-empty event loop
   could consume a schedule-dependent arbitrary number of host events before
   draining private notices. Configured host count has no fixed cap. Coordinator
   approved a 128-event per-tick batch plus consumed-fact removal, preserving
   every notice and existing worker backpressure without blocking the observer
   or dropping facts. Confirmed final source uses `SYNC_DRAIN_BATCH = 128` and
   the regression retains a second full event batch for the next call. Private
   channel uses an unbounded API, but pending production is now bounded by
   the event batch, the two existing 128-event queues and a fixed local-source
   notice tail. Do not describe this as a bounded-channel type.
5. UI missing-runtime errors still used own plain English context in sync and
   loader constructors. Reported to UI owner for paired context at the existing
   UI-thread error boundary. Confirmed sync/loader constructors now select
   paired context without changing their original runtime requirement.
6. The new TUI same-job failure regression expected uppercase `Pi`/`OMP`,
   whereas the actual catalog correctly preserves lowercase stable command
   IDs. With coordinator authorization, reviewer changed only the assertion
   to require the Chinese refusal context plus exact
   `llmusage sync --rebuild --source pi` repair command. No product text changed.
   Direct configured-toolchain `rustfmt --check` and `git diff --check` passed
   on this file after the patch.

## Verification status

No executable Rust tests were run by this reviewer: the coordinator explicitly
owns them after sandbox MSVC linker-temp failures. `git diff --check` passed on
the reviewed shared-sync/subscription files; only LF/CRLF advisory messages.
Static literal catalog-ID coverage passed. After UI freeze, `git diff --check`
also passed for TUI/query/catalog/new integration tests. This is not a full
lint/type-check/test PASS claim.

Coordinator reported (not independently rerun here): TUI library 113 PASS /
2 existing ignored; initial TUI integration 40 PASS / 1 failing new sort-arrow
assertion (expected wrong glyph), assigned to UI owner. Quota worker records
17 subscription PASS before the final Codex producer fix. Full Rust gate and
current-source focused reruns remain required. Native terminal acceptance and
final cross-surface `just ci` remain A ownership and NOT_RUN by this reviewer.

## Final source review and remaining validation

UI source freeze was reviewed. The added same-call Optimize fixture compares
the complete private/public/legacy payloads, creates all four detector facts
and rejects a mismatched payload fingerprint. Behavior render scope tests
paired evidence and nested/mismatched/unwinding restoration. TUI terminal
failure regression rejects stale job facts and projects on the UI thread;
typed loader tests retain original external details. Producer registry tests
also cover bootstrap pricing and simultaneous terminal-write/original failures.

No unresolved source defect remains from this review. All source findings
above are fixed in place by coordinated owners or the reviewer. Coordinator
reported current `cargo check --all-features --tests` PASS and standard fmt
PASS before full gate. First full gate stopped at a quota catalog redundant
guard; coordinator replaced it with the direct `Seconds(604800)` pattern and
is rerunning fmt/full gate. Final lint/test/rustdoc evidence must come from
that run. No current full-gate PASS, whole-T completion, native acceptance or
parent completion is claimed by this source review.

## Coordinator final gate confirmation

`python scripts/ci-rust.py` EXIT0 on the frozen source: 1320 PASS, 19 existing ignored, 0 failed; fmt, strict Clippy and rustdoc -D warnings PASS. `research/final-source-snapshot.json` identifies this tree. The new source/producer/scope/queue regressions and corrected explicit escaped-marker assertion all passed. Earlier failed logs are retained. Native smoke remains NOT_RUN and A final just ci remains required.
