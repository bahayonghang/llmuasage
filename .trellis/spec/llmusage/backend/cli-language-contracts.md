# CLI Language Foundation Contracts

## Scenario: Invocation language, preferences, startup and errors

### 1. Scope / Trigger

Apply when changing `common/i18n`, runtime preferences, CLI startup/help/error
presentation, or commands consuming the shared CLI/TUI language. Web/desktop
language and machine payloads remain independent. CLI/TUI command-output
migration is owned by the relevant presentation contract.

### 2. Signatures

```text
llmusage lang [en|zh|zh-CN]
common::i18n::Language::{English, Chinese}
Language::{parse, code, select}
i18n::scope(language, future).await
i18n::with_language(language, closure)
i18n::{current, text}
preferences::read(root) -> Preferences { language, diagnostic }
Preferences::diagnostic: Option<PreferenceDiagnostic>
PreferenceDiagnostic::render(Language) -> String
preferences::write(root, Language) -> anyhow::Result<()>
commands::entrypoint::{schema, select, run_args, process}
commands::cli_errors::{Diagnostic, application_error}
Diagnostic::{new(english, chinese), with_canonical(canonical, english, chinese)}
Diagnostic::with_facts(canonical, DiagnosticFacts) // crate-private
Diagnostic::with_facts_context(&anyhow::Error, DiagnosticFacts) // crate-private
cli_errors::{canonical_detail, pricing_result} // crate-private
```

`llmusage::run() -> anyhow::Result<()>` retains its public signature and prints
successful help/version before returning `Ok(())`. Binary `process()` owns the
final output and returns `ExitCode` normally so guards unwind.

### 3. Contracts

- Default language is English. Only `en`/`zh` are canonical; `zh-CN` normalizes
  to `zh`. `LANG`/`LC_*` and report `--locale` do not select UI language.
- Root selection follows formal Clap: explicit global `--home` wins over
  `LLMUSAGE_HOME`, then `~/.llmusage`. `clean` retains its existing exception:
  explicit home or `~/.llmusage`, ignoring `LLMUSAGE_HOME`.
- The read-only bootstrap walks the built Clap metadata, skips named argument
  values and short attached values, preserves OsString paths, and respects `--`.
  Formal Clap alone determines validity. Across valid command depths the
  deepest global home wins; same-depth rejected repetition uses the first
  complete home only for choosing diagnostic language. Built-in `help` treats
  its remaining tokens as command-path positionals; option-shaped tokens there
  are not home overrides. `help --zh` forces that help output only.
- Preference reads never create root/logs/DB or acquire settings/worker locks.
  Missing settings use English. Invalid/unreadable settings use English and
  return a typed diagnostic; the caller renders it once in the selected
  language (including forced Chinese help). Known validation reasons translate;
  original OS/serde details may remain original. Reads never rewrite settings.
- Explicit writes use `<root>/cli-settings.json` and independent fs4 OS lock
  `<root>/cli-settings.lock`. Recover sibling residue, then read/update/write
  under that lock using existing `write_file_atomic` without integration audit.
  Preserve unknown JSON object members. Explicit save may replace malformed
  JSON, but must propagate real read failures and preserve old complete bytes
  on write/replace failure. No DB/worker-lock dependency.
- `lang` shows English/中文 choices only when all three streams are terminals.
  Otherwise report the current language and explicit commands without reading
  stdin. Empty input, q/Q and EOF cancel without writing. Unknown codes fail
  before writes; announce the target language only after successful save.
- `help`, version, parser errors and read-only `lang` return before AppContext
  logging initialization. Help/version exit 0; parse errors retain Clap's
  channel and code (normally 2); execution failures retain code 1. Clean uses
  stderr-only logging. Dispatch is boxed at the startup boundary to keep the
  Windows main-thread stack budget; failed sync must still complete its run log.
- Async language is Tokio task-local. Synchronous rendering uses a nested
  thread-local override restored on unwind. Sync override takes precedence,
  then async scope, then English. A sync closure must not suspend. Spawned
  tasks do not inherit language: capture Language in human callbacks/formatters,
  and keep domain workers/data/wire language-neutral.
- Help descriptions/constraints are paired by real command paths/ArgIds;
  parser errors use ErrorKind/context rather than replacing rendered English.
  Preserve safe argument values and stable typed codes. CLI-owned Diagnostic
  context translates user-facing explanations while preserving original error
  sources; do not parse canonical free text or change shared Error Display.
  `Diagnostic::new` retains English Display behavior. When replacing an existing
  own error/context, `with_canonical` preserves its exact original Display
  (including existing Chinese) independently of both UI translations. Do not
  localize an error before storing or logging it.
