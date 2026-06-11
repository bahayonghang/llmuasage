# Implement

## Checklist

- [x] Finalize product priority for the sync command center versus usage-first first viewport.
- [x] Start the Trellis task after planning approval.
- [x] Load `trellis-before-dev` before editing.
- [x] Update the dashboard shell/CSS/renderer with a compact sync status presentation.
- [x] Keep render logic on structured sync fields only.
- [x] Update focused tests that assert sync command center wiring and responsive styling.
- [x] Rebuild or rerun the docs fixture.
- [x] Capture after screenshots at `1440x1100` and `390x844`.
- [x] Re-measure section bounds and compare against the before measurements.
- [x] Run targeted checks, then broader gates if practical.

## Results

- Desktop `1440x1100`: `#overview` reduced from 1055px to 742px; `#kpi-grid` improved from 44% visible to 100% visible; `#trends` moved from fully below the fold to 22% visible.
- Mobile `390x844`: horizontal overflow is gone; mobile sidebar reduced to 53px; `#kpi-grid` is 75% visible in the first viewport.
- Sync status remains visible and actionable as a compact collapsed details surface unless risk/running state opens it.
- Final browser verification used a fresh current-source fixture on `http://127.0.0.1:37424` because the pre-existing `37421` fixture was serving old embedded assets.
- After screenshots were regenerated from `37424`:
  - `output/playwright/serve-after-1440x1100.png`
  - `output/playwright/serve-after-390x844.png`
- Full gate passed with `rtk just ci` after updating the trend chart asset test to assert the new mobile no-overflow contract.

## Validation Commands

```powershell
rtk cargo test web::tests::dashboard_shell_and_assets_wire_sync_command_center
rtk cargo test web::tests::dashboard_assets_style_sync_command_center_responsively
rtk cargo test web::tests::api_dashboard_embeds_sync_command_center_contract
rtk cargo test web::tests::trend_chart_assets_expose_peak_and_empty_styles
rtk cargo fmt --check
rtk git diff --check
rtk just ci
rtk cargo run --features testing --example docs_dashboard_serve -- --port 37421
rtk npx --yes playwright screenshot --channel msedge --viewport-size "1440,1100" --wait-for-timeout 1500 http://127.0.0.1:37421 output\playwright\serve-after-1440x1100.png
rtk npx --yes playwright screenshot --channel msedge --viewport-size "390,844" --wait-for-timeout 1500 http://127.0.0.1:37421 output\playwright\serve-after-390x844.png
```

## Risky Files

- `src/web/assets/render/sync-command-center.js`: must preserve data redaction and structured-key behavior.
- `src/web/shell.rs`: shell order affects anchors and docs expectations.
- `src/web/assets/layout.css`: responsive changes can affect mobile sidebar and filters.
- `src/web/assets/components.css`: sync command center styling must remain accessible in light/dark themes.

## Rollback Point

If the compact sync presentation creates unclear job state or action behavior, revert the renderer/CSS changes and keep only any safe spacing fixes.
