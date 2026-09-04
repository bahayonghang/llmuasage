# Desktop Windows bundle

父任务：`.trellis/tasks/09-04-llmusage-desktop-mvp`。依赖 **shell-ipc、core-ui、secondary-ui、ops-quota 均已完成**。本子任务产出最终未签名安装包，不代替父任务树级集成门禁。

## Goal

本机打出未签名 Windows NSIS/exe，接上 just 与文档，且不破坏根 CI 门禁。

## Requirements

- 继承父任务 Windows 打包范围。不含 macOS/Linux 编译验收。
- R16: 继承父任务未签名 NSIS/exe。
- R18: 继承父任务 `just ci` / `CI gate` 名不变。
- B1. `tauri.conf.json` bundle `nsis`；无 updater endpoints。
- B2. `just desktop-dev` / `desktop-test` / `desktop-build`。
- B3. gitignore：`desktop/node_modules/`、`desktop/dist/`、`desktop/src-tauri/target/`。
- B4. README 与 `docs/dashboard`（及中文页）增加 Desktop 入口。说明 SmartScreen 可能告警。
- B5. 不改 `.github/workflows/ci.yml` 里 `CI gate` 的 `name:`，不把 `tauri build` 加进根 `just ci`。
- B6. 不把 macOS/Linux 编译或发版资产列入本子任务完成条件。

## Out of scope

- 代码签名、公证、GitHub Release、自动更新、macOS/Linux 发版资产
- macOS/Linux 可编译验证（后续未验证目标）
- 用本子任务的 `tauri build` 替代父任务功能验收清单

## Acceptance Criteria

- [ ] AC1（R16）：Windows 上 `just desktop-build` 在 `desktop/src-tauri/target/release/bundle/nsis/`（或文档写明的路径）产出安装包。
- [ ] AC2（R18）：`python scripts/check-ci-gate.py` 仍通过。
- [ ] AC3（R18）：根 `justfile` 的 `ci` recipe 不含 `tauri build`；`CI gate` 作业 `name:` 未改。
- [ ] AC4（R16）：README 与 `docs/dashboard` 中英文页有 Desktop 入口，并写明未签名/SmartScreen。
- [ ] AC5（R16）：开始本子任务前，shell/core/secondary/ops 的产品改动已在同一工作树上。若其中任一未完成，不得将本包称为最终交付。
