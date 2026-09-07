# 父任务设计与责任边界

## Architecture

沿用sync/parser/store/query/adapters现有分层；不按大文件行数重构。详见research/audit.md图及子任务机制。

## Ordering

1. safe-automatic-repair：先阻止普通同步/serve隐式删除历史。
2. semver-workflow → test-gates：共享ci.yml/ci-toolchain-contracts顺序修改。
3. harness-contracts可先独立起草，最后写入实际命令、行为和五工具差异。
4. P2：snapshot-consistency、quota-provenance、remote-accounting仅在获批后执行；remote依赖safe-automatic-repair完成并串行写tests/sync/accounting.rs；quota公开面须在semver基线修复后审查。

## Ownership

父任务仅拥有研究报告、优先级、集成验收。每个子任务design列文件；不把父子关系当自动依赖系统。共享文件一律串行。上游Trellis生成模板、本机ignored副本、用户全局配置和团队库明确分开；默认仅完成本项目tracked说明与spec。

## Review/model policy

本轮主模型负责综合判断，Sol强模型专家完成代码/规则审查；另由独立规划审计员检查任务树。实施后最终审查仍用强模型，不沿用执行者自报结果。便宜执行只适用于定稿的小范围变更，每次绑定实际可用模型，见research/harness-matrix.md。

## Writeback

各子任务完成后回写其spec；harness-contracts最终归总AGENTS/单份五工具说明。每条合同列适用工具、验证日期、实际通过/未验证项。只批准计划不表示合同已生效。跨库永久修复需要另行给出确切范围。
