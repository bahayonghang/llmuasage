# Runtime Log Contracts

## Scope

Apply this contract when changing `src/runtime/logging.rs`, the `logs` command,
or diagnostics/doctor fields that describe structured runtime logging.

## Storage Bounds

- The structured NDJSON writer rotates during the lifetime of one process; a
  restart is never required to trigger a size boundary.
- Production defaults are 10 MiB per shard, 30 MiB across retained shards, at
  most seven files, and a maximum age of seven days.
- The writer serializes rotation and retention maintenance. Active shards are
  never removed, and maintenance runs at rotation plus low-frequency byte
  intervals while writes continue.
- A formatted NDJSON event is indivisible: rotate before writing the event
  rather than splitting it across shards. One event larger than 10 MiB may
  occupy an oversized shard so both files remain parseable.
- An occupied or otherwise undeletable shard is retained and retried by later
  maintenance. Other eligible shards may still be removed so one failure does
  not make growth unbounded.
- Rotation and cleanup failures increment a nonrecursive maintenance counter.
  The failure path must not emit a tracing event or panic.

## Observability

- Keep `tracing_appender`'s lossy non-blocking writer and expose its cumulative
  dropped-line counter as `LogsRuntimeStatus.dropped_event_count`.
- `LogsRuntimeStatus` also exposes current-shard bytes, retained file count,
  total retained bytes, recent error entries, and maintenance failures.
- `diagnostics --json`, `logs --json`, and their human-facing doctor/logs
  consumers must preserve these counters. Public dashboard routes must not gain
  runtime-log access.
- After each source parse, if `ParseIssues::summary_text()` is present, the
  driver emits one `info!` event with `source`, the four class counts, and
  comma-joined sample reasons. It must not include record text, prompts,
  paths, or `error_message`.
- Default `LLMUSAGE_LOG=warn` does not persist this info event. That is
  intentional: skipped and other informational issues are not faults. Capture
  the event in a unit test. `llmusage logs --level info` sees it only when
  the file filter is info or finer. Do not raise the default file level.

## Tail Reads

- Enumerate shards newest first and read file contents backward in fixed-size
  blocks until the requested scan window is satisfied.
- Return parsed entries in chronological order, retaining only the newest
  requested matches across all shards.
- Preserve complete UTF-8 lines and a partial final line. A leading partial
  line created by a reverse seek is discarded.
- Tail work may inspect bounded shard metadata, but must not read all historical
  file contents when a small tail is requested.

## Required Tests

- One writer instance crosses multiple size boundaries and remains within its
  configured count and total-byte budgets.
- Crossing a size boundary never splits one NDJSON event between two shards.
- A full deterministic non-blocking queue increments the dropped counter.
- Multi-shard tails retain chronological order and demonstrate bounded bytes
  read from a much larger shard.
- A simulated occupied-file deletion records a maintenance error, continues,
  and succeeds on a later retry.
- A unit test captures the driver parse-issue info event and asserts source,
  class counts, and sample reasons without depending on the default warn
  file level.

## Scenario: Human Sync Console Routing

### 1. Scope / Trigger

Use this contract when changing human sync progress, console tracing output,
or explicit sync warnings. Indicatif can pad a permanent line to terminal width
without writing a newline; a subsequent raw stderr writer can join that line.

### 2. Signatures

- `install_stderr_sink(StderrSink) -> StderrSinkGuard` is crate-private.
- `StderrSink` is an `Arc<dyn Fn(&[u8]) -> io::Result<()> + Send + Sync>`.
- `HumanRenderer::write_warning(&mut self, &[u8]) -> io::Result<()>` routes
  the complete console message through the active renderer.
- `stderr_warning(&str)` uses the same destination as console tracing.

### 3. Contracts

The human command owns the sink guard and terminal guard. Console tracing
buffers one event before routing it. A bar renderer suspends drawing, writes
the complete newline-terminated message, and restores progress. Permanent
progress lines also require a newline; width padding cannot establish the
boundary. The line renderer ends any active progress line before the warning.

The sink guard restores the previous destination on every return path. Outside
human sync, console messages use normal stderr. Preserve logging filters,
fields, NDJSON file output, and JSON-mode stdout.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Warning after `LockAcquired` | Warning starts on a separate complete line |
| Warning while bars are active | Suspend, write, and restore without joining lines |
| Plain or forced-line output | No ANSI; complete progress and warning lines |
| Early failure or guard unwind | Restore the previous stderr destination |
| JSON-events command with a legacy warning | Every stdout line remains valid NDJSON |

