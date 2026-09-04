# Parent AC1–AC27 gate notes

Recorded after five children were archived. Automated items use named commands. Human/visual items that need a live Desktop window are 未验证: `tauri dev` failed with `listen EACCES: permission denied 127.0.0.1:1420`, and launching the built exe would open the real `~/.llmusage` (forbidden).

NSIS used for AC14: `desktop/src-tauri/target/release/bundle/nsis/llmusage_1.3.0_x64-setup.exe` (5,878,130 bytes). `just desktop-build` exit 0. Not a planted installer.

## Automated (parent implement.md table)

| AC | Verdict | Evidence |
|---|---|---|
| AC2 | pass | `cargo test --manifest-path desktop/src-tauri/Cargo.toml -- --test-threads=1` twice, exit 0. Facade/DTO/Fixture tests in `desktop/src-tauri/tests/ac.rs`. |
| AC3 | pass | `ac3_lock_busy_and_runtime_info` |
| AC4 | pass | `python scripts/check-ci-gate.py` exit 0. `justfile` `ci` has no `tauri build`. `.github/workflows/ci.yml:198` `name: CI gate`. `just ci` exit 0. |
| AC7 rebuild=false | pass | `ac7_convert_sync_rebuild_stays_false`; `syncOptionsFromState` npm tests. Live sync UI is human. |
| AC8 | pass | `ac1` empty-root bootstrap + `ac4` SchemaTooNew |
| AC13 | pass | `desktop/src-tauri/tests/quota.rs` local 127.0.0.1 endpoints; credential bytes unchanged |
| AC14 | pass | NSIS installer non-empty under documented path |
| AC15 | pass | `ac6_cancel_first_request_second_completes`; frontend generation/`cancel_queries` npm tests |
| AC23 | pass | `git diff` on `src/commands/serve.rs` / `src/web` empty vs pre-desktop base. `just ci` exit 0. |
| mapping/generation | pass | `npm --prefix desktop test` 64 tests, exit 0 |

## Human / live window (parent implement.md)

| AC | Verdict | Notes |
|---|---|---|
| AC1 | 未验证 | No live window without real home / EACCES on 1420 |
| AC5 | 未验证 | Ten nav blocks in source/tests; not live-rendered |
| AC6 | 未验证 | npm AC8/core-then-secondary; not live-rendered |
| AC7 UI | 未验证 | Mapping tests pass; live running/lock_busy display not launched |
| AC9 | 未验证 | single-instance plugin configured; focus behavior not launched |
| AC10 | 未验证 | CSV BOM tests pass; system save dialog not used on a real path |
| AC11 | 未验证 | prefs unit tests pass; restart-after-GUI not launched |
| AC12 | 未验证 | quota npm tests pass; live Usage page not launched |
| AC16 | 未验证 | StatusPanel tests pass; live sidebar not launched |
| AC17 | 未验证 | explorer npm tests pass; live controls not launched |
| AC18 | 未验证 | heatmap toggle unit tests pass; live click not launched |
| AC19 | 未验证 | logs npm tests pass; live paging not launched |
| AC20 | 未验证 | fake-timer prefs tests pass; live interval not launched |
| AC21 | 未验证 | project/host click npm tests pass; live click not launched |
| AC22 | 未验证 | cache_hit cargo+npm; live quota page not launched |
| AC24 | 未验证 | quota tests local-only; live process network not observed |
| AC25 | 未验证 | token/layout jsdom tests pass; 1440/720 live viewports not captured |
| AC26 | 未验证 | npm home_overview-after-core; live paint not captured |
| AC27 | pass | independent `desktop/src-tauri/Cargo.toml` path dep; root `Cargo.toml` has no `[workspace]` (structural, not GUI) |

## Not in scope / deferred

- macOS/Linux compile: 未验证 (TPR-10)
- code signing / SmartScreen clearance / GitHub Release / updater: out of scope

## Failures

None recorded.
