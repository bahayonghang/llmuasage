# C pricing validation correction

The independent C reviewer reproduced known application-owned pricing
validation text leaking in Chinese `catalog apply` errors. Generic
ConfigInvalid/original-detail framing did not satisfy the frozen scope. Main
approved the bounded pricing/domain/store correction and spot-reviewed its
private propagation and preserved canonical decisions. The reviewer implemented
this correction directly; its tests are self-verification of that correction,
not a second independent author's implementation review.

## Mechanism and ownership

- `domain/pricing_catalog/diagnostic.rs` adds only private language-neutral
  `Failure { error: LlmusageError, facts: Option<Box<DiagnosticFacts>> }` and closed
  facts. Domain validation/parse cores carry that result. Existing public
  `PricingCatalog::new`/`load_snapshot` and shared `from_document` unwrap the
  exact original public error. There is no locale, command import, database
  dependency, mutable side channel or repeated read/parse in this mechanism.
- Existing pricing Store public methods still return their original error
  type. Private active/apply/activate/reset/status entries return facts from the
  same original decisions and load calls. Known nested load failures retain
  typed nested facts; original external detail remains available. Canonical
  validation strings, conditions, numeric algorithms, IO/order, locks,
  activation/recompute and transaction behavior are retained. The single direct
  SQLite `.optional()` conversion at the upgraded bootstrap boundary maps to
  `LlmusageError` before the private carrier; no domain SQLite conversion exists.
- A concrete corrupted `pricing_recompute_in_progress='{'` marker proved that
  public bootstrap could fail before the command body's new entry. Main approved
  the narrow `schema.rs` private typed core and default/progress-preserving
  entries over the same original bootstrap. Public bootstrap/migration/progress
  methods unwrap their original error; no migration, repair, lock or progress
  sequence changed. Operations owner consumes it at C callsites and the two
  explicitly approved sync bootstrap sites. The helper added by this reviewer is
  `bootstrap_with_progress_and_pricing_facts(sink)`; sync adapters/tests belong
  to the operations owner.
- Paired rendering lives in `common/i18n/commands/pricing.rs`. Operations owner
  registered Pricing in the private shared facts enum and moved the generic CLI
  adapter to `cli_errors::pricing_result`, used by catalog/doctor and the other
  approved callers. Original `LlmusageError` remains downcastable as a cause.

## Persisted canonical-chain regression caught during verification

The first meaningful paired process regression caught a real failure: adding
presentation context around an existing ConfigInvalid duplicated its full
canonical error in `run_log` (`reason: reason`). Raw evidence is retained as
`check-language-catalog-chain-red.log` (**3 passed / 1 failed**). This was not
waived by weakening the exact stored-error assertion.

Main approved an explicit presentation-context marker and canonical storage
projection. Operations owner added `Diagnostic::with_facts_context`, recording
the original chain length, and `canonical_detail`; main changed only the
existing `run_tracked` persistence expression to that formatter. It omits the
added wrapper by position, preserves original repeated/outer contexts and typed
causes, and does not compare/deduplicate natural text. Final error rendering
omits only the covered first ConfigInvalid wrapper while retaining external
causes. File-event `%err`, public error Display and protected data stay exact.

## Actual checks

Command-scoped Cargo toolchain, existing ignored
`output/s-language-rust-target`, `--locked --offline --all-features`, pricing
refresh off and serial tests were used. No manifests/lockfiles, managed symlink,
real usage data, installation or external update was changed.

| Check | Result |
| --- | --- |
| First pricing cargo check | PASS compilation; one newly unused Store import removed |
| Existing `--lib pricing_catalog` slice | PASS 30, raw `check-pricing-units.log` |
| Expanded `--lib pricing` after typed schema core | PASS 70, raw `check-pricing-final-units.log`; includes adapter and strict nested-facts tests plus domain/Store/bootstrap/recompute/refresh/writer regressions |
| `--test cli language_catalog` after persistence integration | PASS 4, 35.83s, raw `check-language-catalog.log` |
| Narrow rustfmt/diff checks | PASS before final one-query equivalent error conversion; added progress helper independently direct-rustfmt checked afterward |
| Strict clippy first combined run | FAIL 30 mechanical operations issues in shared commands/logging only, raw `check-clippy.log`; owner corrected them, final rerun remains pending |

The four process groups exercise 18 invalid-document families, URL/missing-file
refusal, doctor snapshot errors, unchanged public canonical errors, exact file
and stored run errors, unchanged token/status data, successful paired
apply/reset, and corrupted-marker catalog/doctor/init/uninstall early refusal
with marker retention. The original tests remain: no catalog fixture assertions
or algorithms were relaxed. One initial test expectation used the wrong literal
public prefix (`config invalid` rather than existing `invalid config`); that test
mistake was corrected before the real stored-chain regression above.

The final combined gate must cover the last equivalent one-query error
conversion, marked shared helper, progress helper and operations sync test
extension together. Do not sum focused slices into a whole-suite total. The
main-owned pricing/CLI-language/runtime-log/report contracts were reviewed and
match these interfaces. Native terminal acceptance and final `just ci` remain
separate parent/A work; this file does not certify either.

## Probe accuracy

All pricing probes used disposable profiles. A separate reviewer remote probe
incorrectly assumed label `!!!` normalized to empty; it instead attempted SSH
to `unused@example.invalid` and failed exit 255 permission denied. No connection,
import or external change succeeded. Main was informed immediately; subsequent
remote checks use fake transports or validation paths proven to stop before
SSH. A Python SQLite connection initially held one generated temp profile open
during cleanup; after the probe exited the exact generated temporary directory
was verified and removed. Neither incident involved real user configuration.
