# 零调用检测(僵尸技能/MCP)

> 父任务:`06-21-absorb-aiusage-call-analytics`。遵守父任务「共享约束」。
> 顺序建议:在 `06-21-opencode-tool-call-parsing` 之后做,「已用集合」的 OpenCode 来源才完整(非硬阻塞:Claude/Codex 部分可先行)。

## Goal

枚举本地**已装**的技能/MCP 清单,与 llmusage 已统计的**已用**集合求差,标出「装了但从未调用」的僵尸技能 / 僵尸 MCP,以**只读建议**形态接入 behavior 的 Optimize。

## 背景与契合点

- llmusage 已有「Optimize 只读建议」:`src/tui/panels/behavior.rs::render_optimize` + `query::OptimizePayload`(`findings`: severity/title/evidence/recommendation),且已显式声明「**llmusage 不会自动删除、归档、重写或清理任何内容**」。零调用检测天然是一种新 finding。
- 「已用集合」可直接取自 `usage_tool_call`(Claude/Codex 已有;OpenCode 依赖父任务下另一子任务补齐)。
- llmusage 当前**没有**任何「已装清单」探测,故无法区分「装了没用」与「没装」。这是纯增量能力。

## 清单来源(AIUsage 实测,`docs/CALL_ANALYTICS_DESIGN.md` §7)

| 来源 | Skill 目录 | MCP 配置(格式) |
|------|-----------|----------------|
| Claude | `<claude>/skills` 下 `*/SKILL.md` | `~/.claude.json`(JSON,含 `projects.*.mcpServers`) |
| Codex | `<codex>/skills` 下 `*/SKILL.md` | `<codex>/config.toml`(**TOML**:`[mcp_servers.NAME…]` 取首段 NAME) |
| OpenCode | `<opencode>/skills` 下 `*/SKILL.md` | `<opencode>/opencode.json(c)` 的 `mcp`/`mcpServers` 块 |

> 路径必须复用 llmusage 现有的来源根发现(如 `CLAUDE_CONFIG_DIR`、`$CODEX_HOME`、OpenCode data 目录),**不得硬编码 AIUsage 的 macOS `~/.…` 路径**(llmusage 跨平台,含 Windows)。

## Requirements

1. **清单探测**(尽力而为):按来源枚举已装技能(`*/SKILL.md` 目录名)与已装 MCP(JSON + TOML 两种配置)。某来源根缺失则跳过,不报错、不阻塞主统计。
2. **求差**:`已装清单 − 已用集合 = 零调用项`;以 `InstalledItem{source, name}` 为单位,**同名项装在多家各算一条**。
3. **按来源归属**:零调用结果按 Claude/Codex/OpenCode 归属;若 behavior 报告支持来源筛选,则与现有口径同步(选某应用 → 只看该应用自己装且自己用过的)。
4. **可清理范围 = 仅用户自建技能**:排除插件/内置捆绑技能(如 `plugins/cache` 下的子技能)出「可清理」清单;但这些技能的**调用仍照常统计**(用过就进排行、算「已用」),只是不标为可清理候选。
5. **只读接入**:作为 Optimize 的 finding / 专门小节呈现,延续「不自动清理」声明;给出可清理候选名单即可,不执行任何文件操作。
6. **覆盖范围**:只覆盖 llmusage 已解析的三家 CLI;不扫 Cursor/IDE 等自身用量不在这些日志里的来源(否则其条目永远误报为僵尸)。

## 非目标

- **不实现「规则命中」统计**:CLAUDE.md/AGENTS.md/.cursor/rules 是上下文注入,日志无 per-rule 离散信号(AIUsage 同样不做)。
- 不做任何自动删除/归档/重写;不向用户配置写回。
- 不在本任务引入成功率/耗时维度。

## Acceptance Criteria

- [ ] 本机至少一家来源:装了但从未调用的技能/MCP 被列为零调用候选,已调用的不出现在候选里。
- [ ] 清单探测能解析三种配置形态:Claude/OpenCode 的 JSON 与 Codex 的 TOML(`[mcp_servers.NAME]` 取首段)。
- [ ] 路径发现复用 llmusage 既有来源根逻辑,在 Windows 下不依赖 `~/.…` 硬编码。
- [ ] 插件/内置捆绑技能即使从未调用也**不**出现在「可清理」候选,但其调用仍计入「已用」。
- [ ] 缺失某来源根时优雅跳过,不影响其余来源与主 behavior 报告。
- [ ] 结果以只读建议呈现,保留「llmusage 不自动清理」声明;无文件写操作。
- [ ] 针对 SKILL.md 目录枚举、JSON mcpServers、TOML mcp_servers 的单测;`just ci` 通过。

## 风险

- 「可清理」边界(用户自建 vs 捆绑)依赖目录约定,跨平台/跨版本可能漂移 → design 阶段固定判定规则并测试。
- OpenCode「已用集合」完整性依赖父任务下 `opencode-tool-call-parsing` 完成;在其完成前,OpenCode 零调用结果可能偏多(已用集合不全)→ 在 design/实现中标注或排序在后。

## Notes

- `task.py start` 前补 `design.md`(清单探测器 + 求差 + 来源归属 + 可清理判定)与 `implement.md`(执行清单 + 验证命令)。
- 参考实现:AIUsage `QuotaBackend/.../CallAnalytics/CallAnalyticsInventory.swift`(§7 决策记录 6/7)。
