# C operations implementation evidence

This resumes the interrupted operations slice. Product implementation is
authorized by the task's top-of-file authorization note. This record is producer
self-verification, not independent whole-C acceptance. No commit, push, install,
release, real usage sync/rebuild, external SSH/git/update or provider request was
performed. Tests use temporary runtime and third-party roots, offline Cargo and
disabled pricing refresh. Native terminal smoke remains NOT_RUN.

## Coverage and preserved boundaries

| Surface | Presentation and protection | Evidence |
| --- | --- | --- |
| C06 init | Paired completion/next step, original bootstrap decisions and data | operational human process regression |
| C08/C09 remote | Paired registration/list/remove/import/empty results; original handshake JSON and identity/deletion rules. Private registration facts originate at existing validation decisions; public errors remain exact and invalid inputs fail before transport. | paired remote process rejection/local-identity checks; fake version/handshake/protocol unit regression; remote worker separately owns import-protocol refinements |
| C10 status/source-status | Stable status/quality/accounting keys drive human labels. Reader limitations and actual detected/checked root labels remain visible. Private same-call probe facts retain descriptor-label versus detected-label distinction without a second scan or locale read in domain; public probe DTO and canonical detail stay unchanged. Shared sync diagnostics preserve class separation, eight-sample cap and basename/hash privacy. | paired persisted source/doctor diagnostics; neutral producer canonical-equivalence unit regression |
| C11 diagnostics | Default stdout and --out remain canonical JSON. Forget success/rejection/context paired, original deleted_by_user/source isolation unchanged. | default/out snapshot comparison, missing/ambiguous/selected-source process tests |
| C12 doctor | Private serde-skipped facts render current check decisions. JSON checks/detail/status remain canonical. Shared bounded source-fault renderer reused; parse warnings still concern malformed/oversized only. | paired diagnostics process test, existing doctor unit regression; pricing rejection evidence owned by independent pricing correction |
| C13 catalog | Paired result/layer metadata. Private typed pricing correction and tests are owned by the checker; shared adapter used at existing CLI bootstrap sites. | separate language_catalog tests/checker evidence |
| C14 clean | Paired category/action/summary; original preview/delete sets, settings keep, numeric widths and no-create semantics retained. | paired actual preview/--yes, exact discovered migration-backup deletion count, unchanged DB and kept config/settings/unknown files |
| C15 logs | Stable IDs/facts select human replay, old/unknown details retain original marker. Entire recognized nested shape required before hiding raw metadata. JSON and disk bytes unchanged. | paired process replay/JSON/disk comparison; complete/unknown Operations/Domain/S nested-shape projector regressions |
| C16 serve | URL/exposure/SSH/browser/result context paired. Browser policy/listeners untouched. | fake early-server and signal-failure paired unit test, original server supervision regressions |
| C17 dash/tui alias | Paired deprecation/startup shell, command spelling and ANSI policy retained; panels belong to T. | paired deprecation unit regression |
| C18 export | Paired output confirmation. Exported HTML/assets and stable snapshot projections untouched. | English/Chinese real-process exports: compare all non-snapshot file bytes plus models/sources/hosts/projects/costs |
| C19 uninstall | Private same-call cleanup outcomes distinguish owned/noop/residue/unowned decisions. Public IntegrationAction, cleanup signatures, audits, backups, deletion sets and ordering retain canonical behavior. Aggregate errors retain exact canonical Display and typed private facts drive only human copy. | paired cleanup/noop/residue/unowned/purge and aggregate-failure process regressions; audit/run canonical assertions and user-file bytes |
| C20 update | Paired preview/prompt/retry/cancel/success/failure, exact original error Display. Default yes, no/EOF/ref-change policy and second verification unchanged. No update executed. | fake provider/executor paired confirmation/ref-change/success/nonzero/no-exit-code regression and existing update tests |
| C21 tracer | Paired CLI ingestion/server/database/error shell; dashboard assets/HTTP untouched. | paired missing/empty rollout process rejection; existing tracer parser/server/store regressions |

## Shared canonical error handling

Private `Diagnostic::with_facts_context(&original, facts)` marks an added
presentation-only wrapper and records the original chain length. The shared
`canonical_detail` skips exactly that wrapper by its chain position; it does not
compare messages, deduplicate repeated original text or drop original causes.
The regression retains duplicate original context strings, typed IO downcast,
later outer context and unmarked Diagnostic behavior. The coordinator updated
the original run-failure persistence emitter to use this helper. Existing
constructors and public error types keep their behavior. Human rendering omits
only the immediate covered ConfigInvalid cause for known Pricing/Registration/
RemoteImport facts; external details are retained by their typed presentation.

