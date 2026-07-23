# 添加自更新命令

## Goal

为 `llmusage` 增加从官方 GitHub 仓库源码自更新的命令，使用户无需手写
`cargo install` 即可安装稳定分支或开发分支的最新版本。

## Background

- 当前 crate、二进制和顶层命令名均为 `llmusage`；用户消息中的 `ccr update`
  表示要沿用 CCR 的更新语义，实际命令应为 `llmusage update`。
- 官方仓库为 `https://github.com/bahayonghang/llmuasage`，默认分支为
  `main`，同时维护 `dev` 分支。
- GitHub 当前没有可下载的 Release 构建资产，仓库现有安装文档使用 Cargo。
  因此本任务通过 `cargo install --git` 从指定分支编译并强制安装，不设计
  预编译二进制下载协议。
- CCR 的参考实现会显示当前版本、仓库和目标分支，支持 `--check` 预览，
  更新前交互确认，并实时透传 Cargo 输出。

## Requirements

- R1: `llmusage update` 默认从官方仓库的 `main` 分支更新 `llmusage`。
- R2: `llmusage update dev` 从官方仓库的 `dev` 分支更新 `llmusage`。
- R3: 只接受 `main` 和 `dev` 两个公开渠道；其他位置参数必须由 Clap 拒绝，
  避免把任意远端分支误当作受支持的更新渠道。
- R4: 实际安装等价于
  `cargo install --git https://github.com/bahayonghang/llmuasage llmusage --branch <branch> --locked --force`。
- R5: Cargo 的标准输出和标准错误实时显示；Cargo 不存在、进程启动失败或
  返回非零状态时，命令必须失败并保留可执行的人工复现命令。
- R6: 更新逻辑不得读写 `~/.llmusage` 数据库、配置、hook 或本地使用数据。
- R7: README 中英文入口和匹配的 VitePress 中英文页面必须说明稳定/开发
  更新命令、Cargo/Rust 前置条件以及 `dev` 的稳定性风险。
- R8: 自动化测试必须覆盖默认 `main`、显式 `dev`、非法渠道拒绝和外部命令
  失败传播；测试不得访问网络或覆盖开发机上的已安装二进制。
- R9: 完整沿用 CCR 的交互契约：`--check` / `-c` 只显示当前版本、仓库、
  分支和等效 Cargo 命令，绝不启动 Cargo；实际更新前显示相同信息并请求
  确认，空输入视为确认，明确输入 `n` / `no` 时取消且成功退出。
- R10: 无法读取确认输入时必须安全失败，不能把 EOF 或 I/O 错误当作确认。

## Acceptance Criteria

- [x] AC1: `llmusage update --help` 清楚显示默认 `main` 以及 `dev` 渠道。
- [x] AC2: CLI 解析测试证明省略渠道得到 `main`，传入 `dev` 得到 `dev`，
  传入其他值返回解析错误。
- [x] AC3: 通过注入的无网络进程执行器运行聚焦测试时，安装参数精确包含
  官方仓库、包名、目标分支、`--locked` 和 `--force`。
- [x] AC4: 注入执行器返回非零状态或模拟 Cargo 无法启动时，`update` 返回非零，
  且错误信息包含分支和可复制的人工安装命令。
- [x] AC5: `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`
  以及更新命令聚焦测试通过。
- [x] AC6: `just ci` 通过；若因环境原因无法运行，必须记录未验证项，不能视为通过。
- [x] AC7: README 和 VitePress 中英文文档与实际命令保持一致并成功构建。
- [x] AC8: `--check`、拒绝确认和确认输入读取失败均不会调用进程执行器；
  空输入/`y` 会执行，`n`/`no` 会取消。

## Out of Scope

- 发布或下载 GitHub Release 预编译资产。
- 自动更新检查、后台更新、定时提醒或遥测。
- 任意 branch/tag/revision/repository URL 更新。
- 自更新后自动迁移、重建或删除用户数据。
- 改名或新增 `ccr` 二进制别名。
