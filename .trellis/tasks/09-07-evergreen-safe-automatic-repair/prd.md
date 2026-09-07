# 阻止普通同步与启动修复清空历史

## Goal

普通 sync 检测 legacy token accounting 时保留该来源旧数据，跳过该来源本轮写入并明确提示需要显式修复；其他非 legacy 来源继续正常同步。 优先级 P1；状态 planning，等待用户确认后实施。

## Confirmed facts

`src/sync/engine.rs:228` 的普通同步分支在 parser 前提交 source reset；`src/commands/serve.rs:244` 设置 rebuild=true。`tests/sync/accounting.rs:690`、`:916` 的失败回归只检查错误/marker。详细复现由父任务 research 记录。

## Requirements

- R1：普通 sync 检测 legacy token accounting 时保留该来源旧数据，跳过该来源本轮写入并明确提示需要显式修复；其他非 legacy 来源继续正常同步。
- R2：serve 启动发现 legacy 来源时保留并展示已有数据及计费警告，不隐式设置 rebuild=true。
- R3：显式 sync --rebuild 继续遵守现有缺失文件检查与 --allow-lossy-rebuild 授权；不增加自动备份、暂存库或迁移框架。

## Acceptance Criteria

- [x] AC1（R1）：旧数据+缺失 marker+无法解析的源夹具，普通 sync 后 event、raw、bucket、turn、tool、cursor、source_file 的内容与调用前一致，marker 不提升；明确输出需显式修复的 warning，不声称完成修复。
- [x] AC2（R1）：取消发生在 legacy 检测后时仍满足 AC1 的保留不变量；混合 legacy/current 两来源时只跳过 legacy，current 来源正常同步且第二次幂等。
- [x] AC3（R2）：serve 的启动修复入口遇 legacy 不执行 reset，不因该来源解析损坏而阻止看板打开；查询可见旧总量、警告可见。使用函数/fixture验收，不启动真实用户服务器。
- [x] AC4（R3）：显式 rebuild 的拒绝、允许丢失、来源选择与幂等回归继续通过；五套 harness 都能在项目说明找到普通同步与显式重建的区别。

## Out of scope

本轮只审查/规划/隔离复现。禁止改业务代码、运行 task.py start、操作真实使用数据库、提交或远程发布。实施也仅限本任务的已批准文件与行为；不添加兼容框架、可选配置或无证据重构。

## Approval boundary

设计均为待批准提案。用户确认最新摘要后才进入实施；实施前重查HEAD、工作树及依赖任务。第一优先；无需等待门禁修复。与 remote-accounting 都修改 engine/状态时必须串行。
