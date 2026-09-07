# 常青项目审查与五套 Harness 对齐改造

## Goal

基于真实仓库、现有测试与失败工作流，为llmusage提供可逐项批准的常青改造计划，明确Claude Code、Codex、Grok Build、Kimi Code、OMP的权限、上下文、委派和模型分工。修复先于大规模重构；本轮完成审查和父子计划，实施尚未获批。

## Confirmed facts

基线dev / d34dd69a0c3f5db563475a05ead2b83b9e181eba，起始干净；项目结构、实测与因果证据统一见research/audit.md、research/test-results.md、research/harness-matrix.md。隔离探针已复现普通sync数据被清空；常规现有测试均通过，但不能覆盖该保留不变量。

## Requirements

- R1：读取真实项目结构/关键代码/规范并报告有锚点的发现。
- R2：运行现有测试，分开记录PASS、FAIL、SKIPPED、UNVERIFIED，追溯失败工作流因果链。
- R3：给出有优先级、文件所有权、具体验收和依赖顺序的父子计划。
- R4：对照五套harness边界，强模型规划/终审，较便宜模型只执行确定的小范围工作。
- R5：检查AGENTS/CLAUDE/各工具规则冲突与缺失，批准项在实施验收后回写版本化项目说明或spec并标适用工具。
- R6：本轮保持planning；任何业务修改、全局配置、外部库写入、真实数据迁移或发布须在对应范围明确获批。

## Acceptance Criteria

- [x] AC1（R1,R2）：报告包含仓库结构、关键数据流、实测命令/结果和失败的具体因果证据；不会把ignored或历史CI当当前通过。
- [x] AC2（R3）：建立7个子任务，均含PRD/design/implement和真实JSONL上下文，无示例种子；父子回链与路径可验证。
- [x] AC3（R4,R5）：五平台各有当前证据、能力限制、规则加载、模型继承/降价条件和验收边界，包含共享规则与生成副本权属。
- [x] AC4（R3,R5）：用户批准的子项实施后各自通过AC和必要检查，强模型终审、回写位置与适用工具可追溯；未批准项保持planning。
- [x] AC5（R6）：当前只增加任务/研究材料，生产代码和锁文件无改动，未start/commit/push、未修改全局/团队库。

## Child map

- P1 — evergreen-safe-automatic-repair：阻止普通同步与启动修复清空历史。
- P1 — evergreen-semver-workflow：修复 Semver 工作流与同源基线。
- P1 — evergreen-test-gates：补齐桌面与看板测试门禁。
- P1 — evergreen-harness-contracts：对齐五套 Harness 项目契约。
- P2 — evergreen-snapshot-consistency：保证看板数据库快照一致性。
- P2 — evergreen-quota-provenance：统一配额缓存命中来源。
- P2 — evergreen-remote-accounting：校验远程来源计费口径。

## Out of scope

没有证据的大模块拆分、兼容框架、整套harness安装/升级、真实用户数据库重建、原生UI操作、SSH远程修改、发布、上游Trellis修复和团队知识库写入。P2为可单独批准的建议；不自动扩大到修复所有历史数据。

## Decisions for approval

推荐先批准4个P1子项；F1采用停止隐式重建、保留历史并提示显式修复。P2三项可单独排期。若要求维持自动修复体验，F1需改为单独审查的原子暂存方案。当前没有隐藏的作者决定或自动批准；等待用户对这份具体方案的确认。
