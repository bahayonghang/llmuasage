# 对齐五套 Harness 项目契约：设计

## Mechanism and tradeoffs

新增一份短的、受git跟踪的 docs/agents/harness-contracts.md，由AGENTS链接；规则共用，五平台仅列差异。不复制整个global AGENTS、技能库或团队记忆。Kimi优先写清现有 `.kimi-code/skills/trellis-{research,implement,check}/SKILL.md`手动读取适配，不把此非标准目录视作自动发现。确需自动发现时，列为独立可选Trellis模板修复；本项目现有 `.agents/skills` 通用skills继续可用。

Codex hook注释与当前features实际值不一致，说明以现场features/trust为准；Grok当前hooks未开，不假定Claude hook可用；OMP pi/task继承父model，便宜执行须在已有provider/model解析成功后指定实际支持target。Kimi secondary_model涉及用户级配置，本轮默认不改；可通过独立执行会话选择已存在模型，但不得伪造per-agent override能力。

说明中的模型档只描述质量与权限；具体ID依据每套客户端当时可用列表绑定。本轮规划不pin模型、不启动额外付费harness会话、不修改忽略的生成副本。批准后以版本化说明作为五工具共同可读fallback；上游模板维护另行授权。

## File ownership

- 现有Codex/Claude hook从子目录启动的问题，项目内以明确“从仓库根启动或先解析仓库根再执行hook”的手动fallback缓解；不宣称native相对命令已修复。.codex/hooks.json与.claude/settings.json的生成模板整改属于单列上游待办。
- `AGENTS.md`
- `CLAUDE.md`
- `docs/agents/harness-contracts.md`

## Tool and model assignment

Claude Code与Codex强模型交叉审查共享规则；Grok Build核对其原生inspect，Kimi核对技能发现，OMP核对继承/role解析。便宜模型只做已批准的链接、路径、命令和表格整理。

## Failure and rollback

单项提交前用diff保留无关改动；失败只撤销本任务补丁，不重置用户工作树。不存在自动发布、自动升级远程或全局设置的授权。门禁失败需定位具体操作；未执行的原生/远程验证保留UNVERIFIED。

## Documentation writeback

批准并通过验收后同步上述拥有的项目说明/spec；适用工具标注为 Claude Code、Codex、Grok Build、Kimi Code、OMP。跨工具公共说明由 harness-contracts 子任务最终汇总。任务计划本身不是已生效规则。
