# ARCH-002 依赖边界强制执行

## Goal

完成 `sync` 到 `commands` 反向依赖的移除，并用语义可靠的 CI gate 阻止全限定路径、别名或新增文件再次绕过边界。

## Confirmed Evidence

- `src/sync/job_registry.rs:101` 仍直接构造 `crate::commands::sync::CommandSyncExecutor`。
- `.github/workflows/ci.yml:150` 只 grep `use crate::commands`，漏掉全限定路径。
- 旧提交/任务宣称 ARCH-002 已完成，说明仅文本 grep 不足以作为验收。

## Requirements

- application/sync 层只依赖 stable trait/request/result，不构造 CLI adapter。
- composition root 在 commands/web 等外层注入 executor/service。
- CI gate 基于模块依赖解析或覆盖所有 Rust path 形式的可靠检查；禁止只 grep 单一语法。
- gate fixture 必须证明 `use`、`crate::commands::`、alias、nested module 四种违规均被识别。
- 保持 CLI/Web sync 行为不变，不在本任务扩展为 God-module 全量拆分。

## Acceptance Criteria

- [ ] `src/sync/**` 对 `src/commands/**` 无编译或源码依赖。
- [ ] JobRegistry 测试通过注入 fake executor，不引用 command implementation。
- [ ] CI architecture test 对四类违规 fixture 全部失败，对合法 dependency graph 通过。
- [ ] 删除 `CommandSyncExecutor` 或移动它不会要求修改 sync/application 层。
- [ ] 完整 CLI/Web job lifecycle tests 和 `just ci` 通过。

## Out of Scope

- 不执行旧 ARCH-002 任务中的 <800 行 God-module 拆分和整个 1.x public API deprecation 计划。
