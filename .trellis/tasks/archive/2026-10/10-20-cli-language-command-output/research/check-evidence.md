# C independent review — PARTIAL, reports static slice

Active task was explicitly switched from S to C. Loaded C check.jsonl, PRD/design/implement/handoff and referenced parent scope/design/coverage/startup, report/self-update/integration/Web/runtime/CLI-language/CI contracts; carried forward independently checked F/S API and canonical-data evidence. Historical planning-only text is superseded by user implementation authorization. C has separate reports and operations implementers. This evidence is **PARTIAL** while operations/shared glue remains under implementation; it is not whole-C acceptance or a completed lint/type/test gate.

## Static scope reviewed

- Reports: blocks/daily/weekly/monthly/session/focused/unified_report/report_args/statusline, CLI `tui/report_table.rs`, complete new paired reports catalog and registered `tests/cli/language_reports.rs` (including untracked additions), plus Diagnostic facts interactions.
- Constraints remain at their existing decisions with exact prior canonical own strings. New neutral reports facts carry stable source/host values, not translated templates; shared Diagnostic factory preserves its canonical Display separately from paired UI.
- Report table copy now selects language at presentation boundaries and uses Unicode display width for sizing/padding/shortening. Right-aligned numeric columns are excluded from width shrinking. Existing visible-channel saturating sums, cache-zero placeholders, compact units, row/source ordering, color policy and double total separators remain the same. Existing `By host` section intentionally retains authoritative totals; it was not forced to match the visible-channel table sum.
- JSON-producing paths and DTOs retain their values/keys/order and no-cost projection. No query/storage/token algorithm change found in reports diff. `--locale` parsing/passthrough does not select UI language or add numeric culture formatting. New paired tests enumerate 20 focused leaves from the real Clap schema, retain external model/host/project values, and compare fixed JSON data.
- Statusline keeps original hook parsing/cost policy and the same query. Cache paths are latest.en.txt/latest.zh.txt; reads do not fall back to another language or latest.txt, and old latest.txt is not removed. CR/LF are normalized for its required single-line result. Existing unavailable/success/cache failure behavior and exit policy remain subject to final targeted tests.

## Finding reported and owner correction under review

New statusline hook/cache IO `.context(Diagnostic::with_facts("failed to ..."))` initially changed previously direct OS top-level Display into a new own wrapper. The reviewer reported the canonical compatibility issue; the reports owner agreed and implemented `statusline_io(io::Error, ReportFacts)` with canonical equal to the original IO error string, underlying typed IO error retained as cause, and paired human context from neutral facts. A direct en/zh regression covers all three IO fact cases, exact Display and original error kind. Static correction reviewed; independent execution is pending source coordination.

Accuracy boundary: statusline dispatch is direct, without tracked/untracked run logging. This finding concerned its public error Display/cause; there is no claim that statusline itself previously stored these failures in run rows/file events. The tentative persistence concern was corrected after tracing dispatch. Tracked operation context changes require their separate persisted full-chain review by the operations owner/checker.

## Pending shared/whole-C review

Operations/shared Diagnostic/projector changes remain active. Strict rejection of unknown/malformed/mismatched metadata must include nested parameter shapes before hiding raw detail; the reports peer reproduced permissive unit-variant serde handling and converted its empty variants to struct forms. Operations/Domain/S decode hardening remains with the shared owner. No concurrent edits were made by this checker. No full gate launched or whole-C PASS claimed. Independent paired actual report rejection/file canonical tests, F/S regressions and final static checks await stable source. Native terminal smoke remains NOT_RUN.

## Continued independent review, 2026-10-21 — reports PASS, whole C pending

Native SubagentStart injected the correct C check context; the saved full hook
output and omitted references were consulted. The ten report source/test hashes
still exactly match `reports-snapshot.json`. No new report defect or source edit
was needed. Updated report-cli contract language/Unicode/numeric-width and
statusline cache rules match the implementation.

Independent commands use direct Cargo PATH, the existing ignored
`output/s-language-rust-target`, `--locked --offline --all-features`, pricing
refresh off, and serial test execution:

- `--test cli language_reports`: **6 passed**, 43.74s, raw
  `check-reports.log`. This includes all 20 schema-derived focused leaves,
  paired success/empty/rejection/uninitialized paths, protected JSON/locale/
  no-cost, canonical persisted run/file errors versus human replay, and
  statusline cost/cache/single-line/IO behavior.
- `--lib report`: **70 passed / 2 existing ignored**, 33.54s, raw
  `check-report-units.log`; includes all report-table width/values/notes/color
  tests and strict report facts. These are focused slices, not a full Rust gate.
