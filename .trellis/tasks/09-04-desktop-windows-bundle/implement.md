# Desktop Windows bundle — Implement

依赖 `09-04-desktop-shell-ipc`、`09-04-desktop-core-ui`、`09-04-desktop-secondary-ui`、`09-04-desktop-ops-quota` 均已完成产品改动。

## Checklist

1. 确认四条功能子任务已在工作树落地（core 可点、secondary 六卡/explorer、ops logs/额度）。未齐则停止打包。
   - 验证：对照各子任务 Change list 文件存在

2. `tauri.conf.json` nsis、identifier、无 updater。
   - 文件：`desktop/src-tauri/tauri.conf.json`
   - 符号：`bundle.targets`、`identifier`
   - 验证：JSON 无 updater endpoints

3. `just desktop-dev` / `desktop-test` / `desktop-build`。
   - 文件：`justfile`
   - 验证：`just --list` 含三条；`ci` recipe 无 `tauri build`

4. gitignore。
   - 文件：`.gitignore`
   - 验证：三行 desktop 产物路径存在

5. README + `docs/dashboard`（中英文）。
   - 文件：`README.md`、`README.zh-CN.md`、`docs/dashboard/index.md`、中文对应页
   - 验证：含 Desktop 入口与 SmartScreen 说明

6. 本机 `just desktop-build`。
   - 验证：`desktop/src-tauri/target/release/bundle/nsis/` 有安装包

7. `python scripts/check-ci-gate.py`。
   - 验证：退出码 0

完成后回到父 `implement.md` 树级集成门禁，逐条核对父任务验收清单。本子任务通过不等于任务树可归档。

## Validate

`python scripts/check-ci-gate.py` 通过；根 `justfile` 的 `ci` recipe 不含 `tauri build`。

## Rollback

还原 justfile / gitignore / 文档；删除 bundle 配置不影响 `tauri dev`。
