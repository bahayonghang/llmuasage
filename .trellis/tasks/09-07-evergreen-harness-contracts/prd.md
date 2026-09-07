# 对齐五套 Harness 项目契约

## Goal

AGENTS.md是版本化项目事实入口；CLAUDE.md保持有效导入；五套工具的加载、权限、hooks、skills、委派与验证边界可查。 优先级 P1；状态 planning，等待用户确认后实施。

## Confirmed facts

AGENTS.md:5/24 与实际8个Cargo测试target不一致。CLAUDE.md:1有效导入共享规则。五套平台目录和.agents均被.gitignore忽略；本地生成能力见父任务 research/harness-matrix.md。OMP pi/task是特殊继承hint，不是普通可搜索模型，不能因models find无匹配判定失效。

## Requirements

- R1：AGENTS.md是版本化项目事实入口；CLAUDE.md保持有效导入；五套工具的加载、权限、hooks、skills、委派与验证边界可查。
- R2：规定强模型规划/最终审查、可验证小任务交较便宜模型；不把harness名称等同模型价格，不把继承模型误报低成本执行。
- R3：规则修复在项目可追踪说明生效并声明本地生成副本的实际状态；不擅自修改外部Trellis仓库、全局模型配置或团队知识库。

## Acceptance Criteria

- [x] AC1（R1）：AGENTS.md准确列出src/sync、src/remote、desktop和tests/<domain>入口、8个target、按改动选择的命令；CLAUDE仍指向同一规则，不复制另一套。
- [x] AC2（R1）：五套工具矩阵逐一标明文件发现顺序、已配置与未验证、无hook时的手动上下文路径、只读审查/批准后实施边界；Kimi三角色指令必须能由已知路径明确读取，不能声称.kimi-code自动扫描。
- [x] AC3（R2）：每个委派示例含task路径、只读/写入边界、文件所有权、模型档、验收、升级条件；Claude/Codex/Grok/Kimi/OMP各用真实支持机制，并说明未pin时的继承行为；规划与独立终审不能自动降级。
- [x] AC4（R3）：批准且已实施的规则回写AGENTS及docs/agents/harness-contracts.md，每项标注适用工具、证据日期和验证方式；生成模板漂移列出精确路径和上游建议，未实施上游修复不标done。
- [x] AC5（R3）：逐项核对本地CLI inspect/features/版本等只读探针；真实新会话hook/skill/agent握手若未运行则留UNVERIFIED，而非依据文件存在宣称已加载。

## Out of scope

本轮只审查/规划/隔离复现。禁止改业务代码、运行 task.py start、操作真实使用数据库、提交或远程发布。实施也仅限本任务的已批准文件与行为；不添加兼容框架、可选配置或无证据重构。

## Approval boundary

设计均为待批准提案。用户确认最新摘要后才进入实施；实施前重查HEAD、工作树及依赖任务。可先独立起草；在其余批准子任务完成后最后回写实际命令/行为。不修改上述子任务拥有的spec。