- `rustfmt --check --edition 2024` on the ten report files: **PASS**.
- Narrow `git diff --check`: **PASS** (ordinary LF/CRLF notices only).

Shared strict decoding is already implemented by
`logging::complete_facts`: deserialize then require complete JSON Value equality
after reserialization. Inspection confirms this guards DiagnosticFacts and S
FailureKind before original fields are hidden, including nested RebuildRisk.
Consequently remaining operation unit variants are not by themselves a defect;
unknown fields are rejected by that complete-shape gate. Operations owner is
extending Domain-valid/unknown tests. No enum producer change was requested.

### Concrete C rejection coverage gap sent to coordinator

An isolated temporary initialized profile, `language=zh`, pricing refresh/file
logging off and error console enabled reproduces `catalog apply bad.json` with
`bad.json` containing `{}`. Exit remains 1, but both console and final error
contain the app-owned English
`pricing snapshot did not contain any LiteLLM rows with input_cost_per_token`
after the translated generic ConfigInvalid/original-detail prefix. This is a
known application validator reason, not OS/serde/third-party raw detail. Current
catalog command success rendering plus DomainDiagnostic::Config free text does
not satisfy whole-C rejection localization. Main and operations owner were
notified; fixing at a typed producer requires a bounded domain/store contract
refinement, so the checker did not edit those owner files or infer reason from
English. Whole-C acceptance remains pending that decision and operations
completion, including uninstall and no-rescan platform detail facts.

## Operations independent continuation and approved fixes

The pricing finding above was not waived. Main authorized a private pricing
failure carrier, paired validation catalog, exact canonical wrappers and the
reproduced early-bootstrap marker path; reviewer-owned implementation and
self-verification are detailed in `pricing-evidence.md`. Main separately
spot-reviewed those changes and the shared marked context/canonical storage
projection, preserving original decisions, error types/strings/causes and IO.

Independent `--test cli language_commands` now **PASS 9**, 28.96s, raw
`check-operations-paired.log`. Reviewed the actual paired tests and producer
paths for fixed doctor/logs/catalog/diagnostics/handshake JSON, source/parse
diagnostic privacy and bounds, clean deletion sets/keep files and unchanged DB,
legacy cleanup user bytes/audits/noop/partial failure/purge, replay IDs versus
historical raw detail/disk bytes, export browser assets and tracer CLI shell.
The source confirms update retains its original default yes, accepted inputs,
EOF failure and second ref check with fake executor/provider tests; no update,
installation or real user cleanup was executed by this review.

The new platform facts helper was independently diff-reviewed: public probe
wrappers delegate the same private pass; root resolution/detection predicates,
canonical PlatformProbe fields and detail text remain exact. Labels come from
already computed detected/checked branches, not a second probe or error-string
parse. Source-status consumes those neutral labels/statuses at the human edge.
Cleanup public wrappers retain action shape and delegate the same existing
cleanup/recovery/audit order; the paired process test compares user bytes and
canonical audit values across languages. No new public required fields or
translation of machine payloads was found in this slice.

Main's updated CLI-language/runtime-log/report/pricing contracts were reviewed
against source. The marked presentation wrapper and `canonical_detail` retain
original repeated text and typed causes; metadata decoding remains complete at
all nested levels. Report width/cache and pricing same-call/no-rescan contracts
match their implementations. No reviewer doc edit was needed.

### Mechanical gate results and next integration gap

- First strict clippy found 28 static `format!` and two redundant closure issues
  in operations-owned shared files; owner corrected them. Raw original:
  `check-clippy.log`.
- The next strict clippy found only the old crate-private
  `bootstrap_with_progress` dead in production after the two approved sync
  callers moved to the typed path. Its four existing progress regressions still
  use it; reviewer scoped that wrapper `#[cfg(test)]`, retaining those assertions
  and all public bootstrap/progress signatures. Added the approved private
  `bootstrap_with_progress_and_pricing_facts(sink)` over the same core. No lint
  suppression or changed progress event semantics. Raw stopped run:
  `check-clippy-final.log`; final whole-tree rerun awaits remote source freeze.
- Direct configured 1.97.0 `rustfmt --check` verifies the final schema helper.
  `.cargo/bin/rustfmt.exe` is a symlink and began failing OS448 untrusted-mount
  traversal after the execution context changed; direct configured toolchain
  binaries avoid that traversal without a toolchain/config repair.
