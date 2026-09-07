# 修复 Semver 工作流与同源基线

## Goal

Semver 检查使用工具支持的参数和本仓库已验证 release 基线，不从同名但不同项目的 crates.io 条目选基线。 优先级 P1；状态 planning，等待用户确认后实施。

## Confirmed facts

三次失败见父任务 research/ci-main-failure.log、ci-main-previous-failure.log、ci-aug12-failure.log。`.github/workflows/ci.yml:172` 仍为 cargo semver-checks --locked，`:171` 限制 main push。registry-baseline.json 指向 openrijal/llmusage；本地和远程 v1.2.0 tag 已核对。

## Requirements

- R1：Semver 检查使用工具支持的参数和本仓库已验证 release 基线，不从同名但不同项目的 crates.io 条目选基线。
- R2：PR、main push、workflow_dispatch 均可实际执行该检查；保留稳定 CI gate 名称及所有叶子任务聚合。
- R3：项目元数据不把另一个项目的 docs.rs 页面当作本项目 API 文档；明确 --locked 仅用于支持它的 Cargo 子命令。

## Acceptance Criteria

- [x] AC1（R1）：使用 cargo semver-checks --baseline-rev v1.2.0（解析为本项目 commit 9b7a6f3dec12764222891c2d8f5aeb42db7bd490），实际进入 API 比较，不出现 unexpected argument，也不访问同名注册表基线。
- [x] AC2（R1）：baseline 不存在时清晰失败；不静默退回 crates.io。若实际比较产生 SemVer 差异，逐项报告，不自动写兼容层或降低失败等级；是否扩大 API/version 修改需再审批。
- [x] AC3（R2）：本地已修改命令实际跑过，并在获准远程交付后的 PR/main/dispatch 事件证据中能看到非 skipped 的语义检查；CI gate 在叶子失败/成功时分别失败/成功。 GitHub PR/main/dispatch check-runs remain UNVERIFIED (no push).
- [x] AC4（R3）：Cargo.toml documentation 去掉未经验证的 docs.rs 指向；说明记录本地 cargo doc 入口与本仓库文档，CI 规范区分 cargo install --locked 与 cargo semver-checks 参数。

## Out of scope

本轮只审查/规划/隔离复现。禁止改业务代码、运行 task.py start、操作真实使用数据库、提交或远程发布。实施也仅限本任务的已批准文件与行为；不添加兼容框架、可选配置或无证据重构。

## Approval boundary

设计均为待批准提案。用户确认最新摘要后才进入实施；实施前重查HEAD、工作树及依赖任务。在 test-gates 之前修改共享 ci.yml；不与其并行写。
