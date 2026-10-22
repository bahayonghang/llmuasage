# C report implementation evidence

Scope: report/default-daily/focused/statusline presentation only. C as a whole
still depends on operations and independent review. No commits, push, install,
release, real usage sync/rebuild, user data edits, dependencies, or lock changes.

## Producer / human / protected machine map

| Surface | Producer and human boundary | Protected data | Paired behavior evidence |
| --- | --- | --- | --- |
| C01 default daily | `commands/daily.rs` -> `tui/report_table.rs` | Query filters/periods and unified CLI JSON | Default invocation in variant test; English/Chinese output differs while JSON equals |
| C03 daily/weekly/monthly/session | Shared loaders -> unified/focused table labels, titles, total, All/Detected, model overflow and empty messages | Every DTO, source order, token/cache/cost/model/session/project value; visible-channel sums remain separate from authoritative totals | Variant tests include daily history, instances, weekly, compact monthly, session ID, all sections, by-agent, breakdown, no-cost; all three `--locale` values preserve JSON |
| C03 blocks | `commands/blocks.rs` -> blocks table | Block construction, limits, burn/projection, timestamps, count units, costs and JSON | Paired data/empty title, yes/no state, token-limit max and JSON equality; existing report suite retained |
| C04 all focused paths | Real built Clap schema expands the five source hosts into four actual report leaves each; generated help mirrors belong to F and are excluded explicitly | Source injection and shared queries; focused JSON still omits agent/agents | All 20 actual leaves each exercise paired success, empty, conflicting source, text visible total 160 vs authoritative JSON 777, and exact cross-language JSON equality |
| C03/C04 rejections/failures | Typed report diagnostic facts created at existing decisions in daily/session/focused/report_args | Existing canonical error Display; user values, source IDs, host labels, exit codes | Unknown host includes registered user labels in both languages; all/sections/instances/id conflicts; uninitialized failures cover six top-level paths plus 20 focused leaves |
| C05 statusline | Existing hook reader/cost policy -> paired one-line labels | Hook JSON/model/cost units and underlying query/block algorithm | All four cost-source policies, unavailable hook, active/inactive blocks, multiline hook model normalized to one displayed line, paired input/cache I/O failures |
| C05 cache | `statusline-cache/latest.en.txt` and `latest.zh.txt` | Same root/cache enablement and fallback policy | Actual failed query replays only same-language cache; missing Chinese cache emits Chinese unavailable; old latest.txt retained and never read |
| C22 report logging | Same S event IDs; new optional closed neutral report facts passed through shared Diagnostic factory/emitter/projector | Original tracing message/error fields and persisted run errors | RUST_LOG=error + LLMUSAGE_LOG=error paired focused rejection; console and structured-entry replay translate, file message/error and run error remain exact original |

All report table headers are typed catalog keys. CJK labels/user values and
combining marks use Unicode display width for measurement, clipping and padding.
Only text columns shrink to fit; numeric columns keep full formatted values.
Table unit regressions retain visible-channel saturation, compact units, zero
cache placeholders, total separator, source colors, no-cost and ANSI gating.
The pre-existing By host section intentionally retains authoritative numbers
and four-decimal cost formatting; it is not changed into the table projection.

New report diagnostics use a closed 14-family enum with neutral source/host
arguments and stable codes. Fixed variants are empty struct variants so strict
deserialization rejects unknown fields (Serde internally tagged unit variants
ignore them even with enum deny_unknown_fields). Shared logging also validates
complete recognized metadata before suppressing the original error. Canonical
English strings at old report error sites remain exact. Statusline I/O adapters
retain original OS Display and typed source while adding paired UI context.
Persisted RunRecord details have no presentation metadata and remain explicitly
marked original detail; structured tracing entries do have neutral metadata.

## Verification environment

Windows PowerShell; existing Cargo at `C:/Users/lyh/.cargo/bin`, command-scoped
PATH prefix, `CARGO_TARGET_DIR=$PWD/output/s-language-rust-target`,
`LLMUSAGE_PRICING_REFRESH=off`. Explicit `rtk bypass` was needed because wrapper
rewrites of git/rg/Cargo failed to locate Claude config. The broken managed
repository target symlink was not changed. Cargo commands use --locked --offline
and --all-features. Compiler linker-message warnings reflect existing MSVC
localized import-library output; no warning suppressions added.

## Actual results (producer self-verification)

- `cargo check --locked --offline --all-features`: PASS before final fact migration.
- `cargo test --locked --offline --all-features --lib report_table -- --test-threads=1`: PASS 16 (including two new paired width/notes regressions).
- `cargo test --locked --offline --all-features --test cli language_reports -- --test-threads=1`: PASS 6, 40.63s before the final strict-empty-struct and canonical-I/O refinements; final rerun pending below.
- Initial schema inventory correctly surfaced generated help mirrors (55 vs 20); test now excludes only built-in help paths and retains all 20 product leaves.
- An additional visible-token test initially included authoritative By host output; corrected assertion boundaries preserve that original projection.
- `cargo test --locked --offline --all-features --lib report -- --test-threads=1`: initial 69 PASS / 1 FAIL / 2 existing ignored; only failure was the strict unknown-field test described above; fixed and rerun pending below.
- Existing-report test attempt encountered operations-owner in-flight doctor errors (helper visibility and u64 count); owner fixed those; rerun pending below.
- `git diff --check` on owned existing files: PASS (Git only reports LF/CRLF worktree notices).

Native terminal smoke, whole C Rust gate and parent cross-surface gate are NOT_RUN
by this worker. Headless subprocess tests are not native terminal acceptance.

## Final focused reruns

All final commands below use the environment and `rtk bypass` described above.

| Command | Actual result |
| --- | --- |
| `cargo test --locked --offline --all-features --test cli language_reports -- --test-threads=1` | PASS 6 / 0 ignored, 40.55s after strict-facts and canonical-I/O refinements |
| `cargo test --locked --offline --all-features --test cli reports:: -- --skip language_reports:: --test-threads=1` | PASS 13 / 0 ignored, 11.27s; all pre-existing report integration tests |
| `cargo test --locked --offline --all-features --lib report -- --test-threads=1` | PASS 70 / 2 existing ignored, 9.24s; includes all 16 report-table tests, strict report-facts test, report-args and unified projection/section tests |
| `cargo test --locked --offline --all-features --lib commands::focused::tests -- --test-threads=1` | PASS 4, 0.02s; includes canonical Display in both scopes |
| `cargo test --locked --offline --all-features --lib commands::statusline::tests -- --test-threads=1` | PASS 1, 0.00s; all three I/O adapters retain OS Display and typed source with paired human context |
| `rustfmt --check --edition 2024 <all ten owned source/test files>` | PASS, zero output |

`research/reports-snapshot.json` records SHA256 hashes of the ten owned changed
source/test files at this final boundary. Weekly/monthly command files needed no
new report copy changes; their existing S event IDs and common renderer calls
were preserved. The coordinator owns the integration-test module registration.

The final report integration compile still saw operations-owner `sample_summary`
dead-code warning after doctor's renderer migration. That shared issue was
reported to its owner for the later strict whole-C gate; it does not justify
suppressing warnings or removing report coverage. No own-source lint/type error
was observed. Dedicated whole-C clippy/full Rust gate remains the coordinator's
next check after both chunks stabilize.
