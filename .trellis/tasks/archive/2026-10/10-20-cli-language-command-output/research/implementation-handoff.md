# C implementation handoff

This prepares the next child; it does not start C or certify S. Start only after
the coordinator records independent S PASS and selects C as the active task.
The authorization note at the top of the task artifacts supersedes historical
planning-only text. No commit, push, install, release or real-data rebuild.

## Implementation ownership inside C

C remains one delivery with one independent final review. The coordinator may
dispatch two implementers with disjoint ownership:

- Reports: blocks/daily/weekly/monthly/session/focused/unified_report/report_args/
  statusline commands, `tui/report_table.rs`, paired report catalog
  `common/i18n/commands/reports.rs`, and `tests/cli/language_reports.rs` plus
  directly affected existing report tests.
- Operations: the remaining C commands, `common/i18n/commands.rs` (including its
  `reports` declaration), minimal `common/i18n/mod.rs` registration, a required
  canonical-preserving Diagnostic constructor if needed, and
  `tests/cli/language_commands.rs` / affected remote tests.
- Coordinator: new integration-module registrations in `tests/cli/main.rs`,
  task/spec/evidence integration and final gate dispatch. Both implementers
  preserve the other worker's changes; neither spawns nested agents. Coordinate
  the shared Diagnostic constructor before using it. Run the complete gate only
  after both chunks are source-stable; do not infer C acceptance from one chunk.

## Shared interfaces

- F `common::i18n::{Language, current, text, scope, with_language}` selects
  interface text. Capture `Language` before spawned work; task-local language
  deliberately does not flow into `tokio::spawn`. `with_language` is synchronous
  and must not enclose a suspending future.
- CLI startup reads the independent preference profile once. `--locale` retains
  its existing report value and does not select interface language or add numeric
  culture formatting. Do not add separate language discovery to commands.
- `commands::cli_errors::Diagnostic` provides paired own-error context while
  preserving the canonical error. Use typed producers; never `bail!(text(...))`
  for an error that can enter stored run diagnostics or file logs.
  Existing C errors include canonical Chinese as well as English. The current F
  `Diagnostic::new` displays its English argument; do not silently replace an
  existing canonical Chinese error with English. If required, add a narrow
  constructor retaining separate canonical/en/zh strings, with unchanged `new`
  behavior and a regression for exact original `Display` in both UI scopes.
- S `common::i18n::logging::project(Language, level, message, &fields, detailed)`
  is the human log replay adapter. `LogEntry.fields` already retains event IDs.
  Reuse this; do not add public DTO fields or rewrite files. Detailed mode is
  appropriate for DEBUG/TRACE. Unknown/historical records retain an explicit
  original marker and detail. Never identify a known event by its message text.
- Existing S summary issue/sample helpers preserve typed fault/informational
  classification, basename privacy, bounded reasons and offset semantics.

## Surfaces with protected canonical strings

- `source_status.rs` exposes public serializable statuses/builders used outside
  the CLI. Their `detail`, accounting warning, IDs and shape stay canonical.
  Derive human copy from stable status/quality/accounting/source facts; if data is
  missing, add bounded private producer-side presentation facts rather than
  matching English/Chinese details. Keep current observation versus historical
  inventory distinctions.
- `doctor.rs` builds canonical JSON checks, including own human-looking strings.
  Keep that data unchanged. Carry private typed presentation facts at the same
  builder decisions for the human branch; a translated JSON serialization is not
  a human renderer.
- `diagnostics` defaults to JSON, including `--out` content. Only terminal
  confirmation, own errors and forget-file interaction belong to translation.
- `remote handshake`, report/logs/doctor/catalog JSON, HTTP/SSH payloads and
  exported browser content remain unchanged. Third-party cargo/git/OS detail
  can remain original; own explanatory context must have paired copy.

## Behavioral traps to verify

- Expand all 20 focused report leaves from Clap schema. Cover default daily,
  instances, session ID, sections, empty, rejection and failure paths.
- Statusline stays one line with original units/cost policy. Use separate
  `latest.en.txt`/`latest.zh.txt`; never replay old `latest.txt` across languages
  and leave that old file in place.
- Update keeps default yes, exact accepted choices, EOF failure and second ref
  check. Use fake provider/executor; do not execute an update. Remote deletion
  still requires `--yes`; clean remains preview by default; uninstall ownership
  and purge scope stay unchanged.
- Preserve F no-create behavior for missing clean roots, settings/lock/atomic
  residues classified keep, help/parse errors before logging, lang zero-read in
  non-TTY, and all public `run`/AppContext/AppPaths signatures.
- CLI report table width changes are local to `tui/report_table.rs`; do not edit
  interactive TUI panels in C. Keep numeric totals, placeholders and source order.

## Build and evidence

Use direct existing Cargo via command-scoped PATH prefix
`C:\Users\lyh\.cargo\bin;` and
`CARGO_TARGET_DIR=$PWD/output/s-language-rust-target`. The repository `target`
is a broken managed symlink; leave it untouched. Use `--locked`, optional
`--offline`, and existing `LLMUSAGE_PRICING_REFRESH=off` for isolated tests.
No dependency or lockfile changes are needed.

Record a stage baseline before edits, then map every C coverage row to its
producer, human renderer, protected machine branch and meaningful paired test.
Register integration tests in existing explicit targets (`autotests=false`).
Run focused tests before cli/remote/architecture and the full Rust gate. Keep
implementation versus independent-check evidence separate. Native terminal
smoke remains a separate A result; headless tests do not establish native PASS.
