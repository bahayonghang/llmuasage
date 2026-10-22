# Research: TUI language implementation handoff

- Query: Refresh next-child T implementation boundaries against the current source and propose disjoint UI/quota ownership.
- Scope: internal; read-only source research, no builds or product edits.
- Date: 2026-10-21
- Active task: `10-20-cli-language-command-output` (C). T is still planning. This file is here because the research role may write only the active task research directory.

## Findings

### Source-current entry points and shared language

- `src/commands/dash.rs::run` already uses the C paired catalog for deprecated-alias startup copy, creates the Store, then calls `tui::run_terminal`. T should not change that command.
- `src/tui/mod.rs:59` `run_dashboard(&Store)` resolves theme, installs panic cleanup, enters raw mode, calls `event_loop`, and restores the terminal. `run_terminal` at line 102 delegates unchanged. Read `runtime::preferences` using `store.paths.root_dir` before raw mode, then hold `common::i18n::with_language(language, || ...)` on this synchronous UI thread. Keep public signatures unchanged. A Tokio task-local scope alone does not establish scope in spawned threads/tasks.
- `src/common/i18n/mod.rs` already supplies `Language`, `text`, `current`, async `scope`, and synchronous unwind-safe `with_language`. Add only the T catalog registration here; do not redesign F scope or preferences.
- `src/tui/mod.rs:106` owns actions and draw on the same thread. Keep request/drain/dirty ordering, 250ms tick, and theme render snapshot (`draw.rs:26`) unchanged. Language is one startup snapshot, not a setting-file poll or new menu.
- `src/tui/app.rs:409` public `AppState` contains public quota_report and cached String errors. Do not add a new required field for locale/hints. A private event-loop session/local hint variable plus frame-scoped hint access fits the reviewed design.

### T01–T10 source map

| Coverage | Sources and decisions | Required localized boundary |
| --- | --- | --- |
| T01 shell | `app.rs:78/92` Panel long/short labels; `:197` TimeWindow labels; `:319` sort labels; `:519` source filter; `footer.rs:36/70/126`; `nav_bar.rs:12` | Labels, controls, sort direction, loading/auto-refresh/state while enum/query keys and shortcut letters stay stable. |
| T02 Overview | `panels/overview.rs` | Chart/list legends, period/title, data/loading/empty/error; preserve raw model names, visible window and cost order. |
| T03 Usage | `panels/usage.rs:15/86/149/254/318`, diagnostic at `:422`, metric at `:427/455`, readiness at `:509`, snapshot/reset at `:649/679` | Every width layout, action strip, summary/detail/table, auth/credential labels, hidden email, readiness, reset and remaining text. Quota hints handle app-owned message/absolute-value provenance. |
| T04 Models | `panels/models.rs` | Headers, hints, loading/empty/error; stable sort keys and existing width tiers. |
| T05 periods | `panels/{daily,hourly,monthly,period}.rs` | Shared `period_header_labels`, day separators/detail/back labels plus empty/loading/error; date grouping and sort do not change. |
| T06 Stats | `panels/stats.rs`, `mod.rs:478` hit testing | Heatmap/stat labels, favorite N/A, selected Day Breakdown, dates/back controls. Preserve shared shell geometry and date selection. |
| T07 Agents | `panels/behavior.rs:22/53/82/114/200/296` | Four sections, metrics, support/degradation, optimize findings and comparison warnings. See typed-data caveat below. |
| T08 Blocks | `panels/blocks.rs` | Headers, active/ended/projected/limit, empty/loading/error; numeric projection untouched. |
| T09 dialogs | `help_dialog.rs:10`, `source_picker.rs:11/58`, `app.rs:536/557` | All help/settings rows, theme/source/window labels, monitor-only status/action and toggle/status feedback. Preserve theme IDs and key mappings. |
| T10 sync | `panels/sync_status.rs:17/75/179/223/362/402/421/431`, `sync_control.rs:38/135/154`, `mod.rs:325` | Overlay header/reason keys, source monitor rows, fault versus accounting info, last-run display status, progress/start/reject/complete/fail/cancel. No commands dependency, JobRegistry rewrite or lossy rebuild prompt. |

`Panel::Trends` is Usage, Sources is Daily, Projects is Hourly, Health is Stats; do not rename/reorder these enums.

### Navigation width is a functional change

`src/tui/nav_bar.rs:12` paints `format!(" {} {} ", index + 1, label)` with separators. `panel_at_position` at line 55 repeats the choice and line 75 uses `.chars().count()`. Replace this measurement with Unicode display width and preferably share the exact label construction/measurement with painting. Retain existing <60 narrow threshold and right-border clipping. Test each rendered Chinese tab's actual cell span, separator, border and clipped span against the hit function, for both long and short labels. Existing English coordinates at lines 96–106 must remain valid.

### Quota producer/cache contract and split interface

Current public API is `subscription::fetch_all(&FetchContext, bool) -> UsageFetchOutcome { report, cache_hit }` (`mod.rs:61/67`). Public `UsageMetric`, `UsageOutput`, `UsageFetchDiagnostic` and `UsageFetchReport` are literal-constructible/serializable (`types.rs:4/22/43/65`); keep all unchanged.