### 5. Good/Base/Bad Cases

- Good: the renderer serializes a complete warning with progress output.
- Base: a command without human progress writes the same console event to stderr.
- Bad: combining terminal-width padding with direct warning writes.

### 6. Tests Required

- `permanent_progress_lines_end_before_raw_warnings` uses a visible injected
  terminal and tracing to reproduce and reject the original joined line.
- `scoped_warning_sink_handles_interleaved_progress_and_restores_on_error`
  covers interleaving, repeated warnings, and guard cleanup.
- `legacy_warning_keeps_human_lines_and_json_stdout_separate` covers the
  shipped command with an isolated legacy fixture and parses each stdout line.

### 7. Wrong vs Correct

Wrong: write a warning with an independent `eprintln!` while the human renderer
owns terminal output. Correct: route the complete line with `stderr_warning`,
and let the command-scoped sink coordinate with `HumanRenderer::write_warning`.

## Scenario: Commands That Must Not Create The Runtime Home

### 1. Scope / Trigger

Use this contract when a command must succeed against a missing runtime root.
`llmusage clean`, help/version, parser failures and read-only `lang` must not
create `logs/` or any other home directory. Language startup details are in
[CLI Language Foundation Contracts](./cli-language-contracts.md).

### 2. Signatures

- `AppPaths::with_cli_home(home: Option<PathBuf>) -> Result<AppPaths>` resolves
  an explicit root, or `~/.llmusage` when `home` is `None`. It does not read
  `LLMUSAGE_HOME`.
- `AppContext::with_cli_home(None)` is not the same call. It uses
  `AppPaths::discover()`, which does read `LLMUSAGE_HOME`.
- `AppContext::from_paths(AppPaths) -> Result<AppContext>` only attaches the
  current executable. It does not discover paths or create directories.
- `init_stderr_logging() -> Result<()>` installs the stderr tracing layer and
  does not open the rotating file writer.
- `init_logging_for_paths(&AppPaths) -> Result<()>` may create `logs/` when the
  file layer is enabled.

### 3. Contracts

The metadata bootstrap selects clean and its explicit home, reads preferences
without creating directories, then formal Clap parsing validates the command.
For clean, resolve `AppPaths::with_cli_home`, use `init_stderr_logging()`, and
attach `AppContext::from_paths`. It must not call `AppContext::with_cli_home`,
`AppPaths::discover`, or `init_logging_for_paths`. Help/version/parser failures
and lang inspection return before logging. Other runtime commands keep file logging. Do not change the
10 MiB / 30 MiB / seven-file / seven-day limits for this entrypoint. Deletion
scope stays in `integration-file-contracts.md`; this scenario only covers home
and log creation.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| `clean` and the resolved root does not exist | Exit 0, print `Nothing to clean` / `没有可清理内容` in the selected language, create nothing |
| `clean` without `--home` while `LLMUSAGE_HOME` is set | Ignore the variable and use `~/.llmusage` |
| `clean` with `--home <path>` | Use that path and do not create it when it is missing |
| Help/version/parser failure/read-only lang | Return before logging; create nothing |
| Other runtime commands | Keep file logging and existing `LLMUSAGE_HOME` discovery |
| File-log filter disabled | `init_logging_for_paths` still must not be the `clean` path |

### 5. Good/Base/Bad Cases

- Good: a missing `--home` stays missing after `llmusage clean` and `llmusage clean --yes`.
- Base: an existing home is only read, and `--yes` deletes just the reviewed migration and baseline set.
- Bad: routing `clean` through `AppContext::with_cli_home(None)` or `init_logging_for_paths`, which creates `logs/` under the discovered home.

### 6. Tests Required

- `with_cli_home_none_ignores_env` sets `LLMUSAGE_HOME` and asserts
  `AppPaths::with_cli_home(None)` still ends in `.llmusage` and is not the env root.
- `missing_root_is_empty_and_not_created` runs the clean plan with `--yes`
  against a path that does not exist and asserts the path still does not exist.
- Do not point either test at the real user home.

### 7. Wrong vs Correct

Wrong: `let app = AppContext::with_cli_home(cli.home)?; init_logging_for_paths(&app.paths)?;`
for `clean`. Correct: resolve `AppPaths::with_cli_home(cli.home)`, call
`init_stderr_logging()`, then `AppContext::from_paths`.

