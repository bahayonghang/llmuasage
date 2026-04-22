# llmusage

本地优先的 Rust CLI。

目标很直接：用 hook 和本地 SQLite 复现多 CLI 用量采集，不上传、不登录、不连云端 API。

当前 v1 覆盖：

- Codex `config.toml notify`
- Claude `Stop` / `SessionEnd` hooks
- OpenCode `session.updated` plugin event

核心真源：

- 配置目录：`~/.llmusage/`
- 数据库：`~/.llmusage/llmusage.db`
- hook 包装器：`~/.llmusage/bin/llmusage-hook.cmd`、`~/.llmusage/bin/llmusage-hook.sh`

命令：

- `llmusage init`
- `llmusage sync`
- `llmusage status`
- `llmusage diagnostics`
- `llmusage doctor`
- `llmusage serve`
- `llmusage tui`
- `llmusage export html`
- `llmusage uninstall`

开发：

```powershell
cargo check
cargo test
cargo run -- init
cargo run -- sync
cargo run -- serve
```

说明：

- `serve` 只监听 `127.0.0.1`
- `export html` 生成离线静态报告
- `status` / `diagnostics` / `doctor` 都是只读命令
