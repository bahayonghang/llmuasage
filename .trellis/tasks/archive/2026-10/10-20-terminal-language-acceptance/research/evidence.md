# A integration evidence

Date: 2026-10-21. Current active task: A. F/S/C/T implementation and independent
review are complete. A documents the final combined result; no previous stage's
passing log is substituted for the final cross-surface gate.

## Verified before the final gate

- T's unchanged official `python scripts/ci-rust.py` exited 0: 1320 passed,
  19 existing ignored, 0 failed; format, strict all-target/all-feature Clippy
  and warnings-as-errors rustdoc passed. This includes all eight registered
  integration targets, producer/local-mock quota, logging, job registry,
  remote partial commits and paired CLI/TUI tests.
- The complete Rust source/test/manifest snapshot still matched
  `../../10-20-tui-language-consistency/research/final-source-snapshot.json` after
  starting A; A owns documentation and evidence only.
- `capture-fixture.py` ran the actual binary twice in auto-removed temporary
  homes, once per language, with pricing refresh off, no color and a selected
  synthetic Codex source. Both syncs exited 0. The exact stdout captures are
  `fixture-sync.en.txt` / `.zh.txt`; stderr remains separate. The input has
  123 tokens in one event and 482 bytes. The ten-column table counts events,
  not tokens. Measured parse/write durations may differ between invocations.
- Initial fixture assertions mistakenly expected the input token quantity in
  an event-count table and compared pre-write bytes against Windows newline
  conversion. The harness now writes exact UTF-8 bytes and validates actual
  source byte length. Product code was unchanged by this harness correction.
- A context manifests validate without truncation warnings. The current
  source-sync feedback excerpt carries its canonical path and source hash.
- `git diff --check` passed. Existing LF/CRLF notices are informational.

## Scope and compatibility

All four dependency lockfile hashes match their pre-A values:

| File | SHA256 |
| --- | --- |
| Cargo.lock | aca460864a5354c9324c676a40944c939200f4e92b24457d918328d2324c6b46 |
| docs/package-lock.json | d09ad374ecd3b78d1a741742e5d77432cdcafe8461dfe3288cd466672d64adf7 |
| desktop/package-lock.json | 92d45817340a867e4723c0ed5191c8c481122b7e5543e8e99604c4e9ba48a80a |
| desktop/src-tauri/Cargo.lock | 3e67a3b138d895b78c8e7bdc787f084fbbd9b8032ce0b071fea95452e180d32c |

No package version/dependency/CI recipe change, installation, commit, push,
release, real usage import/rebuild or real credential write was performed.
The only syncs run for A were the isolated fixture captures above. Tests use
fake SSH/executors and local endpoints; canonical protocol/log/DTO comparisons
remain in their owning child suites. Browser/desktop language is independent.
The existing broken repository target link is preserved; command-scoped target
and real configured toolchain paths are used. MSVC link temporary files require
the coordinator's already approved execution context.

ADR0015 has a narrow dated update distinguishing its historical automatic
local migration rollout from current skip-and-retain/explicit Pi repair policy.
Original historical decisions and remote migration text remain intact.

## Final acceptance

The original `just ci` recipe exited **0**, recorded in `just-ci.log`:

| Check | Result |
| --- | --- |
| CI contract self-test and check | PASS |
| Root Rust format / strict Clippy / rustdoc | PASS |
| Root Rust tests, all eight integration targets | 1320 PASS, 19 existing ignored, 0 failed |
| Dashboard JS | 66 PASS |
| Desktop frontend | 65 PASS across 18 files, plus 4 Node checks PASS |
| Desktop TypeScript and production build | PASS |
| Desktop Rust | 33 PASS, 0 failed |
| VitePress production docs | PASS; build complete in 6.49s |
| Independent documentation/spec/evidence review | PASS; check-evidence.md |
| Final source snapshot | All 500 recorded files unchanged after gate |
| Dependency locks / final diff whitespace | All four unchanged / PASS |

`final-verification.json` records the final checks. Parent AC1–AC16 are mapped
in the parent's `research/acceptance-index.md`. Native ordinary-window CLI/TUI
smoke is separately **NOT_RUN** in `native-smoke.md`; subprocess/TestBackend
results do not replace it. No MSRV, audit, semver or native PASS is inferred
from this gate. Task records are completed in place to preserve evidence links;
the working tree remains uncommitted and unarchived.

## Subsequent authorized commits and archive

The user subsequently authorized all work commits and archival. Four work commits are recorded in `commit-and-archive.json`; all six completed tasks are now in the same archive month. Current context/artifact links were migrated; raw execution logs retain their historical paths. The isolated language-foundation index export also passed all-feature test-target compilation (`split-foundation-check.log`). Product source and docs remain identical to the full passing CI snapshot. Native smoke remains NOT_RUN; no push, installation or release occurred.