- `mod.rs:88` fetches providers concurrently and `maybe_fetch` at line 123 erases anyhow errors into message strings. Private producer hints must be captured before this erasure, without parsing free text later.
- `http.rs:19` owns HTTP-status explanations. Claude credentials errors are produced at `claude.rs:56/60/89`; Codex at `codex.rs:101/188`; Grok at `grok.rs:45/56/58/64/89/111`; Kimi at `kimi.rs:76/81/93`. Unknown OS/serde/HTTP transport details need a localized original-detail marker; known own errors need neutral typed variants.
- `grok.rs:187` `push_limit_metric` still has the original `limit` and `used` and formats remaining at line 200. `kimi.rs:161` `parse_quota_detail` has original integer remaining/limit and formats at line 173. Capture those numbers there; never reconstruct absolute quantities from percentages or split `remaining_label` strings.
- Provider metric labels also contain own window names. Claude `usage_metrics/window_metric`, Codex `rate_limit_window_label/metric_label/push_rate_limit_metrics`, Grok task labels and Kimi quota keys should emit stable metric identities, preserving any external provider/model/plan values.
- `cache.rs:9` private CacheDocument currently has only fetched_at/report; `load` line 14 enforces 300 seconds using fetched_at; `save` line 24 writes same report. Extend only this private document with optional/defaulted presentation, bind it to the exact report fingerprint, and independently discard malformed/mismatched hints without discarding an otherwise valid fresh report.
- A minimal agreed private result is a wrapper containing original `UsageFetchOutcome` plus `QuotaPresentationHints`; a crate-private `fetch_all_with_presentation` can share the same core as public fetch_all. Public fetch_all must still return the same values and cache_hit semantics. The coordinator should freeze actual names before concurrent edits.
- Hints contain stable typed message/window/unit IDs and original neutral arguments, not locale, translated messages, response bodies, credentials or extra account data. New cache remains readable by an old `{fetched_at,report}` reader; old cache loads with no hints.
- `src/tui/quota.rs:9/51/62` currently channels only report, with cancellation tokens but **no request generation ID**. Panel requests do have generations (`data_loader.rs:31/41`; `mod.rs:649`). A completed old quota result can already be queued when force-fetch cancels/restarts. Carry report/hints together and validate their association; if a private request tag is added to reject superseded hint/result pairs, record this as a new guard required for the T design, not existing quota behavior. Do not broaden request scheduling, cancellation or polling and do not mistake existing panel generation tests for quota coverage.
- `mod.rs:377` applies quota report and creates status messages; session-owned hints should update only when that report result is accepted. Around draw, activate a private hint scope validated against the same report, including direct Usage render tests. Do not add required public AppState fields or use a mutable process-global latest hint buffer.

### Proposed disjoint ownership after C acceptance

1. **UI implementer** owns `src/tui/` except `quota.rs` and `panels/usage.rs`, plus `common/i18n/tui.rs`, its single module registration, and TUI tests excluding new quota-specific tests. Owns startup scope, nav geometry, all non-Usage panels/dialogs, sync status/control and event-loop glue. Owns theme.rs's obsolete language guard update.
2. **Quota implementer** owns `src/subscription/`, `tui/quota.rs`, `tui/panels/usage.rs`, a separate quota catalog module if needed, and quota-specific unit/TestBackend tests. No edits to `tui/mod.rs`, `tui/app.rs`, main T catalog or test-module registrations; instead deliver the small private result/fetch/hint scope contract to UI owner/coordinator first.
3. **Coordinator** owns specs/task records, integration registrations and acceptance evidence. Coordinate before any shared `common/i18n/mod.rs` or test main edits. Do not edit C-owned `tui/report_table.rs`.

This split is useful only after the private result/hint render API is agreed. Otherwise a single implementer is safer than overlapping edits to event loop and catalogs.

### Existing verification anchors

All commands below are filters to run later; this research did not run them. Use the existing alternate CARGO_TARGET_DIR, locked dependencies, local mock endpoints and serial ordering.

```text
cargo test --locked --all-features --test tui -- --test-threads=1
cargo test --locked --all-features --lib tui:: -- --test-threads=1
cargo test --locked --all-features --lib subscription:: -- --test-threads=1
cargo test --locked --all-features --test architecture_dependencies -- --test-threads=1
python scripts/ci-rust.py
```

- `tests/tui/main.rs` is the only explicit TUI integration target and includes shell/panel modules; new files require registration there.
- Existing shell tests: nav shortcut, wide footer, narrow labels and monitor-only picker (`tests/tui/shell.rs`). Keep English assertions and add paired Chinese scenarios.
- `mod.rs` unit tests cover idle ticks, stale_generation_and_filter_results_are_rejected, window mismatch, loading before result, all-theme shells/dialogs, NoColor and shared Stats mouse geometry.
- `sync_control.rs` tests cover source counts, replay versus stream rows, token-accounting repair, start-once/second-trigger cancellation and terminal detection.
- `tests/tui/panels/{models,period,stats,usage}.rs` cover full-versus-visible buffers, sort headings, narrow tiers, selection/back, exact sync counts, rebuild-risk neutrality, quota rows/email toggle and empty states.
- `subscription::tests` cover four providers/unchanged credentials, partial failure, missing credentials, expired/corrupt/fresh-old-mtime cache and bypass; HTTP tests cover no redirects and no Authorization forwarding. Extend these for hints and cache provenance rather than replacing them.
- Add paired T01–T10 data/loading/empty/error/dialog coverage, actual Chinese tab hit tests, no new hint leakage into public serialization, old/new cache reader compatibility, malformed/mismatched hints, original absolute quota values, quota stale generation, and r/R/m behavior. Native terminal acceptance stays separate.

