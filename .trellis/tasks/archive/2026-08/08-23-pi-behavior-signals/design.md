# 设计：Pi 行为信号

## 边界

新增 `src/parsers/behavior.rs` 的 `extract_pi_tools`，在 `src/parsers/pi.rs` 的
记录处理回调里产出 turn 与 tool_call，并通过 `SyncShard` 提交。不改 schema，
不改 `usage_event` 的口径，不改共享分类表与 `safe_tool_preview`。

## 决定 1：工具证据取自助手记录自身，不做跨记录关联

真源把工具调用写在助手记录的 `content[].type == "toolCall"` 里，把结果写在后续独立的
`role == "toolResult"` 记录里。选择只读前者：

- 与 `extract_claude_tools` 同构，可直接复用 `tool_evidence` 管道。
- 不需要在解析器里维护跨记录状态，因此与增量续读（从游标 offset 开始）天然兼容：
  一条助手记录的工具信息全部在这条记录内。
- 代价：拿不到 `isError`，所以本任务不产出工具失败率。若以后要做，需要按
  `toolCallId` 做跨记录关联，并解决「结果记录可能落在下一次增量窗口」的问题。

## 决定 2：arguments 的形态处理

真源当前形态是 **JSON 对象**（2026-08-23 扫描 1449/1449 都是 object，字符串 0）。
`tool_evidence` 的签名是 `input: Option<&Value>`，因此对象形态**直接传引用**，
不做任何解析。

同时保留字符串分支：Pi 家族其他客户端与历史版本可能写序列化后的字符串，
此时先 `serde_json::from_str` 得到 `Value` 再传入；解析失败或类型不是
object/string 时传 `None`，但仍然产出 tool_call 行（只是 `safe_preview` 与
`input_fingerprint` 为 `None`）。

这一点是对上一版设计的更正：上一版写「arguments 是 JSON 字符串，需要 from_str」，
与真源不符。传对象很关键——`safe_tool_preview` 需要结构化 input 才能给出预览，
而 `classify_tools` 判定测试类 Bash 命令时会读预览文本
（`src/parsers/behavior.rs:315`）。

## 决定 3：turn 的构造与幂等

用 `behavior::turn_from_tools(&event, &tools)`：`turn_key` 由
`format!("turn:{}", event.event_key)` 派生，因此 turn 的幂等性直接继承事件键的幂等性
（记录起始 byte offset 参与哈希，重放不变）。`tool_call_key` 同理由
`tool:{source}:{event_key}:{sequence}` 派生。

重放清理已由写入端实现：`shard.reset_path_hashes` 非空时
`reset_behavior_facts_batch_tx` 先清该 path_hash 的旧事实再插入新事实
（`src/store/sync_writer.rs:656`）。本任务只需在测试里验证这条链路（AC4.10）。

## 决定 4：retries 的取值

`turn_from_tools` 返回的 turn 里 `retries` 为 0，需要在 pi 侧覆盖：
读 `message.retryRecovery.attempt`（i64，缺失或非数为 0），赋给 `turn.retries`。
`one_shot` 保持既有语义（有编辑动作且无重试），因此在设置 `retries` 之后要按既有规则
重算，避免「有重试仍标 one_shot」。具体表达式以 `behavior.rs` 现有实现为准，
不在 pi 侧另立规则。

## 决定 5：工具分类沿用共享规则，不为 Pi 立表

上一版设计写「对未命中的名字在通用表里补规则」，这会与既有分类结果冲突。
核对 `classify_tool`（`src/parsers/behavior.rs:340`）后确认：

- `matches_tool_name` 是**子串**匹配，所以 `glob`/`find` 命中 `["grep","glob","find","search"]`
  → `Search`；`web_search` 也因 `search` 子串 → `Search`。
- fallback 是 `ToolKind::Core`，不是 `Other`（`:372`）。
- 因此 `hub`、`eval`、`yield`、`goal`、`ask` 全部落 `Core`。
  注意 `goal` 不含 `plan`/`todo` 子串，不会落 `Planning`。
- `todo_write` 落 `Edit` 而不是 `Planning`：Edit 分支的 `write` 子串先于
  Planning 分支的 `todo` 匹配（`:351` 在 `:360` 之前）。本机仅 2 次调用。
  实现前评审确认：保持共享表，把 PRD R4.5 的期望改为 `todo_write`→`Edit`。

决定：**不改共享表**，PRD R4.5 改为按共享规则的真实结果逐名固定期望值。理由：
共享表同时服务 claude/codex/opencode，改 `glob→Read` 或改 fallback 会静默改动
这三个源已有的 35,704 + 173,900 + 11,970 行行为事实的语义；那是独立决定，
不该由 Pi 接入顺带完成。

若评审希望调整（例如让 `goal` 落 `Planning`），改动范围与影响必须单列评估。

## 决定 6：提交路径与有界 sync

`parse_pi_shard` 的输出结构增加 `turns` 与 `tool_calls` 两个向量，
在 `sync_pi` 的 `commit_shard` 调用点替换掉当前的 `Vec::new()`。

`recent_cutoff` 过滤事件时必须同步过滤 turn 与 tool_call，否则有界 sync 会写入
没有对应事件的孤儿行——当前 `sync_pi` 的 cutoff 分支只处理 `shard.events`
（`src/parsers/pi.rs:213`），需要一并处理（AC4.9）。

## 兼容性

- 无 schema 变化。
- 历史数据按父任务 R8 用 `sync --rebuild --source omp` 回填。
- 回滚：纯代码回退；已写入的 turn / tool_call 行用
  `sync --rebuild --source omp` 清理。
