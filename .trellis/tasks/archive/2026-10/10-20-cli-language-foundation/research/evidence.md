# F implementation evidence — 2026-10-20

Scope: F only. User authorization at the start of each artifact supersedes historical planning-only text. No install, commit, push, release, real usage sync or rebuild was performed. All subprocess data/config tests use temporary roots.

## Shared interface for S / C / T

- `common::i18n::Language::{English, Chinese}`; `Language::parse("en" | "zh" | "zh-CN")`, `code()` (canonical en/zh), and `select(en, zh)`.
- `i18n::current()` uses a nested synchronous override first, then Tokio invocation scope, then English. `i18n::scope(language, future).await` isolates async invocations; `i18n::with_language(language, closure)` restores nested synchronous rendering even on unwind. `i18n::text(en, zh)` is the small presentation helper.
- `spawn` / `spawn_blocking` intentionally do not inherit language. Capture the immutable `Language` in human callbacks, logging formatters and TUI render scopes. Domain workers and wire payloads remain language-neutral.
- `runtime::preferences::read(root)` returns `Preferences { language, diagnostic: Option<PreferenceDiagnostic> }`; diagnostics have typed read/JSON/object/language reasons and `render(Language)` for the selected startup language. It never locks or creates files/directories; missing files use English, malformed/unreadable settings use English and one caller-owned repair diagnostic. `write(root, language)` creates only the root/settings lock and atomically updates `cli-settings.json`, preserving unknown object members. Recovery and document reads happen under `cli-settings.lock`; `worker.lock` / Store are not involved.
- `commands::cli_errors::Diagnostic::new(english, chinese)` provides downcastable CLI-owned diagnostic context. Use as a typed error or `anyhow::Context`; original source chains remain available. `application_error(error, language)` projects typed domain variants/codes without mutating public error Display.
- `commands::entrypoint::run_args(args_os)` is the read-only startup/dispatch adapter. `process()` owns final output and returns `ExitCode`; `run()->anyhow::Result<()>` keeps its signature. Help/version/parse failures/lang short-circuit before AppContext/logging initialization.
- Existing AppContext/AppPaths/report/error structs and public renderer signatures are unchanged. Without a language scope, public rendering remains English. `--locale` remains the existing report parameter and does not select CLI/TUI text.

## Implementation refinements verified against current code

1. Clap 4.6.6 accepts `--home=A daily --home=B` and propagates B back to root matches. Same-depth `--home=A --home=B` is rejected. The selector therefore tracks schema command depth: deepest valid global occurrence wins; an ambiguous same-depth repetition uses the first complete value only for diagnostic language. Formal Clap remains the only validator. This refines the planning assumption that every repeated home is invalid.
2. The extra startup/Tokio scope state machine exposed Windows main-thread stack overflow during sync --rebuild error handling. Boxing the command dispatch future at this boundary fixes it without changing sync behavior. A dedicated regression asserts exit 1 and a completed failed run-log row.
3. `anyhow` context values downcast through `anyhow::Error`; their std error-chain wrappers do not directly downcast as the context value. The final adapter uses the correct downcast and keeps original sources.
4. Compatibility `help --zh` removes only its actual routed flag, never every matching argv token. Named-option arity and descriptions are covered by recursive schema tests, including hidden commands and all focused report leaves.
5. The public `run()` wrapper prints successful help/version and returns `Ok(())`; the binary adapter separately owns final process output. This retains the previous successful top-level-help behavior.
6. With main-session authorization, `report_args.rs` received only an ASCII guard on the existing timezone byte slices: malformed six-byte Chinese input now returns the normal validation error instead of panicking. The paired CLI regression covers Chinese mode. No valid report parsing behavior changed.
7. With main-session authorization, the existing stable-code API subprocess test now uses a temporary explicit `--home`. Its former real-default-root access failed before validation in this sandbox. Product validation ordering was not changed to hide this harness problem.

## Verification