- An independent existing `local_flow::` run did not reach tests: LINK1104 could
  not create a generated `C:/Users/lyh/lnk{...}.tmp`. A workspace TEMP/TMP retry
  (after a successful local write probe) failed at the same external linker
  temporary file. These are failed build attempts, not passed tests:
  `check-local-flow-linker-failure.log` and `check-local-flow.log`. Main was
  notified to resolve the compiler execution permission/context; no assertions,
  linker flags or build-gate scripts were changed to hide it.

Review also surfaced the remaining known remote-import validation messages
still reaching the generic ConfigInvalid/raw-detail route. Main assigned a
separate bounded remote implementation. Whole-C source acceptance remains
**pending that source freeze, independent remote review and final compilation/
lint/full Rust gate**, and must not be inferred from these focused green slices.
Native terminal acceptance remains NOT_RUN.

## Frozen C source review — remote import and final boundaries

Independent review of the frozen remote importer/decoder/CLI and the new neutral
facts catalog found no additional product defect. The public importer and
decoder APIs unwrap the exact original error/outcome; new private entry points
reuse the same body and original validation decisions. The diff retains stream
read/parse, deferred import, commit, accounting certification, session finish,
watermark, lock and heartbeat order. No transport change, second read/parse or
language dependency was introduced below the human adapter.

The CLI records the original failure Display to host contact state before
attaching the marked presentation wrapper. Immediate/final/replayed errors use
the paired catalog, while run details use the shared canonical projection.
Skipped-line warning facts are produced beside the unchanged canonical public
warning. Review covered all eight paired rejection fixtures, historical refusal
and unchanged existing rows/watermark, the deliberately retained partial commit
on missing trailer, canonical contact/error values and original typed causes,
public warning/outcome compatibility, and complete-shape metadata fallback in
normal/detailed en/zh replay. BeforeHeader and ExitWithoutTrailer remain the
original unreachable guards and are only renderer-tested; no runtime coverage
claim is made for them.

The implementer reports cargo check PASS and the new tests reaching MSVC
linking, with zero tests executed because LNK1104 blocked the linker temporary
file. Those are not executable PASS results. The raw failed attempts and
`remote-import-evidence.md` remain intact; the coordinator owns the centralized
full Rust gate and permission-context resolution. No parallel Cargo/linker
retry or frozen source edit was made by this reviewer.

Strict `cargo clippy --locked --offline --all-targets --all-features -- -D warnings`
completed independently with exit 0 in 21.15s (`check-clippy-current.log`).
Only filesystem incremental hard-link/copy fallback notices appeared. This is
the current strict source-lint result before the coordinator's formatting-only
normalization, not a full test gate result. The central first full gate then
stopped at cargo fmt --check; per-file formatting reports had not established
whole-tree formatting. The coordinator preserved
`ci-rust-final-format-failure.log`, ran the real configured cargo fmt, and
restarted the full gate. Do not relabel that original run PASS.

Spec review found one remaining documentation synchronization item: the CLI
language and runtime-log family lists omitted the newly registered RemoteImport
family. The coordinator was notified to add its private same-call facts/public
canonical API boundary. The corrected report Statusline section and the
pricing/marked-wrapper contracts otherwise match source. No reviewer-owned
spec change was made during the gate.

Final independent **source verdict: PASS**, conditional on the centralized
full Rust gate and the noted spec synchronization. Reviewer-authored pricing
fixes are self-verification plus coordinator spot review, not misrepresented as
independent authorship review. The owned fixes and exact API changes remain in
`pricing-evidence.md`. S human consumers that still call the canonical public
remote importer are an explicit coordinator integration follow-up; their new
dynamic reasons are not claimed translated by the C-only private CLI route.
Native-terminal acceptance is still NOT_RUN, and C does not complete T/A or the
parent task.

## Coordinator complete-gate confirmation

Final frozen C scripts/ci-rust.py EXIT0: fmt, strict Clippy and warnings-as-errors rustdoc PASS; library996/API3/architecture12/CLI73/query9/remote8/store2/sync155/TUI35 = 1293 PASS, 19 existing ignored, 0 failed. New remote import tests and sync pricing-marker variants actually executed in this full gate. Raw ci-rust-final.log; final-source-snapshot.json captures the tree before T begins. Escalated linker context resolved the pre-test MSVC temp restriction; no test waiver or toolchain repair. Earlier format failure retained in ci-rust-final-format-failure.log. S human remote-skip consumer follow-up is explicitly assigned to T shared presentation work; C own remote command accepts typed facts and retains canonical data. Native remains NOT_RUN.
