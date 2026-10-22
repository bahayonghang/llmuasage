# Independent A acceptance review

Date: 2026-10-21. Reviewer: `tui_check`. Active task:
`10-20-terminal-language-acceptance`. Latest implementation authorization
supersedes historical planning-only paragraphs; no commit/push/install/release
or real-data rebuild authorization is inferred.

## Evidence and contract checks

- Read A PRD/design/implement/check manifests, parent acceptance index,
  validation plan, C documentation handoff and current source-sync presentation
  excerpt. Prior T source review is in T/research/check-evidence.md.
- Parent AC1-AC16 each have an owner and concrete child evidence. The referenced
  F/S/C/T evidence files exist. Earlier child gate results certify their own
  source snapshots; the final A gate remains final-tree authority.
- Independently read T/research/ci-rust.log: 1017 library + 3 API + 12
  architecture + 73 CLI + 9 query + 8 remote + 2 store + 155 sync + 41 TUI
  tests = 1320 passed, zero failed, 19 existing ignored. All eight explicit
  targets appear; rustdoc completed. T's final coordinator confirmation also
  records fmt and strict Clippy PASS.
- Current source-sync spec SHA256 is
  `0e72a6e4bf305f842f8440856ee6472dda605114479ead8858a071276a5e8c7d`,
  matching the exact excerpt's provenance. Presentation contracts preserve
  canonical public events/reports/errors, private neutral facts, original
  run-log strings, source/worker ordering, numeric semantics and data privacy.
- TUI presentation/runtime/subscription specs describe the implemented startup
  snapshot/private CLI handoff, Unicode navigation, private cache metadata,
  report fingerprint/request association and preserved redraw/network/TTL rules.
- `capture-fixture.py` uses a temporary home/profile and synthetic Codex input,
  disables pricing refresh, selects only Codex and captures real command stdout
  and stderr. Metadata explicitly labels both captures headless, with exit 0,
  482-byte fixture and 123 source tokens. The displayed table's event counts
  are one; tokens are not mislabeled as event counts. Measured durations may
  differ between language invocations.
- Native-smoke.md records English/Chinese ordinary-window CLI and dash/tui as
  NOT_RUN because native APIs are disabled. No TestBackend, subprocess capture
  or compile result is represented as native acceptance.
- The narrow dated ADR0015 note distinguishes current explicit Pi repair
  policy from the retained historical split rollout. Original decision text
  remains available; this is authorized by A/task.json notes/relatedFiles.
- Read-only diff check found no change to Cargo manifests/locks,
  rust-toolchain.toml, justfile or docs package/lock files. Existing command
  relocation pages retain their legacy links/stub role.
- Independently recomputed every hash in T's final source snapshot: all 278
  recorded source/manifest/test files still match the current A working tree.
  T's completed source gate evidence therefore remains applicable.

## Frozen documentation review

Reviewed the final README pair, CLI-reference pair and first-sync guide pair.
The two command-index pages intentionally remain relocation stubs. No local
documentation fix was required after this independent review.

- Both languages consistently describe English default, normalized zh-CN
  alias, explicit persistent language selection, root/profile precedence and
  clean exception, read-only startup/no-create paths and one-shot Chinese help.
- Report locale compatibility is separated from interface language. Stable
  flags/values/reason codes/units/models/user values, machine output, stored
  diagnostics and historical/file logs retain their documented boundaries.
- Statusline cache names, same-root TUI startup snapshot and reopen behavior,
  quota entry/cache/r/R/m behavior, sync y/x safety and unchanged input/theme
  semantics match the implemented source and passing T regressions.
- First-sync guides correctly qualify the ten-column frame, diagnostic sample
  limits, fault/information distinction, privacy locations and reader/timing
  boundaries. Neither guide equates parse+write with command wall time or
  coverage stock differences with an exact loss forecast.
- Independently extracted the first text block from each guide and compared
  it with its actual A stdout capture, normalizing line endings and trailing
  output newlines only: both match exactly. English 8ms and Chinese 7ms parse
  timings are preserved and explicitly identified as varying measured values.
  Neither capture contains ANSI.
- Recovery gate and default keep/EOF/read-error/invalid-input policy match S;
  unreadable/incomplete discovery cannot be bypassed. Pi local repair now
  points to the explicit source rebuild, consistent with the dated ADR note.
- Relevant relative guide/reference anchors and legacy stub destinations are
  preserved. `git diff --check` passed for all eight owned pages and ADR note.
  Existing unrelated source-list omissions remain outside this bounded edit;
  this review does not claim they were corrected.

## Findings

No unresolved documentation, evidence ownership or source/spec contract defect
was found after source freeze. No fixes were necessary in this A review.
Final A `just ci` remains coordinator ownership and pending; native acceptance
remains NOT_RUN independently of automated gate status. Do not convert that
limitation into a native PASS or silently claim all parent checks ran.

## Verification

No executable gates run by this reviewer. T's gate was inspected as recorded
evidence; current A gate results must be appended by the coordinator. This
review does not claim MSRV/audit/semver checks, product publication, native
smoke success, or that the unrelated Web timezone-window issue was repaired.

## Coordinator final gate confirmation

Original `just ci` EXIT0: root Rust1320PASS/19existingignored, dashboardJS66PASS, desktop frontend65PASS plus4Nodechecks, desktopRust33PASS, strict Clippy/rustdoc/TypeScript/builds/VitePress PASS. All500 final-source snapshot files and four dependency locks remain unchanged. Final diff whitespace PASS. Independent source/docs conclusions remain valid; native smoke remains NOT_RUN. No commit/archive/install/push/release.
