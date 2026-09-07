# 补齐桌面与看板测试门禁

## Goal

现有全部看板 JS 测试由同一个执行入口发现并运行，包括 CSV；不维持多份易漏的测试文件清单。 优先级 P1；状态 planning，等待用户确认后实施。

## Confirmed facts

`justfile:89` 的 ci 首行 cargo update；看板 tests 只列6文件，实际7文件。`.github/workflows/ci.yml:119` 同样漏CSV；Cargo.toml不是包含desktop的workspace。desktop/package.json:8 的 build仅 vite build。现有各套测试本轮均已独立执行通过。

## Requirements

- R1：现有全部看板 JS 测试由同一个执行入口发现并运行，包括 CSV；不维持多份易漏的测试文件清单。
- R2：独立 desktop crate、前端类型、现有测试与生产资源构建进入本地和 CI 门禁。
- R3：验证入口不更新 Cargo.lock；更新操作仍留在显式 version-sync/依赖维护流程。

## Acceptance Criteria

- [x] AC1（R1）：统一 JS 入口运行7个现有 suite/66 tests（初始基线）；增加一个有意失败的临时测试文件可使入口非零退出，移除后恢复绿色，且该入口被 just ci 和 Actions 同时调用。
- [x] AC2（R2）：desktop 前端64、Rust29既有测试、tsc --noEmit、vite生产build通过；新增 Windows desktop CI job 被 ci-gate.needs 覆盖，任务失败会阻止 CI gate。
- [x] AC3（R2）：desktop CI 使用独立 Cargo.lock 的 --locked 检查、npm ci和兼容现有 Vite 的 Node版本；不构建/执行安装器，也不把单测作为原生GUI证明。
- [x] AC4（R3）：修订后 just ci前后两个 Cargo.lock 与 package-lock 字节不变；错位锁文件在独立夹具中明确失败，不能由检查自动修复；version-sync 仍可显式更新root锁文件。

## Out of scope

本轮只审查/规划/隔离复现。禁止改业务代码、运行 task.py start、操作真实使用数据库、提交或远程发布。实施也仅限本任务的已批准文件与行为；不添加兼容框架、可选配置或无证据重构。

## Approval boundary

设计均为待批准提案。用户确认最新摘要后才进入实施；实施前重查HEAD、工作树及依赖任务。semver-workflow先完成；两者共享ci.yml和ci-toolchain-contracts.md，串行合并。
