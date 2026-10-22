# S independent review — 2026-10-20

Scope: full S source, including untracked logging/sync catalogs, private notices, recovery adapter, production tracing emitters and registered tests. The reviewer explicitly loaded S check.jsonl, PRD/design/implementation artifacts, parent decisions/research and relevant contracts after switching from F. All probes use isolated temporary data; no real-user import, install, commit, push, release or lockfile change was performed.

## Findings fixed

- `common/i18n/logging.rs`: Clippy rejected an unnecessary explicit lifetime on `field_label`; elided it.
- `commands/sync_summary.rs`: Clippy rejected `display_width(&row)` where `row` was already a reference; removed the extra borrow.
- `common/i18n/logging.rs`: the default known-event whitelist hid essential numeric source/protection/pricing facts along with hashes. Restored visibility and Chinese labels for processed/changed/skipped files, seen events, scanned bytes, malformed/oversized lines, missing files, protected events, deleted orphan buckets and tracer error counts. The default still omits version/path hashes. A paired projection regression checks the values remain visible.

## Checked hypothesis, no product fix needed

The pure sync copy helper interpolates dynamic labels and error strings, but the **actual** `human_progress_line` boundary already applies `safe_text`, and both line and bar renderers use it. Non-determinate file messages separately use the sanitized `short_file_label`. The initial suspicion of a CLI escape leak was rejected after following these paths; no redundant escaping was added. A new paired test exercises actual line/bar output containing ESC/newline/carriage-return in remote labels/reasons and errors, and verifies original SyncEvent JSON is unchanged. It passes.

## Historical findings requiring an approved neutral-data refinement

1. **Known safety refusals remain app-owned English in Chinese output.** With positive Codex history and a tracked missing input in a temporary database, Chinese `sync --rebuild --source codex`, `RUST_LOG=off`, exits 1 with empty stdout. Stderr contains both `同步失败：Refusing lossy sync --rebuild ...` and `错误: 原始详情: Refusing ...`, including the complete English safety explanation. The producer is `sync/engine.rs::assert_lossless_rebuild`'s `bail!`; other known own refusals are in `refuse_omp_before_pi_split_migration` and `rebuild_sources`. The emit-shards invalid RFC3339 wrapper is similarly app-owned English. These are known application explanations, not third-party details. They need neutral typed facts at existing decisions and CLI presentation at progress/final-error/console boundaries. Preserve the canonical source/file detail, original validation order, safety behavior, codes, wire output and public struct literals; do not parse the English error string.
2. **Known database-open failure remains app-owned Chinese in English output.** With a temporary invalid OpenCode DB, default-English `sync --source opencode` preserves its existing success exit and produces `Diagnostics · opencode` followed by `Source failed; original detail: OpenCode SQLite DB 打开失败`. `parsers/opencode.rs` and `parsers/zcode.rs` produce these fixed own strings; the new private SourceFailed notice carries only a free-text detail. A neutral private reason must survive from the producer without inferring it from last_error text, changing the public parser/stat contract, or altering persisted canonical detail. This is a module-boundary/design refinement, so the reviewer reported it to main rather than silently expanding interfaces.

These findings prevented a full S acceptance claim at the pre-remediation snapshot. This historical verdict is superseded by the final independent recheck below; it is retained to explain the approved refinement and original evidence.

## Independent checks at review snapshot

