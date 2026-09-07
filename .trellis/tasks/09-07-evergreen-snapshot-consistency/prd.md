# 保证看板数据库快照一致性

## Goal

同一次复合Dashboard快照的数据库总量、分组与趋势来自一个SQLite读取版本。 优先级 P2；状态 planning，等待用户确认后实施。

## Confirmed facts

src/query/mod.rs:85 的Dashboard只有单连接，无事务保证。src/query/snapshot.rs:159 起多个SELECT依次执行；src/store/connection.rs:47启用WAL。并发commit可被后续SELECT看到；本轮未运行确定性交错复现。

## Requirements

- R1：同一次复合Dashboard快照的数据库总量、分组与趋势来自一个SQLite读取版本。
- R2：事务只包住数据库读取，不包含网络配额或外部文件扫描；查询错误/取消后释放事务，不阻塞下次请求。

## Acceptance Criteria

- [x] AC1（R1）：用barrier在同一复合快照的两个数据库section之间让另一个连接提交新增事件，当前快照的overview/分组/趋势保持旧版本一致，下一次快照看到新版本。
- [x] AC2（R2）：异常或SQLite interrupt使读取事务结束；同一请求监管路径的下次查询成功，原有cancel/timeout回归通过。
- [x] AC3（R1,R2）：无并发写入时复合快照与现有结果相同，保持单连接计数，外部diagnostics明确不是数据库原子快照的一部分。

## Out of scope

本轮只审查/规划/隔离复现。禁止改业务代码、运行 task.py start、操作真实使用数据库、提交或远程发布。实施也仅限本任务的已批准文件与行为；不添加兼容框架、可选配置或无证据重构。

## Approval boundary

设计均为待批准提案。用户确认最新摘要后才进入实施；实施前重查HEAD、工作树及依赖任务。P1先行；与quota-provenance可独立，但不与其他query改动重叠。
