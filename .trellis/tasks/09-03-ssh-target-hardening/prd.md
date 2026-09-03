# SSH 目标拒绝选项注入

## Goal

`remote add` 存下来的 `ssh_target` 不能被当成 OpenSSH 选项。本地进程继续不走 shell。

## Background

`ssh_args` 生成 `-o BatchMode=yes -o ConnectTimeout=N <ssh_target> <remote_argv...>`（`src/remote/transport.rs:21-30`）。没有 `--`。以 `-` 开头的 target 会变成额外 `-o`。本地 argv 拆分已避免本地 shell（`:307-318`）。攻击需要恶意 `remote add` 或被改过的 `host` 行。

## Requirements

- R1. 拒绝以 `-` 开头的 `ssh_target`（`remote add` 与执行路径都拒绝）。
- R2. `ssh` argv 在 destination 前插入 `--`。
- R3. 合法 `user@host`、`host`、`ssh://user@host` 形式继续可用。
- R4. 现有“本地不走 shell”测试保留；新增以 `-o` / `-oProxyCommand` 为 target 的拒绝用例。

## Acceptance Criteria

- [ ] AC1. `ssh_args` 在 target 前含 `--`。
- [ ] AC2. `validate` / `remote add` 对 `-oProxyCommand=...` 返回 `ConfigInvalid`。
- [ ] AC3. `me@devbox` 仍能生成可执行 argv。
- [ ] AC4. 不改变远程侧 `sshd` 对 `--command` 的 shell 解释（文档可注明）。

## Out of scope

- 给 SSH 加认证或改 shard 协议。
- loopback CSRF（独立子任务）。
