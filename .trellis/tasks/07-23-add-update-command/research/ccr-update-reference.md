# CCR update 参考研究

2026-07-23 只读核对 `D:/Documents/Code/Github/ccr`：

- `crates/ccr-cli/src/cli/definitions.rs` 将 branch 位置参数默认设为 `main`，
  文档示例包含 `ccr update dev` 与 `--check`。
- `crates/ccr-cli/src/commands/update.rs` 显示当前版本、仓库和分支，实际执行
  `cargo install --git <repo> ccr --branch <branch> --force`，继承 stdout 并
  实时转发 stderr，非零退出返回错误。
- CCR 会在更新前请求确认，`--check` 只显示命令预览。

llmusage 当前 GitHub 仓库默认分支为 `main`，存在 `dev`，但没有 GitHub
Release 下载资产。其 README 使用 Cargo 源码安装。因此本任务采用同一更新
语义，并增加 `--locked` 以复用仓库锁文件；不引入 Release 下载器。
