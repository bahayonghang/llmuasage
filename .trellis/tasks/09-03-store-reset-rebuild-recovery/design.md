# Design: reset/rebuild/catalog recovery

## reset_usage_data

Add `DELETE FROM source_file;` to the existing batch in the same Immediate transaction. Keep run_log / integration_install / trigger_state.

Update tests from `09-03-test-coverage-risk` that assumed source_file survived (if any). The archived PRD said reset keeps run_log and integration_install only.

## rebuild reset

`reset_for_source` for all selected sources inside one `write_transaction`, then parse. Parse failures do not un-reset (current product: rebuild already deleted). The atomic part is “no mixed old/new schema across sources after a failed reset loop”.

## catalog recovery

Persist in-progress identity (already have a marker). On bootstrap, load that target catalog file; if valid, finish recompute + meta switch; if invalid, error and keep marker. Do not load old active catalog and clear the marker.

## Fencing

All of the above stay behind `validate_write_transaction`.
