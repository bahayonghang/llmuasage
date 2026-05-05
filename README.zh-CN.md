# llmusage

[English](./README.md)

本地优先的 Rust CLI。

目标很直接：用 hook 和本地 SQLite 复现多 CLI 用量采集，不上传、不登录、不连云端 API。

感谢 [vibeusage](https://github.com/victorGPT/vibeusage) 提供思路。`llmusage` 在它的基础上用 Rust 做了重构和改进，并把本地优先这条边界收得更紧。

当前 v1 覆盖：

- Codex `config.toml notify`
- Claude `Stop` / `SessionEnd` hooks
- OpenCode `session.updated` plugin event

核心真源：

- 配置目录：`~/.llmusage/`
- 数据库：`~/.llmusage/llmusage.db`
- hook 包装器：`~/.llmusage/bin/llmusage-hook.cmd`、`~/.llmusage/bin/llmusage-hook.sh`

命令：

报表优先命令（只读本地 SQLite；如果数据看起来过旧，先运行 `llmusage sync`）：

- `llmusage` / `llmusage daily`
- `llmusage monthly`
- `llmusage session`
- `llmusage blocks`
- `llmusage statusline`

`llmusage` / `llmusage daily` 默认按所选时区只显示今天；需要完整 daily 历史时使用 `--all`，需要指定范围时使用 `--since YYYYMMDD` / `--until YYYYMMDD`。

常用报表参数包括 `--since YYYYMMDD`、`--until YYYYMMDD`、`--json`、`--breakdown`、`--order asc|desc`、`--timezone UTC|local|+08:00`、`--locale en-US|zh-CN|ja-JP`、`--compact`、`--source codex|claude|opencode`。

运维命令：

- `llmusage init`
- `llmusage sync`（`--rebuild` 会重新解析本地真源并重建用量行/bucket）
- `llmusage status`
- `llmusage diagnostics`
- `llmusage doctor`
- `llmusage serve`
- `llmusage tui`
- `llmusage export html`
- `llmusage uninstall`

Web 分析页：

下面这张图就是 `llmusage serve` 启动后的本地浏览器分析页。

![llmusage 本地 web 分析页概览](./docs/public/screenshots/web-dashboard-overview.png)

开发：

```powershell
cargo check
cargo test
cargo run -- init`r`ncargo run -- sync`r`ncargo run -- --json`r`ncargo run -- serve
```

说明：

- `serve` 只监听 `127.0.0.1`
- `export html` 生成离线静态报告
- 报表命令都是只读 SQLite 视图，不会自动 sync`r`n- `status`、`diagnostics`、`doctor` 都是只读命令
