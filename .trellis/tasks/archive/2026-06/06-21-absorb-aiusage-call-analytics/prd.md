# 吸收 AIUsage 调用分析能力

## Goal

把参考项目 `ref/AIUsage`(macOS SwiftUI,代理+配额监控)中**唯一与 llmusage「本地只读」理念兼容的能力族 —— Call Analytics(调用分析)** 吸收进 llmusage 的 `behavior` 模块。父任务承载需求集、范围边界与跨子任务验收;**本身不直接实现**。

## 背景:为什么只吸收 Call Analytics

AIUsage 与 llmusage 在「多 provider 使用量/成本」上重叠,但形态相反:

- llmusage = Rust CLI,只读本地工件 → SQLite → 报告/dashboard,**无账号登录、无远程 API、无上传**。
- AIUsage = GUI,核心是**三套代理拦截** + **官方配额 API 查询**(需登录/token)+ Canonical 协议转换(Claude⇄OpenAI)。

AIUsage 的 Call Analytics 是其中**唯一只读、零埋点**的子系统:解析 Claude/Codex/OpenCode 本地会话日志,统计 MCP/技能/工具调用。它与 llmusage 现有 `src/parsers/behavior.rs` 同源,因此可吸收。

## 范围

### 纳入(本次)

1. **OpenCode 工具调用解析(part 表)** — 子任务 `06-21-opencode-tool-call-parsing`
2. **零调用检测(僵尸技能/MCP)** — 子任务 `06-21-zero-call-detection`

### 明确排除(违反本地只读底线,永不吸收)

- 三套代理(Claude/Codex/OpenCode proxy)与请求拦截。
- 官方配额 API 查询(需账号登录/token)。
- Canonical Middle Layer 协议转换(为 proxy 服务)。
- 多账号管理、凭证保险库(Keychain)、cc-switch 同步、统一 API 提供商分发、菜单栏 GUI。

### 已对齐,无需吸收(仅记录)

- **缓存 token 计费归一化**:llmusage 已正确实现 OpenAI Responses 的 `cached_tokens` 子集语义(`src/parsers/codex.rs:527-532`,`input = max(raw - cached, 0)`,带回归测试)。AIUsage `docs/CACHE_COMPATIBILITY.md` 的跨厂商缓存对照表仅可作注释/文档参考。

### 未来可选(本次不建,优先级低)

- Skill 细分到具体技能名 / 成功率·耗时维度(分母独立)/ Claude subagent 分组(读 `agent-<id>.meta.json` 的 `agentType`)。
- Codex skill 启发式(读 `SKILL.md` 推断,弱信号有噪声)。

## 现状基线(llmusage 已有)

- 归一化三层模型 `UsageEvent` / `UsageTurn` / `UsageToolCall`(`src/domain/models.rs`),写入 SQLite。
- 工具调用解析:Claude `extract_claude_tools` + Codex `extract_codex_tools`(`src/parsers/behavior.rs`);**OpenCode 无**(`src/parsers/opencode.rs` 只读 `message` 表的 token/cost)。
- `ToolKind` 分类(core/mcp/bash/skill/agent/planning/read/edit/search/other)、`split_mcp_tool` MCP 拆分。
- 展示层:TUI `src/tui/panels/behavior.rs`(Activity/Tools/Optimize/Compare)+ Web behavior 面板。
- 隐私基线:`input_fingerprint`(哈希)、`safe_preview`(≤120 字截断)、不存全文。

## 共享约束(所有子任务必须遵守)

1. **本地只读**:只解析已存在的本地工件,不发起网络请求、不登录、不写回用户配置。
2. **隐私保持**:延续 `input_fingerprint` / `safe_preview` 口径,绝不持久化完整 prompt 或文件全文。
3. **不回归现有 behavior**:Claude/Codex 既有解析、`behavior` 报告与 TUI/Web 面板的输出口径不被破坏。
4. **确定性**:同一输入产出同一结果(同分稳定排序、哈希稳定),便于测试与快照。
5. **优雅降级**:某来源/某文件缺失或解析失败时跳过并显式说明,不阻塞主统计、不伪造零行。

## 跨子任务验收

- [ ] 两个子任务各自的 `prd.md` 验收项全部满足。
- [ ] OpenCode 会话的工具/MCP/skill 调用与 Claude/Codex 一致地出现在 `behavior` 的 Tools 维度。
- [ ] 零调用检测以「只读建议」形态接入 Optimize,明确标注 llmusage 不自动删除/清理任何内容。
- [ ] `just ci`(fmt + lint + test)通过;新增解析逻辑有针对真实日志形状的单测。
- [ ] 不引入任何网络/登录/上传代码路径。

## Notes

- 子任务可独立规划、实现、验收;无强制先后,但建议先做 OpenCode 解析(为零调用检测的「已用集合」补齐 OpenCode 来源)。
- 子任务复杂度中等:`task.py start` 前各自补 `design.md`(数据流/schema)与 `implement.md`(执行清单)。
