# 将 Cindy 纳入用量统计

## Goal

让 `llmusage sync` 在不改 Cindy、不上传原文的前提下，把 Cindy 正式 profile 里的 Pi 会话用量记成可筛选的独立来源。用户应能在统一报表、`source-status` 和 `--source cindy` 里看到 Cindy，而不是把它混进 Pi，或靠设置 `PI_AGENT_DIR` 临时拐过去。

## Background

当前没有 `cindy` 来源。Pi 解析器只读 `PI_AGENT_DIR` 或 `~/.pi/agent/sessions`。本机 Cindy 会话在 `%APPDATA%\CindyGlobal\pi-agent-home\sessions\`，格式与 Pi v3 JSONL 相同，assistant `usage` 可精确归一化。已有库里没有这些路径。

ccusage 对这种分叉的做法是：默认 Pi 根目录不动，另加命名 store，用自己的 agent 名进统一报表，并拒绝与 Pi 或其他 store 重叠的路径。`PI_AGENT_DIR` / `--pi-path` 只影响默认 Pi。本仓库 ADR 0015 已对 Oh My Pi 做了同样的产品切分。证据见 `research/ccusage-and-cindy.md`。

## Confirmed facts

- 官方 profile 目录名是 `Cindy` 与 `CindyGlobal`。Windows 在 `%APPDATA%`，macOS 在 `~/Library/Application Support/`，Linux 在 `$XDG_CONFIG_HOME` 或 `~/.config`。会话根是 `{userData}/pi-agent-home/sessions/`。`dirs::config_dir()` 与这三处父目录一致。
- `CindyDev` 与 `CindyGlobal-dev2-*` 是开发或沙箱 profile，不是已发布用户数据目录。
- 可计费用量在 assistant `message.usage`：`input`、`output`、`cacheRead`、`cacheWrite`、`totalTokens`，以及 `usage.cost`。全 0 记录没有可计入的用量。`reasoningTokens` 只是诊断字段。
- 原始 JSONL 含提示词、回复和工具内容。解析器只能持久化归一化用量、模型、provider、时间和项目/会话标识。
- `{userData}/pi-agent-home/sessions/shared` 是导入的分享转录，不是用户自己的 Cindy 回合。
- Cindy SQLite 汇总没有覆盖本机这批 Pi 会话。Cindy 隔离的 Codex rollout 属于 Codex 工件。

## Requirements

- R1. 新增稳定来源 id `cindy`，显示名 `Cindy`。它进入同步、统一报表、`source-status` 和 `--source`，不作为 `pi` 的别名，也不新增 `llmusage cindy` 子命令。
- R2. 被动发现正式 profile 的 `{userData}/pi-agent-home/sessions/**/*.jsonl`。目录不存在时视为空，不报失败。不要求用户设置 `PI_AGENT_DIR`，也不读取该变量。
- R3. 复用 Pi 的 token 与 source cost 合同：精确渠道、权威 `totalTokens`、推理 token 不计入总量、正的 `usage.cost.total` 保留。质量标签为 precise。
- R4. 与 `pi` 或 `omp` 的规范化路径重叠时，不把同一文件记进 `cindy`。只跳过冲突文件。
- R5. 不导入 `sessions/shared` 下的分享转录。
- R6. 显式会话目录覆盖只替换 Cindy 根，不影响 Pi / Oh My Pi 发现。未设置时仍扫正式 profile。
- R7. 同步两次不得插入重复事件。截断、改写和追加沿用现有文件游标语义。
- R8. 不读取 Cindy SQLite 汇总，不把 `codex-home` 记成 `cindy`，不扫描开发沙箱 profile。

## Acceptance Criteria

- [x] AC1. 未设置 `PI_AGENT_DIR` 时，正式 Cindy profile 下的非零 assistant usage 在 `llmusage sync` 后以 `source=cindy` 出现，token 渠道与源文件一致。
- [x] AC2. 同一数据库再同步一次，Cindy 事件数不增加。
- [x] AC3. 与 Pi 或 Oh My Pi 根目录重叠的文件不出现在 `cindy`。
- [x] AC4. 没有 Cindy profile 时，sync 成功，且不产生 Cindy 事件。
- [x] AC5. `source-status` 能报告 Cindy 根目录、解析器已注册，以及 precise / 本地工件隐私级别。
- [x] AC6. 夹具来自脱敏样本，测试不持久化提示词或回复正文。`sessions/shared` 不产生事件。
- [x] AC7. README、中文 README 和对应文档页列出 `cindy`。`--source cindy` 可用。没有新的 `cindy` 子命令。
- [x] AC8. 设置会话目录覆盖后，sync 只读这些目录，不读本机真实 Cindy profile，也不改变 Pi 根。

## Out of scope

- 读取 `daily_model_usage`、`daily_session_usage` 或 `sessions.total_token_usage`。
- 把 `{userData}/codex-home` 并入 `codex` 或 `cindy`。
- 扫描 `CindyDev`、`CindyGlobal-dev2-*`、远程 `$HOME/.xdt-server/v1/pi-agent-home`。
- 为 Cindy 单独做费用模式、估算 token，或新增 focused 子命令。
- 安装 hook、写 Cindy 配置，或上传会话。
- 引入 Cindy 商标 logo。没有专属图标时使用现有 fallback。

## Decisions

- 2026-10-25：只统计正式 profile 的 Pi 会话，来源 id 为 `cindy`。不把隔离的 `codex-home` 并入本任务，也不改读 SQLite 汇总。