The same narrow pricing result adapter is reused at existing C bootstrap calls:
init, uninstall, remote, diagnostics-forget, export and serve. The two production
CLI sync progress-bootstrap calls reuse the progress-preserving private helper;
sinks/locks/events/order and public Store bootstrap wrappers remain intact.
The corrupted-marker paired test includes both ordinary sync and --json-events.

## Actual producer checks

Build environment initially used command-scoped `.cargo/bin` and
`CARGO_TARGET_DIR=$PWD/output/s-language-rust-target`. The repository target is
a known broken managed link and was not modified. After OS448 resolving rustup
shims, use the real configured 1.97.0 toolchain bin before `.cargo/bin`; no config,
link, dependency or toolchain was changed. MSVC localized linker-message output
and later incremental hardlink-copy notices are environment diagnostics.

- `operations-resume-check.log`: cargo check --locked --offline --all-features
  PASS at the initial resumed boundary.
- `operations-paired.log`: real-process language_commands PASS 9, no ignored,
  11.56s. Covers paired immutable JSON and real temporary-file behavior above.
- `operations-unit.log`: `cargo test --locked --offline --all-features --lib
  commands:: -- --test-threads=1` PASS 171, 2 existing ignored, 10.44s. Includes
  new canonical-chain, fake update, registration and serve/deprecation checks.
- `git diff --check` owned source/tests: PASS (line-ending notices only).
- Strict clippy found 28 static format calls and two redundant decoder closures
  from the interrupted implementation; these were corrected mechanically.
  Final strict check is independently owned and must use the stable combined tree.
- `operations-sync-pricing.log`: final focused marker integration result is
  recorded below once complete; no PASS inferred from compilation/progress.

## Retained failure evidence

`operations-paired-fixture-failures.log` preserves the first 5 PASS / 3 FAIL
run. All three were isolated test defects, not product cleanup changes:

1. Clean's fixture already contained a bootstrap-generated migration backup;
   its hardcoded Deleted2 expectation ignored that third deletable file. The
   test now inventories exactly the recognized fixture backup files plus its
   one baseline and checks all are removed while DB/keep files remain byte-exact.
2. An empty positional label was rejected by Clap (exit2) before registration;
   process rejection cases now distinguish parser syntax from typed remote
   validation. The option-like target uses `--`; existing direct validator tests
   retain empty-label coverage.
3. The purge test retained its own SQLite read connection through purge,
   causing Windows OS32. All audit assertions remain and the test closes that
   reader before purge; product delete behavior was not changed.

Early operations compile attempts encountered checker-owned pricing conversion
code while that code was in flight; the checker corrected those targeted
conversions. They are not claimed as passing checks. Independent operations
process rerun separately passed9 in check-operations-paired.log.

Whole-C Rust gate, architecture/remote gates and parent integration/native
acceptance remain the coordinator's responsibility. This evidence does not
claim the parent task is complete.

## Final source boundary and environment limitation

The coordinator-approved sync integration is source-complete: two production
bootstrap calls retain their original sinks/ordering and consume the private
progress/pricing result; cfg-test setup is not rewritten. The marker process
test now includes ordinary sync and `--json-events` with explicit temp `--home`
and rejects source-start frames before source discovery. Shared RemoteImport
facts registration is integrated for the separately owned remote worker; the
two resulting orphan RemoteWarning/RemoteError catalog arms were removed.
Domain unit/struct metadata regression verifies complete known facts translate
and future top/nested fields preserve raw detail in both normal/detailed modes.

Final marker integration compilation reached MSVC link and failed before tests
with LNK1104 opening generated `C:/Users/lyh/lnk{...}.tmp`. The original attempt is
retained in `operations-sync-pricing-linker-failure.log`; a single command-scoped
writable workspace TEMP/TMP retry also failed the same way in
`operations-sync-pricing.log`. No further compiler retries or global toolchain,
linker, sandbox or filesystem repairs were attempted by this worker. The
coordinator is resolving the execution context. Thus final sync/probe/Domain
tests are NOT_RUN on the final combined source, not product PASS. Earlier
operations9PASS and commands171PASS/2ignored remain the earlier source-boundary
evidence; independent full-C gate must cover the final additions.
