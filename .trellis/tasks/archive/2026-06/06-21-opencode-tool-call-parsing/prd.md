# OpenCode 工具调用解析(part 表)

> 父任务:`06-21-absorb-aiusage-call-analytics`。遵守父任务「共享约束」。

## Goal

补齐 llmusage `behavior` 对 OpenCode 的空白:从 `opencode.db` 的 `part` 表提取工具/MCP/skill 调用,归一化为 `UsageToolCall`,使 OpenCode 与 Claude/Codex 一致地进入 behavior 的 Tools 维度。

## 背景与缺口

- llmusage `src/parsers/opencode.rs` 当前只查 `message` 表(`json_extract(m.data,'$.role')` 等)取 token/cost,**完全没有解析 `part` 表的工具调用**。
- 对照 `src/parsers/behavior.rs`:Claude 有 `extract_claude_tools`、Codex 有 `extract_codex_tools`,**OpenCode 无对应函数**,故 OpenCode 会话在 Tools 排行、MCP 统计里始终为空。
- llmusage 已具备 OpenCode db 只读访问能力(发现路径 + 查询 message 表),扩展成本低。

## 数据源事实(AIUsage 实测,`docs/CALL_ANALYTICS_DESIGN.md` §2.4)

- 数据目录解析顺序:`$XDG_DATA_HOME/opencode` → `~/.local/share/opencode` → `~/Library/Application Support/opencode`(llmusage 发现逻辑已对齐)。
- 调用数据在同一个 db 的 **`part` 表**,`data`(JSON)的 `type` 分布:`text / step-start / step-finish / tool / reasoning`。
- tool part 形状:
  ```jsonc
  { "type":"tool", "tool":"glob", "callID":"call_…",
    "state": { "status":"completed", "time": { "start":1767339068099, "end":1767339068373 } } }
  // skill: { "type":"tool", "tool":"skill", "state": { "input": { "name":"<技能名>" } } }
  ```
- 提取口径:
  - 工具名 = `part.data.tool`(实测:`webfetch/read/bash/write/glob/edit/skill/question`)。
  - **MCP** = tool 名带 OpenCode 的 MCP 前缀(形如 `<server>_<tool>`);server 归属优先用 `opencode.json` 已装 server 名做**最长前缀匹配**,匹配不到才回退「首个下划线切分」。
  - **Skill** = `tool=="skill"`,技能名取 `state.input.name`。
  - **时间桶**:part 自身无独立 created;join 所属 `message.time_created`(或 part 所在 message 的时间)。
- 复用:OpenCode 已封装「复制 `db`/`-wal`/`-shm` 到临时目录只读打开」;新增一条 `SELECT data FROM part`(必要时 join message 取时间/session)即可,避免二次遍历。

## Requirements

1. 在 `src/parsers/opencode.rs` 解析流程中新增 `part` 表查询,提取 `type=="tool"` 行。
2. 在 `src/parsers/behavior.rs` 新增 `extract_opencode_tools`(或等价路径),把 OpenCode tool part 归一化为 `BehaviorToolEvidence` → `UsageToolCall`,口径与 Claude/Codex 对齐:
   - `tool_kind` 经 `classify_tool` 分类;MCP 经 server 前缀识别填 `mcp_server`/`mcp_tool`。
   - Skill(`tool=="skill"`)技能名取自 `state.input.name`(而非字面 "skill")。
3. 正确关联到归一化模型:part → 所属 message → 对应 `UsageEvent`/`turn`,保证 `session_id`/`project_hash`/`occurred_at` 与既有 OpenCode usage 行口径一致。
4. 隐私:沿用 `input_fingerprint` / `safe_preview`;OpenCode part 的 `state.input` 等只取受控字段,不持久化全文。
5. 增量/幂等:与现有 OpenCode sync 的增量口径一致,重复 sync 不产生重复 `usage_tool_call` 行(沿用 `tool_call_key` 幂等键)。

## 非目标

- 不在本任务做成功率/耗时维度(`state.status` / `state.time` 虽可得,留待父任务「未来可选」项);本任务只保证**调用计数与归类**正确。若实现顺带保留 `state` 原始信号成本极低,可在 design 阶段评估是否预留字段,但不扩展 UI。
- 不碰 Claude/Codex 既有解析。
- 不改 OpenCode 的 token/cost(message 表)解析路径。

## Acceptance Criteria

- [ ] 一个含工具调用的 OpenCode 本地会话,sync 后其 `read/bash/glob/edit/write` 等调用出现在 `behavior` 的 Tools 维度,计数与 db `part` 表实际行数一致。
- [ ] OpenCode 的 MCP 工具被识别为 `ToolKind::Mcp` 且 `mcp_server` 归属正确(含 server 名带下划线的最长前缀匹配场景)。
- [ ] OpenCode 的 `tool=="skill"` 调用技能名取自 `state.input.name`,而非字面 "skill"。
- [ ] 针对真实 part JSON 形状(tool / skill / mcp)的单测覆盖;含「server 名含下划线」的前缀匹配用例。
- [ ] 重复 sync 不产生重复 tool_call 行;Claude/Codex 既有 behavior 输出无回归。
- [ ] `just ci` 通过;无任何网络/登录代码路径。

## 风险

- OpenCode `<server>_<tool>` 命名在「未配置且 server 名含下划线」时仍可能切错(弱信号,AIUsage 亦如此);design 阶段确定回退策略并在测试中标注已知边界。
- part↔message↔event 关联若与现有 message 解析不在同一遍/同一事务,需确认时间与会话归属一致,避免「今日」跨天串味。

## Notes

- `task.py start` 前补 `design.md`(part 表查询 + 与 message 解析的关系 + 归一化数据流)与 `implement.md`(执行清单 + 验证命令)。
- 参考实现:AIUsage `QuotaBackend/.../CallAnalytics/OpenCodeCallEventSource.swift`、`OpenCodeCostProvider+Database.swift`。
