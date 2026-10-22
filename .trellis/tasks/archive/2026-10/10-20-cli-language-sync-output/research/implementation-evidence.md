# Implementation evidence

User authorized implementation of the existing parent and five children on 2026-10-20. Historical planning-only restrictions are superseded; commit, push, installation, release, and real-data rebuild remain outside scope.

## Preflight

- All six `task.py validate` checks: PASS.
- `cargo --version`: 1.97.0. `rustc --version`: 1.97.0.
- `cargo metadata --locked --offline --no-deps --format-version 1`: PASS.
- Product baseline: dev at 2b70e3f; only six pre-existing untracked task directories at session start.
- Lockfile fingerprints: implementation-baseline.json.

## Delivery status

| Child | Status | Evidence |
| --- | --- | --- |
| F | PASS | [Implementation](../../10-20-cli-language-foundation/research/evidence.md), [independent check](../../10-20-cli-language-foundation/research/check-evidence.md): CLI54/API3/architecture12 + targeted units after fixes |
| S | PASS | Independent final source/spec checks PASS; complete Rust gate1253 PASS/19 existing ignored (fmt/clippy/rustdoc) |
| C | IN_PROGRESS | F/S checked; disjoint reports/operations implementation with one final independent review |
| T | PENDING | Runs after C per approved order |
| A | PENDING | Waits for F/S/C/T |

Product ACs, full CI and native terminal smoke have not passed yet. Native smoke is separate from headless tests.

## Integration review notes

- Existing en/zh CLI reference describes `--locale` as title/number formatting; A must reconcile this with the actual retained report behavior and new independent `lang` setting.
- Shell auto-rewrite can make `git`/`rg` fail due to RTK Claude-directory detection. Explicit `rtk bypass` works; no user-level configuration changes made.

- Build preflight refinement: MBX `cargo` wrapper could resolve metadata but build attempted an unwritable managed target. F switched to existing `C:/Users/lyh/.cargo/bin/cargo.exe`; no configuration or toolchain installation changes.

## F implementation review observations

- Focused tests cover explicit help variants with no home creation, normalized/persistent profiles, non-TTY zero-read, independent settings/worker locks, real Windows replacement failure and recovery, and nested/interleaved locale scopes.
- Existing CLI regression exposed Windows stack overflow after adding startup scopes. Dispatch future was boxed at the startup boundary; focused failed-sync run-log regression now passes.
- Schema parity tests corrected cross-depth global home precedence in startup selection. See startup design and cli-startup-errors.md.

- F full Rust gate first pass: fmt/clippy PASS; library tests 951 PASS / 19 ignored. API gate revealed pre-existing non-isolated default-home subprocess in facade.rs. Coordinator verified the source and authorized only TempDir + explicit --home isolation, retaining validation-code assertions. Gate rerun pending.

- Independent F review found final SyncRequestError presentation lost the exact safe invalid source/numeric input while retaining the error internally. Checker is adding CLI-owned typed input context at the existing normal and emit-shards validation producers; no canonical English parsing, public DTO change, validation reordering, or waiver.

- F implementer complete Rust snapshot: `python scripts/ci-rust.py` PASS (1221 passed / 19 ignored, fmt, clippy -Dwarnings, rustdoc -Dwarnings). Source snapshot predates independent checker fixes; own evidence/log identify this boundary.
- Coordinator rechecked all four baseline lockfile hashes: unchanged.

- Final desktop-gate dependency preflight: offline metadata initially lacked tauri-plugin-single-instance; approved direct `cargo fetch --locked --manifest-path desktop/src-tauri/Cargo.toml` completed, then offline metadata PASS. Four lockfile hashes unchanged; no desktop source edits.

## Resume build environment refinement

After the usage-limit interruption and user continue, target was a broken symlink to a missing D:/mbx-cache target. Source/config/symlink are preserved. All subsequent gates use direct Cargo with command-scoped CARGO_TARGET_DIR=<repo>/output/s-language-rust-target, an ignored directory inside the authorized workspace. Cold rebuild is required; prior PASS snapshots remain historical evidence, not a substitute for the final current-tree gate.

## Parent acceptance index

The parent PRD defines the criteria; this index links evidence without redefining them.