## Related specs and internal references

- `.trellis/spec/llmusage/backend/{cli-language,tui-presentation,tui-runtime,tui-subscription,source-sync,ci-toolchain}-contracts.md`.
- Parent `research/{surface-coverage,tui-localization}.md`; T PRD/design/implement are the approved scope (top authorization note supersedes historical planning-only wording).
- ADR 0005 keeps JobRegistry in the sync application layer and TUI independent from commands; ADR 0007 protects public error surface. No external docs/packages/versions were needed or consulted.

## Caveats / Not Found

- `theme.rs:917` `interactive_tui_copy_stays_english` scans all interactive source and existing TUI tests for Han characters. It will reject proper Chinese implementation/tests. It and the English-only spec clause must be deliberately replaced with meaningful paired-language coverage; keeping the guard and evading it with Unicode escapes is wrong. Existing semantic-theme/NoColor guards must remain.
- T07's app-owned query strings are not all externally sourced. The concrete minimal route is below; do not use any English/Chinese text matching or silently declare own producer text an original-detail exception.
- Loader's own semaphore/cancel/task errors are currently String producers (`data_loader.rs:401/402/412/423/429/439`); merely translating panel prefixes leaves these own messages English. Keep worker data neutral and add private typed display facts if needed; never localize stored/shared query errors.
- No product validation or native smoke was performed by this research. Source line anchors are this session's reads and can shift as implementation proceeds.

### T07 concrete neutral-fact route (coordinator review needed)

1. `query/mod.rs:151` `behavior_support` produces only `normalized` (no reason) or `no_data` (fixed own reason). The existing `BehaviorSupport.level` plus `supported` is enough for paired standard support explanation in Activity, Tools and Optimize; no producer/API change is needed here. Preserve unknown reason with an explicit localized original-detail marker.
2. `Dashboard::model_compare` (`query/comparison.rs:331`, plus `model_compare_batch`) already carries `insufficient_models`, `low_sample`, `normalized`, or `missing_model` in `support.level`. At lines 340/385/399 these directly identify the own support/warning variants. Project reason and warning from this level while retaining existing `model_a/model_b`, `candidates` and `low_sample` bools; do not recalculate query thresholds. `CompareMetric.id` (`one_shot_rate`, `retry_rate`, `cost_per_call`, `cost_per_edit_turn`, `cache_efficiency`, etc., line 128 onward) identifies display labels independently of `CompareMetric.label`.
3. `panels/behavior.rs:149–164` displays **title, evidence and recommendation**, not just title. `OptimizeFinding.id` selects static title/recommendation for `low_read_edit_ratio`, `duplicate_reads`, `junk_reads`, `session_outlier`; evidence needs original arguments that are not exposed separately in the public finding. In `query/optimize.rs`: `detect_low_read_edit_ratio` emits `{read_calls,edit_calls}` at lines 315–331; `detect_duplicate_reads` at 386/423–436 has `{session_id, safe_short(target,72), calls}`; `detect_junk_reads` at 439/476–489 has `{calls,safe_short(example,72)}`; `detect_session_outlier` at 492/550–563 has `{session_id,tokens,total_tokens}` (or the already computed ratio). Do not parse these values back from `evidence` or run another SQL query.
4. Smallest complete extension: in the same `query/optimize.rs`, add crate-private fact structs/enum and a crate-private `optimize_with_presentation` returning `(unchanged OptimizePayload, hints)`. Existing public `optimize` delegates the same internal core and discards hints. Each existing detector decision supplies its original canonical finding plus the neutral fact from the same already computed locals; preserve SQL, thresholds, order, rounding, score and estimated savings. Existing test-only `legacy_detect_*` paths stay canonical comparison baselines. No query/common dependency on language and no new required public DTO fields.
5. `tui/data_loader.rs:335` `load_behavior_panel_data` uses the private entry only for its optimize branch. Carry private hints beside the unchanged `BehaviorPanelPayload` through the private `PanelPayload`/`PanelResult` envelope, associated with its existing generation/filter/window and accepted together. Public `AppState.behavior` stays unchanged; private session/frame hints feed the UI renderer. This matches quota's scoped presentation pattern, without a global last-result buffer.
6. This is a **proposed ownership extension** beyond T's current frozen file list (`query/optimize.rs` is absent). The coordinator must explicitly record the narrow permission before an implementer touches that source; it is not authorization supplied by research. Standard support/comparison labels can proceed using existing stable fields without this extension.