- `cargo check --locked --all-features`: PASS.
- `cargo test --locked --offline --all-features --test cli language:: -- --test-threads=1`: PASS, 17 tests after the Windows stack fix. Subsequent selector assertion/refinement also passed its focused test.
- First `python scripts/ci-rust.py`: format/clippy PASS; library 951 PASS / 19 ignored. It stopped at the existing unisolated API subprocess test (see refinement 7). A final complete rerun follows the focused API/CLI/architecture verification.
- Final implementer `python scripts/ci-rust.py`: **PASS (exit 0)**. Format and clippy (`--locked --all-targets --all-features -- -D warnings`) passed. Tests: library 951 passed / 19 ignored; API 3, architecture 12, CLI 51, query 9, remote 8, store 2, sync 150, TUI 35 passed. Total **1221 passed / 19 ignored / 0 failed**. `cargo doc --locked --no-deps` with `RUSTDOCFLAGS=-D warnings` passed. Full output: `research/ci-rust.log`.
- Focused final domain gates: API 3/3, architecture 12/12, CLI 51/51 PASS. Output: `research/focused-gates.log`.
- `git diff --check`: PASS.
- `git diff --exit-code -- Cargo.toml Cargo.lock`: PASS (unchanged).

The complete gate above is the implementer's compiled snapshot before the independently dispatched checker edits. The checker owns additional typed-input diagnosis/help-route regression fixes and their focused verification; see its separate `research/check-evidence.md` for the combined verdict. Do not present the implementer snapshot as validation of later modifications.

Environment: normal cargo resolves to an MBX wrapper whose managed target directory is denied in this sandbox. Use direct `C:/Users/lyh/.cargo/bin/cargo.exe`, or for the gate set `$env:PATH = 'C:\Users\lyh\.cargo\bin;' + $env:PATH` for that command only. The first test attempt lacked locked dev dependencies and hit Windows schannel SEC_E_NO_CREDENTIALS in the sandbox; escalated `cargo fetch --locked` succeeded. No TLS settings, manifests or lockfiles were changed. The Windows linker emits informational localized stdout warnings during linking; clippy itself passes with `-D warnings`.

## Acceptance mapping

| Criterion | Evidence |
| --- | --- |
| F-AC1 | `language_is_persistent_normalized_and_profile_isolated`; `explicit_home_environment_home_and_clean_exception_are_isolated`; LANG/LC_ALL deliberately set to Chinese while missing settings stay English |
| F-AC2 | `concurrent_settings_writers_are_serialized_without_worker_lock`; `language_menu_never_reads_non_tty_and_cancels_empty_eof_or_q`; no-create subprocess lang inspection |
| F-AC3 | malformed/unreadable/unknown-code tests; unknown JSON member preservation; OS-lock concurrency; actual Windows ReplaceFileW sharing denial leaves old complete bytes and a next successful save recovers; shared atomic writer's existing write/flush/replace failpoint tests run in the Rust gate |
| F-AC4 | schema arity traversal; selector-vs-formal parse cases including depth overrides, root/leaf/equals homes, Chinese/space path, short clusters and attached values, option values equal to clean/help/fake home, and -- delimiter |
| F-AC5 | recursive bilingual command/ArgId help coverage; every current ErrorKind projection; real enum/date/timezone/number/conflict/required/unknown errors; framework channels and exit codes. Future non-exhaustive kind fallback is code-reviewed; a nonexistent future variant cannot be instantiated by today's Clap |
| F-AC6 | no-create help/version/error/lang matrix, hidden/nested help, clean's env exception, typed final error/source test and sync failure exit/run-log regression |
| F-AC7 | 50 interleaved yields per language, nested sync scopes, unwind restoration, noninheritance by spawned tasks; full API/architecture and Rust gates PASS |

## Follow-up ownership / limitations

- S owns actual console/bootstrap log text, progress, recovery, final sync table and notices; C owns other command human output and clean copy; T owns all TUI render text and snapshot startup. Their entrypoint receives the correct invocation language now.
- All existing current Clap paths/ArgIds have paired help coverage; adding an unsupported ArgId or a multi-value named option intentionally fails the schema test until the catalog/selector is extended.
- Native interactive terminal smoke: NOT_RUN. Headless BufRead/subprocess tests are not native terminal evidence.
- Unix non-Unicode argv regression is compiled/run only on Unix CI; Windows validates Chinese/space paths and encoded OsString handling.

## Coordinator spec sync

Added backend/cli-language-contracts.md with the verified F API, startup precedence, generated-help positional rules, typed diagnostics, atomic preferences, scope isolation, error/exit compatibility and test requirements. Backend index links it; runtime-log no-create contract now describes language selection before formal parsing and early help/lang exits. Independent check evidence covers subsequent fixes and tests.