## Scenario: Captured CLI Language And Stable Event Projection

### 1. Scope / Trigger

Use when adding a production tracing site, changing CLI console formatting, or
replaying human `logs` output. The file record remains canonical diagnostic data.

### 2. Signatures

- `init_cli_logging_for_paths(&AppPaths, Language) -> Result<()>` and
  `init_cli_stderr_logging(Language) -> Result<()>` capture the startup language.
- `common::i18n::logging::project(Language, &str, Option<&str>, &Value, bool)
  -> String` projects level, original message, fields and detailed-mode selection.
- Production tracing fields include a unique stable `event_code`. Preserve IDs
  when moving a site; do not renumber existing sites to match new line numbers.

### 3. Contracts

The CLI formatter uses the captured language even on worker threads. Existing
non-CLI initialization retains its original formatter. Preserve original message,
level, target and fields in NDJSON, as well as filtering, lossy queue counters,
rotation and complete-event stderr routing. Localization adds optional metadata;
it does not replace original fields or rewrite old files.

Select templates only by `event_code`, never by matching message text. Normal
known-event output hides redundant phase/operation, version, hash and path detail,
while retaining critical source, host, count and error facts. DEBUG/TRACE detailed
projection keeps diagnostic fields. Unknown and historical entries have an
explicit original-log marker. Escape control characters in human projections.

Known sync refusals may carry neutral `failure_code`/`failure_facts` alongside
the unchanged canonical `error`. Suppress those fields only after successful,
matching typed projection. Unknown, malformed or mismatched metadata stays
available as original detail; detailed replay must not silently discard it.

Existing command run-failed sites `.03`, `.04` and update `.05` may add
`diagnostic_code`/`diagnostic_facts` for closed Reports/Operations/Domain/Pricing/RemoteImport facts.
The final CLI error and console/replay adapter share the paired catalog. Original
`error`, message, run status and stored detail stay canonical. Metadata contains
neutral facts, not English/Chinese templates. Before hiding raw data, validate
the complete recognized shape including nested fields. Serde internally tagged
unit variants can ignore extra fields even with `deny_unknown_fields`; an
attribute alone is not proof of strict projection. Future extra fields require
visible fallback just like unknown codes and malformed JSON.

When paired presentation is attached as a context to an existing canonical
error, the context is explicitly marked by `Diagnostic::with_facts_context`.
Run failure storage uses `cli_errors::canonical_detail` to omit only that added
wrapper, retaining the complete original cause chain. Ordinary contexts and
original repeated text are preserved; no string-based deduplication is allowed.
File-event `error` and public error values remain canonical as before.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Known event in en/zh CLI | Paired template and critical facts; original file data unchanged |
| Console runs on a worker | Startup language retained |
| Missing or unknown event ID | Explicit original-log marker and original detail |
| Valid matching typed refusal | Localized human refusal; canonical file error retained |
| Unknown/malformed/mismatched failure facts | Original detail remains visible |
| Known code plus unknown top-level/nested metadata field | Original error and complete metadata remain inspectable |
| Control characters in dynamic text | Escaped in human output; original structured data retained |

### 5. Good/Base/Bad Cases

- Good: human log replay calls the same pure projector as live CLI logging.
- Base: old records without IDs remain inspectable in either language.
- Bad: translating `message` before tracing or discarding unrecognized metadata.

### 6. Tests Required

- `production_tracing_sites_have_unique_paired_catalog_entries` parses current
  Rust syntax, including imported macros, and checks exact production coverage.
- `cli_console_captures_language_while_file_events_keep_original_message_level_and_fields`
  checks console/file separation and the captured language.
- `default_known_events_keep_source_and_protection_counts_visible` retains
  source/protection/pricing counts in normal output.
- `unknown_or_mismatched_failure_metadata_remains_available_as_original_detail`
  checks fallback and detailed replay without message-text classification.
- Command language tests exercise actual paired rejections with `RUST_LOG=error`
  and canonical file data. Metadata tests include known-code extra fields at
  both root and nested argument levels, normal and detailed output, as well as
  stable round-trip serialization and malformed/unknown/mismatched facts.

### 7. Wrong vs Correct

Wrong: identify a refusal with `error.contains("Refusing lossy")` or emit a
translated tracing `error`. Correct: preserve the canonical error, attach neutral
typed facts at its producer, and project only the CLI presentation.
