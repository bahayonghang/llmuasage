# Self-update 不可变版本解析

## Goal

让 stable 自更新只安装可验证的不可变 release 目标，避免把可变 `main` HEAD 描述并执行为 stable/tagged 版本。

## Confirmed Evidence

- `src/commands/update.rs:133` 只对 dev 输出 unsafe warning，stable 输出声称 stable/tagged。
- `src/commands/update.rs:178` 对 main/dev 都使用 `cargo install --branch`。
- 当前流程不解析 tag/commit，也不在确认前展示目标 SHA。
- `.trellis/spec/llmusage/backend/self-update-contracts.md` 要求 preview/check/confirm 与真实 Cargo 参数一致，测试不得执行真实安装。

## Requirements

- stable channel 从受信 release/tag 元数据解析 immutable tag 和 commit SHA；解析失败必须 fail closed。
- preview、`--check`、confirmation 与最终 argv 展示同一个 resolved target。
- install 使用 immutable `--rev <sha>` 或等价不可变发行物，不使用 `--branch main`。
- dev channel 可继续跟踪 `dev`，但必须明确可变、未经稳定发布验证，并展示解析到的当前 commit。
- 网络/解析/安装逻辑保持可注入，测试不访问真实网络、不执行真实 `cargo install`。

## Acceptance Criteria

- [ ] stable plan 的 argv 不包含 `--branch main`，包含已展示的不可变 SHA。
- [ ] tag 被移动、解析结果不一致或 commit 不属于选定 release 时拒绝安装。
- [ ] `--check`、交互确认和 process executor 看到完全一致的 target。
- [ ] dev channel 显示 unsafe/can-change warning，取消流程不回归。
- [ ] 文档不再把 branch HEAD 描述为 tagged stable。

## Out of Scope

- 不执行真实 self-update，不在本任务改造成完整二进制签名发布系统。