Command-scoped `CARGO_TARGET_DIR=<repo>/output/s-language-rust-target` and direct `C:/Users/lyh/.cargo/bin/cargo.exe` were used. The old target symlink was preserved. No TLS/dependency settings changed.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo clippy --locked --offline --all-targets --all-features -- -D warnings` | PASS after the two fixes |
| `cargo check --locked --offline --all-features` | PASS |
| `cargo test --locked --offline --all-features --lib commands::sync -- --test-threads=1` | PASS: 56 |
| `cargo test --locked --offline --all-features --lib common::i18n::logging -- --test-threads=1` | PASS: 4 |
| `cargo test --locked --offline --all-features --lib runtime::logging -- --test-threads=1` | PASS: 16 |
| `cargo test --locked --offline --all-features --test sync --test remote --test architecture_dependencies -- --test-threads=1` | PASS: sync 151, remote 8, architecture 12; raw log `research/check-domain.log` |
| `git diff --check` | PASS |

The 76 unit checks include exact success/keep-history feedback equivalence, cancellation at the actual decision versus late cancellation, safe recovery defaults and stream gating, rendering/sink guards, Unicode-width ten-column borders and saturated totals, capped privacy-safe sample output, production tracing/catalog AST coverage, immutable captured console language and unchanged original file event values, rotation/retention/lossy queue/counters/tail behavior.

Implementer evidence is separate: the pre-review domain run passed architecture 12, remote 8, store 2, sync 151; its first complete Rust gate stopped only on the two Clippy issues above. Do not present that stopped gate as a full PASS. A subsequent full gate will need to identify its source snapshot explicitly.

## Scope/spec/native notes

- Existing public SyncEvent/SyncSummary/SourceSyncStats/options/source-parser fields and serialization remain unchanged. Notice feedback is crate-private and emitted from existing decisions; source inspection found no added discovery or per-file database query and no moved reset/commit/accounting decision.
- The catalog has exact current production-emitter coverage; test-only emitters are excluded. Default and unknown/legacy projection retain level and escaped original details; C can consume `logging::project` without rewriting stored logs.
- S contract synchronization is main-owned: source-sync summary wording needs closed borders and diagnostics after the bottom border, while runtime-log contracts need the captured CLI formatter/catalog/replay APIs. F startup/preference contracts remain intact.
- Native terminal smoke remains **NOT_RUN**; injectable/headless line/bar tests are not native evidence. Final combined `just ci` belongs to A.


## Final independent recheck — approved remediation (supersedes historical NOT_FULL_PASS)

**Reviewer verdict: PASS for S implementation and headless checks.** No unresolved product finding remains in the reviewed scope. Native terminal smoke remains **NOT_RUN**, and the parent A stage retains final combined `just ci` and native acceptance. The implementer full Rust rerun is separate evidence and was still running when this reviewer record was finalized; this record does not claim that pending gate passed.

### Resolved design findings and final flow

- The parent-approved private `sync::failure::{SyncFailure, FailureKind, RebuildRisk}` attaches neutral facts at the three existing refusal decisions. Their canonical format literals, predicates, safety order and original `Display` remain unchanged. Host/source/missing/protected counts survive the typed projection. Human progress renders privately **after reporter drain**; final CLI errors and console logs use the same neutral facts. Machine `SyncEvent::Failed.error`, file errors and underlying source detail remain canonical. The emit-shards invalid-since adapter preserves the chrono cause and validation order while localizing its known wrapper.
- Parent-approved `parsers::feedback::collect` wraps the same driver future. SQLite-open facts arise only at the existing OpenCode/ZCode error branches; public parser/DriveContext/stats/events retain their signatures and serialization. Nested/concurrent/dropped/failed scopes restore isolation; spawned tasks do not inherit the collector.
- The final inventory finding was reproduced with a temporary Codex `sessions` regular file: the previous Chinese summary exposed the fixed English `source file inventory error: root is not a directory`. It is resolved through indexed private `SourceFileListing.inventory_facts`, including correct index offsets on multi-root merges. `error_summary_with_feedback(source)` emits only at the six existing built-in consumers assigning their inventory error; mere discovery, ownership and coverage consultation emit no failure. Canonical `errors` and `error_summary()` are unchanged. Known root/traversal/OS facts are distinct from arbitrary unknown details. Ownership skips remain excluded, the original cap of three and numeric omitted count survive, and known WalkDir human OS detail omits the embedded private path.
- Source inventory warning sites `.01/.02` retain their canonical message/error/file fields and add neutral kind/detail. Console projection translates known context only for valid metadata on those IDs. Unknown/malformed metadata falls back visibly. The final summary keeps fault versus informational absence distinct and diagnostics below the closed table border.
- One-pass enumeration of direct fixed `SourceFailed` producers found no additional unresolved own-message case: SQLite-open and inventory now have producer facts, Antigravity issues/coverage and engine legacy/coverage already have typed notices, and accepted-rebuild failure notices carry the underlying error rather than the unused `rebuild failed:` stat wrapper. External/custom details remain marked original.

### Additional checker self-fixes

- `common/i18n/logging.rs`: neutral refusal metadata was being suppressed even when unknown, malformed or mismatched and therefore unprojectable. Suppression now requires successful typed projection; fallback retains the original error and metadata in normal and detailed output. A paired known/unknown-event regression verifies this. No canonical-string parsing was introduced.
- `commands/sync.rs`: final full gate exposed `clippy::collapsible_if` in the new after-drain failure-render block. Collapsed it to an equivalent let-chain, preserving the same error check then mutex lock order. No behavior/interface change. The implementer owns the separate complete Rust rerun.

### Independent final commands and results

All commands used direct Cargo and command-scoped `CARGO_TARGET_DIR=output/s-language-rust-target`, with offline locked dependencies. Raw sequential evidence is `research/check-final.log`; the exact initial Rust source/test hashes are in `check-final-source-snapshot.json`. Comparison with `check-final-after-lint-source-snapshot.json` identifies **only** the authorized equivalent `src/commands/sync.rs` lint fix during this sequence. Static checks below ran after that fix; an additional actual refusal subprocess check ran after it too (`check-after-lint-failure.log`).

| Command/filter | Result |
| --- | --- |
| `cargo test --locked --offline --all-features --lib parsers::feedback -- --test-threads=1` | PASS 2 |
| same unit command, `parsers::source_files` | PASS 12 |
| same unit command, `commands::sync` | PASS 57 |
| same unit command, `common::i18n::logging` | PASS 7 |
| `cargo test --locked --offline --all-features --test sync bilingual_ -- --test-threads=1` | PASS 5 |
| same integration command, `--test cli language::` | PASS 20 |
| same integration command, `--test api --test architecture_dependencies` (no name filter) | PASS API 3, architecture 12 |
| `cargo fmt --check` | PASS |
| `cargo clippy --locked --offline --all-targets --all-features -- -D warnings` | PASS |
| `cargo check --locked --offline --all-features` | PASS |
| post-lint `--test sync bilingual_lossy_refusal_translates_all_human_sinks_and_keeps_canonical_wire_error` | PASS 1 |
| `git diff --check` | PASS |

The 78 final focused units include exact positive-import/keep-history SQLite snapshot equivalence, actual versus late cancellation, recovery input/three-stream gating, immutable progress language, control escaping through real line/bar boundaries with unchanged event JSON, all 174 current production tracing IDs/paired catalog coverage, neutral failure facts and visible unknown fallbacks, inventory provenance/caps/ownership, closed Unicode-width numeric tables, and capped privacy-safe diagnostics. The five actual paired subprocess tests cover refusal human sinks with `RUST_LOG=off/error`, unchanged canonical Failed events and file errors with protected 12-token history; OMP preflight plus invalid-since; both SQLite-open sources with original success/stats/shards; inventory-root faults without a private-path leak; and plain/NO_COLOR stream separation.

Earlier independent full-domain evidence remains separately valid for the pre-remediation business snapshot (sync151/remote8/architecture12); the final private refinement was additionally code-reviewed for unchanged transaction/accounting/policy/order and exercised through the final targeted tests. Do not add these counts as though they were one complete Rust suite.

### RED attribution correction

The implementer `refinement-red.log` lossy/OMP attempts initially failed while inserting an incomplete SQL fixture (`NOT NULL` token fields), **before** invoking the CLI. Those entries are not behavioral RED proof. The SQLite-open RED was an actual language failure. The reviewer's original manual positive-history lossy probe supplied the independently reproduced refusal failure recorded above. Final fixtures include every required token field, retain positive 12-token history, and the actual subprocess tests all pass. No acceptance assertion was weakened to hide fixture failures.

### Final spec and compatibility consistency

Reviewed the updated seven-section private-feedback contract in `source-sync-contracts.md`, captured-language/projector/fallback contract in `runtime-log-contracts.md`, F `cli-language-contracts.md`, and baseline-anchored curated source-sync excerpt. Current names and flow match the implementation, including Inventory/error_summary_with_feedback, six consumer sites, multi-root indices, cap3/ownership exception, after-drain private failure projection and canonical machine/file detail. No new public required field, wire/schema/accounting change, parser-to-commands dependency, discovery rescan, per-file query or changed protection decision was found. F language/public API/architecture regressions pass on the final implementation. Native terminal smoke and final cross-surface `just ci` remain explicitly outside this completed headless review.


## Full-gate environment follow-up — test temporary root

The subsequent complete Rust attempt stopped in library tests: **969 passed / 6 failed / 19 ignored**, preserved as `research/ci-rust-after-review-attempt2.log`. All six failures were profiling tests returning Windows OS183 from their temporary-root setup before assertions. This is a failed complete gate; the earlier S product/headless PASS remains distinct and does not imply that gate succeeded.

Independent source/root review confirms `store/sync_writer/profiling/tests.rs::temp_root` hardcoded `<manifest>/target/writer-benchmark`, bypassing the configured `CARGO_TARGET_DIR`. The repository `target` is a symlink whose target no longer exists, while `output/s-language-rust-target` exists. `check-profiling-root.json` records the exact link/existence evidence and patched helper hash. A source/test sibling sweep found no other equivalent hardcoded writer-benchmark helper.

Parent approved the implementer's minimum **test-only** correction: use nonempty `CARGO_TARGET_DIR` via `var_os`/`PathBuf`, otherwise retain the original `<manifest>/target` fallback, then append the same `writer-benchmark` and generated TempDir suffix. Both profiling and its tests module are `cfg(test)`. The reviewer independently checked the exact diff: no production change, fixture/assertion weakening, data-policy change, symlink repair/deletion, dependency change or configuration rewrite.

Independent postfix command `cargo test --locked --offline --all-features --lib store::sync_writer::profiling::tests -- --test-threads=1` passed **7 tests / 6 pre-existing ignored / 0 failed**, including every formerly failing test. Raw evidence: `research/check-profiling-target.log`. Test compilation verifies the helper types; independent `cargo fmt --check` and narrow `git diff --check` also pass. Original performance-only ignored tests were not run or reclassified. The implementer owns the fresh complete Rust rerun; no duplicate complete suite was launched by the reviewer. Final full-gate success must be read from that rerun, not inferred from this focused green result. S implementation review remains PASS; native smoke remains NOT_RUN.


## Read-only investigation of next complete-gate failure — local/UTC date boundary

Attempt 3 progressed past profiling but stopped at **974 library tests passed / 1 failed / 19 ignored**. The raw full-gate log identifies `web::tests::api_dashboard_interactive_scope_is_lean_and_selected_window_only`, expecting overview 10 but receiving 0 at `web/mod.rs:6248`. That is a failed full gate, not a product acceptance result. No test or production Web edit was made by the reviewer during this investigation.

The independent trace disproves a half-hour-floor-only explanation. The fixture inserts raw `Utc::now()` into `usage_bucket_30m.hour_start` (`web/mod.rs:6214`). `apply_window_filter` (`web/mod.rs:2142`) instead chooses `chrono::Local::now().date_naive()`, even for a requested `timezone=UTC`; the filter then interprets this date in the requested timezone (`query/filter.rs`), generating an exclusive next-day UTC midnight. `query_lifetime_bucket_overview` directly sums matching bucket rows. On the observed machine, Local was October 20 20:46 at UTC-04 while UTC was October 21 00:46. The raw fixture was therefore outside the generated upper bound October 21 00:00Z.

A fixed-clock, in-memory SQLite proof reproduces the same SQL comparison without waiting or retries: `research/check-dashboard-clock-proof.json`. At October 20 23:46Z, raw-now/floored-now/previous-UTC-day fixtures sum to **10/10/10**; at October 21 00:46Z with the same Local October 20 bounds they sum to **0/0/10**. Merely flooring now to 00:30Z remains outside the bound and does not fix the demonstrated root.

The raw fixture also violates canonical bucket construction: `common/util.rs::bucket_start` floors minutes to 00/30 and seconds/nanos to zero; normal Codex/Claude producers use `bucket_start_from_rfc3339`. That is a separate fixture-contract issue, not the cause of the zero result. For the lean-payload/selected-window test, a canonical fixture safely inside the seven-day interval can remove its accidental current-day assumption while preserving every intended assertion. The pre-existing production Local-date/requested-timezone mismatch remains a separate finding; a test-fixture correction must not be described as curing that production edge. Main was sent the concrete distinction and deterministic proof before any patch authorization. S source changes only added event IDs at existing Web tracing sites; no production date/filter query was changed.


### Approved fixture-only follow-up and independent green

After receiving the deterministic root evidence, main authorized only the lean-payload test fixture correction. Independent tiny-diff review confirms the sole new Web change is the timestamp expression plus its scope/cutoff comment: `crate::util::bucket_start(Utc::now() - ChronoDuration::days(1))`. All inserted token values, 500 cursors, URL and existing payload/health/omission/size assertions remain intact; production date/filter/query logic is unchanged. The previous-day bucket is canonical and safely inside the selected seven-day interval across the observed Local/UTC boundary.

Independent exact command `cargo test --locked --offline --all-features --lib web::tests::api_dashboard_interactive_scope_is_lean_and_selected_window_only -- --exact --test-threads=1` passes **1 / 0 failed**, recorded in `research/check-dashboard-interior-fixture.log`. Independent fmt and narrow diff checks pass. Prior attempt 3 failure/snapshot is retained separately by the implementer. The complete Rust rerun remains implementer-owned and must pass independently; this focused green is not substituted for it.

**Unfixed, outside S:** Web preset range bounds still choose Local calendar today before interpreting dates in the requested UTC timezone. This is an existing production edge, not corrected by the fixture change or the CLI localization work. Keep it visible in parent handoff/evidence; do not claim a Web timezone fix. S implementation review remains PASS, full gate pending, native NOT_RUN.


## Final complete Rust gate result (before C began)

The implementer reports exit **0** for the frozen S snapshot `final-source-snapshot.json` and verified its hashes remained unchanged after completion. Independent inspection of the completed `ci-rust-after-review.log` confirms successful fmt/strict Clippy, **1253 passed / 0 failed / 19 existing ignored** (library975, API3, architecture12, CLI54, query9, remote8, store2, sync155, TUI35), successful binary/doc-tests, and warnings-as-errors rustdoc completion. This supersedes the pending-gate notes above, not the retained failed-attempt evidence. Full-suite execution was implementer-owned; independent focused runs are separately recorded above. C edits began after this frozen S gate, so do not compare current C hashes to claim the old S snapshot is still current. Native smoke/final cross-surface just ci remain A-owned, and the pre-existing Web timezone policy finding remains unresolved.
