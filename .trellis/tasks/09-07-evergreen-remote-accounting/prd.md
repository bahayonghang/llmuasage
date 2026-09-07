# 校验远程来源计费口径

## Goal

远程shard写入前验证source的token-accounting contract；同wire版本不代表token语义相同。 优先级 P2；状态 planning，等待用户确认后实施。

## Confirmed facts

src/remote/protocol.rs:18的Header没有每源accounting版本；src/remote/importer.rs:49忽略Header、:179生成None；src/commands/source_status.rs:57/107读取source全局marker。ADR0014的schema诊断约定未覆盖计费语义。

## Requirements

- R1：远程shard写入前验证source的token-accounting contract；同wire版本不代表token语义相同。
- R2：remote source-status使用host/source自身证据，不用local全局marker掩盖未知或旧远程口径；不自动回填历史计费结论。
- R3：继续只读远程工具数据；协议改变要求显式版本不匹配错误，不添加旧协议兼容推测。

## Acceptance Criteria

- [x] AC1（R1,R3）：协议header携带每个本次来源的accounting版本；缺失/不相等在首个相关shard commit之前拒绝，不改变已有该host/source事件与watermark，错误给出来源和升级动作。
- [x] AC2（R2）：local current+remote unknown/旧口径夹具分别展示准确状态；旧记录没有证据就维持unknown，不能用本地marker宣布current。
- [x] AC5（R1,R2）：已有remote行但无可信历史marker时，当前版本增量stream仍被拒绝；空来源+无since+成功无错误trailer才可建立host/source marker；重开Store后marker仍可读取，中途失败不能建立marker。
- [x] AC3（R1,R2）：匹配版本的重放幂等、跨host隔离、OMP/Pi迁移和缺trailer不推进watermark的已有契约继续通过。
- [x] AC4（R3）：文档声明wire新版本与同时升级要求；本轮不连接或升级真实SSH主机、不重建历史库。

## Out of scope

本轮只审查/规划/隔离复现。禁止改业务代码、运行 task.py start、操作真实使用数据库、提交或远程发布。实施也仅限本任务的已批准文件与行为；不添加兼容框架、可选配置或无证据重构。

## Approval boundary

设计均为待批准提案。用户确认最新摘要后才进入实施；实施前重查HEAD、工作树及依赖任务。safe-automatic-repair之后，tests/sync/accounting.rs共享文件必须串行；真实远程回放不在当前批准范围内。
