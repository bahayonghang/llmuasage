# 五套 Harness 能力与规则矩阵

证据日期2026-09-07；这是审查结果和待批准建议，不是已生效项目规则。能力是harness可调用的机制，模型能力和账户价格是另外两件事。

## 现场状态

| Harness / 本机版本 | 项目入口与上下文 | 规划/独立审查 | 较便宜执行边界 |
| --- | --- | --- | --- |
| Claude Code 2.1.263 | CLAUDE.md导入AGENTS；.claude agents/skills/hooks | 适合共享规则、跨层语义和权限审查；使用可用强模型 | agent model/effort可单配；现有三agent未pin，默认inherit；可用Haiku等必须现场确认 |
| Codex CLI 0.153.4 | AGENTS链；.agents skills；.codex agents与原生hooks | 本次主审环境；适合Rust/SQLite/CI/独立验证 | agent TOML可pin；本地三个model/effort均注释，当前继承；可选已可用mini/Terra/Luna做小范围执行 |
| Grok Build 1.0.21 | grok inspect实见根AGENTS/CLAUDE、97 skills、3 project agents；当前pull-based | 适合原生Grok日志与命令边界第二视角；不能以品牌推定审查质量 | 可用subagents model机制；当前未分层，账户型号不能证明便宜；不要声称已有cheap lane |
| Kimi Code 0.41.0 | AGENTS与共享.agents/skills；内置plan/explore/coder | 适合给足确定上下文后的独立计划/审查 | secondary model或显式支持的model机制需现场验证；用户全局配置本轮不改；highspeed不等于便宜 |
| OMP 18.1.12 | 根AGENTS可由provider向上发现；.omp agents/extensions | 适合利用已有plan/slow角色做模型可替换的计划/复审 | research/implement的pi/task是跟随会话的特殊hint，check默认inherit；smol/prewalk需先有有效配置，不是现已低价 |

上表是工程适配建议，不是五品牌智力排名。强规划/强终审始终独立于执行者；便宜模型只接收已定稿需求、专属文件、红色回归和明确升级条件。不做价格数字比较，因为账户订阅/缓存/费率与可用模型没有统一现场账单证据。

## 发现

### H1 / P1：Codex hook命令依赖启动目录

.codex/hooks.json:9、:20、:32、:44 使用相对 .codex/hooks/ 路径；从src启动相同命令时Python找不到src/.codex/hooks文件，exit2。Claude .claude/settings.json也存在相对命令风险，但未把代码级推断升级为完整会话故障。根目录运行/信任状态并不证明子目录场景正常。Codex官方说明命令cwd为会话cwd；Claude推荐CLAUDE_PROJECT_DIR。修复应在Trellis模板源使用平台支持的稳定root解析，再验证根目录和src入口。[Codex hooks](https://developers.openai.com/codex/hooks)、[Claude hooks](https://code.claude.com/docs/en/hooks)。

### H2 / P1：Kimi角色技能发现路径漂移

本机 .kimi-code/skills/trellis-{research,implement,check}存在，但官方项目自动扫描 .kimi/skills、.claude/skills、.codex/skills、.agents/skills，不扫描.kimi-code/skills。.trellis/workflow.md:223与cli_adapter.py仍指该旧目录。已跟到Trellis安装模板/configurator；不应仅改忽略副本后宣称长期解决。当前workflow明确要求built-in coder/explore主动读取该角色文件，因此手动pull仍可工作，不能说Kimi完全不可用。项目默认补明确手动路径/根目录启动fallback；自动发现的模板修复需要跨仓库另行授权。[Kimi skills](https://moonshotai.github.io/kimi-cli/en/customization/skills.html)、[Kimi agents](https://moonshotai.github.io/kimi-cli/en/customization/agents.html)。

### H3 / P2：共享规则、hook注释、低成本分层没有完整对齐

AGENTS.md:5、:24仍写tests/*.rs；Cargo.toml采用autotests=false和8个domain target，新同事按旧说明加根测试会漏跑。.codex/config.toml:12-16称hooks需显式启用，与现场features hooks=true且官方默认启用不一致；exact hash trust仍是单独条件。

CLAUDE.md:1的@AGENTS.md正确。AGENTS.md缺desktop独立crate和五工具矩阵。五套agent基本继承主model，尚无项目“强审查/便宜执行”契约；不应把pi/task模型搜索失败误判agent失效。[Codex agents](https://developers.openai.com/codex/subagents)、[OMP agent discovery](https://github.com/can1357/oh-my-pi/blob/main/docs/task-agent-discovery.md)。

### H4 / P2：生成副本与项目事实源边界

.gitignore:4-12忽略.agents/.claude/.codex/.grok/.kimi-code/.omp。本次Trellis dry-run显示298 unchanged，不能因此判模板正确，也不能把本地副本当可追踪团队规则。建议在AGENTS的managed block外链接一份tracked docs/agents/harness-contracts.md，保持CLAUDE导入。

Grok当前hooks OFF，不需为“对齐”强行启用；其pull说明准确。OMP可读根AGENTS，不强制添加第二套规则。五工具必须共享授权边界，自动注入不可用时显式读任务manifest与spec即可继续当前已授权工作。[Grok官方](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/README.md)、[OMP context](https://github.com/can1357/oh-my-pi/blob/main/docs/context-files.md)、[Codex AGENTS](https://developers.openai.com/codex/guides/agents-md)。

## 通过与待验证

独立agent已执行版本、trellis platforms、grok inspect、kimi doctor、codex features list、JSON/TOML解析及Python compileall。未启动五套新会话，因此真实加载、hook信任/注入与子agent接收上下文均UNVERIFIED。修复项目说明可在本仓库完成；Trellis模板、用户级模型配置、全局trust不能在本轮偷偷修改。

## 交接最低内容

每项委派注明Active task绝对/仓库相对路径、仅规划/允许实施、专属文件、目标行为、必跑AC、模型ID和effort、不得改的真实数据库/凭据/远程范围。遇到需求不清、红色回归无法解释、权限/事务/协议设计变化时升级给强模型。最终审查不能沿用执行模型自报PASS。
