# 接入 Pi 行为信号

## Goal

Pi / Oh My Pi 产出 `usage_turn` 与 `usage_tool_call`，进入行为看板。

## 背景与证据

- `src/parsers/pi.rs:246` 提交 `turns: Vec::new()` 与 `tool_calls: Vec::new()`。
  库中 `usage_turn` 与 `usage_tool_call` 里 `source='pi'` 均为 0 行。
- 真源的助手记录自带工具调用块，与 Claude 的 `content[].type == "tool_use"` 同构：
  `message.content[].type == "toolCall"`，块结构 `{type,id,name,arguments,intent}`。
  2026-08-23 扫描（28 个文件、递归）：1449 个块，`arguments` **全部是 JSON 对象**
  （object 1449、string 0、其他 0）。
- 工具名分布（16 种）：bash 617、read 309、edit 146、write 120、hub 66、grep 59、
  eval 40、todo 37、glob 25、task 10、yield 9、web_search 5、todo_write 2、find 2、
  goal 1、ask 1。
- 工具结果是独立记录（`role == "toolResult"`，带 `toolCallId`、`toolName`、`isError`）。
- 重试证据在助手记录上：`message.retryRecovery`，形如
  `{kind:"auto-retry", status:"recovered", attempt:1, recovery:"plain", supersededBy:{...}}`，
  本机 3 条。
- 真源不写 `childUsage` / `aggregateUsage`（扫描计数 0），因此命名子会话与其父会话
  之间不存在归集用量的重复计数。
- 现成复用点：`src/parsers/behavior.rs` 的 `tool_evidence`、`turn_from_tools`、
  `tool_calls_from_evidence` 已把分类、MCP 拆分、安全预览与指纹做成通用管道，
  `extract_claude_tools` 是同构先例。
- 写入端已实现路径级行为事实清理：`reset_behavior_facts_batch_tx` 由
  `shard.reset_path_hashes` 驱动（`src/store/sync_writer.rs:656`），
  本任务只需验证，不需另造机制。

## Requirements

- **R4.1** 每条带 usage 的助手记录产出一个 turn（保守的一事件一 turn，
  与 Claude/Codex 一致）。
- **R4.2** 助手记录 `content[].type == "toolCall"` 的块产出 `usage_tool_call`，
  经 `behavior::tool_evidence` 走既有分类管道。
- **R4.3** `arguments` 为 JSON 对象时直接作为 `Value` 传入（真源当前形态）；
  为 JSON 字符串时先 `serde_json::from_str` 再传入（兼容其他 Pi 系客户端）；
  其他类型或解析失败时传 `None`。三种形态都不得丢弃该 tool_call 行。
- **R4.4** `retries` 取自 `message.retryRecovery.attempt`，缺失为 0；
  设置 `retries` 之后按 `turn_from_tools` 的既有规则重算 `one_shot`，
  不得出现「有重试仍标 one_shot」的行。
- **R4.5** 工具种类映射**沿用** `behavior::classify_tool` 的共享规则，
  不为 Pi 单独立表。对本机 16 个工具名，验收时逐名断言实际落到的 `ToolKind`，
  以共享规则的真实结果为准：`bash`→`Bash`；`read`→`Read`；`write`/`edit`→`Edit`；
  `grep`/`glob`/`find`/`web_search`→`Search`（`classify_tool` 用子串匹配，
  `src/parsers/behavior.rs:358`）；`todo`→`Planning`；`todo_write`→`Edit`
  （`classify_tool` 先匹配 `write` 再匹配 `todo`，`src/parsers/behavior.rs:351`）；
  `task`→`Agent`；
  `hub`/`eval`/`yield`/`goal`/`ask`→`Core`（共享规则的 fallback 是 `Core`，
  不是 `Other`，`src/parsers/behavior.rs:372`）。
  只有在评审明确决定要改共享规则时才动 `behavior.rs` 的分类表，
  且必须同时评估对 claude/codex/opencode 既有行的影响。
- **R4.6** 隐私沿用父任务 R7：`safe_preview` 用既有 `safe_tool_preview` 实现与 120
  字符上限，不新增也不收紧脱敏；不落库工具结果内容。

## 非目标

- 不跨记录关联 `toolResult`，因此本任务不产出工具失败率指标（`isError` 留待后续）。
- 不合并多事件为一个 turn。
- 不使用 `contextSnapshot`、`duration`、`ttft`。
- 不改 token 归一：重试产生的多条助手记录各自都有 usage，仍各记一次事件。
- 不改 `safe_tool_preview`（父任务 R7）。
- 不改 `behavior::classify_tool` 的共享分类表（除评审另有决定）。

## 依赖

依赖 `08-23-omp-source-split` 与 `08-23-pi-event-dimensions`
（turn 与 tool_call 都带 `project_hash`，需要项目维度先落地）。
历史回填按父任务 R8 用 `sync --rebuild --source omp`。

## Acceptance Criteria

- [ ] **AC4.1**（R4.1/R4.2）本机 `sync --rebuild --source omp` 后 `usage_turn` 与
      `usage_tool_call` 中存在 `source='omp'` 行。
- [ ] **AC4.2**（R4.2）`usage_tool_call` 中 `source='omp'` 的行数等于当次真源扫描的
      `tool_call_blocks`。
- [ ] **AC4.3**（R4.3）单测三例：`arguments` 为对象、为 JSON 字符串、为非法值。
      前两例 `safe_preview` 与 `input_fingerprint` 均非空，第三例两者为 `None`
      且 tool_call 行仍产出。
- [ ] **AC4.4**（R4.5）单测：对 16 个工具名逐一断言 `ToolKind`，期望值按 R4.5 列出的
      映射（不是只断言类别集合非空）。
- [ ] **AC4.5**（R4.4）单测两例：`retryRecovery.attempt=1` 的 turn `retries=1`
      且 `one_shot=false`；无 `retryRecovery` 的编辑 turn `retries=0`
      且 `one_shot` 与既有规则一致。
- [ ] **AC4.6**（R4.4）本机验证：`usage_turn` 中 `source='omp'` 且 `retries>=1` 的
      行数等于当次扫描的 `retry_records`。
- [ ] **AC4.7**（R4.1）本机验证：`usage_turn` 中 `source='omp'` 的 `project_hash` 非空。
- [ ] **AC4.8**（R4.6）单测：`safe_preview` 长度 <= 120；负向断言不含工具结果内容
      （`toolResult` 记录的 `content`）。
- [ ] **AC4.9**（R4.2）单测：有界 sync（`recent_cutoff`）下 turn 与 tool_call 数量与
      过滤后的事件保持一致，不产生孤儿行。
- [ ] **AC4.10**（R4.2）验证路径级重放：改写一个 `.omp` 文件后重放，
      旧 `path_hash` 的 turn/tool_call 被清理且不重复累积（依赖
      `src/store/sync_writer.rs:656` 的既有机制，只做验证）。
- [ ] **AC4.11**（R4.1）看板行为面板在 `source='omp'` 下有数据、无报错，
      样本不足时的既有提示逻辑不变。
