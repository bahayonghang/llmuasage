# F independent review — 2026-10-20

Scope: the full F implementation, including untracked product files, parent/child artifacts, check.jsonl contracts, and the parent-authorized narrow validation-context change in `commands/sync.rs`. Product code was stable before review; no other agent's edits were reverted. No commit, push, install, release, or real usage operation was performed.

## Findings fixed

1. **Generated help commands were skipped as targets.** `help help` rendered the root overview, and `help remote help` rendered the remote container instead of the generated help command. A new bilingual subprocess regression failed before the fix. Only the first actual helper route receives special handling; subsequent `help` tokens resolve through command metadata. Option-shaped tokens after the helper route are positional command-path input in Clap and can no longer masquerade as `--home` overrides. The selector regression includes `--home=A help remote --home=B`, which is formally rejected and retains A for its diagnostic language.
2. **Sync validation lost the invalid input.** The structured-code projection preserved `unknown_source` / `invalid_recent_days` / `invalid_parallelism` but omitted the user-supplied source/value. A bilingual subprocess regression failed for `bad-source` before the fix. A crate-private CLI helper delegates the same domain validator exactly once and attaches typed presentation context using the original input selected by its typed error code. Normal sync dispatch and emit-shards use this helper. Error-source projection also localizes typed causes below a Diagnostic, preserving the original typed `SyncRequestError` and canonical message in the actual error chain without printing app-owned English as raw detail. A unit test checks exact source equality and original validation precedence.
3. **Forced Chinese help printed an English preference-repair warning.** `preferences::read` formerly returned a preformatted English diagnostic. It now returns a typed `PreferenceDiagnostic`; own validation reasons have paired projection, while IO/serde details remain original. Entrypoint formats the repair warning once using the selected language. New tests cover malformed JSON, non-object documents, wrong language type, and unsupported codes with `help --zh`, checking Chinese output, one warning, unchanged bytes, and no new files. Ordinary malformed-settings reads still fall back to English.

The pre-existing Unicode timezone panic guard and temporary API test-root isolation were reviewed and retained. The clean settings/residue keep classification was reviewed; the cleanup deletion set is unchanged.

## Independent verification after review fixes

All Cargo commands used the direct executable `C:/Users/lyh/.cargo/bin/cargo.exe`; the shell's managed wrappers otherwise fail before invoking Cargo. No manifest, lockfile, toolchain setting, or TLS setting was changed.

| Check | Result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo check --locked --all-features` | PASS |
| `cargo test --locked --all-features --lib commands::cli_errors::tests:: -- --test-threads=1` | PASS: 1 |
| `cargo test --locked --all-features --test api --test architecture_dependencies --test cli -- --test-threads=1` | PASS: API 3; architecture 12; CLI 54 |
| `cargo test --locked --all-features --lib commands::sync::tests::emit_shards_does_not_open_or_lock_the_user_database -- --test-threads=1` | PASS: 1 |
| `git diff --check` | PASS |

The 54 CLI tests include all 20 language tests. The first independent pre-fix full CLI run also passed its original 51 cases; adding targeted regressions exposed the first two issues above (17 passed / 2 failed), then all added cases passed after repair. The Windows linker emits informational localized stdout messages; clippy itself passes with warnings denied.

The implementer's complete `python scripts/ci-rust.py` run passed on the **pre-review-fix snapshot** (1221 tests passed, 19 ignored, plus fmt/clippy/rustdoc; raw log: `research/ci-rust.log`). This review did not rerun the unchanged complete test suite. The post-fix checks above cover the changed adapters and new diagnostics directly. Final integrated `just ci` remains A's gate.

## Acceptance mapping

- F-AC1: cross-process en/zh/zh-CN persistence, LANG/LC_ALL independence, explicit/env/default-root isolation.
- F-AC2: zero-read non-TTY reader, menu empty/q/EOF cancellation and retry, independent settings versus held worker lock, no init/DB required for lang.
- F-AC3: unknown code no write, malformed/unreadable settings preservation, unknown JSON fields, pending recovery before reading, concurrent writers, denied OS lock/Windows replacement with complete prior bytes and subsequent recovery. Shared atomic writer failpoints were already covered in the pre-review full gate.
- F-AC4: schema arity assertion and selector/formal-parse comparison, nested/global home depth, attached/short/Unicode-space paths, values equal to commands or fake options, delimiter, generated help targets and rejected helper-route pseudo-home.
- F-AC5: recursive metadata coverage of all current command paths and ArgIds, current ErrorKind projections, enum/date/timezone/numeric/required/conflict/unknown errors, localized constraint headings and channels. Non-exhaustive future-kind fallback is source-reviewed; a future variant cannot be constructed using today's Clap.
- F-AC6: help/version/parser/noninteractive-lang no-create matrix; clean env exception; public `run()` successful help/version adapter source-reviewed; single final typed error; invalid input/code/source retained on normal and emit-shards branches; failed sync run-log completion.
- F-AC7: interleaved async tasks, nested synchronous scopes, unwind restoration and spawned-task noninheritance; API compatibility and architecture gates pass.

## Findings not fixed / remaining owners

- No unresolved F product defect was found after the repairs.
- Main completed foundation spec synchronization in `backend/cli-language-contracts.md`, the backend index, and the runtime-log no-create contract. This review read the resulting contract and checked it against the actual new APIs, scope precedence, helper-route handling, typed preference diagnostics, validator source preservation and early-return behavior. No spec/API discrepancy remains. Child context manifests include the new contract.
- S/C/T still own their respective runtime human output and TUI migrations. Their pending app-owned English/Chinese strings are not F implementation claims.
- Native interactive terminal smoke remains **NOT_RUN** and is owned by A. Headless subprocess/BufRead tests do not count as native evidence. Unix non-Unicode argv coverage is platform-gated and was not run on Windows.