| Parent AC | Owning delivery | Current evidence/state |
| --- | --- | --- |
| AC1 | F/T/A | F profile/default/persistence PASS; TUI and integration pending |
| AC2 | F | F independent lang/atomic-writer/menu tests PASS |
| AC3 | F/A | F independent metadata/help/error coverage PASS; final integration pending |
| AC4 | C/S/A | S independent/full Rust gate PASS; C command-surface review in progress |
| AC5 | S | Recovery/TTY/flag cases and S complete gate PASS |
| AC6 | S/C | 174 production event IDs/catalog + S projector independently PASS; C replay pending |
| AC7 | S | Closed Unicode table numeric fixtures and S complete gate PASS |
| AC8 | S/C/T | S fault/info/sample fixtures independently PASS; C/T integration pending |
| AC9 | S/C/A | S human/NDJSON/shards + canonical failures independently PASS; C JSON/remote and integration pending |
| AC10 | S/A | S scoped-sink/guard/plain-mode fixtures independently PASS; native A separate |
| AC11 | F/C | F no-create/root/keep/scopes PASS; C must preserve |
| AC12 | A/T | New F language contract added; bilingual docs/TUI contract updates pending |
| AC13 | T | Pending |
| AC14 | T | Pending |
| AC15 | C | Pending |
| AC16 | A | Current combined just ci/native evidence pending |

- S independent snapshot: fmt/clippy/check, units76, sync151/remote8/architecture12 PASS. Verdict NOT_FULL_PASS: reproduced known safety refusal English in zh and SQLite-open fixed Chinese in en. Owning implementer is remediating at neutral typed producer/feedback boundaries; parent design records the approved bounded refinement. No signoff or C start until rechecked.

- S later independent remediation recheck: source/spec PASS; collector2, source_files12, sync adapters57, log projector7, paired actual subprocess5, F language20, public API3 and architecture12 PASS; fmt/clippy/check PASS. Known refusals now retain canonical errors while localizing all three human sinks; SQLite and consumed inventory errors carry neutral private provenance. Full after-review Rust gate remains pending. Earlier NOT_FULL_PASS and broken test-seed attempts remain historical evidence, not current verdicts. Four lockfile hashes rechecked unchanged and default git diff --check PASS.

- S interrupted full gate completed FAIL: fmt/clippy PASS, library969 PASS/6 FAIL/19 ignored; all failures use profiling/tests.rs::temp_root hardcoded target/writer-benchmark, which bypasses the valid command CARGO_TARGET_DIR and hits the known broken target symlink. Raw directory creation fails on that path and succeeds in output/s-language-rust-target/writer-benchmark. One source/test hardcoded-path helper found. Authorized test-only environment-path correction and independent affected-test review; no product/protection change or symlink repair. C remains pending until full rerun PASS.

- S next full gate FAIL974/1/19: sole existing Web interactive payload-shape fixture used raw UTC now. Independent fixed-clock SQLite proof shows Local-date window cutoff excludes the fixture after UTC midnight (raw/floored0 vs prior-day10); previous attempt ran before that boundary and passed. Authorized only existing canonical prior-day bucket seed/comment, preserving every assertion and production predicates. The pre-existing production Local-date versus requested-timezone behavior is OUT_OF_SCOPE and remains unresolved; this fixture adjustment does not fix it. Proof: S/research/check-dashboard-clock-proof.json.

- S final complete scripts/ci-rust.py EXIT0: lib975/API3/architecture12/CLI54/query9/remote8/store2/sync155/TUI35 =1253 PASS,19 existing ignored; fmt/clippy/rustdoc PASS. Source snapshot unchanged through final gate, diffcheck PASS. Historical failed attempts retained. C validated/started only after this result plus independent S PASS.

## 2026-10-21 continuation

Resumed in a new chat after reading the referenced interrupted implementation
chat and current task artifacts. Restored current task to C (already
in_progress); no new task or scope approval is needed. Existing no-commit,
no-install, no-push, no-release and no-real-data-operation limits remain.

- C report self-verification is complete; a fresh independent six-test paired
  process slice passed on the current report snapshot. Whole-C review is pending.
- Registered the existing language_commands file in tests/cli/main.rs so its
  operations tests are actually discovered by the explicit Cargo target.
- Operations implementation continues with uninstall private outcomes and
  complete source-status presentation, including approved same-call probe label
  facts. Public/canonical payloads and scan behavior must stay unchanged.
- T handoff research is read-only while C remains active. T/A product work and
  final combined gate have not started; native smoke remains NOT_RUN.