- Logged C command diagnostics use private closed `DiagnosticFacts` families
  (Reports/Operations/Domain/Pricing/RemoteImport) with stable codes and neutral typed arguments.
  `with_facts` derives paired presentation from the same catalog used for log
  replay while preserving canonical Display. `cli_errors::log_facts` also
  projects existing typed `LlmusageError` variants when no facts-backed CLI
  diagnostic exists. Unknown external detail remains explicit original detail;
  do not infer a code from English/Chinese text or store translated templates.
- A presentation context attached to an already complete canonical error uses
  `with_facts_context`, which marks only its newly added wrapper using the
  original chain length. `canonical_detail` skips that wrapper when saving run
  details, preserving every original nested context and typed cause, including
  equal strings and later outer contexts. Unmarked constructors keep their
  original behavior. Human cause rendering is independent of this storage
  projection; never remove useful external detail by general string deduplication.
- Pricing validators carry private neutral facts beside the unchanged canonical
  `LlmusageError`; public Store/domain APIs unwrap that original error. CLI
  adapters use `pricing_result` and the private bootstrap entry so a known
  pricing error is not lost before the command body. The same-call private
  registration/probe/cleanup facts preserve public errors/actions/payloads and
  avoid repeated reads, probes or validation. Domain/store/remote do not select
  a language or depend on commands.
- Private importer/decoder facts preserve the public canonical import outcome,
  warning strings and errors. CLI remote presentation saves canonical contact
  data before attaching marked display context; it does not change partial
  commit, accounting certification, Pi/OMP or watermark behavior.
- Existing public AppContext/AppPaths/error/report structs gain no required
  fields. JSON/NDJSON/SSH/HTTP keys, values and numerical semantics do not
  change with language. Clean keeps settings/lock and exact writer residues.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Missing settings, Chinese system locale | English; no files created by reads |
| `lang zh-CN`, then new invocation | Stored zh; Chinese help/text scope |
| Corrupt settings + `help --zh` | Chinese help/repair reason once; bytes unchanged |
| Non-TTY `lang` | Zero stdin reads; no write |
| Worker lock occupied | Language inspection/save remains independent |
| Settings replacement denied | Nonzero result; no success message; old bytes complete |
| Concurrent settings writers | Serialized recovery/read/replace; no stale rollback |
| `--home A daily --home B` | B matches formal Clap |
| `--project clean daily` / `--` | Values/delimited tokens do not change routing |
| Invalid source/timezone/numeric value | Localized reason, safe input and code retained |
| Existing own Chinese/English error becomes paired UI | Original Display and source chain retained in either language |
| Known command error reaches live/replayed run-failed log | Same typed facts select paired copy; canonical error/run data retained |
| Presentation-only context wraps an existing canonical chain | Saved run detail equals original chain; typed original cause remains accessible |
| Nested/interleaved/unwinding scopes | No language leakage; previous scope restored |

### 5. Good/Base/Bad Cases

- Good: choose language before parser diagnostics and initialize logging only
  for an actual command that needs it.
- Base: a public renderer called without a language scope uses English.
- Bad: mutable process-global language, scanning argv for the word clean,
  holding worker.lock for preferences, or translating serialized machine JSON.

### 6. Tests Required

`tests/cli/language.rs` covers profile isolation/defaults/normalization,
no-create help/version/errors/lang, nested and generated help, every current
command/ArgId and ErrorKind, schema arity/selector parity, typed final errors
and input preservation, Windows failed-sync stack regression, corrupt and
unreadable preferences, unknown fields/codes, OS-locked writers, actual Windows
replacement denial/recovery, non-TTY zero-read, menu cancel and async/sync scopes.
`tests/api/facade.rs` verifies stable validation codes with a temporary explicit
home. Keep architecture dependency tests and shared atomic-writer failpoint
coverage. Native terminal smoke remains separate from headless evidence.

### 7. Wrong vs Correct

Wrong: call `Cli::parse()`, initialize logs, then inspect language; or localize
an error by parsing its English message. Correct: bootstrap from the same Clap
schema, read preferences without side effects, formally parse, then project
structured error kind/code plus typed CLI input context before dispatch.
