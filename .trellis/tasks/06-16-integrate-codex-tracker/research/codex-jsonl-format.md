# Codex JSONL 格式研究笔记

## 核心发现（基于 codex-usage-tracker/parser.py）

### 1. JSONL 事件结构

每行是一个 JSON 对象（envelope），包含：

```json
{
  "type": "event_msg" | "session_meta" | "turn_context" | ...,
  "timestamp": "2026-06-16T10:00:00.123Z",
  "payload": { ... }
}
```

### 2. Token Count 事件

关键事件类型：`type == "event_msg"` 且 `payload.type == "token_count"`

```json
{
  "type": "event_msg",
  "timestamp": "2026-06-16T10:00:00.123Z",
  "payload": {
    "type": "token_count",
    "info": {
      "last_token_usage": {
        // 当前这次调用的 token
        "input_tokens": 1000,
        "cached_input_tokens": 600,
        "output_tokens": 200,
        "reasoning_output_tokens": 50,
        "total_tokens": 1200
      },
      "total_token_usage": {
        // 累积 token（session 级别）
        "input_tokens": 5000,
        "cached_input_tokens": 3000,
        "output_tokens": 1000,
        "reasoning_output_tokens": 200,
        "total_tokens": 6000
      },
      "model_context_window": 128000
    }
  }
}
```

**关键字段**：

- `last_token_usage` - 当前调用的 token 数
- `total_token_usage` - 累积 token 数（session 内）
- `uncached_input_tokens` = `input_tokens - cached_input_tokens` （计算字段）

### 3. Session Metadata 事件

`type == "session_meta"` - 记录 session 和 subagent 信息

```json
{
  "type": "session_meta",
  "payload": {
    "id": "session-uuid",
    "thread_source": "user" | "auto-review" | "subagent",
    "source": {
      "subagent": {
        "other": "custom-type",
        "thread_spawn": {
          "agent_role": "researcher",
          "agent_nickname": "search-agent",
          "parent_thread_id": "parent-session-uuid"
        }
      }
    }
  }
}
```

### 4. Turn Context 事件

`type == "turn_context"` - 记录当前 turn 的上下文

```json
{
  "type": "turn_context",
  "timestamp": "2026-06-16T10:00:00.123Z",
  "payload": {
    "turn_id": "turn-uuid",
    "cwd": "/path/to/project",
    "model": "gpt-4",
    "effort": "medium",
    "current_date": "2026-06-16",
    "timezone": "UTC"
  }
}
```

### 5. Thread Key 生成逻辑

`_thread_key()` 函数逻辑：

- 如果有 `session_info.thread_name`，使用 `hash(session_id + thread_name)`
- 否则使用 `session_id` 作为 thread_key

### 6. Record ID 生成逻辑

`_record_id()` 函数逻辑：

```python
hash_input = f"{session_id}:{turn_id or ''}:{event_timestamp}:{cumulative_total_tokens}:{total_tokens}"
record_id = hashlib.sha256(hash_input.encode()).hexdigest()[:16]
```

### 7. 解析流程

1. 逐行读取 JSONL 文件
2. 维护状态：`session_id`, `session_meta`, `current_turn`, `last_cumulative_total`
3. 遇到 `session_meta` → 更新 session 元数据
4. 遇到 `turn_context` → 更新 turn 上下文
5. 遇到 `token_count` → 生成 `UsageEvent`
6. 累积验证：`cumulative_total > last_cumulative_total`（去重）

### 8. 增量解析支持

- 记录 `start_byte` 和 `start_line` - 下次从这里继续
- 记录 `ParserState` - 保存 session/turn 上下文
- 使用 `file.seek(start_byte)` 跳过已解析部分

### 9. Call Origin 分类

通过分析 event sequence 判断调用来源：

- `user-turn` - 用户输入
- `auto-review` - 自动 review
- `subagent` - 子 agent 调用

### 10. 实现清单

**Phase 2.2 需要实现的函数**：

- `parse_codex_jsonl_for_tracer(file_path) -> Vec<CodexTracerEvent>`
- 提取 `last_token_usage` 和 `total_token_usage`
- 维护 session/turn 状态
- 计算 `uncached_input_tokens`
- 生成 `record_id` (SHA256 hash)

**Phase 2.3 需要实现的函数**：

- `compute_thread_key(session_id, thread_name) -> String`
- `link_previous_next(events) -> Vec<CodexTracerEvent>` - 设置 previous/next record_id

**Phase 2.4 需要实现的函数**：

- 增量解析游标：记录 `byte_offset` 和 `line_number`
- 状态序列化/反序列化

## 参考实现

- `ref/codex-usage-tracker/src/codex_usage_tracker/parser.py:313-462` - 主解析循环
- `ref/codex-usage-tracker/src/codex_usage_tracker/parser.py:465-548` - \_build_event()
- `ref/codex-usage-tracker/src/codex_usage_tracker/parser.py:597-614` - \_record_id()
