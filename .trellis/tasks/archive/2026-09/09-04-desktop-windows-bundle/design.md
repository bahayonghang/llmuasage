# Desktop Windows bundle — Design

沿用父 `design.md` 打包节。最终包在四条功能子任务完成之后产出。

## 边界

- bundle target：`nsis`。无 updater。
- 产出路径以 `tauri.conf.json` 为准；文档写明实际目录。
- 不改 `.github/workflows/ci.yml` 的 `CI gate` `name:`。
- 不把 `tauri build` 加入根 `just ci`。
- 不增加 macOS/Linux runner 或 `cargo check` 矩阵。

## Change list

| 文件 | 动作 | 关键符号 |
|---|---|---|
| `desktop/src-tauri/tauri.conf.json` | 修改 | `bundle.targets = ["nsis"]`；无 updater |
| `justfile` | 修改 | `desktop-dev` / `desktop-test` / `desktop-build` |
| `.gitignore` | 修改 | `desktop/node_modules/`、`desktop/dist/`、`desktop/src-tauri/target/` |
| `README.md`、`README.zh-CN.md` | 修改 | Desktop 入口 |
| `docs/dashboard/index.md`、对应中文页 | 修改 | Desktop 入口、未签名说明 |
| `.github/workflows/ci.yml` | 禁止改 `name: CI gate` | — |

## Contract

```text
just desktop-build
  → npm --prefix desktop + cargo tauri build
  → desktop/src-tauri/target/release/bundle/nsis/*.exe

just ci recipe 不得新增 tauri build
CI gate job name 字符串保持现网契约
```

## Verification boundary

- `python scripts/check-ci-gate.py`
- 本机 NSIS 目录有文件
- `rg "tauri build" justfile` 在 `ci` recipe 中无匹配
- 不验证 macOS/Linux 编译
- 功能验收由父任务集成门禁执行，本子任务不重复为完成条件

## 已考虑不做

- 把 macOS/Linux 编译留在完成条件（TPR-10 / 3A 已移出）。
- 在 secondary/ops 未完成时打「最终」包。
- 代码签名、GitHub Release、updater。
